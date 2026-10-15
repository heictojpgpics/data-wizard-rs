//! NTFS MFT walker — the core metadata scan.
//!
//! This module implements the "quick scan" phase for NTFS: walk every
//! MFT record, extract file names + data runs, resolve directory
//! hierarchy, and build [`RecoveredEntry`]s that get appended to the
//! [`ResultsStore`].
//!
//! Algorithm (clean-room, derived from public MS-NTFS spec + Linux
//! kernel `fs/ntfs3/` + libntfs-3g; see `docs/algorithms/ntfs-recovery.md`):
//!
//! 1. Read the VBR to get the MFT offset, record size, and cluster size.
//! 2. Read MFT record 0 (the $MFT file itself). Parse its $DATA attribute
//!    to get the full MFT's cluster chain.
//! 3. **Pass 1** — Walk every MFT record, collect into an in-memory `Vec`:
//!    - record_number
//!    - parent_record_number (from $FILE_NAME)
//!    - name (from $FILE_NAME)
//!    - file_size + cluster_chain (from $DATA)
//!    - timestamps, deleted flag, is_directory flag
//! 4. **Path resolution** — Build a `record → (parent, name)` map, then
//!    for each entry, walk up the parent chain to build the full path.
//!    Cycles are detected and broken (corrupt MFT can have cycles).
//! 5. **Pass 2** — Insert resolved entries into the `ResultsStore`.
//! 6. Emit progress events throughout.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, trace, warn};
use uuid::Uuid;

use dw_core::{
    CoreError, CoreResult, Extent, ProgressEvent, ProgressPhase, ProgressSink, RecoveredEntry,
    RecoveredEntryId, RecoveryStatus, ResultsStore, ScanMode,
};
use dw_disk::DiskSource;

use crate::NtfsError;
use crate::boot::Vbr;
use crate::filename::{FileName, namespace};
use crate::mft::{DataRun, FileRecord, decode_run_list};

/// Build canonical extents from decoded NTFS data runs.
///
/// Each non-sparse run contributes one extent `(lcn * bpc, length * bpc)`.
/// Consecutive runs merge (already handled by the caller-visible trim):
/// we keep the trim-total-to-`file_size` rule shared with the FAT/exFAT
/// walkers. Sparse runs (offset = None) contribute nothing and leave a
/// coverage gap, which the caller reports as PartiallyRecovered.
pub fn ntfs_extents_from_runs(
    runs: &[DataRun],
    bytes_per_cluster: u64,
    file_size: u64,
) -> Vec<Extent> {
    if bytes_per_cluster == 0 || file_size == 0 {
        return Vec::new();
    }
    let mut extents: Vec<Extent> = Vec::new();
    let mut remaining = file_size;
    for run in runs {
        if remaining == 0 {
            break;
        }
        let Some(lcn) = run.offset else { continue }; // sparse hole
        let want = (run.length * bytes_per_cluster).min(remaining);
        let start = lcn * bytes_per_cluster;
        if let Some(last) = extents.last_mut()
            && last.start_byte + last.length == start
        {
            last.length += want;
        } else {
            extents.push(Extent {
                start_byte: start,
                length: want,
            });
        }
        remaining -= want;
    }
    extents
}

/// Configuration for the MFT walker.
#[derive(Debug, Clone)]
pub struct WalkConfig {
    /// How often to emit progress events (every N records).
    pub progress_interval: u32,
    /// Maximum number of records to walk (0 = unlimited).
    pub max_records: u64,
    /// Whether to include deleted records in the results.
    pub include_deleted: bool,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            progress_interval: 1024,
            max_records: 0,
            include_deleted: true,
        }
    }
}

/// In-memory record collected during pass 1.
struct CollectedRecord {
    record_number: u64,
    parent_record_number: u64,
    name: String,
    file_size: u64,
    cluster_chain: Vec<u64>,
    /// Canonical physical extents from the $DATA run list (empty for
    /// resident data).
    extents: Vec<dw_core::Extent>,
    /// Inline resident $DATA content (small files).
    resident_data: Option<Vec<u8>>,
    /// True when the run list does not fully cover `file_size`
    /// (sparse runs / truncated chain).
    is_partial: bool,
    is_deleted: bool,
    #[allow(dead_code)]
    is_directory: bool,
    mtime: Option<time::OffsetDateTime>,
    ctime: Option<time::OffsetDateTime>,
    atime: Option<time::OffsetDateTime>,
}

