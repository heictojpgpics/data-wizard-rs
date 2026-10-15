//! Scan checkpoints: save, list, load, and resume.
//!
//! A checkpoint captures everything needed to continue an interrupted
//! scan on a later launch:
//!  - the source identity: URI + size + a 5-point sampled BLAKE3
//!    fingerprint (1 MiB windows at 0%, 25%, 50%, 75%, and the last
//!    MiB; small sources are hashed whole). A resume is REFUSED when the
//!    current source's fingerprint differs — resuming against a different
//!    image would silently attribute the old entries to the wrong data;
//!  - the scan configuration and lifecycle state;
//!  - the full results snapshot (all RecoveredEntry values, including
//!    their extents, so recovered-file I/O works after resume without
//!    rescanning).
//!
//! Serialization: framed `postcard` binary. Version 2 streams the save:
//! a metadata frame first, then one length-prefixed frame per entry,
//! pulled from the results store in bounded pages — a multi-million-entry
//! scan no longer materializes a second full copy of itself in RAM just
//! to write its checkpoint. Version 1 files (monolithic postcard blob)
//! still load. Both versions are written atomically (temp file + fsync +
//! rename) so a crash mid-write never corrupts an existing checkpoint;
//! the version field gates format evolution; unknown versions are
//! refused rather than guessed.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tracing::info;
use uuid::Uuid;

use crate::results::RecoveredEntry;
use crate::scan::{ScanConfig, ScanHandle, ScanState};
use crate::{CoreError, CoreResult};
use dw_disk::ReadAt;

pub const CHECKPOINT_VERSION: u32 = 2;
/// The legacy monolithic format (pre-streaming).
const CHECKPOINT_VERSION_V1: u32 = 1;
/// How many entries a streaming save pulls from the store per page.
const SAVE_PAGE: usize = 4096;

/// Fingerprint window size (1 MiB).
const FINGERPRINT_WINDOW: u64 = 1024 * 1024;
/// Sources at or below this size are hashed whole (8 MiB).
const FINGERPRINT_WHOLE_MAX: u64 = 8 * 1024 * 1024;

/// Metadata frame of a v2 checkpoint (everything but the entries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub version: u32,
    pub scan_id: Uuid,
    /// ISO-8601 creation timestamp.
    pub created_at: OffsetDateTime,
    pub source_uri: String,
    pub source_size: u64,
    /// BLAKE3-256 over the sampled windows.
    pub source_fingerprint: [u8; 32],
    pub config: ScanConfig,
    pub state: ScanState,
    /// Number of entry frames that follow the meta frame.
    pub entry_count: u64,
}

/// Serialized checkpoint (fully materialized; the load-side type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCheckpoint {
    pub version: u32,
    pub scan_id: Uuid,
    /// ISO-8601 creation timestamp.
    pub created_at: OffsetDateTime,
    pub source_uri: String,
    pub source_size: u64,
    /// BLAKE3-256 over the sampled windows.
    pub source_fingerprint: [u8; 32],
    pub config: ScanConfig,
    pub state: ScanState,
    pub entries: Vec<RecoveredEntry>,
}

/// Listing row (no entry payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSummary {
    pub scan_id: Uuid,
    pub source_uri: String,
    pub created_at: OffsetDateTime,
    pub state: ScanState,
    pub entries: u64,
    pub mode: String,
}

/// On-disk store of checkpoints (one file per scan under `dir`).
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    dir: PathBuf,
}

