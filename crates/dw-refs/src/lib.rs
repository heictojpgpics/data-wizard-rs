//! dw-refs: ReFS parser plugin for data-wizard-rs.
//!
//! Scope (honest, validated against a real Windows-formatted ReFS 3.14
//! volume generated on CI):
//! - boot sector (FSRS) validation: magic, geometry (512 B sectors, 4/64
//!   KiB clusters), version 3.x; no checksum claim (the v3.14 coverage
//!   could not be pinned, so none is made)
//! - signature-scoped record scan: name records and file-info records,
//!   joined by file id - exact sizes and the four FILETIME timestamps
//! - deep pass: ids that only appear in old-generation (headless) page
//!   copies surface as deleted
//! - extents are not decoded in v1 (data-run table walk is the roadmap);
//!   entries are metadata-only

pub mod boot;
pub mod error;
pub mod page;
pub mod plugin;
pub mod record;
pub mod walker;

pub use plugin::RefsPlugin;
