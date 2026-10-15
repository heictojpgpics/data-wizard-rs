//! FsPlugin impl for ext2/3/4.

use std::sync::Arc;

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use uuid::Uuid;

use crate::superblock::{SUPER_OFFSET, SuperBlock};
use crate::walker::{self, ExtVolume};

pub struct ExtPlugin;

impl Default for ExtPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for ExtPlugin {
    fn id(&self) -> &'static str {
        "ext"
    }
    fn name(&self) -> &'static str {
        "ext2/3/4 parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let read_at = source.as_read_at();
        let mut buf = [0u8; 1024];
        if read_at.read_at(SUPER_OFFSET, &mut buf).is_err() {
            return Ok(ProbeOutcome::No);
        }
        match SuperBlock::parse(&buf) {
            Ok(sb) => {
                let total_bytes = sb.blocks_count * u64::from(sb.block_size);
                let probe = FsProbe {
                    fs_kind: sb.fs_kind().to_string(),
                    volume_label: if sb.volume_name.is_empty() {
                        None
                    } else {
                        Some(sb.volume_name.clone())
                    },
                    volume_serial: Some(sb.volume_uuid_string.clone()),
                    total_bytes,
                    cluster_size: u64::from(sb.block_size),
                    estimated_quick_seconds: Some(estimate_quick_seconds(total_bytes)),
                    estimated_deep_seconds: Some(estimate_deep_seconds(total_bytes)),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(crate::error::ExtError::NotExt) => Ok(ProbeOutcome::No),
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("ext superblock: {e}"))),
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
        // The volume is re-validated inside scan_ext (probe/scan TOCTOU
        // would otherwise trust a stale superblock).
        let read_at = source.as_read_at();
        match ExtVolume::read(read_at.as_ref()) {
            Ok(_) => {}
            Err(crate::error::ExtError::NotExt) => {
                return Err(dw_core::CoreError::Plugin(
                    "ext scan: source is not an ext volume".into(),
                ));
            }
            Err(e) => {
                return Err(dw_core::CoreError::Plugin(format!("ext open: {e}")));
            }
        }

        let scan_id = Uuid::now_v7();
        walker::scan_ext(scan_id, source, mode, results, progress, cancel)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("ext scan: {e}")))
    }
}

fn estimate_quick_seconds(total_bytes: u64) -> u32 {
    // Metadata walk: dominated by inode-table reads; ~50 MB/s worst case
    // for metadata plus directory blocks on spinning disks.
    let est = total_bytes / (50 * 1024 * 1024);
    est.try_into().unwrap_or(u32::MAX)
}

fn estimate_deep_seconds(total_bytes: u64) -> u32 {
    // Free-space carving at the measured ~230 MiB/s aggregate rate plus
    // the metadata pass.
    let carve = total_bytes / (230 * 1024 * 1024);
    let meta = total_bytes / (50 * 1024 * 1024);
    (carve + meta).try_into().unwrap_or(u32::MAX)
}
