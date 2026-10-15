//! Tauri command handlers — thin wrappers around the dw-core API.
//!
//! Each command:
//!  1. Validates input.
//!  2. Acquires the app state lock.
//!  3. Calls into dw-core / dw-disk.
//!  4. Converts the result to the IPC DTO.
//!  5. Returns to the webview.

use std::sync::Arc;

use parking_lot::RwLock;
use tauri::State;
use tracing::{info, warn};

use dw_ipc::commands::*;

use crate::state::{AppState, ScanRecord};

#[tauri::command]
pub async fn list_disks(
    _state: State<'_, Arc<RwLock<AppState>>>,
    _req: ListDisksRequest,
) -> Result<ListDisksResponse, String> {
    // TODO: enumerate Windows physical drives + logical volumes via
    //       QueryDosDevice + FindFirstVolumeW. For now, return an empty
    //       list — the UI will let the user pick a file-backed image too.
    info!("list_disks called");
    Ok(ListDisksResponse { disks: Vec::new() })
}

#[tauri::command]
pub async fn start_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: StartScanRequest,
) -> Result<StartScanResponse, String> {
    info!(source = %req.source_uri, deep = req.deep, "start_scan");
    let mode = if req.deep {
        dw_core::ScanMode::Deep
    } else {
        dw_core::ScanMode::Quick
    };
    let cfg = dw_core::ScanConfig {
        source_uri: req.source_uri.clone(),
        mode,
        max_results: req.max_results.max(1) as usize,
        include_carving: req.deep,
        file_filter: req.file_filter,
    };
    dw_core::scan::validate_config(&cfg).map_err(|e| e.to_string())?;
    let handle = dw_core::ScanHandle::new(cfg);
    let scan_id = handle.id();
    let record = ScanRecord::new(handle);
    {
        let mut g = state.write();
        g.scans.insert(scan_id, record);
    }
    Ok(StartScanResponse {
        scan_id: scan_id.to_string(),
    })
}

#[tauri::command]
pub async fn cancel_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: CancelScanRequest,
) -> Result<CancelScanResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %id, "cancel_scan");
    let mut g = state.write();
    if let Some(record) = g.scans.get_mut(&id) {
        record.handle.set_state(dw_core::ScanState::Cancelled);
        Ok(CancelScanResponse { cancelled: true })
    } else {
        warn!(scan_id = %id, "cancel_scan: scan not found");
        Ok(CancelScanResponse { cancelled: false })
    }
}

#[tauri::command]
pub async fn get_scan_progress(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: GetScanProgressRequest,
) -> Result<GetScanProgressResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    let g = state.read();
    let record = g.scans.get(&id).ok_or_else(|| "scan not found".to_string())?;
    let state_dto = match record.handle.state() {
        dw_core::ScanState::Created | dw_core::ScanState::Probing => dw_ipc::ScanPhase::Probing,
        dw_core::ScanState::QuickScan => dw_ipc::ScanPhase::QuickScan,
        dw_core::ScanState::DeepScan => dw_ipc::ScanPhase::DeepScan,
        dw_core::ScanState::Finalizing => dw_ipc::ScanPhase::Finalizing,
        dw_core::ScanState::Done => dw_ipc::ScanPhase::Done,
        dw_core::ScanState::Cancelled => dw_ipc::ScanPhase::Cancelled,
        dw_core::ScanState::Failed => dw_ipc::ScanPhase::Failed,
    };
    Ok(GetScanProgressResponse {
        progress: dw_ipc::ScanProgress {
            scan_id: id.to_string(),
            phase: state_dto,
            current: 0,
            total: 0,
            message: String::new(),
        },
    })
}

#[tauri::command]
pub async fn list_results(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: ListResultsRequest,
) -> Result<ListResultsResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    let g = state.read();
    let record = g.scans.get(&id).ok_or_else(|| "scan not found".to_string())?;
    let snapshot = record.handle.results().snapshot();
    let total = snapshot.len() as u64;
    let items: Vec<dw_ipc::RecoveredFile> = snapshot
        .into_iter()
        .skip(req.offset as usize)
        .take(req.limit as usize)
        .map(|e| dw_ipc::RecoveredFile {
            id: e.id.0.to_string(),
            name: e.name,
            parent_path: e.parent_path,
            size_bytes: e.size,
            mtime: e.mtime.map(|t| t.to_string()),
            status: match e.status {
                dw_core::RecoveryStatus::Intact => dw_ipc::RecoveryStatusDto::Intact,
                dw_core::RecoveryStatus::PartiallyRecovered => dw_ipc::RecoveryStatusDto::PartiallyRecovered,
                dw_core::RecoveryStatus::Overwritten => dw_ipc::RecoveryStatusDto::Overwritten,
                dw_core::RecoveryStatus::Carved => dw_ipc::RecoveryStatusDto::Carved,
                dw_core::RecoveryStatus::PendingValidation => dw_ipc::RecoveryStatusDto::PendingValidation,
            },
            extension: e.extension,
            mime: e.mime,
            confidence: e.confidence,
            was_deleted: e.was_deleted,
        })
        .collect();
    Ok(ListResultsResponse {
        total,
        spilled: record.handle.results().spilled_count() as u64,
        items,
    })
}

#[tauri::command]
pub async fn recover_files(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: RecoverFilesRequest,
) -> Result<RecoverFilesResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %id, count = req.entry_ids.len(), dest = %req.destination_dir, "recover_files");
    let g = state.read();
    let _record = g.scans.get(&id).ok_or_else(|| "scan not found".to_string())?;
    // TODO: implement actual recovery — read source clusters, write to dest.
    Ok(RecoverFilesResponse {
        recovered: 0,
        failed: 0,
        errors: vec!["recovery not yet implemented".into()],
    })
}
