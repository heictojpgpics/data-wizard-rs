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
use tauri::{Emitter, State};
use tracing::{info, warn};

use dw_disk::{DiskSource, FileBackedSource, PartitionView};

use dw_ipc::commands::*;
use dw_ipc::types as dto;

use crate::state::{AppState, ScanRecord};

/// Resolve the on-disk checkpoint directory (app data dir / checkpoints).
fn checkpoint_dir(state: &State<'_, Arc<RwLock<AppState>>>) -> PathBuf {
    use tauri::Manager;
    let app = state.read().app.clone();
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("checkpoints");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Scratch dir for results-store spill files.
fn spills_dir(state: &State<'_, Arc<RwLock<AppState>>>) -> PathBuf {
    use tauri::Manager;
    let app = state.read().app.clone();
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("spills");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// All registered filesystem plugins (shared by start_scan and
/// list_partitions probing).
fn all_plugins() -> Vec<Arc<dyn dw_core::FsPlugin>> {
    vec![
        Arc::new(dw_ntfs::NtfsPlugin),
        Arc::new(dw_fat::FatPlugin),
        Arc::new(dw_exfat::ExfatPlugin),
        Arc::new(dw_ext::ExtPlugin),
        Arc::new(dw_hfs::HfsPlugin),
        Arc::new(dw_btrfs::BtrfsPlugin),
        Arc::new(dw_zfs::ZfsPlugin),
        Arc::new(dw_refs::RefsPlugin),
    ]
}

/// Probe the filesystem of one partition; returns the fs_kind string of
/// the first plugin that recognizes it.
async fn probe_partition_fs(view: &PartitionView) -> Option<String> {
    for plugin in all_plugins() {
        if let Ok(dw_core::ProbeOutcome::Yes(probe)) = plugin.probe(view).await {
            return Some(probe.fs_kind);
        }
    }
    None
}

/// Open a scan/recovery source: raw Windows devices go through
/// `WindowsRawSource`, everything else through `FileBackedSource`.
fn open_source(path: &Path) -> Result<Arc<dyn DiskSource>, String> {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.starts_with("\\\\.\\") {
            let src = dw_disk::windows_raw::WindowsRawSource::open(path)
                .map_err(|e| format!("open device: {e}"))?;
            return Ok(Arc::new(src));
        }
    }
    let src = FileBackedSource::open(path).map_err(|e| format!("open source: {e}"))?;
    Ok(Arc::new(src))
}

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
    let file_source = open_source(&source_path)?;

    // Optionally narrow to one partition inside the source.
    let source: Arc<dyn DiskSource> = match req.partition {
        None => file_source,
        Some(index) => {
            let read_at = file_source.as_read_at();
            let table = dw_disk::discover_partitions(read_at.as_ref())
                .map_err(|e| format!("discover partitions: {e}"))?;
            let entries = match &table {
                dw_disk::DiscoveredTable::Gpt { entries, .. } => entries,
                dw_disk::DiscoveredTable::Mbr { entries } => entries,
                dw_disk::DiscoveredTable::None => {
                    return Err(format!(
                        "source has no partition table; cannot select partition {index}"
                    ));
                }
            };
            let entry = entries.iter().find(|e| e.index == index).ok_or_else(|| {
                format!(
                    "partition {index} not found (source has {} partitions)",
                    entries.len()
                )
            })?;
            let view = PartitionView::new(
                file_source.as_read_at(),
                entry.offset_bytes,
                entry.size_bytes,
            )
            .map_err(|e| format!("partition view: {e}"))?;
            Arc::new(view)
        }
    };

    // Spill overflow results to per-scan scratch files instead of
    // growing memory without bound on deep scans.
    let spill_dir = spills_dir(&state);
    let handle = dw_core::ScanHandle::with_spill_dir(cfg, Some(spill_dir));
    let scan_id = handle.id();
    let record = ScanRecord::new(handle.clone(), req.source_uri.clone());

    let plugins = all_plugins();

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
            Ok(outcome) => {
                // No plugin recognized the source: run the raw-carving
                // fallback here in the app layer (dw-core stays free of
                // dw-carver; layering: carver depends on core).
                //
                // Everything the fallback needs is collected under a
                // SHORT lock; the guard is dropped before any await
                // (holding a lock guard across an await is both a
                // Send violation and a deadlock risk).
                let mut carved_fallback: Option<u64> = None;
                if outcome.plugin_id.is_none() && matches!(mode, dw_core::plugin::ScanMode::Deep) {
                    let fallback = {
                        let g = state_clone.read();
                        g.scans.get(&scan_id).map(|record| {
                            (
                                record.source_uri.clone(),
                                record.handle.results(),
                                record.handle.cancel_flag(),
                                g.app.clone(),
                            )
                        })
                    };
                    if let Some((source_uri, results, cancel, app)) = fallback {
                        let source_path = match parse_source_uri(&source_uri) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        if let Ok(src) = open_source(&source_path) {
                            let size = dw_disk::ReadAt::size(src.as_read_at().as_ref());
                            let db = dw_signatures::SignatureDb::from_embedded();
                            let read_at = src.as_read_at();
                            warn!(scan_id = %scan_id, size, "Raw-carving fallback (unknown filesystem)");
                            carved_fallback = dw_carver::header_footer::carve_region_async(
                                read_at,
                                0,
                                size,
                                std::sync::Arc::new(db),
                                results,
                                Arc::new(crate::progress_sink::TauriProgressSink::new(
                                    app, scan_id,
                                )),
                                scan_id,
                                1 << 20,
                            )
                            .await
                            .ok();
                            // Cancellation from the raw-carve path: honor it.
                            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                                let g = state_clone.read();
                                if let Some(record) = g.scans.get(&scan_id) {
                                    record.handle.set_state(dw_core::ScanState::Cancelled);
                                }
                            }
                        }
                    }
                }
                info!(
                    scan_id = %scan_id,
                    entries = outcome.entries,
                    raw_carved = carved_fallback.unwrap_or(0),
                    "Scan task completed"
                );
            }
            Err(e) => {
                warn!(scan_id = %scan_id, error = %e, "Scan task failed");
                let g = state_clone.read();
                if let Some(record) = g.scans.get(&scan_id)
                    && record.handle.state() != dw_core::ScanState::Cancelled
                {
                    record.handle.set_state(dw_core::ScanState::Failed);
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
        // Both channels: the lifecycle state (phase transitions check it)
        // and the cooperative flag (carver worker pools poll it).
        record.handle.request_cancel();
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
    let record = g
        .scans
        .get(&id)
        .ok_or_else(|| "scan not found".to_string())?;
    let state_dto = match record.handle.state() {
        dw_core::ScanState::Created | dw_core::ScanState::Probing => dto::ScanPhase::Probing,
        dw_core::ScanState::QuickScan => dto::ScanPhase::QuickScan,
        dw_core::ScanState::DeepScan => dto::ScanPhase::DeepScan,
        dw_core::ScanState::Finalizing => dto::ScanPhase::Finalizing,
        dw_core::ScanState::Done => dto::ScanPhase::Done,
        dw_core::ScanState::Cancelled => dto::ScanPhase::Cancelled,
        dw_core::ScanState::Failed => dto::ScanPhase::Failed,
    };
    let total = record.handle.results().len() as u64;
    Ok(GetScanProgressResponse {
        progress: dto::ScanProgress {
            scan_id: id.to_string(),
            phase: state_dto,
            current: total,
            total: 0,
            message: format!("{total} entries found"),
        },
    })
}

/// Map a core RecoveryStatus to the wire DTO. Exhaustive by design: a new
/// core variant must surface here consciously (the previous non-exhaustive
/// match failed to compile on Windows only, after the lib workspace went
/// green on Linux — this helper keeps the mapping in ONE place).
fn status_to_dto(status: dw_core::RecoveryStatus) -> dto::RecoveryStatusDto {
    match status {
        dw_core::RecoveryStatus::Intact => dto::RecoveryStatusDto::Intact,
        dw_core::RecoveryStatus::PartiallyRecovered => dto::RecoveryStatusDto::PartiallyRecovered,
        dw_core::RecoveryStatus::Overwritten => dto::RecoveryStatusDto::Overwritten,
        dw_core::RecoveryStatus::Carved => dto::RecoveryStatusDto::Carved,
        dw_core::RecoveryStatus::CarvedValidated => dto::RecoveryStatusDto::CarvedValidated,
        dw_core::RecoveryStatus::PendingValidation => dto::RecoveryStatusDto::PendingValidation,
    }
}

#[tauri::command]
pub async fn list_results(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: ListResultsRequest,
) -> Result<ListResultsResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    let g = state.read();
    let record = g
        .scans
        .get(&id)
        .ok_or_else(|| "scan not found".to_string())?;
    let store = record.handle.results();
    let total = store.len() as u64;
    let spilled = store.spilled_count() as u64;
    // Paged read: clones only the requested window, not the whole store.
    let items: Vec<dto::RecoveredFile> = store
        .page(req.offset as usize, req.limit.min(1000) as usize)
        .into_iter()
        .map(|e| dto::RecoveredFile {
            id: e.id.0.to_string(),
            name: e.name,
            parent_path: e.parent_path,
            size_bytes: e.size,
            mtime: e.mtime.map(|t| t.to_string()),
            status: status_to_dto(e.status),
            extension: e.extension,
            mime: e.mime,
            confidence: e.confidence,
            was_deleted: e.was_deleted,
        })
        .collect();
    Ok(ListResultsResponse {
        total,
        spilled,
        items,
    })
}

