//! FsPlugin impl stub. To be filled in with real parser logic.

use async_trait::async_trait;
use dw_core::{CoreResult, FsPlugin, ProbeOutcome, ProgressSink, ResultsStore, ScanMode};
use dw_disk::DiskSource;
use std::sync::Arc;

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
        "Btrfs parser (stub)"
    }
    async fn probe(&self, _source: &dyn DiskSource) -> CoreResult<ProbeOutcome> {
        // TODO: real probe logic
        Ok(ProbeOutcome::No)
    }
    async fn scan(
        &self,
        _source: Arc<dyn DiskSource>,
        _mode: ScanMode,
        _results: Arc<ResultsStore>,
        _progress: Arc<dyn ProgressSink>,
        _cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> CoreResult<()> {
        // TODO: real scan logic
        Ok(())
    }
}