/// Walk the MFT and append every recoverable entry to `results`.
///
/// This is the "quick scan" phase for NTFS. It does NOT do raw carving
/// (that's the "deep scan" phase, handled by `dw-carver`).
/// Outcome of an MFT walk.
#[derive(Debug, Default)]
pub struct WalkOutcome {
    pub entries: u64,
    /// $Bitmap (MFT record 6) $DATA extents + size, captured during the
    /// walk so the deep phase can carve ONLY unallocated clusters.
    pub bitmap: Option<(Vec<Extent>, u64)>,
}

pub async fn walk_mft(
    source: Arc<dyn DiskSource>,
    vbr: &Vbr,
    mode: ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    config: &WalkConfig,
) -> CoreResult<WalkOutcome> {
    let scan_id = Uuid::now_v7();
    let read_at = source.as_read_at();
    let mft_offset = vbr.mft_offset_bytes();
    let record_size = usize::try_from(vbr.mft_record_size_bytes).map_err(|_| {
        CoreError::Plugin(format!(
            "MFT record size too large: {}",
            vbr.mft_record_size_bytes
        ))
    })?;

    info!(
        mft_offset,
        record_size,
        bytes_per_cluster = vbr.bytes_per_cluster,
        mode = ?mode,
        "Starting NTFS MFT walk"
    );

    // Read MFT record 0 (the $MFT file itself) to get the MFT's own cluster chain.
    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: 0,
            total: 1,
            message: "Reading $MFT record 0".to_string(),
        })
        .await;

    let mut record_buf = vec![0u8; record_size];
    read_at
        .read_at(mft_offset, &mut record_buf)
        .map_err(CoreError::Disk)?;

    let mft_record = FileRecord::parse(&record_buf, record_size)
        .map_err(|e| CoreError::Plugin(format!("parse $MFT record 0: {e}")))?;

    let mft_data = mft_record
        .data_attribute()
        .ok_or_else(|| CoreError::Plugin("$MFT has no $DATA attribute".into()))?;

    let mft_runs = match &mft_data.data {
        crate::mft::AttributeData::NonResident { run_list, .. } => decode_run_list(run_list)
            .map_err(|e| CoreError::Plugin(format!("decode $MFT run list: {e}")))?,
        crate::mft::AttributeData::Resident { .. } => {
            return Err(CoreError::Plugin(
                "$MFT $DATA is resident (expected non-resident)".into(),
            ));
        }
    };

    let mft_total_bytes: u64 =
        mft_runs.iter().map(|r| r.length).sum::<u64>() * u64::from(vbr.bytes_per_cluster);
    let total_records = mft_total_bytes / u64::from(vbr.mft_record_size_bytes);
    info!(total_records, "MFT record count from $MFT $DATA");

    // Build a run-position index for mapping MFT byte offsets to disk offsets.
    let mut run_boundaries: Vec<(u64, u64, u64)> = Vec::new();
    let mut cursor = 0u64;
    for run in &mft_runs {
        let len_bytes = run.length * u64::from(vbr.bytes_per_cluster);
        if let Some(disk_off) = run.offset {
            run_boundaries.push((
                cursor,
                cursor + len_bytes,
                disk_off * u64::from(vbr.bytes_per_cluster),
            ));
        }
        cursor += len_bytes;
    }

    let total_records_to_walk = if config.max_records > 0 {
        config.max_records.min(total_records)
    } else {
        total_records
    };

    info!(
        total_records_to_walk,
        include_deleted = config.include_deleted,
        "Beginning MFT record iteration (pass 1: collect)"
    );

    // ---- Pass 1: Collect all records into memory ----
    let mut collected: Vec<CollectedRecord> = Vec::new();
    let mut records_processed: u64 = 0;
    let mut errors: u64 = 0;
    let mut bitmap_capture: Option<(Vec<Extent>, u64)> = None;

    for record_number in 0..total_records_to_walk {
        let mft_byte_offset = record_number * u64::from(vbr.mft_record_size_bytes);

        let disk_offset = match locate_in_runs(&run_boundaries, mft_byte_offset) {
            Some(off) => off + mft_offset,
            None => continue,
        };

        if let Err(e) = read_at.read_at(disk_offset, &mut record_buf) {
            trace!(record_number, error = %e, "Failed to read MFT record");
            errors += 1;
            continue;
        }

        let record = match FileRecord::parse(&record_buf, record_size) {
            Ok(r) => r,
            Err(NtfsError::Corrupt(msg)) => {
                if !msg.starts_with("MFT record bad magic") {
                    trace!(record_number, error = %msg, "MFT record parse error");
                }
                continue;
            }
            Err(e) => {
                trace!(record_number, error = %e, "MFT record parse error");
                errors += 1;
                continue;
            }
        };

        if record.base_record_reference != 0 {
            continue;
        }

        let is_deleted = !record.is_in_use();
        if is_deleted && !config.include_deleted {
            continue;
        }

        let file_name = match pick_best_file_name(&record) {
            Some(fn_) => fn_,
            None => continue,
        };

        let (file_size, cluster_chain, extents, resident_data, is_partial) = match record
            .data_attribute()
        {
            Some(attr) => match &attr.data {
                crate::mft::AttributeData::Resident { content } => {
                    // Resident $DATA: content is inline in the MFT record.
                    // Capture it so recovery does not need cluster I/O.
                    (
                        content.len() as u64,
                        Vec::new(),
                        Vec::new(),
                        Some(content.clone()),
                        false,
                    )
                }
                crate::mft::AttributeData::NonResident {
                    run_list,
                    real_size,
                    ..
                } => {
                    let runs = decode_run_list(run_list).unwrap_or_default();
                    let chain: Vec<u64> = runs.iter().filter_map(|r| r.offset).collect();
                    // Build canonical extents from the run list: each
                    // non-sparse run contributes (LCN * bpc, length * bpc),
                    // trimmed in total to `real_size`. Sparse runs (holes)
                    // leave gaps -> is_partial.
                    let extents =
                        ntfs_extents_from_runs(&runs, u64::from(vbr.bytes_per_cluster), *real_size);
                    let covered: u64 = extents.iter().map(|e| e.length).sum();
                    let is_partial = covered < *real_size;
                    (*real_size, chain, extents, None, is_partial)
                }
            },
            None => (0, Vec::new(), Vec::new(), None, false),
        };

        // Capture $Bitmap (record 6) for the deep phase.
        let is_bitmap_record = record_number == 6;
        if is_bitmap_record {
            bitmap_capture = Some((extents.clone(), file_size));
        }

        collected.push(CollectedRecord {
            record_number,
            parent_record_number: file_name.parent_record_number,
            name: file_name.name.clone(),
            file_size,
            cluster_chain,
            extents,
            resident_data,
            is_partial,
            is_deleted,
            is_directory: record.is_directory(),
            mtime: file_name.modification_time,
            ctime: file_name.creation_time,
            atime: file_name.read_time,
        });

        records_processed += 1;

        if records_processed.is_multiple_of(u64::from(config.progress_interval)) {
            progress
                .emit(ProgressEvent {
                    scan_id,
                    phase: ProgressPhase::QuickScan,
                    current: records_processed,
                    total: total_records_to_walk,
                    message: format!(
                        "Pass 1: walked {records_processed}/{total_records_to_walk} records, {} collected",
                        collected.len()
                    ),
                })
                .await;
        }
    }

    info!(
        records_processed,
        collected = collected.len(),
        errors,
        "Pass 1 complete, resolving paths"
    );

    // ---- Path resolution: build record → (parent, name) map ----
    let mut parent_map: HashMap<u64, (u64, &str)> = HashMap::with_capacity(collected.len());
    for r in &collected {
        parent_map.insert(r.record_number, (r.parent_record_number, r.name.as_str()));
    }

    // ---- Pass 2: Resolve paths and insert into results ----
    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Finalizing,
            current: 0,
            total: collected.len() as u64,
            message: format!("Resolving paths for {} entries", collected.len()),
        })
        .await;

    let mut entries_emitted: u64 = 0;
    for r in &collected {
        let parent_path = resolve_path(r.record_number, &parent_map);

        let extension = r
            .name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_lowercase())
            .unwrap_or_default();

        let mut entry = RecoveredEntry {
            id: RecoveredEntryId(Uuid::now_v7()),
            fs_kind: "ntfs".to_string(),
            parent_path,
            name: r.name.clone(),
            size: r.file_size,
            start_lba: None, // recomputed from extents below
            mtime: r.mtime,
            ctime: r.ctime,
            atime: r.atime,
            status: if r.is_partial {
                RecoveryStatus::PartiallyRecovered
            } else {
                RecoveryStatus::Intact
            },
            extension,
            mime: None,
            confidence: 1000,
            was_deleted: r.is_deleted,
            cluster_chain: r.cluster_chain.clone(),
            extents: r.extents.clone(),
            resident_data: r.resident_data.clone(),
        };
        entry.recompute_start_lba();

        results.insert(entry);
        entries_emitted += 1;
    }

    info!(entries_emitted, "MFT walk complete");

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: records_processed,
            total: total_records_to_walk,
            message: format!("MFT walk done: {entries_emitted} entries, {errors} errors"),
        })
        .await;

    Ok(WalkOutcome {
        entries: entries_emitted,
        bitmap: bitmap_capture,
    })
}