#[tauri::command]
pub async fn scan_stats(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: ScanStatsRequest,
) -> Result<ScanStatsResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    let g = state.read();
    let record = g
        .scans
        .get(&id)
        .ok_or_else(|| "scan not found".to_string())?;
    let stats = dw_core::classify::compute_stats(record.handle.results().as_ref());

    Ok(ScanStatsResponse {
        stats: dto::ScanStatsDto {
            scan_id: id.to_string(),
            total_files: stats.total_files,
            total_bytes: stats.total_bytes,
            by_status: stats
                .by_status
                .into_iter()
                .map(|(status, files, bytes)| dto::StatusCountDto {
                    status,
                    files,
                    bytes,
                })
                .collect(),
            by_category: stats
                .by_category
                .into_iter()
                .map(|(cat, files, bytes)| dto::CategoryCountDto {
                    category: serde_json::to_string(&cat)
                        .unwrap_or_else(|_| "\"other\"".into())
                        .trim_matches('"')
                        .to_string(),
                    label: cat.label().to_string(),
                    files,
                    bytes,
                })
                .collect(),
        },
    })
}

#[tauri::command]
pub async fn recover_files(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: RecoverFilesRequest,
) -> Result<RecoverFilesResponse, String> {
    let scan_id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %scan_id, count = req.entry_ids.len(), dest = %req.destination_dir, "recover_files");

    let policy = match req.collision_policy.as_str() {
        "error" => dw_core::CollisionPolicy::Error,
        "keep_both" | "" => dw_core::CollisionPolicy::KeepBoth,
        "overwrite" => dw_core::CollisionPolicy::Overwrite,
        other => {
            return Err(format!(
                "unknown collision policy '{other}' (error|keep_both|overwrite)"
            ));
        }
    };

    // Get the scan record (source URI + results).
    let (source_uri, results, app) = {
        let g = state.read();
        let record = g
            .scans
            .get(&scan_id)
            .ok_or_else(|| "scan not found".to_string())?;
        (
            record.source_uri.clone(),
            record.handle.results(),
            g.app.clone(),
        )
    };

    // Resolve every entry BEFORE any I/O: failures to resolve count as
    // failed outcomes, and the batch size must be known for the
    // free-space preflight.
    let mut entries: Vec<(String, dw_core::RecoveredEntry)> =
        Vec::with_capacity(req.entry_ids.len());
    for id_str in &req.entry_ids {
        match uuid::Uuid::parse_str(id_str) {
            Ok(u) => match results.get(dw_core::RecoveredEntryId(u)) {
                Some(e) => entries.push((id_str.clone(), e)),
                None => {
                    warn!(entry = %id_str, "recover: entry not found");
                }
            },
            Err(e) => warn!(entry = %id_str, error = %e, "recover: invalid entry id"),
        }
    }
    let unresolved = req.entry_ids.len().saturating_sub(entries.len());

    let total_bytes: u64 = entries.iter().map(|(_, e)| e.size).sum();
    let dest_dir = PathBuf::from(&req.destination_dir);
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("create destination: {e}"))?;

    let source_path = parse_source_uri(&source_uri)?;
    dw_core::recovery::preflight_destination(&dest_dir, total_bytes, Some(&source_path))
        .map_err(|e| e.to_string())?;

    // Re-open the source disk.
    let source = open_source(&source_path)?;
    let read_at = source.as_read_at();

    let mut outcomes: Vec<dto::FileRecoveryOutcome> = Vec::with_capacity(entries.len());
    let mut warnings: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut recovered: u32 = 0;
    let mut failed: u32 = unresolved as u32;
    let mut bytes_written: u64 = 0;
    let total = req.entry_ids.len() as u32;

    for (idx, (entry_id_str, entry)) in entries.iter().enumerate() {
        // Sanitize the name: no path separators, no parent references.
        let safe_name = sanitize_filename(&entry.name);
        let dest_path = dest_dir.join(&safe_name);

        let outcome = match dw_core::recovery::recover_entry(
            entry,
            read_at.as_ref(),
            &dest_path,
            policy,
        ) {
            Ok(out) => {
                recovered += 1;
                bytes_written += out.bytes_written;
                if out.partial {
                    warnings.push(format!(
                        "'{}' recovered partially: {} of {} bytes (extents cover less than the declared size)",
                        entry.name, out.bytes_written, entry.size
                    ));
                }
                dto::FileRecoveryOutcome {
                    entry_id: entry_id_str.clone(),
                    name: entry.name.clone(),
                    ok: true,
                    path: Some(out.path.to_string_lossy().into_owned()),
                    bytes_written: out.bytes_written,
                    partial: out.partial,
                    error: None,
                }
            }
            Err(e) => {
                failed += 1;
                let msg = format!("recover '{}': {e}", entry.name);
                errors.push(msg.clone());
                dto::FileRecoveryOutcome {
                    entry_id: entry_id_str.clone(),
                    name: entry.name.clone(),
                    ok: false,
                    path: None,
                    bytes_written: 0,
                    partial: false,
                    error: Some(msg),
                }
            }
        };

        // Live progress to the UI (best-effort: an emit failure must
        // not abort the batch).
        let _ = app.emit(
            "recovery:progress",
            dto::RecoveryProgress {
                scan_id: req.scan_id.clone(),
                done: (idx + 1) as u32,
                total,
                current_file: entry.name.clone(),
                bytes_written,
            },
        );
        outcomes.push(outcome);
    }

    if unresolved > 0 {
        errors.push(format!(
            "{unresolved} selected entries no longer exist in the results (removed by dedup?)"
        ));
    }

    info!(recovered, failed, bytes_written, "Recovery complete");

    Ok(RecoverFilesResponse {
        recovered,
        failed,
        bytes_written,
        results: outcomes,
        warnings,
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

#[tauri::command]
pub async fn save_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: SaveScanRequest,
) -> Result<SaveScanResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %id, "save_scan");

    let g = state.read();
    let record = g
        .scans
        .get(&id)
        .ok_or_else(|| "scan not found".to_string())?;
    let source_uri = record.source_uri.clone();
    drop(g);

    // Re-open the source for fingerprinting (read-only).
    let path = parse_source_uri(&source_uri)?;
    let source = open_source(&path)?;

    let g = state.read();
    let record = g
        .scans
        .get(&id)
        .ok_or_else(|| "scan not found".to_string())?;
    let store = dw_core::CheckpointStore::new(checkpoint_dir(&state));
    let entries = record.handle.results().len() as u64;
    store
        .save(&record.handle, &source_uri, source.as_read_at().as_ref())
        .map_err(|e| e.to_string())?;
    Ok(SaveScanResponse {
        saved: true,
        entries,
    })
}

