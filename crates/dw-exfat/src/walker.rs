//! exFAT directory walker — the core metadata scan.
//!
//! Walks the exFAT directory tree starting from the root directory
//! cluster chain (`root_dir_first_cluster` from the boot sector),
//! following FAT chains for directories and streams that need them.
//!
//! Per entry set (exFAT spec §7.3-7.5):
//!
//! ```text
//! FileDirectory (0x85 active / 0x05 deleted)   <- primary
//!   +-- StreamExtension (0xC0 / 0x40 deleted)  <- exactly 1
//!   +-- FileName (0xC1 / 0x41 deleted) x N     <- 1..=17
//! ```
//!
//! The walker:
//!  1. reads the FAT region once (shared with the plugin);
//!  2. follows the root directory's cluster chain;
//!  3. parses entry sets, reassembles file names, decodes timestamps
//!     (FAT-family format, shared decoder in `dw_core::fat_timestamp`);
//!  4. computes the cluster chain: `NoFatChain` streams are contiguous
//!     (never consult the FAT), others follow the FAT;
//!  5. translates chains into canonical physical extents
//!     (`dw_core::merge_and_trim_offsets`);
//!  6. recurses into directories (in-use always; deleted only when
//!     NoFatChain keeps the content readable), with a cycle guard;
//!  7. captures the Allocation Bitmap directory entry + volume label so
//!     the plugin can run the deep scan afterwards.
//!
//! Deleted-file handling: deleting a file in exFAT clears the InUse bits
//! of the whole set and zeroes the FAT entries of its chain. The
//! directory-entry bytes (including `first_cluster`, `data_length` and
//! `NoFatChain`) survive, so:
//!  - deleted + NoFatChain -> full extents still computable (contiguous);
//!  - deleted + FAT-chained -> only the first cluster is known
//!    (chain yields a single cluster); the entry is emitted with partial
//!    extents and status PartiallyRecovered.

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use dw_core::fat_timestamp::decode_fat_datetime;
use dw_core::merge_and_trim_offsets;
use dw_core::{
    CoreResult, Extent, ProgressEvent, ProgressPhase, ProgressSink, RecoveredEntry,
    RecoveredEntryId, RecoveryStatus, ResultsStore,
};
use dw_disk::{DiskSource, ReadAt};

use crate::bitmap::BitmapEntry;
use crate::boot::ExfatBoot;
use crate::direntry::{self, FileNameEntry, StreamExtensionEntry};
use crate::fat::ExfatFat;

/// Upper bound on a single directory's in-memory buffer (64 MiB — a
/// directory holding 2M entries at 32 bytes each). Legitimate volumes
/// stay far below this; the cap only guards against corrupt chains.
const MAX_DIR_BUFFER_BYTES: usize = 64 * 1024 * 1024;

/// Upper bound on chain length per stream/directory (1M clusters).
const MAX_CHAIN_CLUSTERS: u32 = 1_000_000;

/// Outcome of a completed metadata walk.
#[derive(Debug, Default)]
pub struct WalkOutcome {
    /// Number of entries emitted into the results store.
    pub entries: u64,
    /// Allocation Bitmap directory entry found in the root directory
    /// (needed by the deep scan to compute free regions). None when the
    /// root has no bitmap entry (non-spec image or corrupt).
    pub bitmap: Option<BitmapEntry>,
    /// Volume label from the root directory, if present.
    pub volume_label: Option<String>,
}

/// Read the FAT region of the volume into an [`ExfatFat`].
pub fn read_fat(source: &dyn DiskSource, boot: &ExfatBoot) -> CoreResult<ExfatFat> {
    let read_at = source.as_read_at();
    let offset = boot.fat_offset_bytes();
    let len = u64::from(boot.fat_length) * u64::from(boot.bytes_per_sector);
    let mut data = vec![0u8; len as usize];
    read_at
        .read_at(offset, &mut data)
        .map_err(dw_core::CoreError::Disk)?;
    ExfatFat::parse(data).map_err(|e| dw_core::CoreError::Plugin(format!("exFAT FAT parse: {e}")))
}