impl CheckpointStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{id}.ckpt"))
    }

    /// Persist a snapshot of `handle` against `source_uri`.
    /// The source is fingerprinted NOW (via `read_at`). Entries stream
    /// from the store in bounded pages (v2 format): no full-copy spike.
    pub fn save(
        &self,
        handle: &ScanHandle,
        source_uri: &str,
        read_at: &dyn ReadAt,
    ) -> CoreResult<Uuid> {
        let size = read_at.size();
        let fingerprint = fingerprint_source(read_at)?;
        let results = handle.results();
        let entry_count = results.len() as u64;
        let meta = CheckpointMeta {
            version: CHECKPOINT_VERSION,
            scan_id: handle.id(),
            created_at: OffsetDateTime::now_utc(),
            source_uri: source_uri.to_string(),
            source_size: size,
            source_fingerprint: fingerprint,
            config: handle.config(),
            state: handle.state(),
            entry_count,
        };
        self.write_streamed(&meta, &results)?;
        info!(
            scan_id = %meta.scan_id,
            entries = entry_count,
            "Checkpoint saved (streamed)"
        );
        Ok(meta.scan_id)
    }

    /// v2 writer: meta frame, then length-prefixed entry frames pulled
    /// from the store in `SAVE_PAGE` pages.
    fn write_streamed(
        &self,
        meta: &CheckpointMeta,
        results: &crate::results::ResultsStore,
    ) -> CoreResult<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| CoreError::Plugin(format!("create checkpoint dir: {e}")))?;
        let final_path = self.path_for(meta.scan_id);
        let tmp_path = self.dir.join(format!("{}.tmp", meta.scan_id));
        let f = std::fs::File::create(&tmp_path)
            .map_err(|e| CoreError::Plugin(format!("create tmp checkpoint: {e}")))?;
        let mut w = std::io::BufWriter::with_capacity(1 << 20, f);

        write_frame(&mut w, meta)
            .map_err(|e| CoreError::Plugin(format!("write checkpoint meta: {e}")))?;

        let mut written: u64 = 0;
        let mut offset: usize = 0;
        loop {
            let page = results.page(offset, SAVE_PAGE);
            if page.is_empty() {
                break;
            }
            for entry in &page {
                write_frame(&mut w, entry)
                    .map_err(|e| CoreError::Plugin(format!("write entry frame: {e}")))?;
                written += 1;
            }
            offset += page.len();
            if page.len() < SAVE_PAGE {
                break;
            }
        }
        if written != meta.entry_count {
            // The store changed under us (a live scan inserted more).
            // The file is still a consistent, count-honest snapshot if we
            // rewrite the meta count... but the meta frame is already
            // written. Refuse instead of shipping a mismatched file.
            let _ = std::fs::remove_file(&tmp_path);
            return Err(CoreError::Plugin(format!(
                "results changed during save ({} written, {} expected); retry",
                written, meta.entry_count
            )));
        }

        w.flush()
            .map_err(|e| CoreError::Plugin(format!("flush checkpoint: {e}")))?;
        w.get_ref()
            .sync_all()
            .map_err(|e| CoreError::Plugin(format!("fsync checkpoint: {e}")))?;
        drop(w);
        std::fs::rename(&tmp_path, &final_path)
            .map_err(|e| CoreError::Plugin(format!("publish checkpoint: {e}")))?;
        Ok(())
    }

    /// List all checkpoints (newest first).
    pub fn list(&self) -> Vec<CheckpointSummary> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ckpt") {
                continue;
            }
            if let Ok(cp) = self.load_path(&path) {
                out.push(CheckpointSummary {
                    scan_id: cp.scan_id,
                    source_uri: cp.source_uri,
                    created_at: cp.created_at,
                    state: cp.state,
                    entries: cp.entries.len() as u64,
                    mode: match cp.config.mode {
                        crate::plugin::ScanMode::Quick => "quick",
                        crate::plugin::ScanMode::Deep => "deep",
                    }
                    .to_string(),
                });
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        out
    }

    fn load_path(&self, path: &Path) -> CoreResult<ScanCheckpoint> {
        let bytes =
            std::fs::read(path).map_err(|e| CoreError::Plugin(format!("read checkpoint: {e}")))?;
        // v2 files are framed (meta first). Detect by peeking the meta
        // frame; fall back to the v1 monolithic blob.
        match read_frame::<CheckpointMeta>(&bytes) {
            Ok((meta, consumed)) if meta.version == CHECKPOINT_VERSION => {
                let mut entries = Vec::with_capacity(
                    usize::try_from(meta.entry_count)
                        .unwrap_or(0)
                        .min(5_000_000),
                );
                let mut rest = &bytes[consumed..];
                for _ in 0..meta.entry_count {
                    let (entry, used) = read_frame::<RecoveredEntry>(rest)
                        .map_err(|e| CoreError::Plugin(format!("entry frame: {e}")))?;
                    entries.push(entry);
                    rest = &rest[used..];
                }
                Ok(ScanCheckpoint {
                    version: meta.version,
                    scan_id: meta.scan_id,
                    created_at: meta.created_at,
                    source_uri: meta.source_uri,
                    source_size: meta.source_size,
                    source_fingerprint: meta.source_fingerprint,
                    config: meta.config,
                    state: meta.state,
                    entries,
                })
            }
            _ => {
                let cp: ScanCheckpoint = postcard::from_bytes(&bytes)
                    .map_err(|e| CoreError::Plugin(format!("deserialize checkpoint: {e}")))?;
                if cp.version == CHECKPOINT_VERSION_V1 {
                    Ok(cp)
                } else {
                    Err(CoreError::Plugin(format!(
                        "checkpoint version {} not supported",
                        cp.version
                    )))
                }
            }
        }
    }

    /// Load one checkpoint by scan id.
    pub fn load(&self, id: Uuid) -> CoreResult<ScanCheckpoint> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(CoreError::Plugin(format!("checkpoint {id} not found")));
        }
        self.load_path(&path)
    }

    /// Delete one checkpoint.
    pub fn delete(&self, id: Uuid) -> CoreResult<()> {
        let path = self.path_for(id);
        std::fs::remove_file(&path)
            .map_err(|e| CoreError::Plugin(format!("delete checkpoint: {e}")))?;
        Ok(())
    }
}

