//! FAT directory walker — the core metadata scan.
//!
//! Walks the FAT directory tree starting from the root directory:
//!
//! - **FAT12/16**: Root directory is at a fixed location (after the FATs).
//!   Walk all 32-byte entries in the fixed-size root dir region.
//! - **FAT32**: Root directory is a cluster chain starting at
//!   `root_cluster`. Follow the FAT to walk the chain.
//!
//! For each directory entry:
//! 1. Parse it (short 8.3 entry + optional LFN entries).
//! 2. If it's a subdirectory, recurse into it.
//! 3. If it's a file, extract name/cluster/size and build a `RecoveredEntry`.
//! 4. Deleted entries (0xE5 first byte) are collected with `was_deleted=true`.

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use dw_core::{
    CoreError, CoreResult, ProgressEvent, ProgressPhase, ProgressSink, RecoveredEntry,
    RecoveredEntryId, RecoveryStatus, ResultsStore, ScanMode,
};
use dw_disk::{DiskSource, ReadAt};

use crate::bpb::{FatBpb, FatKind};
use crate::direntry::{DirEntry, END_MARKER, ShortEntry};
use crate::error::FatError;
use crate::fat::FatTable;
use crate::timestamp::{decode_fat_access_date, decode_fat_datetime};

pub use dw_core::merge_and_trim_offsets;

/// Walk the FAT directory tree and append entries to `results`.
pub async fn walk_fat(
    source: Arc<dyn DiskSource>,
    bpb: &FatBpb,
    fat_table: &FatTable<'_>,
    mode: ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
) -> CoreResult<u64> {
    let scan_id = Uuid::now_v7();
    let read_at = source.as_read_at();

    info!(
        kind = ?bpb.kind,
        total_sectors = bpb.total_sectors,
        cluster_size = bpb.bytes_per_cluster,
        mode = ?mode,
        "Starting FAT walk"
    );

    // Collect all entries by walking the directory tree.
    let mut entries_emitted: u64 = 0;
    let mut visited_dirs: HashSet<u32> = HashSet::new(); // cycle detection

    // Start walking from the root.
    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: 0,
            total: 1,
            message: "Reading root directory".to_string(),
        })
        .await;

    match bpb.kind {
        FatKind::Fat12 | FatKind::Fat16 => {
            // FAT12/16: root directory is at a fixed location.
            let root_offset = bpb
                .root_dir_offset_bytes()
                .ok_or_else(|| CoreError::Plugin("FAT12/16 root dir offset is None".into()))?;
            let root_size = u64::from(bpb.root_dir_sectors) * u64::from(bpb.bytes_per_sector);
            let mut root_buf = vec![0u8; root_size as usize];
            read_at
                .read_at(root_offset, &mut root_buf)
                .map_err(CoreError::Disk)?;
            walk_directory(
                &root_buf,
                "\\",
                None, // no cluster chain for FAT12/16 root
                fat_table,
                bpb,
                &read_at,
                &results,
                &progress,
                scan_id,
                &mut entries_emitted,
                &mut visited_dirs,
            )
            .map_err(|e| CoreError::Plugin(format!("FAT root dir walk: {e}")))?;
        }
        FatKind::Fat32 => {
            // FAT32: root directory is a cluster chain starting at root_cluster.
            let root_cluster = bpb.root_cluster.unwrap_or(2);
            walk_directory_chain(
                root_cluster,
                "\\",
                fat_table,
                bpb,
                &read_at,
                &results,
                &progress,
                scan_id,
                &mut entries_emitted,
                &mut visited_dirs,
            )
            .map_err(|e| CoreError::Plugin(format!("FAT32 root dir walk: {e}")))?;
        }
    }

    info!(entries_emitted, "FAT walk complete");

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::QuickScan,
            current: entries_emitted,
            total: entries_emitted,
            message: format!("FAT walk done: {entries_emitted} entries"),
        })
        .await;

    Ok(entries_emitted)
}

