//! File recovery writer: streams a RecoveredEntry's bytes from the scan
//! source to a destination path.
//!
//! Priority order per entry:
//!  1. `resident_data` (NTFS resident $DATA) - exact inline bytes.
//!  2. `extents` - streamed in file order; fragmented files (multi-extent
//!     chains) work natively. Extents were trimmed to the file size by the
//!     walkers, so the write is exact.
//!  3. size == 0 - empty file.
//!  4. otherwise - typed error. Refusing to guess beats writing wrong
//!     bytes: entries with neither resident data nor extents are not
//!     recoverable through metadata.
//!
//! Destination safety contract (a recovery tool must never damage what it
//! is asked to produce):
//!  - Writes go to a hidden temp file in the destination directory, are
//!    fsync'd, and are then *published* (linked/renamed) to the final name.
//!    A crash mid-recovery leaves at most a temp file, never a truncated
//!    file at a name the user cares about.
//!  - Publishing is no-clobber by default: with `CollisionPolicy::Error`
//!    an existing destination is an error; `KeepBoth` writes under the
//!    next free `name (n).ext`; `Overwrite` is explicit opt-in.
//!
//! Durability: every file is flushed and fsync'd before success is
//! reported, and the destination directory is fsync'd after publish so
//! the new name survives a crash. Partial coverage (extents shorter than
//! the declared size) is written as-is and reported honestly with a warn
//! log.

use std::io::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::results::RecoveredEntry;
use dw_disk::ReadAt;

/// Per-read I/O chunk when streaming a large extent (8 MiB).
const RECOVERY_IO_CHUNK: u64 = 8 * 1024 * 1024;

/// How to handle an existing file at the destination path.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum CollisionPolicy {
    /// Refuse: an existing destination is an error. The safe default.
    #[default]
    Error,
    /// Write under the next free `name (n).ext` alongside the existing
    /// file. Nothing the user already had is ever touched.
    KeepBoth,
    /// Replace the existing file. Explicit opt-in only.
    Overwrite,
}

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
    #[error("destination already exists: {path}")]
    Collision { path: PathBuf },
    #[error("destination preflight failed: {reason}")]
    Preflight { reason: String },
}

/// Where a recovered file actually landed (the collision policy may pick
/// a different final name than the requested one).
#[derive(Debug, Clone)]
pub struct RecoveryOutcome {
    pub path: PathBuf,
    pub bytes_written: u64,
    pub partial: bool,
}

/// Recover a single entry with the no-clobber contract (see module docs).
pub fn recover_entry(
    entry: &RecoveredEntry,
    read_at: &dyn ReadAt,
    dest_path: &Path,
    policy: CollisionPolicy,
) -> Result<RecoveryOutcome, RecoveryError> {
    let dest_dir = dest_path.parent().ok_or_else(|| RecoveryError::Preflight {
        reason: format!(
            "destination '{}' has no parent directory",
            dest_path.display()
        ),
    })?;

    let expected = expected_bytes(entry);

    // Stream to a temp file next to the destination (same filesystem, so
    // publish is a metadata-only operation).
    let temp = tempfile::Builder::new()
        .prefix(".dw-recover-")
        .tempfile_in(dest_dir)
        .map_err(RecoveryError::WriteFailed)?;

    let written = {
        let mut writer = std::io::BufWriter::with_capacity(1 << 20, temp.as_file());
        let written = stream_entry(entry, read_at, &mut writer, expected)?;
        writer.flush().map_err(RecoveryError::WriteFailed)?;
        // fsync the DATA before publish; the publish itself is fsync'd
        // at the directory level below.
        writer
            .get_ref()
            .sync_all()
            .map_err(RecoveryError::WriteFailed)?;
        written
    };

    let partial = written < entry.size;
    if partial {
        tracing::warn!(
            name = %entry.name,
            written,
            declared = entry.size,
            "partial recovery: extents cover less than the declared size"
        );
    }

    // Publish under the collision policy. The temp file is consumed
    // (renamed/linked) on success and cleaned up on any failure.
    let final_path = publish_temp(temp, dest_path, policy)?;
    sync_dir(dest_dir);

    Ok(RecoveryOutcome {
        path: final_path,
        bytes_written: written,
        partial,
    })
}

