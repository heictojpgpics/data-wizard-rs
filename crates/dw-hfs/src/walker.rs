//! HFS+ catalog walker: the quick-scan phase.
//!
//! 1. Read the volume header (offset 1024).
//! 2. Read the catalog B-tree header node (node 0 of the catalog fork)
//!    for geometry: node size, first/last leaf.
//! 3. Walk the leaf chain (`fLink`; 0 = last), bounded by `totalNodes`
//!    and a visited set.
//! 4. Two-pass: collect all records, build a CNID -> (parent, name) map
//!    from folder records, then resolve full paths from the root (CNID 2).
//! 5. Emit files (data fork) with extents from the fork's inline extents;
//!    a fork whose logical size exceeds the inline coverage is marked
//!    PartiallyRecovered (extents-overflow B-tree is v1-future work).
//!
//! Deleted files: HFS+ removes catalog records on delete (no tombstone
//! marker like FAT's 0xE5), so the quick pass yields live entries only;
//! deleted-file recovery is the deep pass (allocation-bitmap carving).

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

use dw_core::{
    CoreResult, Extent, ProgressSink, RecoveredEntry, RecoveredEntryId, RecoveryStatus,
    ResultsStore,
};
use dw_disk::ReadAt;

use crate::btree::{BTreeGeometry, node_kind, parse_geometry, read_node_at};
use crate::catalog::{CatalogRecord, parse_record};
use crate::error::HfsError;
use crate::volumeheader::VolumeHeader;

/// Outcome of a completed walk.
#[derive(Debug, Default)]
pub struct WalkOutcome {
    pub entries: u64,
    pub folders: u64,
    pub leaves_walked: u32,
    pub records: u32,
    /// CNIDs of the files emitted from the catalog (live). The deep
    /// pass uses this to tell orphaned overflow fragments from forks
    /// that are still live.
    pub live_file_ids: std::collections::HashSet<u32>,
}

/// Hard cap on nodes per walk (guards against corrupt chains).
const MAX_NODES: u32 = 100_000;

pub fn read_volume_header(read_at: &dyn ReadAt) -> crate::error::HfsResult<VolumeHeader> {
    let mut buf = [0u8; 512];
    read_at.read_at(1024, &mut buf).map_err(HfsError::Disk)?;
    VolumeHeader::parse(&buf)
}

/// Read the B-tree geometry from the catalog's header node (node 0).
pub fn read_catalog_geometry(
    read_at: &dyn ReadAt,
    vh: &VolumeHeader,
) -> crate::error::HfsResult<BTreeGeometry> {
    // The node size is itself stored in node 0 (offset 32 within the
    // node); read a provisional window at the node-0 base first.
    let base = vh
        .fork_offset_to_volume(&vh.catalog_file, 0)
        .ok_or_else(|| HfsError::Corrupt("catalog node 0 beyond inline extents".into()))?;
    let mut win = [0u8; 64];
    read_at.read_at(base, &mut win).map_err(HfsError::Disk)?;
    // nodeSize@32 (u16 BE), totalNodes@34, freeNodes@38.
    let node_size = u16::from_be_bytes([win[32], win[33]]);
    if node_size == 0 || !node_size.is_power_of_two() {
        return Err(HfsError::Corrupt(format!(
            "catalog node size {node_size} invalid"
        )));
    }
    let node = read_node_at(read_at, vh, &vh.catalog_file, 0, node_size)?;
    // Node 0 is the header node by definition; its kind byte is 0x01 on
    // real volumes (empirical). A different value is logged, not fatal.
    if node.kind != node_kind::HEADER {
        warn!(kind = node.kind, "catalog node 0 has an unexpected kind");
    }
    parse_geometry(&node)
}

/// Walk the catalog leaf chain and emit entries.
pub fn walk_catalog(
    read_at: &dyn ReadAt,
    vh: &VolumeHeader,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
) -> CoreResult<WalkOutcome> {
    walk_catalog_with_overflow(read_at, vh, results, progress, scan_id, &Default::default())
}