/// Walk a directory whose contents are in a cluster chain (FAT32) or in
/// a fixed buffer (FAT12/16 root).
#[allow(clippy::too_many_arguments)]
fn walk_directory(
    dir_buf: &[u8],
    dir_path: &str,
    dir_cluster: Option<u32>,
    fat_table: &FatTable,
    bpb: &FatBpb,
    read_at: &Arc<dyn ReadAt>,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    entries_emitted: &mut u64,
    visited_dirs: &mut HashSet<u32>,
) -> Result<(), FatError> {
    // Mark this directory as visited (cycle detection).
    if let Some(cluster) = dir_cluster
        && !visited_dirs.insert(cluster)
    {
        warn!(cluster, "Directory cycle detected, skipping");
        return Ok(());
    }

    // Parse directory entries in 32-byte chunks.
    let mut pending_lfn: Vec<DirEntry> = Vec::new();
    let mut offset = 0;
    while offset + 32 <= dir_buf.len() {
        let entry_buf: &[u8; 32] = dir_buf[offset..offset + 32].try_into().unwrap();

        // Check for end-of-directory marker.
        if entry_buf[0] == END_MARKER {
            break;
        }

        let entry = DirEntry::parse(entry_buf)?;

        match entry {
            DirEntry::End => break,
            DirEntry::Lfn { .. } => {
                pending_lfn.push(entry);
            }
            DirEntry::Short {
                entry: short,
                long_name: _,
            } => {
                // Try to reassemble LFN if we have pending entries.
                let long_name = if !pending_lfn.is_empty() {
                    let name = crate::direntry::reassemble_lfn(&pending_lfn);
                    pending_lfn.clear();
                    name
                } else {
                    None
                };

                process_short_entry(
                    &short,
                    long_name,
                    dir_path,
                    fat_table,
                    bpb,
                    read_at,
                    results,
                    progress,
                    scan_id,
                    entries_emitted,
                    visited_dirs,
                )?;

                // Periodically emit progress.
                if *entries_emitted > 0 && entries_emitted.is_multiple_of(256) {
                    // Progress emission is sync here; the caller handles async.
                    let _ = progress;
                    let _ = scan_id;
                }
            }
        }

        offset += 32;
    }

    Ok(())
}

/// Walk a directory whose contents are in a cluster chain (FAT32, or
/// subdirectories on FAT12/16/32).
#[allow(clippy::too_many_arguments)]
fn walk_directory_chain(
    start_cluster: u32,
    dir_path: &str,
    fat_table: &FatTable,
    bpb: &FatBpb,
    read_at: &Arc<dyn ReadAt>,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    entries_emitted: &mut u64,
    visited_dirs: &mut HashSet<u32>,
) -> Result<(), FatError> {
    // Follow the cluster chain for this directory.
    let chain = fat_table.follow_chain(start_cluster, 1_000_000)?;

    // Read all clusters in the chain into one buffer.
    let cluster_size = bpb.bytes_per_cluster as usize;
    let mut dir_buf = vec![0u8; chain.len() * cluster_size];
    for (i, &cluster) in chain.iter().enumerate() {
        let offset = bpb.cluster_to_byte(cluster);
        read_at.read_at(
            offset,
            &mut dir_buf[i * cluster_size..(i + 1) * cluster_size],
        )?;
    }

    walk_directory(
        &dir_buf,
        dir_path,
        Some(start_cluster),
        fat_table,
        bpb,
        read_at,
        results,
        progress,
        scan_id,
        entries_emitted,
        visited_dirs,
    )
}

