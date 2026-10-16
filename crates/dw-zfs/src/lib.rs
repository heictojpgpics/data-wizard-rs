//! dw-zfs: ZFS (OpenZFS on-disk format) parser plugin for data-wizard-rs.
//!
//! Scope (honest, fixture-validated):
//! - single-device pools (one top-level `disk`/`file` vdev); mirrors,
//!   RAID-Z, multi-vdev, gang blocks, and native encryption are out of
//!   scope and fall back to raw carving with the reason recorded
//! - verified reads: fletcher2/fletcher4/SHA-256 block checksums and
//!   fletcher4 label checksums; LZJB/GZIP/ZLE/LZ4/ZSTD decompression
//! - quick pass: live tree walk from the newest uberblock
//! - deep pass: previous-uberblock diff (recently deleted entries keep
//!   names, timestamps, and extents)
//! - physical extent mapping (on-disk DVA ranges); logical
//!   reconstruction of compressed streams is a recovery-writer feature

pub mod blkptr;
pub mod checksum;
pub mod compress;
pub mod dmu;
pub mod error;
pub mod label;
pub mod nvlist;
pub mod plugin;
pub mod sa;
pub mod walker;
pub mod zap;

pub use error::{ZfsError, ZfsResult};
pub use plugin::ZfsPlugin;
