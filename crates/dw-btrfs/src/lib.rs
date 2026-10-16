//! # dw-btrfs
//!
//! Btrfs parser plugin for `data-wizard-rs`.
//!
//! Clean-room implementation against the kernel UAPI on-disk format
//! (`include/uapi/linux/btrfs_tree.h`); every struct offset was compiled
//! against that header (see the research workspace's
//! `re-evidence/specs/`).
//!
//! Scope, stated honestly:
//! - single-device volumes, non-RAID profiles only (probe returns
//!   Unsupported otherwise - no guessing, no pretend support);
//! - crc32c-verified nodes and superblock; torn blocks are rejected;
//! - quick: live subvolume walks with fragmented files in content
//!   order, inline files as resident data;
//! - deep: the orphan-tree-block pass (COW keeps old generations, so
//!   verified-but-unreachable FS leaves still name and map deleted
//!   files);
//! - compressed extents surface as metadata-only (decompression is out
//!   of v1 and the entry says so).

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod btree;
pub mod chunk;
pub mod error;
pub mod items;
pub mod plugin;
pub mod superblock;
pub mod walker;

pub use error::{BtrfsError, BtrfsResult};
pub use plugin::BtrfsPlugin;