/// Walk the exFAT directory tree and append entries to `results`.
pub async fn walk_exfat(
    source: Arc<dyn DiskSource>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
) -> CoreResult<WalkOutcome> {
    let scan_id = Uuid::now_v7();
    let read_at = source.as_read_at();
    let mut outcome = WalkOutcome::default();
    let mut visited_dirs: HashSet<u32> = HashSet::new();

    info!(
        cluster_size = boot.bytes_per_cluster,
        cluster_count = boot.cluster_count,
        root = boot.root_dir_first_cluster,
        "Starting exFAT walk"
    );

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: 0,
            total: 1,
            message: "Reading root directory".to_string(),
        })
        .await;

    // Root directory: always FAT-chained (no stream entry exists for it).
    let root_buf = read_dir_chain(&read_at, boot, fat, boot.root_dir_first_cluster)?;
    let mut state = DirState::default();

    walk_directory_buffer(
        &root_buf,
        "\\",
        &mut state,
        &read_at,
        boot,
        fat,
        &results,
        &mut visited_dirs,
        &mut outcome,
    )?;

    outcome.volume_label = state.volume_label.clone();
    outcome.bitmap = state.bitmap;

    info!(entries = outcome.entries, "exFAT walk complete");

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: outcome.entries,
            total: outcome.entries,
            message: format!("exFAT walk done: {} entries", outcome.entries),
        })
        .await;

    Ok(outcome)
}

/// State accumulated while parsing one directory (and shared across the
/// walk for root-level metadata).
#[derive(Default)]
struct DirState {
    volume_label: Option<String>,
    bitmap: Option<BitmapEntry>,
}

/// Parse one directory's buffer (all clusters of its chain, in order).
#[allow(clippy::too_many_arguments)]
fn walk_directory_buffer(
    dir_buf: &[u8],
    dir_path: &str,
    state: &mut DirState,
    read_at: &Arc<dyn ReadAt>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    results: &Arc<ResultsStore>,
    visited_dirs: &mut HashSet<u32>,
    outcome: &mut WalkOutcome,
) -> CoreResult<()> {
    let mut offset = 0usize;
    let total = dir_buf.len();

    while offset + 32 <= total {
        let entry_buf: &[u8; 32] = dir_buf[offset..offset + 32]
            .try_into()
            .expect("32-byte slice");

        // End of directory: 0x00 in the type byte ends THIS directory.
        if entry_buf[0] == direntry::entry_type::END_OF_DIRECTORY {
            break;
        }

        // Skip free slots within the directory (type byte 0x00 handled
        // above; exFAT has no 0xE5-style marker — deletion clears InUse).
        match direntry::DirEntry::parse(entry_buf) {
            Ok(direntry::DirEntry::Bitmap) => {
                if state.bitmap.is_none() {
                    match BitmapEntry::parse(entry_buf) {
                        Ok(bm) => state.bitmap = Some(bm),
                        Err(e) => warn!("root bitmap entry corrupt: {e}"),
                    }
                }
                offset += 32;
            }
            Ok(direntry::DirEntry::UpcaseTable) => {
                offset += 32;
            }
            Ok(direntry::DirEntry::VolumeLabel) => {
                if state.volume_label.is_none() {
                    state.volume_label = Some(parse_volume_label(entry_buf));
                }
                offset += 32;
            }
            Ok(direntry::DirEntry::FileDirectory(primary)) => {
                // An entry set: primary + `secondary_count` secondaries.
                let n = primary.secondary_count as usize;
                let set_start = offset;
                let set_end = (offset + 32 * (n + 1)).min(total);

                if offset + 32 * (n + 1) > total {
                    // Claimed secondaries run past the buffer: corrupt set.
                    debug!(dir = dir_path, "entry set truncated, skipping");
                    offset = total;
                    continue;
                }

                // Collect secondaries.
                let mut stream: Option<StreamExtensionEntry> = None;
                let mut names: Vec<FileNameEntry> = Vec::new();
                let mut bad_set = false;
                for s in 1..=n {
                    let s_buf: &[u8; 32] = dir_buf[set_start + s * 32..set_start + s * 32 + 32]
                        .try_into()
                        .expect("32-byte slice");
                    match direntry::DirEntry::parse(s_buf) {
                        Ok(direntry::DirEntry::StreamExtension(se)) => {
                            if stream.is_some() {
                                bad_set = true; // two streams in one set
                            } else {
                                stream = Some(se);
                            }
                        }
                        Ok(direntry::DirEntry::FileName(fn_)) => names.push(fn_),
                        _ => {
                            bad_set = true; // unexpected secondary type
                        }
                    }
                }

                offset = set_end; // consume the whole claimed set regardless

                let Some(stream) = stream else {
                    if !bad_set {
                        debug!(dir = dir_path, "entry set without stream, skipped");
                    }
                    continue;
                };

                if let Err(e) = process_entry_set(
                    primary,
                    stream,
                    names,
                    dir_path,
                    read_at,
                    boot,
                    fat,
                    results,
                    visited_dirs,
                    outcome,
                ) {
                    // Per-entry failure must not abort the walk.
                    trace!(dir = dir_path, error = %e, "entry set processing failed");
                }
            }
            Ok(_) => {
                offset += 32;
            }
            Err(e) => {
                trace!(dir = dir_path, error = %e, "bad directory entry, skipping");
                offset += 32;
            }
        }
    }

    Ok(())
}

