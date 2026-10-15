//! # dw-exfat
//!
//! exFAT filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from Microsoft exFAT spec +
//! Linux kernel `fs/exfat/`):
//!
//!  1. **Probe**: Read sector 0 Boot Region, verify "EXFAT   " magic,
//!     parse Main Boot Parameters (bytes-per-sector, sectors-per-cluster,
//!     FAT offset/length, cluster-heap offset, cluster count, root-dir
//!     first cluster).
//!  2. **Quick scan**:
//!     - Walk the root directory's generic-primary + stream-extension +
//!       file-name directory entries.
//!     - Use the allocation bitmap to know which clusters are "live".
//!     - Use the upcase-table to normalize case-sensitive names.
//!  3. **Deep scan**:
//!     - Find clusters NOT in the allocation bitmap -- these are
//!       unallocated, prime carving territory.
//!     - For each deleted directory entry (InUse bit cleared in
//!       GenericPrimary), try to recover the chain via the stream
//!       extension's NoFatChain flag / DataLength.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bitmap;
pub mod boot;
pub mod direntry;
pub mod error;
pub mod fat;
pub mod plugin;

pub use error::{ExfatError, ExfatResult};
pub use plugin::ExfatPlugin;