/// [`walk_catalog`] with the extents-overflow map (see
/// [`crate::extents::overflow_map`]): files whose data fork exceeds the
/// inline extent coverage are completed with their overflow extents
/// instead of staying `PartiallyRecovered`.
pub fn walk_catalog_with_overflow(
    read_at: &dyn ReadAt,
    vh: &VolumeHeader,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    overflow: &std::collections::HashMap<u32, Vec<crate::volumeheader::ForkExtent>>,
) -> CoreResult<WalkOutcome> {
    let geometry = read_catalog_geometry(read_at, vh)
        .map_err(|e| dw_core::CoreError::Plugin(format!("catalog geometry: {e}")))?;
    info!(
        node_size = geometry.node_size,
        total_nodes = geometry.total_nodes,
        first_leaf = geometry.first_leaf_node,
        last_leaf = geometry.last_leaf_node,
        "HFS+ catalog walk starting"
    );

    let mut outcome = WalkOutcome::default();

    // ---- Pass 1: collect records from the leaf chain ----
    let mut records: Vec<CatalogRecord> = Vec::new();
    let mut visited: HashMap<u32, ()> = HashMap::new();
    let mut current = geometry.first_leaf_node;
    let mut leaves: u32 = 0;
    while current != 0 && leaves < MAX_NODES {
        if visited.insert(current, ()).is_some() {
            warn!(node = current, "leaf chain cycle, stopping");
            break;
        }
        let node = match read_node_at(read_at, vh, &vh.catalog_file, current, geometry.node_size) {
            Ok(n) => n,
            Err(e) => {
                warn!(node = current, error = %e, "failed to read catalog node");
                break;
            }
        };
        if node.kind != node_kind::LEAF {
            warn!(
                node = current,
                kind = node.kind,
                "expected leaf node, stopping chain"
            );
            break;
        }
        for i in 0..node.num_records {
            if let Ok(slice) = node.record_slice(i) {
                match parse_record(slice) {
                    Ok(Some(rec)) => records.push(rec),
                    Ok(None) => {}
                    Err(e) => {
                        tracing::trace!(node = current, record = i, error = %e, "bad record");
                    }
                }
            }
        }
        outcome.leaves_walked += 1;
        leaves += 1;
        current = node.f_link;
    }
    outcome.records = records.len() as u32;

    // ---- Pass 2: CNID -> (parent, name) map from folder records ----
    let mut folders: HashMap<u32, (u32, String)> = HashMap::new();
    for rec in &records {
        if let CatalogRecord::Folder { key, folder_id, .. } = rec {
            folders.insert(*folder_id, (key.parent_id, key.name.clone()));
        }
    }

    // ---- Pass 3: emit file records with resolved paths ----
    {
        // Diagnostic counters (debug builds only via tracing).
        let mut n_file = 0u32;
        let mut n_folder = 0u32;
        let mut n_thread = 0u32;
        for r in &records {
            match r {
                CatalogRecord::File { .. } => n_file += 1,
                CatalogRecord::Folder { .. } => n_folder += 1,
                CatalogRecord::Thread { .. } => n_thread += 1,
            }
        }
        tracing::debug!(
            files = n_file,
            folders = n_folder,
            threads = n_thread,
            "record type census"
        );
    }
    let resolve_path = |cnid: u32, name: &str| -> String {
        // Walk up folder CNIDs to the root (2).
        let mut parts: Vec<String> = Vec::new();
        parts.push(name.to_string());
        let mut cur = cnid;
        let mut steps = 0;
        while cur != crate::volumeheader::cnid::ROOT && steps < 64 {
            steps += 1;
            let Some((parent, fname)) = folders.get(&cur) else {
                // Unknown parent: emit under a placeholder.
                return format!(
                    "\\[cnid-{cur}]\\{}",
                    parts.first().cloned().unwrap_or_default()
                );
            };
            parts.push(fname.clone());
            cur = *parent;
        }
        parts.reverse();
        format!("\\{}", parts.join("/"))
    };

    for rec in &records {
        let CatalogRecord::File {
            key,
            file_id,
            create_date,
            modify_date,
            access_date,
            data_fork,
            resource_fork: _,
        } = rec
        else {
            continue;
        };
        // Skip reserved files (CNIDs < 16 belong to the volume's own
        // special files). The PARENT check was wrong: files directly in
        // the root have parent 2; the reserved test belongs on file_id.
        if *file_id < crate::volumeheader::cnid::USER_FIRST {
            continue;
        }
        let full_path = resolve_path(key.parent_id, &key.name);
        let (parent_path, name) = split_path(&full_path);

        let size = data_fork.logical_size;
        // Complete the fork with its overflow extents when the inline
        // coverage falls short (TN1150: forks beyond 8 extents continue
        // in the extents-overflow B-tree).
        let full_extents = crate::extents::fork_extents_with_overflow(
            &data_fork.extents,
            overflow.get(file_id),
            vh.block_size,
            size,
        );
        let completed_fork = crate::volumeheader::ForkData {
            logical_size: data_fork.logical_size,
            clump_size: data_fork.clump_size,
            total_blocks: data_fork.total_blocks,
            extents: full_extents,
        };
        let extent_pairs =
            crate::catalog::fork_extents_to_bytes(vh.block_size, &completed_fork, size);
        let covered: u64 = extent_pairs.iter().map(|e| e.1).sum();
        let extents: Vec<Extent> = extent_pairs
            .iter()
            .map(|&(s, l)| Extent {
                start_byte: s,
                length: l,
            })
            .collect();
        let status = if size == 0 || covered >= size {
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
            fs_kind: if vh.signature == 0x4858 {
                "hfsx"
            } else {
                "hfs+"
            }
            .to_string(),
            parent_path,
            name,
            size,
            start_lba: None,
            mtime: *modify_date,
            ctime: *create_date,
            atime: *access_date,
            status,
            extension,
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: data_fork
                .extents
                .iter()
                .map(|e| u64::from(e.start_block))
                .collect(),
            extents,
            resident_data: None,
        };
        entry.recompute_start_lba();
        results.insert(entry);
        outcome.entries += 1;
        outcome.live_file_ids.insert(*file_id);
    }
    outcome.folders = folders.len() as u64;

    info!(
        entries = outcome.entries,
        leaves = outcome.leaves_walked,
        records = outcome.records,
        "HFS+ catalog walk complete"
    );

    let _ = progress;
    let _ = scan_id;
    Ok(outcome)
}

/// Split "\\a/b/c.txt" into ("\\a/b", "c.txt").
fn split_path(p: &str) -> (String, String) {
    match p.rfind(['/', '\\']) {
        Some(0) => ("\\".to_string(), p[1..].to_string()),
        Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
        None => ("\\".to_string(), p.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    #[test]
    fn split_path_works() {
        assert_eq!(
            split_path("\\a/b/c.txt"),
            ("\\a/b".to_string(), "c.txt".to_string())
        );
        assert_eq!(
            split_path("\\root.txt"),
            ("\\".to_string(), "root.txt".to_string())
        );
    }
}
