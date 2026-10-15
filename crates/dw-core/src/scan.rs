//! Scan lifecycle and configuration.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    plugin::{FsPlugin, ScanMode},
    progress::ProgressSink,
    results::ResultsStore,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    pub source_uri: String,
    pub mode: ScanMode,
    pub max_results: usize,
    pub include_carving: bool,
    pub file_filter: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScanState {
    Created,
    Probing,
    QuickScan,
    DeepScan,
    Finalizing,
    Done,
    Cancelled,
    Failed,
}

#[derive(Clone)]
pub struct ScanHandle {
    inner: Arc<ScanInner>,
}

struct ScanInner {
    id: Uuid,
    config: RwLock<ScanConfig>,
    state: RwLock<ScanState>,
    results: Arc<ResultsStore>,
}

impl ScanHandle {
    #[must_use]
    pub fn new(config: ScanConfig) -> Self {
        let id = Uuid::now_v7();
        let capacity = config.max_results.max(10_000);
        Self {
            inner: Arc::new(ScanInner {
                id,
                config: RwLock::new(config),
                state: RwLock::new(ScanState::Created),
                results: ResultsStore::new(capacity, None),
            }),
        }
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.inner.id
    }

    #[must_use]
    pub fn state(&self) -> ScanState {
        *self.inner.state.read()
    }

    #[must_use]
    pub fn results(&self) -> Arc<ResultsStore> {
        Arc::clone(&self.inner.results)
    }

    pub fn set_state(&self, state: ScanState) {
        *self.inner.state.write() = state;
    }

    pub fn config(&self) -> ScanConfig {
        self.inner.config.read().clone()
    }
}

/// A high-level scan driver. The Tauri command layer constructs a `Scan`,
/// drives it through its phases, and emits progress events.
pub struct Scan {
    pub handle: ScanHandle,
    pub plugins: Vec<Arc<dyn FsPlugin>>,
    pub progress: Arc<dyn ProgressSink>,
}

impl Scan {
    #[must_use]
    pub fn new(
        handle: ScanHandle,
        plugins: Vec<Arc<dyn FsPlugin>>,
        progress: Arc<dyn ProgressSink>,
    ) -> Self {
        Self {
            handle,
            plugins,
            progress,
        }
    }

    /// Run the scan to completion (or until cancelled by dropping the
    /// progress sink). The actual heavy lifting lives in the per-plugin
    /// `scan` methods; this method is the orchestrator.
    pub async fn run(self) -> CoreResult<()> {
        self.handle.set_state(ScanState::Probing);
        // The orchestrator's job is to: pick a plugin, run it, fall back
        // to raw carving if no plugin claims the region. The per-plugin
        // implementations are stubbed for now and will be filled in as
        // each FS-parser crate is built out.
        let _ = self.plugins.len();
        self.handle.set_state(ScanState::Done);
        Ok(())
    }
}

impl ScanConfig {
    #[must_use]
    pub fn quick(source_uri: impl Into<String>) -> Self {
        Self {
            source_uri: source_uri.into(),
            mode: ScanMode::Quick,
            max_results: 100_000,
            include_carving: false,
            file_filter: Vec::new(),
        }
    }

    #[must_use]
    pub fn deep(source_uri: impl Into<String>) -> Self {
        Self {
            source_uri: source_uri.into(),
            mode: ScanMode::Deep,
            max_results: 1_000_000,
            include_carving: true,
            file_filter: Vec::new(),
        }
    }
}

#[allow(clippy::missing_errors_doc)]
pub fn validate_config(cfg: &ScanConfig) -> CoreResult<()> {
    if cfg.source_uri.is_empty() {
        return Err(CoreError::Plugin("source_uri must not be empty".into()));
    }
    if cfg.max_results == 0 {
        return Err(CoreError::Plugin("max_results must be > 0".into()));
    }
    Ok(())
}