#[tauri::command]
pub async fn list_saved_scans(
    state: State<'_, Arc<RwLock<AppState>>>,
    _req: ListSavedScansRequest,
) -> Result<ListSavedScansResponse, String> {
    let store = dw_core::CheckpointStore::new(checkpoint_dir(&state));
    let scans = store
        .list()
        .into_iter()
        .map(|s| SavedScanSummary {
            scan_id: s.scan_id.to_string(),
            source_uri: s.source_uri,
            created_at: s.created_at.to_string(),
            mode: s.mode,
            state: format!("{:?}", s.state),
            entries: s.entries,
        })
        .collect();
    Ok(ListSavedScansResponse { scans })
}

#[tauri::command]
pub async fn resume_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: ResumeScanRequest,
) -> Result<ResumeScanResponse, String> {
    let id = uuid::Uuid::parse_str(&req.saved_scan_id).map_err(|e| e.to_string())?;
    info!(saved_scan_id = %id, deep = req.deep, "resume_scan");

    // Load + verify against the current source.
    let store = dw_core::CheckpointStore::new(checkpoint_dir(&state));
    let cp = store.load(id).map_err(|e| e.to_string())?;
    let path = parse_source_uri(&cp.source_uri)?;
    let source = open_source(&path)?;
    dw_core::verify_source_matches(&cp, source.as_read_at().as_ref())
        .map_err(|e| format!("refusing to resume: {e}"))?;

    // Continue with the requested mode.
    let mut config = cp.config.clone();
    config.mode = if req.deep {
        dw_core::plugin::ScanMode::Deep
    } else {
        dw_core::plugin::ScanMode::Quick
    };
    config.include_carving = req.deep;

    let handle_arc = dw_core::checkpoint::restore_handle(dw_core::ScanCheckpoint {
        config: config.clone(),
        ..cp
    });
    let handle = (*handle_arc).clone();
    let scan_id = handle.id();
    // Resume always re-runs the plugin pass over the restored entries;
    // identical findings replace their older twins at the end.
    handle.set_state(dw_core::ScanState::Created);
    let record = ScanRecord::new(handle.clone(), source_uri_of(&config));

    let plugins = all_plugins();
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
    let mode = config.mode;
    tokio::spawn(async move {
        info!(scan_id = %scan_id, "Resumed scan task started");
        let result = scan.run().await;
        if result.is_ok() {
            // Dedup: the re-run duplicates the restored entries.
            let g = state_clone.read();
            if let Some(record) = g.scans.get(&scan_id) {
                let before = record.handle.results().len();
                record.handle.results().dedup_by_identity();
                let after = record.handle.results().len();
                info!(scan_id = %scan_id, before, after, "Resumed scan deduped");
            }
        } else {
            warn!(scan_id = %scan_id, "Resumed scan failed");
            let g = state_clone.read();
            if let Some(record) = g.scans.get(&scan_id)
                && record.handle.state() != dw_core::ScanState::Cancelled
            {
                record.handle.set_state(dw_core::ScanState::Failed);
            }
        }
        let _ = mode;
    });

    Ok(ResumeScanResponse {
        scan_id: scan_id.to_string(),
    })
}