/// Process a parsed short directory entry — build a `RecoveredEntry` and
/// recurse into subdirectories.
#[allow(clippy::too_many_arguments)]
fn process_short_entry(
    short: &ShortEntry,
    long_name: Option<String>,
    dir_path: &str,
    fat_table: &FatTable,
    bpb: &FatBpb,
    read_at: &Arc<dyn ReadAt>,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    entries_emitted: &mut u64,
    visited_dirs: &mut HashSet<u32>,
) -> Result<(), FatError> {
    // Skip volume label entries.
    if short.is_volume_label() {
        return Ok(());
    }

    // Skip "." and ".." entries.
    let name = long_name.unwrap_or_else(|| short.full_name());
    if name == "." || name == ".." {
        return Ok(());
    }

    let full_path = if dir_path.ends_with('/') || dir_path.ends_with('\\') {
        format!("{dir_path}{name}")
    } else {
        format!("{dir_path}/{name}")
    };

    let is_dir = short.is_directory();
    let is_deleted = short.is_deleted;

    // Build the recovered entry.
    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default();

    // Follow the cluster chain to get the file's cluster list.
    let cluster_chain: Vec<u64> = if short.first_cluster >= 2 {
        match fat_table.follow_chain(short.first_cluster, 1_000_000) {
            Ok(chain) => chain.iter().map(|&c| u64::from(c)).collect(),
            Err(e) => {
                trace!(cluster = short.first_cluster, error = %e, "Failed to follow cluster chain");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Translate the FS cluster chain into canonical physical extents:
    // consecutive clusters merge into one extent; the final extent is
    // trimmed so the total equals the directory-entry file size. This is
    // what makes fragmented-file recovery work.
    //
    // Unit correctness: `cluster_chain` holds FS cluster numbers (2-based);
    // `bpb.cluster_to_byte()` maps them to absolute byte offsets in the
    // volume (data-region offset + (cluster-2) * cluster_size).
    let offsets: Vec<u64> = cluster_chain
        .iter()
        .map(|&c| bpb.cluster_to_byte(u32::try_from(c).unwrap_or(u32::MAX)))
        .collect();
    let extents = merge_and_trim_offsets(
        &offsets,
        u64::from(bpb.bytes_per_cluster),
        u64::from(short.file_size),
    );

    // Honest status: chain shorter than file size -> PartiallyRecovered.
    let chain_bytes = u64::from(bpb.bytes_per_cluster) * cluster_chain.len() as u64;
    let status = if short.file_size == 0 || !cluster_chain.is_empty() {
        if chain_bytes >= u64::from(short.file_size) || short.file_size == 0 {
            RecoveryStatus::Intact
        } else {
            RecoveryStatus::PartiallyRecovered
        }
    } else {
        RecoveryStatus::PartiallyRecovered
    };

    let mut entry = RecoveredEntry {
        id: RecoveredEntryId(Uuid::now_v7()),
        fs_kind: match bpb.kind {
            FatKind::Fat12 => "fat12".to_string(),
            FatKind::Fat16 => "fat16".to_string(),
            FatKind::Fat32 => "fat32".to_string(),
        },
        parent_path: dir_path.to_string(),
        name,
        size: u64::from(short.file_size),
        start_lba: None, // recomputed from extents below
        mtime: decode_fat_datetime(short.last_write_date, short.last_write_time, None),
        ctime: decode_fat_datetime(
            short.creation_date,
            short.creation_time,
            Some(short.creation_time_tenths),
        ),
        atime: decode_fat_access_date(short.last_access_date),
        status,
        extension,
        mime: None,
        confidence: 1000,
        was_deleted: is_deleted,
        cluster_chain,
        extents,
        resident_data: None,
    };
    entry.recompute_start_lba();

    results.insert(entry);
    *entries_emitted += 1;

    // Recurse into subdirectories.
    if is_dir && short.first_cluster >= 2 {
        debug!(path = %full_path, cluster = short.first_cluster, "Recursing into subdirectory");
        walk_directory_chain(
            short.first_cluster,
            &full_path,
            fat_table,
            bpb,
            read_at,
            results,
            progress,
            scan_id,
            entries_emitted,
            visited_dirs,
        )?;
    }

    Ok(())
}
