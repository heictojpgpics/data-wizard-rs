//! FsPlugin impl for Btrfs.

use std::sync::Arc;

use crate::superblock::SuperBlock;
use crate::walker;
use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;

pub struct BtrfsPlugin;

impl Default for BtrfsPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for BtrfsPlugin {
    fn id(&self) -> &'static str {
        "btrfs"
    }
    fn name(&self) -> &'static str {
        "Btrfs parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let read_at = source.as_read_at();
        match SuperBlock::read_best(read_at.as_ref()) {
            Some((sb, _)) => {
                // Only single-device, non-RAID geometry is in scope; say
                // so instead of half-parsing.
                if sb.num_devices != 1 {
                    // No Unsupported variant exists by design; a volume we
                    // cannot honestly scan falls back to raw carving with
                    // the reason recorded.
                    return Ok(ProbeOutcome::Corrupt(format!(
                        "btrfs with {} devices: out of scope, raw carving only",
                        sb.num_devices
                    )));
                }
                let probe = FsProbe {
                    fs_kind: "btrfs".to_string(),
                    volume_label: if sb.label.is_empty() {
                        None
                    } else {
                        Some(sb.label.clone())
                    },
                    volume_serial: Some(format_uuid(&sb.fsid)),
                    total_bytes: sb.total_bytes,
                    cluster_size: u64::from(sb.sectorsize),
                    estimated_quick_seconds: Some(estimate_quick_seconds(sb.total_bytes)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(sb.total_bytes)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            None => {
                // Distinguish "not btrfs" from "btrfs but broken/unsupported".
                // read_best already tried every copy; check the magic raw.
                let mut buf = vec![0u8; 4096];
                if read_at.size() >= 65_536 + 4096
                    && read_at.read_at(65_536, &mut buf).is_ok()
                    && buf.get(0x40..0x48) == Some(&crate::superblock::BTRFS_MAGIC.to_le_bytes())
                {
                    Ok(ProbeOutcome::Corrupt(
                        "btrfs superblock found but no copy verifies (csum/profile)".into(),
                    ))
                } else {
                    Ok(ProbeOutcome::No)
                }
            }
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
        let Some((sb, _)) = SuperBlock::read_best(read_at.as_ref()) else {
            return Err(dw_core::CoreError::Plugin(
                "btrfs scan: no verifiable superblock".into(),
            ));
        };
        walker::scan_btrfs(source, sb, mode, results, progress, cancel)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("btrfs scan: {e}")))
    }
}

fn format_uuid(u: &[u8; 16]) -> String {
    let h = |r: std::ops::Range<usize>| -> String {
        u[r].iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    format!(
        "{}-{}-{}-{}-{}",
        h(0..4),
        h(4..6),
        h(6..8),
        h(8..10),
        h(10..16)
    )
}

fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    (total_bytes / (50 * 1024 * 1024))
        .try_into()
        .unwrap_or(u32::MAX)
}
fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    (total_bytes / (230 * 1024 * 1024) + total_bytes / (50 * 1024 * 1024))
        .try_into()
        .unwrap_or(u32::MAX)
}