/// Resolve the full path of a record by walking up the parent chain.
///
/// The root directory (record 5) has parent = 5 (self-referential), which
/// terminates the recursion.
///
/// Cycles are detected via a visited set — if we revisit a record, we
/// stop and return the path accumulated so far (prefixed with `\[cycle]`).
fn resolve_path(record_number: u64, parent_map: &HashMap<u64, (u64, &str)>) -> String {
    const ROOT_RECORD: u64 = 5;
    const MAX_DEPTH: u32 = 64;

    let mut components: Vec<&str> = Vec::new();
    let mut current = record_number;
    let mut visited = Vec::with_capacity(16);
    let mut depth: u32 = 0;

    loop {
        if depth >= MAX_DEPTH {
            warn!(record_number, depth, "Path resolution exceeded max depth");
            break;
        }

        // Check for cycle
        if visited.contains(&current) {
            warn!(
                record_number,
                cycle_at = current,
                "Cycle detected in parent chain"
            );
            // Return what we have so far with a cycle marker
            components.reverse();
            if components.is_empty() {
                return String::from("\\[cycle]");
            }
            return format!("\\[cycle]\\{}", components.join("\\"));
        }
        visited.push(current);

        let Some((parent, name)) = parent_map.get(&current) else {
            // Parent not in our map — this record's parent was deleted or
            // we didn't collect it. Return what we have with a placeholder.
            components.reverse();
            if components.is_empty() {
                return format!("\\[orphan-{current}]");
            }
            return format!("\\[orphan]\\{}", components.join("\\"));
        };

        if current == ROOT_RECORD {
            // Root directory — stop here.
            break;
        }

        if *parent == ROOT_RECORD {
            // Parent is root — the name goes in the path, then stop.
            components.push(name);
            break;
        }

        components.push(name);
        current = *parent;
        depth += 1;
    }

    components.reverse();
    if components.is_empty() {
        String::from("\\")
    } else {
        format!("\\{}", components.join("\\"))
    }
}

