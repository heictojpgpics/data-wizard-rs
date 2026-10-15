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

/// Hard cap on a single async read allocation. Deep-scan carvers read in
/// multi-megabyte chunks; anything larger signals a corrupt length field
/// and must fail fast instead of attempting a giant allocation.
pub const MAX_SINGLE_READ: u64 = 32 * 1024 * 1024;

/// Fill `buf` completely from `file` starting at `offset`, retrying short
/// reads. Positional read APIs (`pread` / `ReadFile` with OVERLAPPED) may
/// legally return fewer bytes than requested; accepting a short read as
/// success would leave the buffer tail as zero-filled garbage that
/// filesystem parsers would treat as on-disk data. EOF before the buffer
/// is full is a typed [`DiskError::ShortRead`] error, never success.
fn read_exact_at(
    file: &std::fs::File,
    offset: u64,
    buf: &mut [u8],
    source_size: u64,
) -> DiskResult<()> {
    let requested = buf.len() as u64;
    if offset.saturating_add(requested) > source_size {
        return Err(DiskError::OutOfRange {
            offset,
            len: requested,
            source_size,
        });
    }
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileExt;
                file.read_at(&mut buf[filled..], offset + filled as u64)?
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::FileExt;
                file.seek_read(&mut buf[filled..], offset + filled as u64)?
            }
        };
        if n == 0 {
            return Err(DiskError::ShortRead {
                offset,
                got: filled as u64,
                requested,
                source_size,
            });
        }
        filled += n;
    }
    Ok(())
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

impl ReadAt for FileBackedSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        read_exact_at(&self.file, offset, buf, self.size)
    }

    fn size(&self) -> u64 {
        self.size
    }
}

#[async_trait]
impl DiskSource for FileBackedSource {
    async fn read_at_async(&self, offset: u64, len: u64) -> DiskResult<Bytes> {
        if len > MAX_SINGLE_READ {
            return Err(DiskError::RequestTooLarge {
                len,
                cap: MAX_SINGLE_READ,
            });
        }
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

impl ReadAt for FileBackedReadAt {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
        read_exact_at(&self.file, offset, buf, self.size)
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
        if len > MAX_SINGLE_READ {
            return Err(DiskError::RequestTooLarge {
                len,
                cap: MAX_SINGLE_READ,
            });
        }
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

#[cfg(test)]
mod strict_read_tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn read_beyond_source_end_is_rejected() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"0123456789").unwrap();
        let src = FileBackedSource::open(temp.path()).unwrap();
        let read_at = src.as_read_at();

        let mut buf = [0u8; 8];
        // Fully in range: OK.
        read_at.read_at(2, &mut buf).unwrap();
        assert_eq!(&buf, b"23456789");

        // Starts in range, runs past the end: must error, not zero-fill.
        let mut buf = [0xFFu8; 12];
        let err = read_at.read_at(4, &mut buf).unwrap_err();
        assert!(
            matches!(err, DiskError::OutOfRange { .. }),
            "expected OutOfRange, got {err:?}"
        );

        // Starts past the end: error.
        let mut buf = [0u8; 4];
        assert!(read_at.read_at(10, &mut buf).is_err());
    }

    #[test]
    fn read_exactly_at_boundary_succeeds() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"0123456789").unwrap();
        let src = FileBackedSource::open(temp.path()).unwrap();
        let read_at = src.as_read_at();

        let mut buf = [0u8; 10];
        read_at.read_at(0, &mut buf).unwrap();
        assert_eq!(&buf, b"0123456789");
    }

    #[tokio::test]
    async fn async_read_caps_allocation() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"0123456789").unwrap();
        let src = FileBackedSource::open(temp.path()).unwrap();

        // Over-cap request: typed error, no giant allocation attempt.
        let err = src.read_at_async(0, MAX_SINGLE_READ + 1).await.unwrap_err();
        assert!(
            matches!(err, DiskError::RequestTooLarge { .. }),
            "expected RequestTooLarge, got {err:?}"
        );

        // In-range async read still works.
        let bytes = src.read_at_async(0, 10).await.unwrap();
        assert_eq!(&bytes[..], b"0123456789");
    }

    #[test]
    fn partition_view_enforces_its_own_bounds() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), vec![0xA5u8; 4096]).unwrap();
        let src = Arc::new(FileBackedSource::open(temp.path()).unwrap());
        let view = PartitionView::new(src.as_read_at(), 512, 1024).unwrap();

        let mut buf = [0u8; 16];
        view.read_at(1000, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0xA5));

        // Past the partition's own end (even though the source is larger).
        let mut buf = [0u8; 16];
        assert!(view.read_at(1024, &mut buf).is_err());
    }
}
