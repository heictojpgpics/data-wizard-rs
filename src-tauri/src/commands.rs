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

use dw_disk::{DiskSource, FileBackedSource, PartitionView};

use dw_ipc::commands::*;

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
        Arc::new(dw_refs::RefsPlugin),
        Arc::new(dw_zfs::ZfsPlugin),
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
    let file_source = Arc::new(
        FileBackedSource::open(&source_path).map_err(|e| format!("open source: {e}"))?,
    );

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
                    ))
                }
            };
            let entry = entries
                .iter()
                .find(|e| e.index == index)
                .ok_or_else(|| {
                    format!(
                        "partition {index} not found (source has {} partitions)",
                        entries.len()
                    )
                })?;
            let view = PartitionView::new(file_source.as_read_at(), entry.offset_bytes, entry.size_bytes)
                .map_err(|e| format!("partition view: {e}"))?;
            Arc::new(view)
        }
    };

    let handle = dw_core::ScanHandle::new(cfg);
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
    let mode = cfg.mode;
    tokio::spawn(async move {
        info!(scan_id = %scan_id, "Scan task started");
        let result = scan.run().await;
        match &result {
            Ok(outcome) => {
                // No plugin recognized the source: run the raw-carving
                // fallback here in the app layer (dw-core stays free of
                // dw-carver; layering: carver depends on core).
                let mut carved_fallback: Option<u64> = None;
                if outcome.plugin_id.is_none() && matches!(mode, dw_core::plugin::ScanMode::Deep) {
                    if let Some(record) = state_clone.read().scans.get(&scan_id) {
                        let source_path = match parse_source_uri(&record.source_uri) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        if let Ok(src) = FileBackedSource::open(&source_path) {
                            let size = src.size();
                            let db = dw_signatures::SignatureDb::from_embedded();
                            let read_at = src.as_read_at();
                            let results = record.handle.results();
                            let progress = record.progress_sink(
                                &(state_clone.read().app.clone()),
                            );
                            warn!(scan_id = %scan_id, size, "Raw-carving fallback (unknown filesystem)");
                            carved_fallback = dw_carver::header_footer::carve_region_async(
                                read_at,
                                0,
                                size,
                                std::sync::Arc::new(db),
                                results,
                                progress,
                                scan_id,
                                1 << 20,
                            )
                            .await
                            .ok();
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

#[tauri::command]
pub async fn save_scan(
    state: State<'_, Arc<RwLock<AppState>>>,
    req: SaveScanRequest,
) -> Result<SaveScanResponse, String> {
    let id = uuid::Uuid::parse_str(&req.scan_id).map_err(|e| e.to_string())?;
    info!(scan_id = %id, "save_scan");

    let g = state.read();
    let record = g.scans.get(&id).ok_or_else(|| "scan not found".to_string())?;
    let source_uri = record.source_uri.clone();
    drop(g);

    // Re-open the source for fingerprinting (read-only).
    let path = parse_source_uri(&source_uri)?;
    let source = FileBackedSource::open(&path).map_err(|e| format!("open source: {e}"))?;

    let g = state.read();
    let record = g.scans.get(&id).ok_or_else(|| "scan not found".to_string())?;
    let store = dw_core::CheckpointStore::new(checkpoint_dir(&state));
    let entries = record.handle.results().len() as u64;
    store
        .save(&record.handle, &source_uri, source.as_read_at().as_ref())
        .map_err(|e| e.to_string())?;
    Ok(SaveScanResponse { saved: true, entries })
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
    let source = Arc::new(
        FileBackedSource::open(&path).map_err(|e| format!("open source: {e}"))?,
    ) as Arc<dyn DiskSource>;
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

    let handle = dw_core::checkpoint::restore_handle(dw_core::ScanCheckpoint {
        config: config.clone(),
        ..cp
    });
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
            if let Some(record) = g.scans.get(&scan_id) {
                if record.handle.state() != dw_core::ScanState::Cancelled {
                    record.handle.set_state(dw_core::ScanState::Failed);
                }
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
    let source = FileBackedSource::open(&path).map_err(|e| format!("open source: {e}"))?;
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
        let fs_kind = match PartitionView::new(source.as_read_at(), e.offset_bytes, e.size_bytes)
        {
            Ok(view) => probe_partition_fs(&view).await,
            Err(_) => None,
        };
        partitions.push(dw_ipc::types::PartitionInfo {
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
        let size = source.size();
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

        let partitions: Vec<dw_ipc::types::PartitionInfo> = entries
            .iter()
            .map(|e| dw_ipc::types::PartitionInfo {
                index: e.index,
                offset_bytes: e.offset_bytes,
                size_bytes: e.size_bytes,
                fs_kind: None, // probed on demand via list_partitions
                label: e.label.clone(),
                drive_letter: None,
                is_system: e.type_name == "EFI System Partition",
            })
            .collect();

        disks.push(dw_ipc::types::DiskInfo {
            id: path,
            display_name: format!("Physical Drive {n}"),
            size_bytes: size,
            is_removable: false, // TODO: IOCTL_STORAGE_GET_HOT_PLUG_INFO
            is_system: partitions.iter().any(|p| p.is_system),
            bus_type: dw_ipc::types::BusType::Sata, // TODO: IOCTL_STORAGE_QUERY_PROPERTY
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
        let size = match WindowsRawSource::open(std::path::Path::new(&volume_path)) {
            Ok(src) => src.size(),
            Err(_) => 0,
        };
        disks.push(dw_ipc::types::DiskInfo {
            id: volume_path,
            display_name,
            size_bytes: size,
            is_removable: false, // TODO: GetDriveTypeW
            is_system: letter.eq_ignore_ascii_case("C:"),
            bus_type: dw_ipc::types::BusType::Sata,
            partitions: Vec::new(),
        });
    }

    disks
}