/// Pick the best `$FILE_NAME` attribute from a record.
///
/// Preference order:
/// 1. Win32 namespace (the normal long name)
/// 2. POSIX namespace (case-sensitive name)
/// 3. Win32+DOS namespace (paired name; treat as Win32)
/// 4. DOS namespace (8.3 short name — last resort)
fn pick_best_file_name(record: &FileRecord) -> Option<FileName> {
    let mut win32: Option<FileName> = None;
    let mut posix: Option<FileName> = None;
    let mut dos: Option<FileName> = None;

    for attr in record.file_name_attributes() {
        if let Some(content) = attr.resident_content() {
            match FileName::parse(content) {
                Ok(fn_attr) => {
                    if fn_attr.is_win32_namespace() && win32.is_none() {
                        win32 = Some(fn_attr);
                    } else if fn_attr.namespace == namespace::POSIX && posix.is_none() {
                        posix = Some(fn_attr);
                    } else if fn_attr.is_dos_namespace() && dos.is_none() {
                        dos = Some(fn_attr);
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse $FILE_NAME attribute");
                }
            }
        }
    }

    win32.or(posix).or(dos)
}

/// Locate an absolute MFT byte offset within the run boundaries.
fn locate_in_runs(run_boundaries: &[(u64, u64, u64)], mft_byte_offset: u64) -> Option<u64> {
    for &(start, end, disk_off) in run_boundaries {
        if mft_byte_offset >= start && mft_byte_offset < end {
            return Some(disk_off + (mft_byte_offset - start));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_in_runs_finds_first_run() {
        let runs = vec![(0, 1000, 10_000), (1000, 2000, 20_000)];
        assert_eq!(locate_in_runs(&runs, 0), Some(10_000));
        assert_eq!(locate_in_runs(&runs, 500), Some(10_500));
        assert_eq!(locate_in_runs(&runs, 999), Some(10_999));
    }

    #[test]
    fn locate_in_runs_finds_second_run() {
        let runs = vec![(0, 1000, 10_000), (1000, 2000, 20_000)];
        assert_eq!(locate_in_runs(&runs, 1000), Some(20_000));
        assert_eq!(locate_in_runs(&runs, 1500), Some(20_500));
    }

    #[test]
    fn locate_in_runs_returns_none_for_gap() {
        let runs = vec![(0, 1000, 10_000), (2000, 3000, 30_000)];
        assert_eq!(locate_in_runs(&runs, 1500), None);
    }

    #[test]
    fn locate_in_runs_returns_none_for_empty() {
        let runs: Vec<(u64, u64, u64)> = vec![];
        assert_eq!(locate_in_runs(&runs, 0), None);
    }

    #[test]
    fn resolve_path_root_file() {
        // File in root: record 100, parent = 5 (root), name = "test.txt"
        let mut map = HashMap::new();
        map.insert(100u64, (5u64, "test.txt"));
        map.insert(5u64, (5u64, "."));
        let path = resolve_path(100, &map);
        assert_eq!(path, "\\test.txt");
    }

    #[test]
    fn resolve_path_nested_file() {
        // File in C:\Users\alice\doc.txt
        // record 5 = root, 10 = Users, 20 = alice, 30 = doc.txt
        let mut map = HashMap::new();
        map.insert(5u64, (5u64, "."));
        map.insert(10u64, (5u64, "Users"));
        map.insert(20u64, (10u64, "alice"));
        map.insert(30u64, (20u64, "doc.txt"));
        let path = resolve_path(30, &map);
        assert_eq!(path, "\\Users\\alice\\doc.txt");
    }

    #[test]
    fn resolve_path_deeply_nested() {
        // 5 → 10 → 20 → 30 → 40 → 50 (deeply nested)
        let mut map = HashMap::new();
        map.insert(5u64, (5u64, "."));
        map.insert(10u64, (5u64, "a"));
        map.insert(20u64, (10u64, "b"));
        map.insert(30u64, (20u64, "c"));
        map.insert(40u64, (30u64, "d"));
        map.insert(50u64, (40u64, "file.txt"));
        let path = resolve_path(50, &map);
        assert_eq!(path, "\\a\\b\\c\\d\\file.txt");
    }

    #[test]
    fn resolve_path_handles_orphan() {
        // Parent not in map
        let mut map = HashMap::new();
        map.insert(100u64, (999u64, "orphan.txt"));
        // 999 is not in the map
        let path = resolve_path(100, &map);
        assert!(path.starts_with("\\[orphan"));
    }

    #[test]
    fn resolve_path_detects_cycle() {
        // 10 → 20 → 10 (cycle)
        let mut map = HashMap::new();
        map.insert(5u64, (5u64, "."));
        map.insert(10u64, (20u64, "a"));
        map.insert(20u64, (10u64, "b"));
        let path = resolve_path(10, &map);
        assert!(
            path.contains("cycle"),
            "path should contain 'cycle': {path}"
        );
    }

    #[test]
    fn resolve_path_root_itself() {
        let mut map = HashMap::new();
        map.insert(5u64, (5u64, "."));
        let path = resolve_path(5, &map);
        assert_eq!(path, "\\");
    }
}