/// Write one length-prefixed postcard frame.
fn write_frame<W: Write, T: Serialize>(w: &mut W, value: &T) -> std::io::Result<()> {
    let bytes = postcard::to_allocvec(value).map_err(std::io::Error::other)?;
    let len = u32::try_from(bytes.len())
        .map_err(|_| std::io::Error::other("checkpoint frame exceeds u32 length"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&bytes)
}

/// Read one length-prefixed postcard frame; returns the value and the
/// total bytes consumed (header + payload).
fn read_frame<T: for<'de> Deserialize<'de>>(buf: &[u8]) -> Result<(T, usize), String> {
    if buf.len() < 4 {
        return Err("truncated frame header".into());
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    let end = 4 + len;
    if buf.len() < end {
        return Err(format!(
            "truncated frame body (need {end}, have {})",
            buf.len()
        ));
    }
    let value = postcard::from_bytes(&buf[4..end]).map_err(|e| e.to_string())?;
    Ok((value, end))
}

/// Compute the 5-point sampled fingerprint of a source.
pub fn fingerprint_source(read_at: &dyn ReadAt) -> CoreResult<[u8; 32]> {
    let size = read_at.size();
    let mut hasher = blake3::Hasher::new();

    let mut feed = |start: u64, len: u64| -> CoreResult<()> {
        let mut remaining = len;
        let mut pos = start;
        let mut buf = vec![0u8; 256 * 1024];
        while remaining > 0 {
            let want = remaining.min(buf.len() as u64) as usize;
            read_at
                .read_at(pos, &mut buf[..want])
                .map_err(CoreError::Disk)?;
            hasher.update(&buf[..want]);
            pos += want as u64;
            remaining -= want as u64;
        }
        Ok(())
    };

    if size <= FINGERPRINT_WHOLE_MAX {
        // Small source: hash everything.
        feed(0, size)?;
    } else {
        let w = FINGERPRINT_WINDOW.min(size);
        // 0%, 25%, 50%, 75% windows, then the final MiB.
        for frac in [0u64, 1, 2, 3] {
            let start = (size / 4) * frac;
            let start = start.min(size - w); // clamp so the window fits
            feed(start, w)?;
        }
        feed(size - w, w)?;
    }

    let digest: [u8; 32] = *hasher.finalize().as_bytes();
    Ok(digest)
}

/// Verify that the current source matches a checkpoint's fingerprint.
pub fn verify_source_matches(cp: &ScanCheckpoint, read_at: &dyn ReadAt) -> CoreResult<()> {
    if read_at.size() != cp.source_size {
        return Err(CoreError::Plugin(format!(
            "source size changed: checkpoint {} bytes, current {} bytes",
            cp.source_size,
            read_at.size()
        )));
    }
    let current = fingerprint_source(read_at)?;
    if current != cp.source_fingerprint {
        return Err(CoreError::Plugin(
            "source fingerprint mismatch: the image changed since the checkpoint was saved; \
             refusing to resume (entries would be attributed to the wrong data)"
                .into(),
        ));
    }
    Ok(())
}

/// Restore a ScanHandle from a checkpoint: same scan id, config, state,
/// and results pre-seeded with the checkpoint's entries.
pub fn restore_handle(cp: ScanCheckpoint) -> Arc<ScanHandle> {
    ScanHandle::restore(cp.scan_id, cp.config, cp.state, cp.entries)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use dw_disk::DiskSource;

    struct MemSource {
        data: Vec<u8>,
    }
    impl ReadAt for MemSource {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> dw_disk::DiskResult<()> {
            let s = offset as usize;
            let e = s + buf.len();
            if e > self.data.len() {
                return Err(dw_disk::DiskError::OutOfRange {
                    offset,
                    len: buf.len() as u64,
                    source_size: self.data.len() as u64,
                });
            }
            buf.copy_from_slice(&self.data[s..e]);
            Ok(())
        }
        fn size(&self) -> u64 {
            self.data.len() as u64
        }
    }

    fn sample_source() -> Vec<u8> {
        // 16 MiB, deterministic pattern
        (0..16 * 1024 * 1024).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn fingerprint_is_stable_for_identical_sources() {
        let data = sample_source();
        let a = MemSource { data: data.clone() };
        let b = MemSource { data };
        let fa = fingerprint_source(&a).unwrap();
        let fb = fingerprint_source(&b).unwrap();
        assert_eq!(fa, fb);
    }

    #[test]
    fn fingerprint_changes_when_data_changes() {
        let mut data = sample_source();
        let a = MemSource { data: data.clone() };
        // Flip a byte inside the 50% window
        data[(8 * 1024 * 1024) + 100] ^= 0xFF;
        let b = MemSource { data };
        let fa = fingerprint_source(&a).unwrap();
        let fb = fingerprint_source(&b).unwrap();
        assert_ne!(fa, fb);
    }

    #[test]
    fn small_source_fingerprints_whole() {
        // <= 8 MiB: every byte matters.
        let data: Vec<u8> = (0..1024).map(|i| (i % 17) as u8).collect();
        let a = MemSource { data: data.clone() };
        let fa = fingerprint_source(&a).unwrap();
        // Same content -> same fingerprint (whole-hash path)
        let b = MemSource { data };
        assert_eq!(fa, fingerprint_source(&b).unwrap());
    }

    #[test]
    fn verify_rejects_changed_source() {
        let data = sample_source();
        let cp = ScanCheckpoint {
            version: CHECKPOINT_VERSION,
            scan_id: Uuid::new_v4(),
            created_at: OffsetDateTime::now_utc(),
            source_uri: "test://x".into(),
            source_size: data.len() as u64,
            source_fingerprint: fingerprint_source(&MemSource { data: data.clone() }).unwrap(),
            config: ScanConfig {
                source_uri: "test://x".into(),
                mode: crate::plugin::ScanMode::Deep,
                max_results: 1000,
                include_carving: true,
                file_filter: vec![],
            },
            state: ScanState::DeepScan,
            entries: vec![],
        };

        // Identical source: OK
        assert!(verify_source_matches(&cp, &MemSource { data: data.clone() }).is_ok());

        // Changed source: rejected
        let mut data2 = data;
        let flip_at = data2.len() - 5;
        data2[flip_at] ^= 0x01; // inside the final window [size-1MiB, size)
        let err = verify_source_matches(
            &cp,
            &MemSource {
                data: data2.clone(),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("fingerprint mismatch"), "{err}");

        // Size change: rejected
        data2.truncate(1024);
        let err = verify_source_matches(
            &cp,
            &MemSource {
                data: data2.clone(),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("size changed"), "{err}");
    }

    #[test]
    fn checkpoint_roundtrip_and_restore() {
        use dw_disk::FileBackedSource;
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());

        let data = sample_source();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &data).unwrap();
        let src = FileBackedSource::open(tmp.path()).unwrap();
        let read_at = src.as_read_at();

        // Build a handle with entries.
        let cfg = ScanConfig {
            source_uri: tmp.path().to_string_lossy().to_string(),
            mode: crate::plugin::ScanMode::Deep,
            max_results: 1000,
            include_carving: true,
            file_filter: vec![],
        };
        let handle = ScanHandle::new(cfg);
        handle.set_state(ScanState::DeepScan);
        handle.results().insert(RecoveredEntry {
            id: crate::RecoveredEntryId(Uuid::new_v4()),
            fs_kind: "ntfs".into(),
            parent_path: "\\".into(),
            name: "a.txt".into(),
            size: 10,
            start_lba: Some(2),
            mtime: None,
            ctime: None,
            atime: None,
            status: crate::RecoveryStatus::Intact,
            extension: "txt".into(),
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: vec![2],
            extents: vec![crate::Extent {
                start_byte: 1024,
                length: 10,
            }],
            resident_data: None,
        });

        let id = store.save(&handle, "file://x", read_at.as_ref()).unwrap();
        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].scan_id, id);
        assert_eq!(listed[0].entries, 1);

        let cp = store.load(id).unwrap();
        assert_eq!(cp.entries.len(), 1);
        assert_eq!(cp.entries[0].name, "a.txt");
        assert_eq!(cp.entries[0].extents.len(), 1);

        // Restore: same id, seeded results.
        let restored = restore_handle(cp);
        assert_eq!(restored.id(), handle.id());
        assert_eq!(restored.results().len(), 1);
        assert_eq!(restored.results().snapshot()[0].name, "a.txt");
        assert_eq!(restored.state(), ScanState::DeepScan);

        // Delete
        store.delete(id).unwrap();
        assert!(store.list().is_empty());
        assert!(store.load(id).is_err());
    }

    #[test]
    fn version_mismatch_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let cp = ScanCheckpoint {
            version: 99,
            scan_id: Uuid::new_v4(),
            created_at: OffsetDateTime::now_utc(),
            source_uri: "x".into(),
            source_size: 0,
            source_fingerprint: [0; 32],
            config: ScanConfig {
                source_uri: "x".into(),
                mode: crate::plugin::ScanMode::Quick,
                max_results: 1,
                include_carving: false,
                file_filter: vec![],
            },
            state: ScanState::Done,
            entries: vec![],
        };
        // Serialize the (bogus-version) monolithic v1-shaped blob by hand
        // and place it where load() expects it.
        let bytes = postcard::to_stdvec(&cp).unwrap();
        let path = store.path_for(cp.scan_id);
        std::fs::create_dir_all(&store.dir).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let err = store.load(cp.scan_id).unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");
    }
    #[test]
    fn v1_legacy_monolithic_checkpoint_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let cp = ScanCheckpoint {
            version: 1,
            scan_id: Uuid::new_v4(),
            created_at: OffsetDateTime::now_utc(),
            source_uri: "legacy".into(),
            source_size: 16,
            source_fingerprint: [7; 32],
            config: ScanConfig {
                source_uri: "legacy".into(),
                mode: crate::plugin::ScanMode::Quick,
                max_results: 1,
                include_carving: false,
                file_filter: vec![],
            },
            state: ScanState::Done,
            entries: vec![],
        };
        // v1 files are one monolithic postcard blob.
        let bytes = postcard::to_stdvec(&cp).unwrap();
        let path = store.path_for(cp.scan_id);
        std::fs::create_dir_all(&store.dir).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let loaded = store.load(cp.scan_id).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.source_uri, "legacy");
    }

    #[test]
    fn streamed_save_loads_across_many_pages() {
        // More entries than SAVE_PAGE: the v2 writer must paginate and
        // the loader must rebuild every one.
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().to_path_buf());
        let src = MemSource {
            data: sample_source(),
        };
        let config = ScanConfig {
            source_uri: "many".into(),
            mode: crate::plugin::ScanMode::Deep,
            max_results: 1_000_000,
            include_carving: true,
            file_filter: vec![],
        };
        let handle = ScanHandle::new(config);
        let results = handle.results();
        let total = SAVE_PAGE * 2 + 137; // three pages, ragged last page
        for i in 0..total {
            results.insert(RecoveredEntry {
                id: crate::RecoveredEntryId(Uuid::new_v4()),
                fs_kind: "test".into(),
                parent_path: format!("/d{}", i % 7),
                name: format!("f{i}.bin"),
                size: i as u64,
                start_lba: Some(i as u64),
                mtime: None,
                ctime: None,
                atime: None,
                status: crate::RecoveryStatus::Intact,
                extension: "bin".into(),
                mime: None,
                confidence: 1000,
                was_deleted: false,
                cluster_chain: vec![],
                extents: vec![],
                resident_data: None,
            });
        }
        store.save(&handle, "many", &src).unwrap();
        let loaded = store.load(handle.id()).unwrap();
        assert_eq!(loaded.entries.len(), total);
        assert_eq!(loaded.version, CHECKPOINT_VERSION);
        // Spot-check first and last.
        assert_eq!(loaded.entries[0].name, "f0.bin");
        assert_eq!(
            loaded.entries[total - 1].name,
            format!("f{}.bin", total - 1)
        );
        assert_eq!(loaded.entries[total - 1].size, (total - 1) as u64);
    }
}