/// Process one complete entry set: emit a RecoveredEntry and recurse into
/// directories when their content is readable.
#[allow(clippy::too_many_arguments)]
fn process_entry_set(
    primary: direntry::FileDirectoryEntry,
    stream: StreamExtensionEntry,
    names: Vec<FileNameEntry>,
    dir_path: &str,
    read_at: &Arc<dyn ReadAt>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    results: &Arc<ResultsStore>,
    visited_dirs: &mut HashSet<u32>,
    outcome: &mut WalkOutcome,
) -> CoreResult<()> {
    let name = direntry::reassemble_file_name(&names);
    if name.is_empty() {
        return Ok(()); // unnamed set: nothing useful to emit
    }

    let full_path = if dir_path.ends_with('\\') || dir_path.ends_with('/') {
        format!("{dir_path}{name}")
    } else {
        format!("{dir_path}/{name}")
    };

    let is_dir = primary.file_attributes & 0x0010 != 0;
    let deleted = !primary.in_use;

    // Timestamps: u32 = date in high 16 bits, time in low 16 bits.
    let mtime = decode_fat_datetime(
        (primary.last_modified_timestamp >> 16) as u16,
        primary.last_modified_timestamp as u16,
        None,
    );
    let ctime = decode_fat_datetime(
        (primary.create_timestamp >> 16) as u16,
        primary.create_timestamp as u16,
        Some(primary.create_time_10ms),
    );
    let atime = decode_fat_datetime(
        (primary.last_accessed_timestamp >> 16) as u16,
        primary.last_accessed_timestamp as u16,
        None,
    );

    // Cluster chain + extents.
    let cluster_size = u64::from(boot.bytes_per_cluster);
    let file_size = stream.data_length;
    let (cluster_chain, extents, covered) = if stream.first_cluster < 2 || file_size == 0 {
        (Vec::new(), Vec::new(), 0u64)
    } else if stream.no_fat_chain {
        // Contiguous: first_cluster .. first_cluster + ceil(len / cs).
        let n_clusters = file_size.div_ceil(cluster_size).max(1);
        let chain: Vec<u64> = (0..n_clusters)
            .map(|i| u64::from(stream.first_cluster + i as u32))
            .collect();
        let offsets: Vec<u64> = chain
            .iter()
            .map(|&c| boot.cluster_to_byte(c as u32))
            .collect();
        let extents = merge_and_trim_offsets(&offsets, cluster_size, file_size);
        let covered: u64 = extents.iter().map(|e| e.length).sum();
        (chain, extents, covered)
    } else {
        // FAT-chained: follow the chain (single cluster when deleted —
        // the OS zeroes FAT entries on delete).
        let chain32: Vec<u32> = fat
            .follow_chain(stream.first_cluster, MAX_CHAIN_CLUSTERS)
            .unwrap_or_else(|e| {
                trace!(cluster = stream.first_cluster, error = %e, "chain follow failed");
                vec![stream.first_cluster]
            });
        let chain: Vec<u64> = chain32.iter().map(|&c| u64::from(c)).collect();
        let offsets: Vec<u64> = chain32.iter().map(|&c| boot.cluster_to_byte(c)).collect();
        let extents = merge_and_trim_offsets(&offsets, cluster_size, file_size);
        let covered: u64 = extents.iter().map(|e| e.length).sum();
        (chain, extents, covered)
    };

    let status = if file_size == 0 || covered >= file_size {
        RecoveryStatus::Intact
    } else {
        RecoveryStatus::PartiallyRecovered
    };

    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default();

    let mut entry = RecoveredEntry {
        id: RecoveredEntryId(Uuid::now_v7()),
        fs_kind: "exfat".to_string(),
        parent_path: dir_path.to_string(),
        name,
        size: file_size,
        start_lba: None,
        mtime,
        ctime,
        atime,
        status,
        extension,
        mime: None,
        confidence: 1000,
        was_deleted: deleted,
        cluster_chain,
        extents,
        resident_data: None,
    };
    entry.recompute_start_lba();
    results.insert(entry);
    outcome.entries += 1;

    // Recurse into directories whose content we can still read:
    //  - in-use directories (normal case), or
    //  - deleted directories that were NoFatChain (contents readable).
    // Deleted FAT-chained directories are skipped: their chain was zeroed.
    if is_dir && stream.first_cluster >= 2 {
        let readable = !deleted || stream.no_fat_chain;
        if readable {
            if !visited_dirs.insert(stream.first_cluster) {
                warn!(
                    cluster = stream.first_cluster,
                    "directory cycle detected, skipping"
                );
                return Ok(());
            }
            let dir_buf = read_dir_chain(read_at, boot, fat, stream.first_cluster)?;
            walk_directory_buffer(
                &dir_buf,
                &full_path,
                &mut DirState::default(), // non-root: no bitmap/label here
                read_at,
                boot,
                fat,
                results,
                visited_dirs,
                outcome,
            )?;
        } else {
            debug!(path = %full_path, "deleted FAT-chained directory: contents unreadable, not recursing");
        }
    }

    Ok(())
}

