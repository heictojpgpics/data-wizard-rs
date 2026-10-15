//! # dw-hfs
//!
//! HFS+ filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from Apple Technical Note 1150
//! "HFS Plus Volume Format" + the `hfsutils` / Linux `hfsplus` sources):
//!
//!  1. **Probe**: Read Volume Header at offset 1024 (magic 'H+' = 0x482B
//!     for HFS+, 'HX' for case-sensitive HFS+). Parse allocation bitmap,
//!     extents-overflow file, catalog file B-tree header.
//!  2. **Quick scan**:
//!     - Walk the catalog B-tree (CNID 4) to enumerate every file &
//!       folder thread record.
//!     - Deleted records leave gaps in the B-tree nodes -- nodes marked
//!       free in the B-tree header's free-nodes bitmap but whose keys
//!       are still readable.
//!  3. **Deep scan**:
//!     - Walk the extents-overflow B-tree (CNID 3) to find orphaned
//!       extents (fragments of files whose catalog record was deleted
//!       but whose extents are still indexed).
//!     - Apply raw carving on unallocated allocation blocks.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod volumeheader;
pub mod btree;
pub mod catalog;
pub mod extents;
pub mod plugin;
pub mod error;

pub use error::{HfsError, HfsResult};
pub use plugin::HfsPlugin;
