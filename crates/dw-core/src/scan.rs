//! Scan lifecycle and configuration.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    CoreError, CoreResult,
    plugin::{FsPlugin, ProbeOutcome, ScanMode},
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
    pub source: Arc<dyn dw_disk::DiskSource>,
    pub plugins: Vec<Arc<dyn FsPlugin>>,
    pub progress: Arc<dyn ProgressSink>,
}

impl Scan {
    #[must_use]
    pub fn new(
        handle: ScanHandle,
        source: Arc<dyn dw_disk::DiskSource>,
        plugins: Vec<Arc<dyn FsPlugin>>,
        progress: Arc<dyn ProgressSink>,
    ) -> Self {
        Self {
            handle,
            source,
            plugins,
            progress,
        }
    }

    /// Run the scan to completion (or until cancelled).
    ///
    /// Orchestrates:
    /// 1. Probing — ask each plugin if it recognizes the source.
    /// 2. Quick scan — run the matching plugin's scan method.
    /// 3. Deep scan — if mode == Deep, the plugin's scan method also
    ///    runs carving (the plugin handles this internally).
    /// 4. Finalizing — mark the scan as Done.
    ///
    /// If no plugin claims the source, we fall back to raw carving
    /// (TODO — not yet implemented).
    pub async fn run(self) -> CoreResult<()> {
        let mode = self.handle.config().mode;

        // ---- Phase 1: Probing ----
        self.handle.set_state(ScanState::Probing);
        info!(scan_id = %self.handle.id(), "Scan probing phase");

        let mut matched_plugin: Option<Arc<dyn FsPlugin>> = None;
        for plugin in &self.plugins {
            match plugin.probe(self.source.as_ref()).await {
                Ok(ProbeOutcome::Yes(probe)) => {
                    info!(
                        plugin = plugin.id(),
                        fs_kind = %probe.fs_kind,
                        "Plugin claimed the source"
                    );
                    matched_plugin = Some(Arc::clone(plugin));
                    break;
                }
                Ok(ProbeOutcome::No) => {
                    info!(plugin = plugin.id(), "Plugin rejected the source");
                }
                Ok(ProbeOutcome::Corrupt(msg)) => {
                    warn!(plugin = plugin.id(), reason = %msg, "Plugin found corrupt structure");
                }
                Err(e) => {
                    warn!(plugin = plugin.id(), error = %e, "Plugin probe errored");
                }
            }
        }

        let Some(plugin) = matched_plugin else {
            // No plugin claimed the source — fall back to raw carving.
            // TODO: implement raw carving fallback via dw-carver.
            warn!("No plugin claimed the source; raw carving not yet implemented");
            self.handle.set_state(ScanState::Done);
            return Ok(());
        };

        // Check for cancellation before starting the scan
        if self.handle.state() == ScanState::Cancelled {
            return Err(CoreError::Cancelled);
        }

        // ---- Phase 2: Scan ----
        let state = match mode {
            ScanMode::Quick => ScanState::QuickScan,
            ScanMode::Deep => ScanState::DeepScan,
        };
        self.handle.set_state(state);

        let results = self.handle.results();
        let result = plugin
            .scan(
                Arc::clone(&self.source),
                mode,
                results,
                Arc::clone(&self.progress),
            )
            .await;

        // Check for cancellation
        if self.handle.state() == ScanState::Cancelled {
            return Err(CoreError::Cancelled);
        }

        match result {
            Ok(()) => {
                self.handle.set_state(ScanState::Finalizing);
                self.handle.set_state(ScanState::Done);
                self.progress.done().await;
                info!(scan_id = %self.handle.id(), "Scan completed successfully");
                Ok(())
            }
            Err(e) => {
                self.handle.set_state(ScanState::Failed);
                self.progress.fail(e.to_string()).await;
                Err(e)
            }
        }
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
