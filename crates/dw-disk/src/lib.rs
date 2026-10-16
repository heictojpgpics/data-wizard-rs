//! # dw-disk
//!
//! Raw disk I/O abstraction.
//!
//! Provides a unified [`DiskSource`] trait that the filesystem parsers and
//! the carver can read from, regardless of whether the underlying storage
//! is a live Windows physical drive, a flat file image, or (for tests) a
//! memory-mapped image.

#![deny(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod geometry;
pub mod partition;
pub mod pausable;
pub mod source;
pub mod vhd;
#[cfg(windows)]
pub mod windows_enum;
#[cfg(windows)]
pub mod windows_raw;

pub use error::{DiskError, DiskResult};
pub use geometry::{
    DiskGeometry, Partition, PartitionFsHint, PartitionTable, PartitionType, SectorSize,
};
pub use partition::{DiscoveredTable, PartitionEntry, discover_partitions};
pub use source::{DiskSource, FileBackedSource, PartitionView, ReadAt};

#[cfg(windows)]
pub use windows_enum::{
    PhysicalDriveInfo, StorageBus, VolumeEntry, VolumeKind, enumerate_physical_drives,
    enumerate_volumes, has_raw_disk_access,
};
