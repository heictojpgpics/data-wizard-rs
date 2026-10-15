//! FsPlugin impl for NTFS.
//!
//! The probe pass reads the VBR and verifies the NTFS OEM ID. The scan
//! pass is stubbed for now — MFT walking is the next implementation
//! milestone (see `docs/algorithms/ntfs-recovery.md`).

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;

use crate::boot::Vbr;

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
        _source: Arc<dyn DiskSource>,
        _mode: ScanMode,
        _results: Arc<ResultsStore>,
        _progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {
        // TODO: implement MFT walk + carving. See docs/algorithms/ntfs-recovery.md.
        Ok(())
    }
}

/// Rough ETA for the quick scan (metadata walk). Assumes ~1 M MFT
/// records per second on a modern SSD.
fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    // NTFS typically allocates 1 MFT record per ~8 KiB of disk, so:
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
