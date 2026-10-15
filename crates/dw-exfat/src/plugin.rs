//! FsPlugin impl for exFAT.
//!
//! The probe pass reads the boot sector and verifies the "EXFAT   " OEM ID.
//! The scan pass is stubbed for now — directory walking is the next
//! implementation milestone (see `docs/algorithms/exfat-recovery.md`).

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;

use crate::boot::ExfatBoot;

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
        _source: Arc<dyn DiskSource>,
        _mode: ScanMode,
        _results: Arc<ResultsStore>,
        _progress: Arc<dyn ProgressSink>,
    ) -> CoreResult<()> {
        // TODO: implement directory walk + bitmap + stream extension parsing.
        // See docs/algorithms/exfat-recovery.md.
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
