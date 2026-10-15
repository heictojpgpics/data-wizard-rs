//! # dw-btrfs
//!
//! Btrfs filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from wiki.btrfs and the Linux
//! kernel `fs/btrfs/`):
//!
//!  1. **Probe**: Read the 64 KiB boot superblock at offset 64 KiB on
//!     each device. Verify magic `_BHRfS_M` (0x4D5F536648425F). Parse
//!     chunk-tree root, root-tree root, default subvolume.
//!  2. **Quick scan**:
//!     - Walk the root tree (ROOT_TREE_OBJECTID) to enumerate every
//!       subvolume and file tree.
//!     - For each file tree: walk INODE_ITEM / INODE_REF /
//!       FILE_EXTENT_ITEM records.
//!  3. **Deep scan**:
//!     - Use the checksum tree (CSUM_TREE_OBJECTID) to find logical
//!       extents that have a checksum but no FILE_EXTENT_ITEM pointing
//!       at them -- these are orphaned file-data extents.
//!     - Apply raw carving on unallocated chunks.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod superblock;
pub mod chunk;
pub mod root;
pub mod fs_tree;
pub mod plugin;
pub mod error;

pub use error::{BtrfsError, BtrfsResult};
pub use plugin::BtrfsPlugin;
