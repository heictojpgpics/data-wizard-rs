//! FsPlugin impl for NTFS.
//!
//! The probe pass reads the VBR and verifies the NTFS OEM ID. The scan
//! pass runs the MFT walker (quick mode) and optionally the carver
//! (deep mode).

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use tracing::info;

use crate::boot::Vbr;
use crate::walker::{WalkConfig, walk_mft};

pub struct NtfsPlugin;

impl Default for NtfsPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for NtfsPlugin {
    fn id(&self) -> &'static str {
        "ntfs"
    }

    fn name(&self) -> &'static str {
        "NTFS 3.x parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        // Read the first 512 bytes (the VBR).
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("ntfs probe read: {e}")))?;

        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);

        match Vbr::parse(&buf) {
            Ok(vbr) => {
                let probe = FsProbe {
                    fs_kind: "ntfs".to_string(),
                    volume_label: None, // NTFS stores the label in $Volume, not in the VBR
                    volume_serial: Some(format!("{:016X}", vbr.volume_serial_number)),
                    total_bytes: vbr.total_bytes,
                    cluster_size: u64::from(vbr.bytes_per_cluster),
                    estimated_quick_seconds: Some(estimate_quick_seconds(vbr.total_bytes)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(vbr.total_bytes)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(crate::boot::VbrError::NotNtfs(_, _)) => Ok(ProbeOutcome::No),
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("NTFS VBR parse: {e}"))),
        }
    }

    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {
        // Re-read the VBR to get the MFT offset + record size.
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("ntfs scan VBR read: {e}")))?;
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);
        let vbr = Vbr::parse(&buf)
            .map_err(|e| dw_core::CoreError::Plugin(format!("ntfs scan VBR parse: {e}")))?;

        info!(
            mft_offset = vbr.mft_offset_bytes(),
            record_size = vbr.mft_record_size_bytes,
            mode = ?mode,
            "Starting NTFS scan"
        );

        let config = WalkConfig::default();
        let entries = walk_mft(
            Arc::clone(&source),
            &vbr,
            mode,
            results.clone(),
            progress.clone(),
            &config,
        )
        .await?;

        info!(entries, "NTFS MFT walk complete");

        // For deep scans, run the carver on the entire volume to find
        // deleted files that are no longer in the MFT.
        if mode == ScanMode::Deep {
            info!("Starting carving pass (deep scan)");
            let db = dw_signatures::SignatureDb::from_embedded();
            let read_at = source.as_read_at();
            let carved = dw_carver::header_footer::carve_region(
                read_at.as_ref(),
                0,
                vbr.total_bytes,
                &db,
                &results,
                &progress,
                uuid::Uuid::now_v7(),
                1 << 20, // 1 MiB chunks
            )
            .map_err(|e| dw_core::CoreError::Plugin(format!("carver error: {e}")))?;
            info!(entries, carved, "NTFS deep scan complete (MFT + carving)");
        } else {
            info!(entries, "NTFS quick scan complete (MFT only)");
        }

        Ok(())
    }
}

/// Rough ETA for the quick scan (metadata walk). Assumes ~1 M MFT
/// records per second on a modern SSD.
fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    let est_records = total_bytes / (8 * 1024);
    let secs = est_records / 1_000_000;
    secs.try_into().unwrap_or(u32::MAX)
}

/// Rough ETA for the deep scan (metadata + carving). Assumes ~500 MB/s
/// carving throughput.
fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    let secs = total_bytes / (500 * 1024 * 1024);
    secs.try_into().unwrap_or(u32::MAX)
}
