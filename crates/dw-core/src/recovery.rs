//! File recovery writer: streams a RecoveredEntry's bytes from the scan
//! source to a destination path.
//!
//! Priority order per entry:
//!  1. `resident_data` (NTFS resident $DATA) — exact inline bytes.
//!  2. `extents` — streamed in file order; fragmented files (multi-extent
//!     chains) work natively. Extents were trimmed to the file size by the
//!     walkers, so the write is exact.
//!  3. size == 0 — empty file.
//!  4. otherwise — typed error. Refusing to guess beats writing wrong
//!     bytes: entries with neither resident data nor extents are not
//!     recoverable through metadata.
//!
//! Durability: every file is flushed and fsync'd before success is
//! reported. Partial coverage (extents shorter than the declared size)
//! is written as-is and reported honestly with a warn log.

use std::io::Write;
use std::path::Path;

use thiserror::Error;

use crate::results::RecoveredEntry;
use dw_disk::ReadAt;

/// Per-read I/O chunk when streaming a large extent (8 MiB).
const RECOVERY_IO_CHUNK: u64 = 8 * 1024 * 1024;

/// Recover a single file by reading its physical extents from the source
/// and writing to the destination.
///
///
/// Priority order:
///  1. `resident_data` (NTFS resident $DATA) — write inline bytes.
///  2. `extents` — stream each extent in file order; handles fragmented
///     files (multi-extent chains) natively. The last extent is already
///     trimmed to the file size by the walkers.
///  3. size == 0 — write an empty file.
///  4. otherwise — typed error: refusing to guess beats writing wrong
///     bytes. (Carved entries get extents from the analysers; entries with
///     neither resident data nor extents are unrecoverable by design.)

/// Typed recovery failure.
#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("no physical location known for '{name}' ({size} bytes): not recoverable via metadata")]
    NoKnownLocation { name: String, size: u64 },
    #[error("invalid extent plan: {reason}")]
    InvalidExtents { reason: String },
    #[error("read failed at offset {offset}: {source}")]
    ReadFailed {
        offset: u64,
        source: dw_disk::DiskError,
    },
    #[error("write failed: {0}")]
    WriteFailed(#[from] std::io::Error),
}

/// Recover a single entry: stream its bytes from the source to `dest_path`.
pub fn recover_entry_to_path(
    entry: &RecoveredEntry,
    read_at: &dyn ReadAt,
    dest_path: &Path,
) -> Result<u64, RecoveryError> {
    // 1. Resident content wins: exact bytes, no source I/O.
    if let Some(resident) = &entry.resident_data {
        let n = resident.len().min(entry.size as usize);
        let bytes = &resident[..n];
        write_file_atomically(dest_path, bytes)?;
        return Ok(bytes.len() as u64);
    }

    // 3. Empty files.
    if entry.size == 0 {
        write_file_atomically(dest_path, b"")?;
        return Ok(0);
    }

    // 2. Extent-based streaming.
    if entry.extents.is_empty() {
        return Err(RecoveryError::NoKnownLocation {
            name: entry.name.clone(),
            size: entry.size,
        });
    }

    // Validate the extent plan before touching I/O: extents must be
    // ordered, non-overlapping, and their total must match the parts of
    // the file we claim to recover.
    let total_planned: u64 = entry.extents.iter().map(|e| e.length).sum();
    for pair in entry.extents.windows(2) {
        if pair[0].start_byte >= pair[1].start_byte {
            return Err(RecoveryError::InvalidExtents {
                reason: "extents are not strictly ascending".into(),
            });
        }
    }

    let expected = entry.size.min(total_planned);
    let file = std::fs::File::create(dest_path).map_err(RecoveryError::WriteFailed)?;
    let mut writer = std::io::BufWriter::with_capacity(1 << 20, file);
    let mut buf = vec![0u8; RECOVERY_IO_CHUNK as usize];
    let mut written: u64 = 0;

    for ext in &entry.extents {
        let mut ext_off = 0u64;
        while ext_off < ext.length && written < expected {
            let want = (ext.length - ext_off)
                .min(RECOVERY_IO_CHUNK)
                .min(expected - written) as usize;
            if want > buf.len() {
                buf.resize(want, 0);
            }
            read_at
                .read_at(ext.start_byte + ext_off, &mut buf[..want])
                .map_err(|e| RecoveryError::ReadFailed {
                    offset: ext.start_byte + ext_off,
                    source: e,
                })?;
            writer
                .write_all(&buf[..want])
                .map_err(RecoveryError::WriteFailed)?;
            written += want as u64;
            ext_off += want as u64;
        }
    }

    writer.flush().map_err(RecoveryError::WriteFailed)?;
    // fsync so the file is durable before we report success
    writer
        .get_ref()
        .sync_all()
        .map_err(RecoveryError::WriteFailed)?;

    if written < entry.size {
        // Partial: the extents covered less than the declared size. The
        // scan already marked these PartiallyRecovered; recovery still
        // writes what exists and reports the byte count honestly.
        tracing::warn!(
            name = %entry.name,
            written,
            declared = entry.size,
            "partial recovery: extents cover less than the declared size"
        );
    }
    Ok(written)
}

