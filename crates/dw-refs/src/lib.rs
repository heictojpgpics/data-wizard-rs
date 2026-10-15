//! # dw-refs
//!
//! ReFS filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from Microsoft ReFS documentation
//! + the `refs` source available in the Windows Driver Kit samples + the
//! open-source `refsfs` reverse-engineering notes):
//!
//!  1. **Probe**: Read the ReFS boot sector at offset 0; verify the
//!     "ReFS" OEM magic. Parse the super-block located at offset 0xC00
//!     (checksum-validated).
//!  2. **Quick scan**:
//!     - Walk the metadata table starting at the system table root.
//!     - For each object: extract object ID, parent ID, name, and
//!       data extent list.
//!     - ReFS uses a B-tree called the "object ID table" to find any
//!       object by its 16-byte ObjectID. Deleted objects leave the
//!       entry in the parent directory's B-tree marked as a tombstone.
//!  3. **Deep scan**:
//!     - Walk all allocation units not in the live allocation bitmap,
//!       apply raw carving.
//!
//! NOTE: ReFS is a moving target (v3.5 in Server 2022, v3.7 in Azure Stack
//! HCI). We target v3.x for the initial release. v3.7+ deviates in the
//! checksum tree layout.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod boot;
pub mod superblock;
pub mod object;
pub mod btree;
pub mod plugin;
pub mod error;

pub use error::{RefsError, RefsResult};
pub use plugin::RefsPlugin;