/// How many bytes this entry should produce.
fn expected_bytes(entry: &RecoveredEntry) -> u64 {
    if entry.resident_data.is_some() {
        entry
            .resident_data
            .as_ref()
            .map_or(0, |r| r.len().min(entry.size as usize) as u64)
    } else if entry.size == 0 {
        0
    } else {
        entry
            .extents
            .iter()
            .map(|e| e.length)
            .sum::<u64>()
            .min(entry.size)
    }
}

/// Stream the entry's content into `writer` per the module's priority
/// order. Returns the number of bytes written.
fn stream_entry<W: Write>(
    entry: &RecoveredEntry,
    read_at: &dyn ReadAt,
    writer: &mut W,
    expected: u64,
) -> Result<u64, RecoveryError> {
    // 1. Resident content wins: exact bytes, no source I/O.
    if let Some(resident) = &entry.resident_data {
        let n = resident.len().min(entry.size as usize);
        writer
            .write_all(&resident[..n])
            .map_err(RecoveryError::WriteFailed)?;
        return Ok(n as u64);
    }

    // 2. Empty files.
    if entry.size == 0 {
        return Ok(0);
    }

    // 3. Extent-based streaming.
    if entry.extents.is_empty() {
        return Err(RecoveryError::NoKnownLocation {
            name: entry.name.clone(),
            size: entry.size,
        });
    }

    // Validate the extent plan before touching I/O: extents must be
    // ordered and non-overlapping.
    for pair in entry.extents.windows(2) {
        if pair[0].start_byte >= pair[1].start_byte {
            return Err(RecoveryError::InvalidExtents {
                reason: "extents are not strictly ascending".into(),
            });
        }
        if pair[0].start_byte.saturating_add(pair[0].length) > pair[1].start_byte {
            return Err(RecoveryError::InvalidExtents {
                reason: "extents overlap".into(),
            });
        }
    }

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
    Ok(written)
}

/// Publish the fully-written temp file to `dest_path` per the policy.
///
/// No-replace strategy: `hard_link(temp, dest)` then unlink the temp
/// name. `link(2)` / `CreateHardLinkW` fail atomically when the target
/// exists, which is airtight on NTFS, ext4, APFS, XFS. Destinations on
/// filesystems without hard links (FAT, exFAT) fall back to an
/// existence-check + rename (documented TOCTOU window, single-writer
/// app). `Overwrite` uses plain `rename` (atomic on both platforms).
pub fn publish_temp(
    temp: tempfile::NamedTempFile,
    dest_path: &Path,
    policy: CollisionPolicy,
) -> Result<PathBuf, RecoveryError> {
    let target = match policy {
        CollisionPolicy::Overwrite => dest_path.to_path_buf(),
        CollisionPolicy::Error => {
            if dest_path.exists() {
                return Err(RecoveryError::Collision {
                    path: dest_path.to_path_buf(),
                });
            }
            dest_path.to_path_buf()
        }
        CollisionPolicy::KeepBoth => next_free_path(dest_path)?,
    };

    if policy == CollisionPolicy::Overwrite {
        temp.persist(target)
            .map_err(|e| RecoveryError::WriteFailed(e.error))?;
        return Ok(dest_path.to_path_buf());
    }

    // No-replace publish: hard link first (atomic fail-if-exists) ...
    match std::fs::hard_link(temp.path(), &target) {
        Ok(()) => {
            let _ = std::fs::remove_file(temp.path()); // drop the temp name
            Ok(target)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(RecoveryError::Collision { path: target })
        }
        Err(e)
            if e.raw_os_error() == Some(1)       // EPERM (FS without links)
                || e.raw_os_error() == Some(45)  // EOPNOTSUPP (BSD)
                || e.raw_os_error() == Some(95)  // EOPNOTSUPP (Linux)
                || e.kind() == std::io::ErrorKind::Unsupported =>
        {
            // Fallback for link-less filesystems (FAT/exFAT): checked
            // rename. The TOCTOU window is real but bounded to
            // single-writer usage; documented.
            if target.exists() {
                return Err(RecoveryError::Collision { path: target });
            }
            temp.persist(&target)
                .map_err(|e| RecoveryError::WriteFailed(e.error))?;
            Ok(target)
        }
        Err(e) => Err(RecoveryError::WriteFailed(e)),
    }
}

