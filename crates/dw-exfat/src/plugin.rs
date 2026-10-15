//! FsPlugin impl stub. To be filled in with real parser logic.

use std::sync::Arc;
use async_trait::async_trait;
use dw_core::{FsPlugin, ProbeOutcome, ScanMode, ProgressSink, ResultsStore, CoreResult};
use dw_disk::DiskSource;

pub struct ExfatPlugin;

impl Default for ExfatPlugin {
    fn default() -> Self { Self }
}

#[async_trait]
impl FsPlugin for ExfatPlugin {
    fn id(&self) -> &'static str { "exfat" }
    fn name(&self) -> &'static str { "exFAT parser (stub)" }
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
    ) -> CoreResult<()> {
        // TODO: real scan logic
        Ok(())
    }
}
