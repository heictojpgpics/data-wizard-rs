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
pub mod source;
#[cfg(windows)]
pub mod windows_raw;

pub use error::{DiskError, DiskResult};
pub use geometry::{DiskGeometry, Partition, PartitionFsHint, PartitionTable, PartitionType, SectorSize};
pub use source::{DiskSource, FileBackedSource, ReadAt};
