//! # dw-core
//!
//! Core scan orchestration for `data-wizard-rs`.
//!
//! Responsibilities:
//!  - Drive a [`Scan`] through its lifecycle
//!    (`Created -> Probing -> QuickScan -> DeepScan -> Finalized`).
//!  - Own the [`ResultsStore`] -- a memory-bounded, disk-backed store of recovered
//!    file entries.
//!  - Schedule work across filesystem-parser plugins ([`FsPlugin`]) and the carver.
//!  - Emit progress events through a typed channel ([`ProgressEvent`]).
//!
//! Design notes (clean-room, derived from observable architecture of reference
//! products):
//!  - The reference product splits each filesystem parser into a "quick"
//!    (metadata-only) and a "full" (metadata + carving) pass. We mirror that via
//!    [`FsPlugin::quick_scan`] and [`FsPlugin::deep_scan`].
//!  - The reference product exposes the scanner via a factory function returning
//!    an opaque handle (e.g. `EUCreateNTFSSearcher`). We use a trait object
//!    instead -- safer and idiomatic Rust.
//!  - We do NOT split the engine into a separate Windows service yet. Raw-disk
//!    handles are opened lazily inside the Tauri app, with elevation requested
//!    via UAC only when needed. A privileged-helper split is planned for later.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod checkpoint;
pub mod classify;
pub mod error;
pub mod extents;
pub mod fat_timestamp;
pub mod plugin;
pub mod progress;
pub mod recovery;
pub mod results;
pub mod scan;

pub use checkpoint::{
    CHECKPOINT_VERSION, CheckpointStore, CheckpointSummary, ScanCheckpoint, verify_source_matches,
};
pub use error::{CoreError, CoreResult};
pub use extents::merge_and_trim_offsets;
pub use fat_timestamp::{decode_fat_access_date, decode_fat_datetime};
pub use plugin::{FsPlugin, FsProbe, ProbeOutcome, ScanMode};
pub use progress::{ProgressEvent, ProgressPhase, ProgressSink};
pub use recovery::{CollisionPolicy, RecoveryError, RecoveryOutcome, recover_entry};
pub use results::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
pub use scan::{Scan, ScanConfig, ScanHandle, ScanOutcome, ScanState};
