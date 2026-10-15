//! # dw-ext
//!
//! ext2 / ext3 / ext4 filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from `e2fsprogs` source +
//! Linux kernel `fs/ext4/` + the ext4 documentation in the kernel tree):
//!
//!  1. **Probe**: Read superblock at offset 1024, verify magic 0xEF53,
//!     parse feature flags to determine ext2 vs ext3 vs ext4.
//!  2. **Quick scan**:
//!     - Parse block-group descriptors (32 or 64 bytes each).
//!     - For each block group: walk the inode table, applying the
//!       inode bitmap as a "live" filter.
//!       - Deleted inodes have their bitmap bit clear but the inode
//!         struct on-disk often still has dtime set and the i_blocks
//!         array intact.
//!     - Follow extent trees (ext4) or indirect block maps (ext2/3).
//!  3. **Deep scan**:
//!     - For each block not in any block bitmap as "allocated": apply
//!       raw carving.
//!     - Use the journal (if ext3/4) to recover recently-deleted inodes
//!       whose block pointers were overwritten by the journal commit.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod superblock;
pub mod group;
pub mod inode;
pub mod extent;
pub mod journal;
pub mod plugin;
pub mod error;

pub use error::{ExtError, ExtResult};
pub use plugin::ExtPlugin;
