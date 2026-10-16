//! FsPlugin impl for HFS+/HFSX.
//!
//! Probe: volume header at offset 1024, signature 'H+' (0x482B) or
//! 'HX' (0x4858, case-sensitive HFSX).
//!
//! Quick scan: catalog B-tree walk (see `walker.rs`) - live entries only
//! (HFS+ removes catalog records on delete; there is no tombstone).
//!
//! Deep scan: allocation-bitmap free-run carving. The allocation bitmap
//! is a fork in the volume header; free allocation blocks are exactly
//! where deleted-file remnants live. We verify the computed free-run
//! total against the volume header's `freeBlocks` and log a warning on
//! mismatch (corrupt bitmap) without failing the scan.

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use tracing::{info, warn};

use crate::volumeheader::VolumeHeader;
use crate::walker;

pub struct HfsPlugin;

impl Default for HfsPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for HfsPlugin {
    fn id(&self) -> &'static str {
        "hfs"
    }
    fn name(&self) -> &'static str {
        "HFS+/HFSX parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let read_at = source.as_read_at();
        match walker::read_volume_header(read_at.as_ref()) {
            Ok(vh) => {
                let fs_kind = if vh.signature == 0x4858 {
                    "hfsx"
                } else {
                    "hfs+"
                };
                let total_bytes = u64::from(vh.total_blocks) * u64::from(vh.block_size);
                let probe = FsProbe {
                    fs_kind: fs_kind.to_string(),
                    volume_label: None, // HFS+ has no volume label in the header
                    volume_serial: Some(format!("cnid-next:{:08X}", vh.next_catalog_id)),
                    total_bytes,
                    cluster_size: u64::from(vh.block_size),
                    estimated_quick_seconds: Some(estimate_quick_seconds(total_bytes)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(total_bytes)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(crate::error::HfsError::NotHfsPlus) => Ok(ProbeOutcome::No),
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("HFS+ volume header: {e}"))),
        }
    }

    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> CoreResult<()> {
        let read_at = source.as_read_at();
        let vh = walker::read_volume_header(read_at.as_ref())
            .map_err(|e| dw_core::CoreError::Plugin(format!("HFS+ volume header: {e}")))?;

        let scan_id = uuid::Uuid::now_v7();

        progress
            .emit(dw_core::ProgressEvent {
                scan_id,
                phase: dw_core::ProgressPhase::QuickScan,
                current: 0,
                total: 1,
                message: "Walking the catalog B-tree".to_string(),
            })
            .await;

        // Extents-overflow B-tree: completes live files beyond inline
        // coverage (both modes) and supplies the deep pass's orphaned
        // fragments.
        let overflow_records =
            crate::extents::walk_overflow(read_at.as_ref(), &vh).unwrap_or_else(|e| {
                warn!(error = %e, "extents-overflow walk failed; inline coverage only");
                Vec::new()
            });
        let overflow = crate::extents::overflow_map(&overflow_records);

        let outcome = walker::walk_catalog_with_overflow(
            read_at.as_ref(),
            &vh,
            &results,
            &progress,
            scan_id,
            &overflow,
        )?;

        if mode == ScanMode::Deep {
            // Orphaned overflow fragments: fork ids with no catalog
            // record are deleted files whose extents are still indexed.
            let mut orphaned = 0u64;
            for (fork_id, extents) in &overflow {
                if *fork_id < crate::volumeheader::cnid::USER_FIRST {
                    continue; // the volume's own metadata forks
                }
                if outcome.live_file_ids.contains(fork_id) {
                    continue; // still live: already completed above
                }
                let bs = u64::from(vh.block_size);
                let extent_list: Vec<dw_core::Extent> = extents
                    .iter()
                    .map(|e| dw_core::Extent {
                        start_byte: u64::from(e.start_block) * bs,
                        length: u64::from(e.block_count) * bs,
                    })
                    .collect();
                let size: u64 = extent_list.iter().map(|e| e.length).sum();
                if size == 0 {
                    continue;
                }
                let _ = results.insert(dw_core::RecoveredEntry {
                    id: dw_core::RecoveredEntryId(uuid::Uuid::now_v7()),
                    fs_kind: if vh.signature == 0x4858 {
                        "hfsx"
                    } else {
                        "hfs+"
                    }
                    .to_string(),
                    parent_path: "/".to_string(),
                    // No catalog record survives: the CNID is the only
                    // name the evidence supports (same policy as the
                    // ext engine's orphan inodes).
                    name: format!("cnid_{fork_id}"),
                    size,
                    start_lba: extent_list.first().map(|e| e.start_byte / 512),
                    mtime: None,
                    ctime: None,
                    atime: None,
                    status: dw_core::RecoveryStatus::PartiallyRecovered,
                    extension: String::new(),
                    mime: None,
                    confidence: 800,
                    was_deleted: true,
                    cluster_chain: extents.iter().map(|e| u64::from(e.start_block)).collect(),
                    extents: extent_list,
                    resident_data: None,
                });
                orphaned += 1;
            }
            if orphaned > 0 {
                info!(orphaned, "HFS+ orphaned overflow fragments surfaced");
            }

            let carved =
                deep_carve_free_blocks(&source, &vh, &results, &progress, scan_id, &cancel)
                    .await
                    .unwrap_or(0);
            info!(
                entries = outcome.entries,
                carved, "HFS+ deep scan complete (catalog + overflow + carving)"
            );
        } else {
            info!(
                entries = outcome.entries,
                "HFS+ quick scan complete (catalog + overflow completion)"
            );
        }

        Ok(())
    }
}