fn source_uri_of(config: &dw_core::ScanConfig) -> String {
    config.source_uri.clone()
}

#[tauri::command]
pub async fn delete_saved_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: DeleteSavedScanRequest,
) -> Result<DeleteSavedScanResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    let store = dw_core::CheckpointStore::new(checkpoint_dir(&state));
    store.delete(id).map_err(|e| e.to_string())?;
    Ok(DeleteSavedScanResponse { deleted: true })
}

#[tauri::command]
pub async fn list_partitions(
    _state: State<'_, Arc<RwLock<AppState>>>,
    req: ListPartitionsRequest,
) -> Result<ListPartitionsResponse, String> {
    info!(source = %req.source_uri, "list_partitions called");
    let path = parse_source_uri(&req.source_uri)?;
    let source = open_source(&path)?;
    let read_at = source.as_read_at();

    let table = dw_disk::discover_partitions(read_at.as_ref())
        .map_err(|e| format!("discover partitions: {e}"))?;
    let kind = match &table {
        dw_disk::DiscoveredTable::Gpt { .. } => "gpt",
        dw_disk::DiscoveredTable::Mbr { .. } => "mbr",
        dw_disk::DiscoveredTable::None => "none",
    };

    let entries = match &table {
        dw_disk::DiscoveredTable::Gpt { entries, .. } => entries.clone(),
        dw_disk::DiscoveredTable::Mbr { entries } => entries.clone(),
        dw_disk::DiscoveredTable::None => Vec::new(),
    };

    let mut partitions = Vec::with_capacity(entries.len());
    for e in &entries {
        // Probe the filesystem by opening a view of this partition.
        let fs_kind = match PartitionView::new(source.as_read_at(), e.offset_bytes, e.size_bytes) {
            Ok(view) => probe_partition_fs(&view).await,
            Err(_) => None,
        };
        partitions.push(dto::PartitionInfo {
            index: e.index,
            offset_bytes: e.offset_bytes,
            size_bytes: e.size_bytes,
            fs_kind,
            label: e.label.clone(),
            drive_letter: None,
            is_system: e.type_name == "EFI System Partition",
        });
    }

    info!(kind, count = partitions.len(), "list_partitions done");
    Ok(ListPartitionsResponse {
        table_kind: kind.to_string(),
        partitions,
    })
}

