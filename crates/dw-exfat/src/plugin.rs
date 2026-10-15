//! FsPlugin impl for exFAT.
//!
//! The probe pass reads the boot sector and verifies the "EXFAT   " OEM ID.
//! The scan pass walks the directory tree (quick phase); in Deep mode it
//! additionally carves the *unallocated* regions computed from the
//! allocation bitmap — deleted files surface either through surviving
//! directory entries or through the carver, never by touching allocated
//! clusters.

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;

use crate::bitmap::ExfatBitmap;
use crate::boot::ExfatBoot;
use crate::fat::ExfatFat;
use crate::walker::{self, WalkOutcome};

pub struct ExfatPlugin;

impl Default for ExfatPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for ExfatPlugin {
    fn id(&self) -> &'static str {
        "exfat"
    }

    fn name(&self) -> &'static str {
        "exFAT parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("exfat probe read: {e}")))?;

        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);

        match ExfatBoot::parse(&buf) {
            Ok(boot) => {
                let probe = FsProbe {
                    fs_kind: "exfat".to_string(),
                    volume_label: None, // exFAT stores the label in the root directory, not the boot sector
                    volume_serial: Some(format!("{:08X}", boot.volume_serial_number)),
                    total_bytes: boot.total_bytes(),
                    cluster_size: u64::from(boot.bytes_per_cluster),
                    estimated_quick_seconds: Some(estimate_quick_seconds(boot.total_bytes())),
                    estimated_deep_seconds: Some(estimate_deep_seconds(boot.total_bytes())),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(crate::error::ExfatError::NotExfat) => Ok(ProbeOutcome::No),
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("exFAT boot parse: {e}"))),
        }
    }

    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {
        // Re-read the boot sector.
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("exfat scan boot read: {e}")))?;
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);
        let boot = ExfatBoot::parse(&buf)
            .map_err(|e| dw_core::CoreError::Plugin(format!("exfat scan boot parse: {e}")))?;

        let fat = walker::read_fat(source.as_ref(), &boot)?;

        let outcome: WalkOutcome = walker::walk_exfat(
            source.clone(),
            &boot,
            &fat,
            results.clone(),
            progress.clone(),
        )
        .await?;
        let entries = outcome.entries;

        if mode == ScanMode::Deep {
            // Deep phase: carve exactly the unallocated regions.
            let carved =
                deep_carve_unallocated(&source, &boot, &fat, &outcome, &results, &progress)
                    .await
                    .unwrap_or(0);
            tracing::info!(entries, carved, "exFAT deep scan complete (walk + carving)");
        } else {
            tracing::info!(entries, "exFAT quick scan complete (walk only)");
        }

        Ok(())
    }
}

fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    let est_entries = total_bytes / (16 * 1024);
    let secs = est_entries / 2_000_000;
    secs.try_into().unwrap_or(u32::MAX)
}

fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    let secs = total_bytes / (500 * 1024 * 1024);
    secs.try_into().unwrap_or(u32::MAX)
}

/// Deep-scan pass: carve the unallocated regions only.
///
/// Returns the number of carved entries (0 when no usable bitmap exists —
/// never a whole-volume carve: without the allocation bitmap we cannot
/// know which regions are free, and carving allocated regions produces
/// duplicates of live files).
async fn deep_carve_unallocated(
    source: &Arc<dyn DiskSource>,
    boot: &ExfatBoot,
    fat: &ExfatFat,
    outcome: &WalkOutcome,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
) -> Option<u64> {
    let bitmap_entry = outcome.bitmap.as_ref()?;
    let read_at = source.as_read_at();

    let data = walker::read_bitmap_data(&read_at, boot, fat, bitmap_entry).ok()?;
    let bitmap = ExfatBitmap::new(data, boot.cluster_count).ok()?;
    let runs = bitmap.free_runs();
    let extents = walker::free_runs_to_extents(boot, &runs);
    if extents.is_empty() {
        tracing::debug!("exFAT deep scan: volume has no free clusters");
        return Some(0);
    }

    let db = dw_signatures::SignatureDb::from_embedded();
    let scan_id = uuid::Uuid::now_v7();
    let mut carved_total = 0u64;
    for ext in extents {
        let carved = dw_carver::header_footer::carve_region(
            read_at.as_ref(),
            ext.start_byte,
            ext.length,
            &db,
            results,
            progress,
            scan_id,
            1 << 20,
        )
        .unwrap_or(0);
        carved_total += carved;
    }
    Some(carved_total)
}