/// Deep pass: carve the free allocation blocks.
async fn deep_carve_free_blocks(
    source: &Arc<dyn DiskSource>,
    vh: &VolumeHeader,
    results: &Arc<ResultsStore>,
    _progress: &Arc<dyn ProgressSink>,
    _scan_id: uuid::Uuid,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Option<u64> {
    let read_at = source.as_read_at();
    let bs = u64::from(vh.block_size);
    if bs == 0 {
        return None;
    }

    // Read the allocation bitmap from its fork's inline extents.
    let fork = &vh.allocation_file;
    let bytes_needed = (vh.total_blocks as u64).div_ceil(8);
    let mut bitmap = vec![0u8; bytes_needed as usize];
    let mut filled = 0usize;
    for ext in &fork.extents {
        if filled >= bitmap.len() {
            break;
        }
        let base = u64::from(ext.start_block) * bs;
        let end = (filled + (ext.block_count as usize) * bs as usize).min(bitmap.len());
        if read_at.read_at(base, &mut bitmap[filled..end]).is_err() {
            warn!("allocation bitmap read failed; skipping deep carve");
            return None;
        }
        filled = end;
    }
    if filled < bitmap.len() {
        warn!("allocation bitmap shorter than needed; skipping deep carve");
        return None;
    }

    // Free runs: bit i (LSB-first, Linux-driver convention) = block i+...?
    // HFS+ allocation bitmap: bit N corresponds to allocation block N,
    // LSB of byte 0 = block 0.
    let mut free_count: u64 = 0;
    let mut runs: Vec<(u64, u64)> = Vec::new(); // (start_byte, len)
    let total = vh.total_blocks;
    let mut block: u32 = 0;
    while block < total {
        let bit = (bitmap[(block / 8) as usize] >> (block % 8)) & 1;
        if bit == 0 {
            let start = block;
            while block < total && ((bitmap[(block / 8) as usize] >> (block % 8)) & 1) == 0 {
                block += 1;
            }
            let len_blocks = u64::from(block - start);
            free_count += len_blocks;
            let start_byte = u64::from(start) * bs;
            let len_bytes = len_blocks * bs;
            if let Some(last) = runs.last_mut()
                && last.0 + last.1 == start_byte
            {
                last.1 += len_bytes;
            } else {
                runs.push((start_byte, len_bytes));
            }
        } else {
            block += 1;
        }
    }

    // Sanity check against the header's freeBlocks. Real volumes can
    // drift by a block or two (lazy header updates); only a substantial
    // disagreement signals a corrupt bitmap.
    let header_free = u64::from(vh.free_blocks);
    let tolerance = 1 + u64::from(vh.total_blocks) / 1000;
    if free_count.abs_diff(header_free) > tolerance {
        warn!(
            computed = free_count,
            header = header_free,
            "allocation bitmap free count disagrees with the volume header"
        );
    } else {
        tracing::debug!(
            computed = free_count,
            header = header_free,
            "bitmap free count within tolerance"
        );
    }

    if runs.is_empty() {
        return Some(0);
    }

    let db = dw_signatures::SignatureDb::from_embedded();
    let mut carved_total = 0u64;
    for &(start, len) in &runs {
        let carved = dw_carver::header_footer::carve_region(
            read_at.as_ref(),
            start,
            len,
            &db,
            results,
            Some(cancel),
            None, // sync per-chunk progress: plugin reports around the loop
        );
        carved_total += carved;
    }
    Some(carved_total)
}

fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    let est_entries = total_bytes / (32 * 1024);
    let secs = est_entries / 2_000_000;
    secs.try_into().unwrap_or(u32::MAX)
}

fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    let secs = total_bytes / (500 * 1024 * 1024);
    secs.try_into().unwrap_or(u32::MAX)
}