/// `report.pdf` -> `report (2).pdf`, `report (3).pdf`, ... first free.
fn next_free_path(dest_path: &Path) -> Result<PathBuf, RecoveryError> {
    if !dest_path.exists() {
        return Ok(dest_path.to_path_buf());
    }
    let stem = dest_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("recovered");
    let ext = dest_path.extension().and_then(|e| e.to_str());
    let parent = dest_path.parent().unwrap_or_else(|| Path::new("."));
    for n in 2..u32::MAX {
        let candidate = match ext {
            Some(e) => parent.join(format!("{stem} ({n}).{e}")),
            None => parent.join(format!("{stem} ({n})")),
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(RecoveryError::Preflight {
        reason: format!("no free collision slot for '{}'", dest_path.display()),
    })
}

/// fsync the directory entry so a just-published name survives a crash.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(f) = std::fs::File::open(dir) {
            let _ = f.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

/// Preflight a recovery batch destination:
///  - the directory exists and is a directory,
///  - there is room for `total_bytes` (plus 64 MiB headroom for metadata
///    and rounding),
///  - the destination is not the scan source itself (writing recovered
///    data back into the image being recovered from would corrupt it).
pub fn preflight_destination(
    dest_dir: &Path,
    total_bytes: u64,
    source_path: Option<&Path>,
) -> Result<(), RecoveryError> {
    if let Some(src) = source_path
        && same_path(dest_dir, src)
    {
        return Err(RecoveryError::Preflight {
            reason: "destination must not be the scan source".into(),
        });
    }
    let meta = std::fs::metadata(dest_dir).map_err(|e| RecoveryError::Preflight {
        reason: format!("destination not accessible: {e}"),
    })?;
    if !meta.is_dir() {
        return Err(RecoveryError::Preflight {
            reason: format!("'{}' is not a directory", dest_dir.display()),
        });
    }
    if total_bytes > 0 {
        let free = fs4::free_space(dest_dir).map_err(|e| RecoveryError::Preflight {
            reason: format!("cannot query free space: {e}"),
        })?;
        let headroom = total_bytes.saturating_add(64 * 1024 * 1024);
        if free < headroom {
            return Err(RecoveryError::Preflight {
                reason: format!(
                    "not enough space: need at least {} bytes (incl. headroom), {} free",
                    headroom, free
                ),
            });
        }
    }
    Ok(())
}

/// Canonicalizing path equality (case-insensitive on Windows).
fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| -> PathBuf {
        let mut s = p.to_string_lossy().to_string();
        if cfg!(windows) {
            s.make_ascii_lowercase();
        }
        PathBuf::from(s)
    };
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => norm(a) == norm(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::Extent;
    use dw_disk::{DiskSource, FileBackedSource};

    /// In-memory source implementing ReadAt over a Vec.
    pub(super) struct MemSource {
        pub data: Vec<u8>,
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
        let n = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite)
            .unwrap()
            .bytes_written;
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
        let n = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite)
            .unwrap()
            .bytes_written;
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
        let n = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite)
            .unwrap()
            .bytes_written;
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
        let n = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite)
            .unwrap()
            .bytes_written;
        assert_eq!(n, 0);
        assert_eq!(std::fs::read(t.path()).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn unknown_location_is_a_typed_error() {
        let src = MemSource { data: vec![0; 16] };
        // size > 0 but no extents and no resident data
        let e = entry(vec![], 100, None);
        let t = tmp();
        let err = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite).unwrap_err();
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
        let n = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite)
            .unwrap()
            .bytes_written;
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
        let err = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite).unwrap_err();
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
        let err = recover_entry(&e, &src, t.path(), CollisionPolicy::Overwrite).unwrap_err();
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
        let n = recover_entry(
            &e,
            source.as_read_at().as_ref(),
            t.path(),
            CollisionPolicy::Overwrite,
        )
        .unwrap()
        .bytes_written;
        assert_eq!(n, 14);
        assert_eq!(
            std::fs::read(t.path()).unwrap(),
            vec![1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 9, 9, 9, 9]
        );
    }
}