/// Enumerate available disks.
///
/// On Windows: physical drives plus volume letters.
/// On other platforms, returns an empty list (the UI supports file-backed images).
fn enumerate_disks() -> Vec<dto::DiskInfo> {
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
fn enumerate_disks_windows() -> Vec<dto::DiskInfo> {
    use dw_disk::windows_raw::WindowsRawSource;

    let mut disks = Vec::new();

    // Physical drives: \\.\PhysicalDrive0..N. Opening failures (index
    // gaps / permissions) are skipped silently.
    for n in 0..64u32 {
        let path = format!("\\\\.\\PhysicalDrive{n}");
        let Ok(source) = WindowsRawSource::open(std::path::Path::new(&path)) else {
            continue;
        };
        let read_at = source.as_read_at();
        let size = dw_disk::ReadAt::size(read_at.as_ref());
        if size == 0 {
            continue;
        }

        // Discover the partition layout (MBR + EBR chains / GPT with CRC
        // validation). On corrupt tables, report the disk with no
        // partitions rather than dropping it.
        let table = dw_disk::discover_partitions(read_at.as_ref()).ok();
        let entries = match &table {
            Some(dw_disk::DiscoveredTable::Gpt { entries, .. }) => entries.clone(),
            Some(dw_disk::DiscoveredTable::Mbr { entries }) => entries.clone(),
            _ => Vec::new(),
        };

        let partitions: Vec<dto::PartitionInfo> = entries
            .iter()
            .map(|e| dto::PartitionInfo {
                index: e.index,
                offset_bytes: e.offset_bytes,
                size_bytes: e.size_bytes,
                fs_kind: None, // probed on demand via list_partitions
                label: e.label.clone(),
                drive_letter: None,
                is_system: e.type_name == "EFI System Partition",
            })
            .collect();

        disks.push(dto::DiskInfo {
            id: path,
            display_name: format!("Physical Drive {n}"),
            size_bytes: size,
            is_removable: false, // TODO: IOCTL_STORAGE_QUERY_PROPERTY BusType
            is_system: partitions.iter().any(|p| p.is_system),
            bus_type: dto::BusType::Sata, // TODO: IOCTL_STORAGE_QUERY_PROPERTY
            partitions,
        });
    }

    // Volumes (drive letters) — retained so the UI can scan a volume
    // directly without picking a partition.
    use windows::Win32::Storage::FileSystem::GetLogicalDriveStringsW;
    let mut buf = [0u16; 260];
    let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if len == 0 {
        return disks;
    }
    let strings = String::from_utf16_lossy(&buf[..len as usize]);
    for drive_str in strings.split('\0') {
        if drive_str.is_empty() || drive_str.len() < 2 {
            continue;
        }
        let letter = &drive_str[..2]; // e.g. "C:"
        let display_name = format!("Local Disk ({letter})");
        let volume_path = format!("\\\\.\\{letter}");
        let (size, removable) = match WindowsRawSource::open(std::path::Path::new(&volume_path)) {
            Ok(src) => (
                dw_disk::ReadAt::size(src.as_read_at().as_ref()),
                drive_kind(&format!("{letter}\\")),
            ),
            Err(_) => (0, false),
        };
        disks.push(dto::DiskInfo {
            id: volume_path,
            display_name,
            size_bytes: size,
            is_removable: removable,
            is_system: is_system_drive(letter),
            bus_type: dto::BusType::Sata, // TODO: IOCTL_STORAGE_QUERY_PROPERTY
            partitions: Vec::new(),
        });
    }

    disks
}

/// DRIVE_REMOVABLE / DRIVE_CDROM etc. via GetDriveTypeW (volume root
/// form "C:\\").
#[cfg(windows)]
fn drive_kind(root: &str) -> bool {
    const DRIVE_REMOVABLE: u32 = 2;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let ty = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
    ty == DRIVE_REMOVABLE
}

/// The system drive is where the Windows directory lives, not a hardcoded
/// "C:" (a long-standing wart of naive enumerators).
#[cfg(windows)]
fn is_system_drive(letter: &str) -> bool {
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buf = [0u16; 260];
    let len = unsafe { GetWindowsDirectoryW(Some(&mut buf)) };
    if len == 0 || len as usize >= buf.len() {
        // Fall back to the SYSTEMDRIVE environment variable.
        return std::env::var("SYSTEMDRIVE")
            .map(|d| d.eq_ignore_ascii_case(letter))
            .unwrap_or(false);
    }
    let windir = String::from_utf16_lossy(&buf[..len as usize]);
    windir
        .get(..2)
        .is_some_and(|root| root.eq_ignore_ascii_case(letter))
}

/// Dev hooks for the CI screenshot tour (env-var driven; harmless in
/// production builds — nothing reads these variables unless set):
///   DATAWIZARD_TOUR=1     the TourDriver auto-cycles every UI state.
///   DATAWIZARD_TOUR_MODE  reserved program selector (ui default).
///   DATAWIZARD_SCAN       optional source URI to auto-scan first.
#[derive(Debug, Clone, serde::Serialize)]
#[allow(clippy::struct_field_names)]
pub struct DevHooks {
    pub tour: bool,
    pub tour_mode: Option<String>,
    pub scan: Option<String>,
}

#[tauri::command]
pub fn get_dev_hooks() -> DevHooks {
    DevHooks {
        tour: std::env::var("DATAWIZARD_TOUR")
            .map(|v| v == "1")
            .unwrap_or(false),
        tour_mode: std::env::var("DATAWIZARD_TOUR_MODE").ok(),
        scan: std::env::var("DATAWIZARD_SCAN").ok(),
    }
}
