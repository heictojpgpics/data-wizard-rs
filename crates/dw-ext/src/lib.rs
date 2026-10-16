//! # dw-ext
//!
//! ext2 / ext3 / ext4 filesystem engine for `data-wizard-rs`.
//!
//! Clean-room implementation against the public on-disk format (Linux
//! kernel `fs/ext4/ext4.h` layouts; `e2fsprogs` semantics), cross-checked
//! against the reference product's module map (see
//! `open-recovery-wizard/docs/08-driver-and-module-re.md`).
//!
//! Scan phases:
//!
//! 1. **Quick** - live metadata walk from root inode 2 (extent trees and
//!    legacy indirect maps, content-order physical extents for fragmented
//!    files), plus the shadow-dirent pass: directory blocks are scanned
//!    for structurally valid entries the live chain skips, so recently
//!    deleted files surface with their real names.
//! 2. **Deep** - quick + the orphan pass (unallocated inodes whose
//!    on-disk records still describe files) + free-block carving through
//!    the shared signature carver, restricted to the block bitmaps' free
//!    runs so live files are never duplicated.
//!
//! Journal-based deleted-dirent recovery (jbd2) is a documented roadmap
//! item; it is not implemented and nothing pretends otherwise.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod dir;
pub mod error;
pub mod extent;
pub mod group;
pub mod inode;
pub mod plugin;
pub mod superblock;
pub mod walker;

pub use error::{ExtError, ExtResult};
pub use inode::Inode;
pub use plugin::ExtPlugin;
