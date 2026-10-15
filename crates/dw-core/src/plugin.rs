//! The [`FsPlugin`] trait -- the contract every filesystem parser must
//! implement.

use std::sync::Arc;

use async_trait::async_trait;
use dw_disk::DiskSource;
use serde::{Deserialize, Serialize};

use crate::{CoreResult, progress::ProgressSink, results::ResultsStore};

/// What the probe pass discovered about a given disk region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsProbe {
    /// Filesystem family, e.g. `"ntfs"`, `"fat32"`, `"exfat"`, `"ext4"`.
    pub fs_kind: String,
    /// Volume label if recoverable from the boot sector.
    pub volume_label: Option<String>,
    /// Volume serial / UUID if present.
    pub volume_serial: Option<String>,
    /// Total bytes in the probed region.
    pub total_bytes: u64,
    /// Bytes-per-cluster / allocation unit.
    pub cluster_size: u64,
    /// Quick-scan estimated duration (seconds), if known.
    pub estimated_quick_seconds: Option<u32>,
    /// Deep-scan estimated duration (seconds), if known.
    pub estimated_deep_seconds: Option<u32>,
}

/// Outcome of probing a region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProbeOutcome {
    /// This plugin recognises the region and can scan it.
    Yes(FsProbe),
    /// This plugin does not recognise the region.
    No,
    /// This plugin recognised the magic but the structure is corrupt;
    /// fall back to raw carving only.
    Corrupt(String),
}

/// Which pass to run.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScanMode {
    /// Metadata-only walk of the filesystem's own indexes.
    Quick,
    /// Full scan: metadata walk + raw carving on unallocated regions.
    Deep,
}

/// The contract every filesystem parser implements.
///
/// Designed to mirror the observable "factory + opaque handle" pattern of
/// the reference product (`EUCreate{FS}Searcher` / `EUCreate{FS}QuickSearcher`)
/// in idiomatic Rust: each plugin is a trait object rather than a function
/// returning an opaque pointer.
#[async_trait]
pub trait FsPlugin: Send + Sync {
    /// Short stable identifier, e.g. `"ntfs"`. Used in serialized scan
    /// states and in the UI.
    fn id(&self) -> &'static str;

    /// Human-readable name, e.g. `"NTFS 3.x parser"`.
    fn name(&self) -> &'static str;

    /// Probe a disk region. The plugin should read only the minimum
    /// necessary to decide whether it can handle the region.
    async fn probe(&self, source: &dyn DiskSource) -> CoreResult<ProbeOutcome>;

    /// Run a scan (quick or deep) against `source`, appending recovered
    /// entries to `results` and emitting progress through `progress`.
    ///
    /// `cancel` flips true when the user cancels; long-running inner
    /// loops (deep carving above all) must poll it and bail early
    /// (returning `Err(CoreError::Cancelled)`).
    async fn scan(
        &self,
        source: Arc<dyn DiskSource>,
        mode: ScanMode,
        results: Arc<ResultsStore>,
        progress: Arc<dyn ProgressSink>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> CoreResult<()>;
}
