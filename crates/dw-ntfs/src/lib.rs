//! # dw-ntfs
//!
//! NTFS filesystem parser plugin for `data-wizard-rs`.
//!
//! Implements the [`FsPlugin`](dw_core::FsPlugin) trait for NTFS 3.0+ volumes.
//!
//! Recovery strategy (clean-room, derived from public NTFS documentation:
//! Linux kernel `fs/ntfs3/`, libntfs-3g, Microsoft Open Specifications
//! MS-NTFS):
//!
//!  1. **Probe**: Read VBR (sector 0), verify "NTFS    " OEM magic, parse BPB,
//!     extract bytes-per-cluster, MFT offset, MFT record size.
//!  2. **Quick scan**: Walk the MFT directly. For each FILE record:
//!     - Apply resident/$DATA + $INDEX_ROOT/$INDEX_ALLOCATION reconstruction.
//!     - Honour $ATTRIBUTE_LIST to follow fragmented attribute chains.
//!     - Use $STANDARD_INFORMATION for timestamps; $FILE_NAME for the
//!       pre-delete name (a deleted file often still has its $FILE_NAME
//!       in its parent directory's $INDEX_ROOT but its $FILE record marked
//!       free in the MFT bitmap).
//!  3. **Deep scan**: For each cluster not claimed by a live MFT entry,
//!     apply raw carving via [`dw-carver`](dw_carver).
//!  4. **USN journal cross-reference**: Read \$UsnJrnl:\$J to surface
//!     files that were renamed/deleted very recently and may not yet have
//!     their $FILE records reused.
//!
//! All raw structures are parsed with `bytemuck::Pod` zero-copy views into
//! mmap'd regions -- no per-field allocations on the hot path.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod boot;
pub mod mft;
pub mod attribute;
pub mod index;
pub mod plugin;
pub mod error;

pub use error::{NtfsError, NtfsResult};
pub use plugin::NtfsPlugin;
