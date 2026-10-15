//! Tauri command handlers — thin wrappers around the dw-core API.
//!
//! Each command:
//!  1. Validates input.
//!  2. Acquires the app state lock.
//!  3. Calls into dw-core / dw-disk.
//!  4. Converts the result to the IPC DTO.
//!  5. Returns to the webview.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use tauri::State;
use tracing::{info, warn};

use dw_disk::{DiskSource, FileBackedSource};

use dw_ipc::commands::*;

use crate::state::{AppState, ScanRecord};

#[tauri::command]
pub async fn list_disks(
    _state: State<'_, Arc<RwLock<AppState>>>,
    _req: ListDisksRequest,
) -> Result<ListDisksResponse, String> {
    info!("list_disks called");
    let disks = enumerate_disks();
    Ok(ListDisksResponse { disks })
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

    // Open the source.
    let source_path = parse_source_uri(&req.source_uri)?;
    let source = Arc::new(
        FileBackedSource::open(&source_path).map_err(|e| format!("open source: {e}"))?,
    );

    let handle = dw_core::ScanHandle::new(cfg);
    let scan_id = handle.id();
    let record = ScanRecord::new(handle.clone(), req.source_uri.clone());

    // Register all FS plugins.
    let plugins: Vec<Arc<dyn dw_core::FsPlugin>> = vec![
        Arc::new(dw_ntfs::NtfsPlugin),
        Arc::new(dw_fat::FatPlugin),
        Arc::new(dw_exfat::ExfatPlugin),
        Arc::new(dw_ext::ExtPlugin),
        Arc::new(dw_hfs::HfsPlugin),
        Arc::new(dw_btrfs::BtrfsPlugin),
        Arc::new(dw_refs::RefsPlugin),
        Arc::new(dw_zfs::ZfsPlugin),
    ];

    let app = {
        let g = state.read();
        g.app.clone()
    };

    let progress = record.progress_sink(&app);
    let scan = dw_core::Scan::new(handle.clone(), source, plugins, progress);

    {
        let mut g = state.write();
        g.scans.insert(scan_id, record);
    }

    let state_clone = Arc::clone(state.inner());
    tokio::spawn(async move {
        info!(scan_id = %scan_id, "Scan task started");
        let result = scan.run().await;
        match &result {
            Ok(()) => info!(scan_id = %scan_id, "Scan task completed"),
            Err(e) => {
                warn!(scan_id = %scan_id, error = %e, "Scan task failed");
                let g = state_clone.read();
                if let Some(record) = g.scans.get(&scan_id) {
                    if record.handle.state() != dw_core::ScanState::Cancelled {
                        record.handle.set_state(dw_core::ScanState::Failed);
                    }
                }
            }
        }
    });

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
    let total = record.handle.results().len() as u64;
    Ok(GetScanProgressResponse {
        progress: dw_ipc::ScanProgress {
            scan_id: id.to_string(),
            phase: state_dto,
            current: total,
            total: 0,
            message: format!("{total} entries found"),
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
                dw_core::RecoveryStatus::PartiallyRecovered => {
                    dw_ipc::RecoveryStatusDto::PartiallyRecovered
                }
                dw_core::RecoveryStatus::Overwritten => dw_ipc::RecoveryStatusDto::Overwritten,
                dw_core::RecoveryStatus::Carved => dw_ipc::RecoveryStatusDto::Carved,
                dw_core::RecoveryStatus::PendingValidation => {
                    dw_ipc::RecoveryStatusDto::PendingValidation
                }
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
    let scan_id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %scan_id, count = req.entry_ids.len(), dest = %req.destination_dir, "recover_files");

    // Get the scan record (source URI + results).
    let (source_uri, results) = {
        let g = state.read();
        let record = g.scans.get(&scan_id).ok_or_else(|| "scan not found".to_string())?;
        (record.source_uri.clone(), record.handle.results())
    };

    // Re-open the source disk.
    let source_path = parse_source_uri(&source_uri)?;
    let source = FileBackedSource::open(&source_path).map_err(|e| format!("reopen source: {e}"))?;
    let read_at = source.as_read_at();

    // Create the destination directory if it doesn't exist.
    let dest_dir = PathBuf::from(&req.destination_dir);
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let mut recovered: u32 = 0;
    let mut failed: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    for entry_id_str in &req.entry_ids {
        let entry_uuid = match uuid::Uuid::parse_str(entry_id_str) {
            Ok(u) => u,
            Err(e) => {
                failed += 1;
                errors.push(format!("invalid entry ID '{entry_id_str}': {e}"));
                continue;
            }
        };

        let entry = match results.get(dw_core::RecoveredEntryId(entry_uuid)) {
            Some(e) => e,
            None => {
                failed += 1;
                errors.push(format!("entry not found: {entry_id_str}"));
                continue;
            }
        };

        // Build the destination file path.
        // Sanitize the name: no path separators, no parent dir references.
        let safe_name = sanitize_filename(&entry.name);
        let dest_path = dest_dir.join(&safe_name);

        // Read the file data from the source.
        match dw_core::recovery::recover_entry_to_path(&entry, read_at.as_ref(), &dest_path) {
            Ok(bytes_written) => {
                info!(
                    entry = %entry_id_str,
                    name = %entry.name,
                    bytes_written,
                    "Recovered file"
                );
                recovered += 1;
            }
            Err(e) => {
                failed += 1;
                errors.push(format!("recover '{}': {e}", entry.name));
            }
        }
    }

    info!(recovered, failed, "Recovery complete");

    Ok(RecoverFilesResponse {
        recovered,
        failed,
        errors,
    })
}

/// Sanitize a filename for safe writing to the destination directory.
///
/// Removes path separators and parent-dir references to prevent path
/// traversal attacks.
fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();

    // Remove leading dots (prevent hidden files / parent dir on Unix).
    let sanitized = sanitized.trim_start_matches('.');

    if sanitized.is_empty() {
        "recovered_file".to_string()
    } else {
        sanitized.to_string()
    }
}

/// Parse a source URI into a filesystem path.
fn parse_source_uri(uri: &str) -> Result<PathBuf, String> {
    if let Some(path) = uri.strip_prefix("file://") {
        Ok(Path::new(path).to_path_buf())
    } else if uri.starts_with('/') || uri.starts_with('\\') || uri.contains(':') {
        Ok(Path::new(uri).to_path_buf())
    } else {
        Err(format!("unsupported source URI: {uri}"))
    }
}

/// Enumerate available disks.
///
/// On Windows, this uses `GetLogicalDriveStringsW` to enumerate drive letters.
/// On other platforms, returns an empty list (the UI supports file-backed images).
fn enumerate_disks() -> Vec<dw_ipc::types::DiskInfo> {
    #[cfg(windows)]
    {
        enumerate_disks_windows()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn enumerate_disks_windows() -> Vec<dw_ipc::types::DiskInfo> {
    use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsW;

    let mut buf = [0u16; 260];
    let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if len == 0 {
        return Vec::new();
    }

    let mut disks = Vec::new();
    let strings = String::from_utf16_lossy(&buf[..len as usize]);
    for drive_str in strings.split('\0') {
        if drive_str.is_empty() || drive_str.len() < 2 {
            continue;
        }
        let letter = &drive_str[..2]; // e.g. "C:"
        let display_name = format!("Local Disk ({letter})");

        // Try to get the size by opening the volume.
        let volume_path = format!("\\\\.\\{letter}");
        let size = match dw_disk::FileBackedSource::open(std::path::Path::new(&volume_path)) {
            Ok(src) => src.size(),
            Err(_) => 0,
        };

        disks.push(dw_ipc::types::DiskInfo {
            id: volume_path,
            display_name,
            size_bytes: size,
            is_removable: false, // TODO: check with GetDriveTypeW
            is_system: letter.eq_ignore_ascii_case("C:"),
            bus_type: dw_ipc::types::BusType::Sata,
            partitions: Vec::new(),
        });
    }

    disks
}