/// Read a directory's full cluster chain into one buffer.
fn read_dir_chain(
    read_at: &Arc<dyn ReadAt>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    first_cluster: u32,
) -> CoreResult<Vec<u8>> {
    let chain = fat
        .follow_chain(first_cluster, MAX_CHAIN_CLUSTERS)
        .map_err(|e| dw_core::CoreError::Plugin(format!("directory chain: {e}")))?;

    let cluster_size = boot.bytes_per_cluster as usize;
    let mut total = chain.len() * cluster_size;
    if total > MAX_DIR_BUFFER_BYTES {
        warn!(
            clusters = chain.len(),
            "directory chain exceeds {MAX_DIR_BUFFER_BYTES} bytes, truncating read"
        );
        total = MAX_DIR_BUFFER_BYTES;
    }
    let mut buf = vec![0u8; total];
    let mut filled = 0usize;
    for &c in &chain {
        if filled >= total {
            break;
        }
        let off = boot.cluster_to_byte(c);
        let end = (filled + cluster_size).min(total);
        read_at
            .read_at(off, &mut buf[filled..end])
            .map_err(dw_core::CoreError::Disk)?;
        filled = end;
    }
    Ok(buf)
}

/// Parse the volume label from a Volume Label entry (UTF-16, 11 chars,
/// bytes 2..24 absolute).
fn parse_volume_label(buf: &[u8; 32]) -> String {
    let mut units = Vec::with_capacity(11);
    let mut i = 2usize;
    while i + 1 < 24 {
        let unit = u16::from_le_bytes([buf[i], buf[i + 1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
        i += 2;
    }
    units
        .iter()
        .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Read the allocation bitmap data from its cluster chain.
///
/// Returns the bitmap bytes covering `cluster_count` clusters. Used by
/// the plugin's deep scan to compute free regions.
pub fn read_bitmap_data(
    read_at: &Arc<dyn ReadAt>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    entry: &BitmapEntry,
) -> CoreResult<Vec<u8>> {
    if entry.first_cluster < 2 || entry.length == 0 {
        return Err(dw_core::CoreError::Plugin(
            "bitmap entry has no chain".into(),
        ));
    }
    let cluster_size = u64::from(boot.bytes_per_cluster);
    let n_clusters = entry.length.div_ceil(cluster_size).max(1);
    let n_clusters = (n_clusters as u32).min(MAX_CHAIN_CLUSTERS);
    let chain = fat
        .follow_chain(entry.first_cluster, n_clusters.max(1))
        .map_err(|e| dw_core::CoreError::Plugin(format!("bitmap chain: {e}")))?;

    let mut data = vec![0u8; entry.length as usize];
    let mut filled = 0usize;
    for &c in &chain {
        if filled >= data.len() {
            break;
        }
        let off = boot.cluster_to_byte(c);
        let end = (filled + cluster_size as usize).min(data.len());
        read_at
            .read_at(off, &mut data[filled..end])
            .map_err(dw_core::CoreError::Disk)?;
        filled = end;
    }
    Ok(data)
}

/// Convenience: build `Vec<Extent>` from free runs for the carver.
///
/// Each free run `(first_cluster, len_clusters)` maps to the absolute
/// byte range `[cluster_to_byte(first), + len * cluster_size)`.
pub fn free_runs_to_extents(boot: &ExfatBoot, runs: &[(u32, u32)]) -> Vec<Extent> {
    runs.iter()
        .map(|&(first, len)| Extent {
            start_byte: boot.cluster_to_byte(first),
            length: u64::from(len) * u64::from(boot.bytes_per_cluster),
        })
        .collect()
}
