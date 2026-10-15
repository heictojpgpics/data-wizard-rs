//! # dw-fat
//!
//! FAT12 / FAT16 / FAT32 filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from Microsoft FAT spec +
//! Linux kernel `fs/fat/`):
//!
//!  1. **Probe**: Read sector 0 BPB, determine FAT type by cluster count
//!     (< 4085 = FAT12, < 65525 = FAT16, else FAT32).
//!  2. **Quick scan**:
//!     - FAT32: walk root directory via the cluster chain starting at the
//!       BPB `RootCluster` field.
//!     - FAT12/16: walk the fixed-size root directory region.
//!     - For each directory entry: capture long-name slots (LFN) first,
//!       then the short 8.3 entry. Deleted entries (first byte 0xE5) are
//!       cross-referenced against the FAT to determine cluster chain
//!       recoverability.
//!  3. **Deep scan**:
//!     - Walk every FAT chain to find orphaned chains (allocated clusters
//!       with no live directory entry pointing at them).
//!     - Apply raw carving on unallocated clusters via [`dw-carver`](dw_carver).
//!  4. **FSINFO validation** (FAT32 only): use \$FSINFO free-cluster count
//!     and next-free hint only as a hint; the FAT is authoritative.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bpb;
pub mod fat;
pub mod direntry;
pub mod plugin;
pub mod error;

pub use error::{FatError, FatResult};
pub use plugin::FatPlugin;
