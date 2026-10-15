//! FsPlugin impl for FAT12/16/32.

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use tracing::info;

use crate::bpb::{FatBpb, FatKind};
use crate::walker::walk_fat;

pub struct FatPlugin;

impl Default for FatPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for FatPlugin {
    fn id(&self) -> &'static str {
        "fat"
    }

    fn name(&self) -> &'static str {
        "FAT12/16/32 parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("fat probe read: {e}")))?;

        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);

        match FatBpb::parse(&buf) {
            Ok(bpb) => {
                let kind_str = match bpb.kind {
                    FatKind::Fat12 => "fat12",
                    FatKind::Fat16 => "fat16",
                    FatKind::Fat32 => "fat32",
                };
                let total_bytes = bpb.total_sectors * u64::from(bpb.bytes_per_sector);
                let probe = FsProbe {
                    fs_kind: kind_str.to_string(),
                    volume_label: bpb.volume_label.clone(),
                    volume_serial: bpb.volume_id.map(|v| format!("{:08X}", v)),
                    total_bytes,
                    cluster_size: u64::from(bpb.bytes_per_cluster),
                    estimated_quick_seconds: Some(estimate_quick_seconds(total_bytes)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(total_bytes)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(e) => {
                tracing::debug!("fat probe rejected: {e}");
                Ok(ProbeOutcome::No)
            }
        }
    }

    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {
        // Re-read the BPB.
        let bytes = source
            .read_at_async(0, 512)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("fat scan BPB read: {e}")))?;
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&bytes);
        let bpb = FatBpb::parse(&buf)
            .map_err(|e| dw_core::CoreError::Plugin(format!("fat scan BPB parse: {e}")))?;

        info!(kind = ?bpb.kind, mode = ?mode, "Starting FAT scan");

        let entries = walk_fat(source, &bpb, mode, results.clone(), progress.clone()).await?;

        info!(entries, "FAT scan complete");
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