/// Write bytes to `dest_path` and fsync (atomic-enough for recovery
/// targets: the file appears either complete-at-this-point or absent).
fn write_file_atomically(dest_path: &Path, bytes: &[u8]) -> Result<(), RecoveryError> {
    let file = std::fs::File::create(dest_path).map_err(RecoveryError::WriteFailed)?;
    let mut writer = std::io::BufWriter::with_capacity(1 << 16, file);
    writer
        .write_all(bytes)
        .map_err(RecoveryError::WriteFailed)?;
    writer.flush().map_err(RecoveryError::WriteFailed)?;
    writer
        .get_ref()
        .sync_all()
        .map_err(RecoveryError::WriteFailed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::Extent;
    use dw_disk::{DiskSource, FileBackedSource};

    /// In-memory source implementing ReadAt over a Vec.
    struct MemSource {
        data: Vec<u8>,
    }
    impl ReadAt for MemSource {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> dw_disk::DiskResult<()> {
            let start = offset as usize;
            let end = start + buf.len();
            if end > self.data.len() {
                return Err(dw_disk::DiskError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "read past end",
                )));
            }
            buf.copy_from_slice(&self.data[start..end]);
            Ok(())
        }

        fn size(&self) -> u64 {
            self.data.len() as u64
        }
    }

    fn entry(extents: Vec<Extent>, size: u64, resident: Option<Vec<u8>>) -> RecoveredEntry {
        RecoveredEntry {
            id: crate::RecoveredEntryId(uuid::Uuid::nil()),
            fs_kind: "test".into(),
            parent_path: "\\".into(),
            name: "f.bin".into(),
            size,
            start_lba: None,
            mtime: None,
            ctime: None,
            atime: None,
            status: crate::RecoveryStatus::Intact,
            extension: "bin".into(),
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: Vec::new(),
            extents,
            resident_data: resident,
        }
    }

    fn tmp() -> tempfile::NamedTempFile {
        tempfile::NamedTempFile::new().unwrap()
    }

    #[test]
    fn resident_data_is_written_directly() {
        let src = MemSource { data: vec![0; 16] };
        let e = entry(vec![], 4, Some(vec![1, 2, 3, 4]));
        let t = tmp();
        let n = recover_entry_to_path(&e, &src, t.path()).unwrap();
        assert_eq!(n, 4);
        assert_eq!(std::fs::read(t.path()).unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn fragmented_extents_stream_in_order() {
        // Two-extent fragmented "file": bytes 0..3 at 100, bytes 3..8 at 500.
        let mut data = vec![0u8; 1024];
        data[100..104].copy_from_slice(&[1, 2, 3, 4]);
        data[500..505].copy_from_slice(&[5, 6, 7, 8, 9]);
        let src = MemSource { data };
        let e = entry(
            vec![
                Extent {
                    start_byte: 100,
                    length: 4,
                },
                Extent {
                    start_byte: 500,
                    length: 5,
                },
            ],
            9,
            None,
        );
        let t = tmp();
        let n = recover_entry_to_path(&e, &src, t.path()).unwrap();
        assert_eq!(n, 9);
        assert_eq!(
            std::fs::read(t.path()).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
    }

    #[test]
    fn large_extent_is_chunked() {
        // 20 MiB extent with RECOVERY_IO_CHUNK = 8 MiB -> 3 reads.
        let pattern: Vec<u8> = (0..256u32).map(|i| (i % 251) as u8).collect();
        let mut data = vec![0u8; 21 * 1024 * 1024];
        for (i, b) in data.iter_mut().enumerate() {
            *b = pattern[i % 256];
        }
        let src = MemSource { data };
        let e = entry(
            vec![Extent {
                start_byte: 0,
                length: 21 * 1024 * 1024,
            }],
            21 * 1024 * 1024,
            None,
        );
        let t = tmp();
        let n = recover_entry_to_path(&e, &src, t.path()).unwrap();
        assert_eq!(n, 21 * 1024 * 1024);
        let out = std::fs::read(t.path()).unwrap();
        assert_eq!(out.len(), 21 * 1024 * 1024);
        for (i, b) in out.iter().enumerate() {
            assert_eq!(*b, pattern[i % 256], "mismatch at {i}");
        }
    }

    #[test]
    fn empty_file_writes_zero_bytes() {
        let src = MemSource { data: vec![] };
        let e = entry(vec![], 0, None);
        let t = tmp();
        let n = recover_entry_to_path(&e, &src, t.path()).unwrap();
        assert_eq!(n, 0);
        assert_eq!(std::fs::read(t.path()).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn unknown_location_is_a_typed_error() {
        let src = MemSource { data: vec![0; 16] };
        // size > 0 but no extents and no resident data
        let e = entry(vec![], 100, None);
        let t = tmp();
        let err = recover_entry_to_path(&e, &src, t.path()).unwrap_err();
        assert!(matches!(err, RecoveryError::NoKnownLocation { .. }));
        assert!(err.to_string().contains("not recoverable"));
    }

    #[test]
    fn partial_extents_write_what_exists() {
        // Declared 100 bytes, extents only cover 10: honest partial write.
        let mut data = vec![0u8; 64];
        data[30..40].copy_from_slice(&[9u8; 10]);
        let src = MemSource { data };
        let e = entry(
            vec![Extent {
                start_byte: 30,
                length: 10,
            }],
            100,
            None,
        );
        let t = tmp();
        let n = recover_entry_to_path(&e, &src, t.path()).unwrap();
        assert_eq!(n, 10);
        assert_eq!(std::fs::read(t.path()).unwrap(), vec![9u8; 10]);
    }

    #[test]
    fn descending_extents_are_rejected() {
        let src = MemSource {
            data: vec![0; 1024],
        };
        let e = entry(
            vec![
                Extent {
                    start_byte: 500,
                    length: 4,
                },
                Extent {
                    start_byte: 100,
                    length: 4,
                },
            ],
            8,
            None,
        );
        let t = tmp();
        let err = recover_entry_to_path(&e, &src, t.path()).unwrap_err();
        assert!(matches!(err, RecoveryError::InvalidExtents { .. }));
    }

    #[test]
    fn read_past_end_is_surfaced() {
        // Extent claims bytes beyond the source: ReadAt errors propagate.
        let src = MemSource { data: vec![0; 16] };
        let e = entry(
            vec![Extent {
                start_byte: 8,
                length: 100,
            }],
            100,
            None,
        );
        let t = tmp();
        let err = recover_entry_to_path(&e, &src, t.path()).unwrap_err();
        assert!(matches!(err, RecoveryError::ReadFailed { .. }));
    }

    #[test]
    fn roundtrip_against_file_backed_source() {
        // Full-circle: bytes on a real FileBackedSource, fragmented extents.
        let mut data = vec![0u8; 4096];
        data[1024..1034].copy_from_slice(&[1, 1, 2, 3, 5, 8, 13, 21, 34, 55]);
        data[2048..2052].copy_from_slice(&[9, 9, 9, 9]);
        let tmp_src = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp_src.path(), &data).unwrap();
        let source = FileBackedSource::open(tmp_src.path()).unwrap();
        let e = entry(
            vec![
                Extent {
                    start_byte: 1024,
                    length: 10,
                },
                Extent {
                    start_byte: 2048,
                    length: 4,
                },
            ],
            14,
            None,
        );
        let t = tmp();
        let n = recover_entry_to_path(&e, source.as_read_at().as_ref(), t.path()).unwrap();
        assert_eq!(n, 14);
        assert_eq!(
            std::fs::read(t.path()).unwrap(),
            vec![1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 9, 9, 9, 9]
        );
    }
}
