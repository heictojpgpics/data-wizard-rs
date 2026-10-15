//! The [`DiskSource`] trait and the cross-platform file-backed implementation.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{DiskError, DiskGeometry, DiskResult};

/// Synchronous random-access reader. Used internally by the FS parsers --
/// they read short bursts of metadata and prefer not to pay the `async`
/// state-machine overhead on every cluster read.
pub trait ReadAt: Send + Sync {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()>;
    fn size(&self) -> u64;
}

/// Async disk source. The carver uses this for large sequential reads;
/// the FS parsers use [`ReadAt`] for metadata.
#[async_trait]
pub trait DiskSource: Send + Sync {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<Bytes>;
    async fn geometry(&self) -> DiskResult<DiskGeometry>;
    fn as_read_at(&self) -> Arc<dyn ReadAt>;
}

/// File-backed source -- opens a regular file (image) and serves reads
/// from it via pread / ReadFile.
pub struct FileBackedSource {
    file: std::fs::File,
    size: u64,
    geometry: DiskGeometry,
}

impl FileBackedSource {
    pub fn open(path: &Path) -> DiskResult<Self> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            file,
            size,
            geometry: DiskGeometry {
                total_bytes: size,
                bytes_per_sector: 512,
                sectors_per_track: 0,
                heads: 0,
                cylinders: 0,
            },
        })
    }

    pub fn file(&self) -> &std::fs::File {
        &self.file
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(unix)]
impl ReadAt for FileBackedSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        use std::os::unix::fs::FileExt;
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        let _ = self.file.read_at(buf, offset)?;
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(windows)]
impl ReadAt for FileBackedSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        use std::os::windows::fs::FileExt;
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        let _ = self.file.seek_read(buf, offset)?;
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[async_trait]
impl DiskSource for FileBackedSource {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<Bytes> {
        if offset.saturating_add(len) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len,
                source_size: self.size,
            });
        }
        let mut buf = vec![0u8; len as usize];
        <Self as ReadAt>::read_at(self, offset, &mut buf)?;
        Ok(Bytes::from(buf))
    }

    async fn geometry(&self) -> DiskResult<DiskGeometry> {
        Ok(self.geometry.clone())
    }

    fn as_read_at(&self) -> Arc<dyn ReadAt> {
        Arc::new(FileBackedReadAt {
            file: self.file.try_clone().expect("file handle clone"),
            size: self.size,
        })
    }
}

/// A clonable handle to a file-backed source. Used when an FS parser needs
/// its own `Arc<dyn ReadAt>` without re-opening the file.
pub struct FileBackedReadAt {
    pub file: std::fs::File,
    pub size: u64,
}

#[cfg(unix)]
impl ReadAt for FileBackedReadAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        use std::os::unix::fs::FileExt;
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        let _ = self.file.read_at(buf, offset)?;
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(windows)]
impl ReadAt for FileBackedReadAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        use std::os::windows::fs::FileExt;
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        let _ = self.file.seek_read(buf, offset)?;
        Ok(())
    }

    fn size(&self) -> u64 {
        self.size
    }
}

// ---------------------------------------------------------------------------
// PartitionView: scan a single partition inside a larger source
// ---------------------------------------------------------------------------

/// A view over one partition of a larger source.
///
/// All reads are translated by `offset` and bounds-checked against the
/// partition's own `size`, so filesystem parsers see exactly the
/// partition as if it were a standalone volume. This is how
/// `start_scan(partition)` works without copying or slicing images.
pub struct PartitionView {
    inner: Arc<dyn ReadAt>,
    offset: u64,
    size: u64,
}

impl PartitionView {
    /// Create a view of `[offset, offset + size)` within `inner`.
    pub fn new(inner: Arc<dyn ReadAt>, offset: u64, size: u64) -> DiskResult<Self> {
        if offset.saturating_add(size) > inner.size() {
            return Err(DiskError::OutOfRange {
                offset,
                len: size,
                source_size: inner.size(),
            });
        }
        Ok(Self {
            inner,
            offset,
            size,
        })
    }
}

impl ReadAt for PartitionView {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        if offset.saturating_add(buf.len() as u64) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len: buf.len() as u64,
                source_size: self.size,
            });
        }
        self.inner.read_at(self.offset + offset, buf)
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[async_trait]
impl DiskSource for PartitionView {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<Bytes> {
        if offset.saturating_add(len) > self.size {
            return Err(DiskError::OutOfRange {
                offset,
                len,
                source_size: self.size,
            });
        }
        let mut buf = vec![0u8; len as usize];
        self.inner.read_at(self.offset + offset, &mut buf)?;
        Ok(Bytes::from(buf))
    }

    async fn geometry(&self) -> DiskResult<DiskGeometry> {
        Ok(DiskGeometry {
            total_bytes: self.size,
            bytes_per_sector: 512,
            sectors_per_track: 0,
            heads: 0,
            cylinders: 0,
        })
    }

    fn as_read_at(&self) -> Arc<dyn ReadAt> {
        // Return a shared view: PartitionView itself is ReadAt, but the
        // trait needs a fresh Arc; clone the parts into a new view.
        Arc::new(Self {
            inner: Arc::clone(&self.inner),
            offset: self.offset,
            size: self.size,
        })
    }
}
