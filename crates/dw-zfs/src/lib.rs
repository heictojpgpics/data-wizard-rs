//! # dw-zfs
//!
//! ZFS filesystem parser plugin for `data-wizard-rs`.
//!
//! Recovery strategy (clean-room, derived from OpenZFS source
//! <https://github.com/openzfs/zfs> + the on-disk format documentation in
//! `module/zfs/`):
//!
//!  1. **Probe**: Read all four vdev labels (each 256 KiB, located at the
//!     start/end of each device in a mirror/raidz topology). Each label
//!     contains a 8 KiB uberblock ring (128 uberblocks of 1 KiB each,
//!     monotonically transaction-numbered). Pick the highest-txg uberblock.
//!  2. **Quick scan**:
//!     - Read the MOS (meta-object set) from the uberblock's root block
//!       pointer. The MOS contains a fixed OBJECT_NUMBERS enumeration
//!       including the master node (obj 1) which points at the
//!       DSL (dataset layer) and the DELETE_QUEUE.
//!     - Walk each dataset's objset to find live dnodes.
//!  3. **Deep scan**:
//!     - For each previous uberblock (older txg) in the ring, walk the
//!       MOS to find dnodes that have since been deleted but are still
//!       reachable via snapshot of the deadlist.
//!     - Apply raw carving on free blocks in the metaslab's space map.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod dnode;
pub mod error;
pub mod label;
pub mod mos;
pub mod plugin;
pub mod uberblock;

pub use error::{ZfsError, ZfsResult};
pub use plugin::ZfsPlugin;