#[cfg(test)]
mod collision_tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::tests::MemSource;
    use super::*;
    use crate::Extent;

    fn mem_src() -> MemSource {
        MemSource {
            data: vec![7u8; 64],
        }
    }

    fn file_entry(name: &str) -> RecoveredEntry {
        RecoveredEntry {
            id: crate::RecoveredEntryId(uuid::Uuid::nil()),
            fs_kind: "test".into(),
            parent_path: "\\".into(),
            name: name.into(),
            size: 8,
            start_lba: Some(0),
            mtime: None,
            ctime: None,
            atime: None,
            status: crate::RecoveryStatus::Intact,
            extension: "bin".into(),
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: vec![],
            extents: vec![Extent {
                start_byte: 0,
                length: 8,
            }],
            resident_data: None,
        }
    }

    #[test]
    fn error_policy_refuses_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.bin");
        std::fs::write(&dest, b"ORIGINAL").unwrap();

        let out = recover_entry(
            &file_entry("f.bin"),
            &mem_src(),
            &dest,
            CollisionPolicy::Error,
        );
        assert!(matches!(out, Err(RecoveryError::Collision { .. })));
        // The user's file is untouched.
        assert_eq!(std::fs::read(&dest).unwrap(), b"ORIGINAL");
        // No temp leftovers.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".dw-recover-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files must be cleaned up on failure"
        );
    }

    #[test]
    fn keep_both_policy_writes_next_free_slot() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.bin");
        std::fs::write(&dest, b"ORIGINAL").unwrap();
        // Pre-existing collision slots.
        std::fs::write(dir.path().join("f (2).bin"), b"X").unwrap();

        let out = recover_entry(
            &file_entry("f.bin"),
            &mem_src(),
            &dest,
            CollisionPolicy::KeepBoth,
        )
        .unwrap();
        assert_eq!(out.path, dir.path().join("f (3).bin"));
        assert_eq!(std::fs::read(&out.path).unwrap(), vec![7u8; 8]);
        // Both prior files untouched.
        assert_eq!(std::fs::read(&dest).unwrap(), b"ORIGINAL");
        assert_eq!(std::fs::read(dir.path().join("f (2).bin")).unwrap(), b"X");
    }

    #[test]
    fn overwrite_policy_replaces_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.bin");
        std::fs::write(&dest, b"OLD").unwrap();

        let out = recover_entry(
            &file_entry("f.bin"),
            &mem_src(),
            &dest,
            CollisionPolicy::Overwrite,
        )
        .unwrap();
        assert_eq!(out.path, dest);
        assert_eq!(std::fs::read(&dest).unwrap(), vec![7u8; 8]);
    }

    #[test]
    fn fresh_destination_succeeds_with_default_policy() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("new.bin");
        let out = recover_entry(
            &file_entry("new.bin"),
            &mem_src(),
            &dest,
            CollisionPolicy::Error,
        )
        .unwrap();
        assert_eq!(out.path, dest);
        assert_eq!(out.bytes_written, 8);
        assert!(!out.partial);
    }

    #[test]
    fn preflight_rejects_destination_equal_to_source() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("image.dd");
        std::fs::write(&src, b"0123456789").unwrap();
        // dest_dir == source path (a file, also not a dir)
        let err = preflight_destination(&src, 10, Some(&src)).unwrap_err();
        assert!(matches!(err, RecoveryError::Preflight { .. }));
        assert!(err.to_string().contains("scan source"));
    }

    #[test]
    fn preflight_rejects_missing_directory() {
        let err = preflight_destination(Path::new("/nonexistent/xyz"), 10, None).unwrap_err();
        assert!(matches!(err, RecoveryError::Preflight { .. }));
    }

    #[test]
    fn preflight_passes_with_room() {
        let dir = tempfile::tempdir().unwrap();
        preflight_destination(dir.path(), 1024, None).unwrap();
    }

    #[test]
    fn preflight_rejects_insufficient_space() {
        let dir = tempfile::tempdir().unwrap();
        // u64::MAX can never fit: headroom arithmetic saturates far past
        // any real volume size.
        let err = preflight_destination(dir.path(), u64::MAX, None).unwrap_err();
        assert!(matches!(err, RecoveryError::Preflight { .. }));
        assert!(err.to_string().contains("not enough space"));
    }

    #[test]
    fn no_temp_files_left_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("clean.bin");
        recover_entry(
            &file_entry("clean.bin"),
            &mem_src(),
            &dest,
            CollisionPolicy::Error,
        )
        .unwrap();
        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["clean.bin".to_string()]);
    }
}
