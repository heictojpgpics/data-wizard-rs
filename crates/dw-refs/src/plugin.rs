//! FsPlugin impl for ReFS.

use crate::boot::BootSector;
use crate::walker::scan_refs;
use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, FsProbe, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct RefsPlugin;

impl Default for RefsPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl FsPlugin for RefsPlugin {
    fn id(&self) -> &'static str {
        "refs"
    }
    fn name(&self) -> &'static str {
        "ReFS parser"
    }

    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        let read = source.as_read_at();
        let mut boot = vec![0u8; 512];
        if read.size() < 512 || read.read_at(0, &mut boot).is_err() {
            return Ok(ProbeOutcome::No);
        }
        match BootSector::parse(&boot) {
            Ok(b) => {
                let total = read.size();
                let probe = FsProbe {
                    fs_kind: "refs".to_string(),
                    volume_label: None,
                    volume_serial: Some(format!("{:016x}", b.serial)),
                    total_bytes: total,
                    cluster_size: b.cluster_size(),
                    estimated_quick_seconds: Some(
                        (total / (40 * 1024 * 1024)).try_into().unwrap_or(u32::MAX),
                    ),
                    estimated_deep_seconds: Some(
                        (total / (40 * 1024 * 1024) * 2)
                            .try_into()
                            .unwrap_or(u32::MAX),
                    ),
                };
                Ok(ProbeOutcome::Yes(probe))
            }
            Err(crate::error::RefsError::NotRefs) => Ok(ProbeOutcome::No),
            Err(e) => Ok(ProbeOutcome::Corrupt(format!("refs probe: {e}"))),
        }
    }

    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
        cancel: Arc<AtomicBool>,
    ) -> CoreResult<()> {
        scan_refs(source, mode, results, progress, cancel)
            .await
            .map_err(|e| dw_core::CoreError::Plugin(format!("refs scan: {e}")))
    }
}
