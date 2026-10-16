//! FsPlugin impl for ZFS: single-device (disk or file vdev) pools.

use crate::error::ZfsError;
use crate::label::load_labels;
use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct ZfsPlugin;

impl Default for ZfsPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for ZfsPlugin {
    fn id(&self) -> &'static str {
        "zfs"
    }
    fn name(&self) -> &'static str {
        "ZFS parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let read_at = source.as_read_at();
        let cancel = Arc::new(AtomicBool::new(false));
        match load_labels(read_at.as_ref(), read_at.size(), &cancel) {
            Ok(set) => {
                let cfg = set
                    .labels
                    .iter()
                    .find(|l| l.nvlist_valid)
                    .map(|l| &l.config)
                    .expect("load_labels returned at least one valid label");
                if !cfg.single_leaf {
                    let reason = if cfg.top_level_vdevs > 1 {
                        format!("pool spans {} top-level vdevs", cfg.top_level_vdevs)
                    } else {
                        "single top-level vdev is a group (mirror/raidz)".to_string()
                    };
                    // No Unsupported variant exists by design; a volume we
                    // cannot honestly scan falls back to raw carving with
                    // the reason recorded.
                    return Ok(ProbeOutcome::Corrupt(format!(
                        "zfs pool {0:?}: {reason}, raw carving only",
                        cfg.name
                    )));
                }
                let total = read_at.size();
                let probe = FsProbe {
                    fs_kind: "zfs".to_string(),
                    volume_label: Some(cfg.name.clone()),
                    volume_serial: Some(format!("pool-guid-{:016x}", cfg.pool_guid)),
                    total_bytes: total,
                    cluster_size: 512u64 << cfg.ashift.clamp(9, 16).saturating_sub(9),
                    estimated_quick_seconds: Some(estimate_quick_seconds(total)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(total)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(ZfsError::SourceTooSmall { .. }) => Ok(ProbeOutcome::No),
            Err(ZfsError::NoValidLabel) | Err(ZfsError::NoUberblock) => {
                // Raw 0x00bab10c without a verifying label: not ours.
                Ok(ProbeOutcome::No)
            }
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("zfs probe: {e}"))),
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
        let read_at_probe = source.as_read_at();
        let labels = load_labels(read_at_probe.as_ref(), read_at_probe.size(), &cancel)
            .map_err(|e| dw_core::CoreError::Plugin(format!("zfs scan: labels: {e}")))?;
        crate::walker::scan_zfs(source, labels, mode, results, progress, cancel)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("zfs scan: {e}")))
    }
}

fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    (total_bytes / (40 * 1024 * 1024))
        .try_into()
        .unwrap_or(u32::MAX)
}
fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    (total_bytes / (40 * 1024 * 1024) + total_bytes / (40 * 1024 * 1024))
        .try_into()
        .unwrap_or(u32::MAX)
}
