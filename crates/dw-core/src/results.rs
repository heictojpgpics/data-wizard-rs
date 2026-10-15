//! Bounded results store: hot entries in memory, overflow on disk.
//!
//! Backing strategy (the deep-scan memory contract):
//!  - Hot index in memory (`HashMap<RecoveredEntryId, RecoveredEntry>`),
//!    bounded at `capacity` entries.
//!  - Overflow spills to an append-only file of length-prefixed postcard
//!    frames. A small in-memory index maps id -> (offset, len), so a
//!    spilled entry costs ~24 bytes of RAM instead of a full clone of the
//!    entry (names, extents, cluster chains).
//!  - Insertion order is preserved (`order` deque) so paging is stable
//!    and dedup can walk every id, resident or spilled.
//!  - When no spill path is configured the store stays purely in-memory
//!    (unit tests, checkpoint restore seeds).
//!
//! The spill file is a temporary structure of one scan session: it is
//! deleted when the store's last reference goes away. Durable state
//! belongs to checkpoints, not to the spill.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::CoreError;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoveredEntryId(pub Uuid);

/// A canonical physical extent on the scan source: `start_byte .. start_byte + length`.
///
/// Extents are absolute byte offsets into the *scan source* (image file or
/// raw device). They are the single source of truth for recovery I/O: the
/// recovery writer walks these extents in order, which handles fragmented
/// files naturally (FAT chains, NTFS run lists, contiguous carver ranges).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    /// Absolute byte offset of this extent on the source.
    pub start_byte: u64,
    /// Length of this extent in bytes.
    pub length: u64,
}

impl Extent {
    /// Byte range covered by this extent.
    pub fn range(&self) -> std::ops::Range<u64> {
        self.start_byte..self.start_byte + self.length
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveredEntry {
    pub id: RecoveredEntryId,
    pub fs_kind: String,
    pub parent_path: String,
    pub name: String,
    pub size: u64,
    /// First LBA on the source disk where this entry's data begins.
    /// `None` for entries recovered from carving where the original FS
    /// metadata is gone.
    ///
    /// Derived from `extents` when populated (see `recompute_start_lba`).
    pub start_lba: Option<u64>,
    /// Modification time as stored on disk (FS-native).
    pub mtime: Option<OffsetDateTime>,
    /// Creation time as stored on disk.
    pub ctime: Option<OffsetDateTime>,
    /// Last-access time as stored on disk (FAT has date-only; NTFS/exFAT full).
    /// NOTE: no `skip_serializing_if` here — postcard checkpoints are a
    /// positional format; a skipped field desynchronizes the reader.
    #[serde(default)]
    pub atime: Option<OffsetDateTime>,
    /// Recovery status -- is the data intact, partially recovered, etc.
    pub status: RecoveryStatus,
    /// Recovered file extension (lowercase, no dot).
    pub extension: String,
    /// Carver-assigned MIME type if known.
    pub mime: Option<String>,
    /// Carver confidence 0..=1000.
    pub confidence: u16,
    /// Whether the entry was deleted from the source filesystem.
    pub was_deleted: bool,
    /// Original cluster chain (FS-native), if known. Display/diagnostic use;
    /// recovery I/O uses `extents` instead.
    pub cluster_chain: Vec<u64>,
    /// Canonical physical extents on the source, in file order. Populated by
    /// the FS walkers (FAT chain, NTFS run list, exFAT chain/contiguous) and
    /// by the carver (validated header..footer range). Empty when the
    /// physical layout is unknown.
    /// (No `skip_serializing_if`: positional serialization formats
    /// (postcard checkpoints) cannot signal an absent field.)
    #[serde(default)]
    pub extents: Vec<Extent>,
    /// Inline (resident) file content, for filesystems that store small
    /// files inside metadata (NTFS resident $DATA). Takes priority over
    /// `extents` during recovery.
    #[serde(default)]
    pub resident_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecoveryStatus {
    Intact,
    PartiallyRecovered,
    Overwritten,
    Carved,
    /// Header matched and the format-specific analyser (JPEG EOI / PNG IEND /
    /// ZIP EOCD walk) confirmed the structure. Highest carver confidence.
    CarvedValidated,
    PendingValidation,
}

impl RecoveredEntry {
    /// Recompute `start_lba` from `extents` (first extent start / 512).
    /// Call after populating extents so the LBA is physically meaningful
    /// (FS cluster numbers are NOT LBAs and must not leak into this field).
    pub fn recompute_start_lba(&mut self) {
        self.start_lba = self.extents.first().map(|e| e.start_byte / 512);
    }

    /// Full display path (`parent/name`).
    #[must_use]
    pub fn path(&self) -> String {
        format!("{}/{}", self.parent_path, self.name)
    }
}

// ---------------------------------------------------------------------------
// Spill file: append-only length-prefixed postcard frames
// ---------------------------------------------------------------------------

/// One spilled entry in the spill file: `u32 LE frame length` + postcard bytes.
const SPILL_FRAME_HEADER: u64 = 4;

struct Spill {
    file: std::fs::File,
    path: PathBuf,
    /// id -> (offset, frame_len) of the postcard bytes (header excluded).
    index: HashMap<RecoveredEntryId, (u64, u32)>,
    end: u64,
}

impl Spill {
    fn create(path: PathBuf) -> Result<Self, CoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CoreError::Spill(format!("create spill dir: {e}")))?;
        }
        // A stale file from a previous session must never be appended to:
        // remove it and start fresh. (OpenOptions::truncate cannot be
        // combined with read+append without explicit write access, so the
        // unlink-then-create form is the portable one.)
        let _ = std::fs::remove_file(&path);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|e| CoreError::Spill(format!("create spill file: {e}")))?;
        Ok(Self {
            file,
            path,
            index: HashMap::new(),
            end: 0,
        })
    }

    /// Append one entry; returns its (offset, len) frame location.
    fn append(&mut self, entry: &RecoveredEntry) -> Result<(u64, u32), CoreError> {
        let bytes = postcard::to_allocvec(entry)
            .map_err(|e| CoreError::Spill(format!("serialize: {e}")))?;
        let len = u32::try_from(bytes.len())
            .map_err(|_| CoreError::Spill("frame exceeds u32 length".into()))?;
        let mut frame = Vec::with_capacity(SPILL_FRAME_HEADER as usize + bytes.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(&bytes);
        self.file
            .write_all(&frame)
            .map_err(|e| CoreError::Spill(format!("write: {e}")))?;
        let offset = self.end + SPILL_FRAME_HEADER;
        self.end += frame.len() as u64;
        Ok((offset, len))
    }

    /// Read one entry back (positional read: safe under a shared read lock).
    fn read(&self, id: RecoveredEntryId) -> Option<RecoveredEntry> {
        let (offset, len) = *self.index.get(&id)?;
        let mut header = [0u8; SPILL_FRAME_HEADER as usize];
        read_positional(&self.file, offset - SPILL_FRAME_HEADER, &mut header)?;
        let frame_len = u32::from_le_bytes(header);
        if frame_len != len {
            return None; // corrupt frame: treat as missing
        }
        let mut buf = vec![0u8; len as usize];
        read_positional(&self.file, offset, &mut buf)?;
        postcard::from_bytes(&buf).ok()
    }
}

impl Drop for Spill {
    fn drop(&mut self) {
        // Best-effort cleanup: the spill is session-scoped scratch space.
        let _ = self.file.flush();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Positional read that fills `buf` entirely or returns None.
/// Uses pread/seek_read so concurrent readers never race the file cursor.
fn read_positional(file: &std::fs::File, offset: u64, buf: &mut [u8]) -> Option<()> {
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileExt;
                file.read_at(&mut buf[filled..], offset + filled as u64)
                    .ok()?
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::FileExt;
                file.seek_read(&mut buf[filled..], offset + filled as u64)
                    .ok()?
            }
        };
        if n == 0 {
            return None;
        }
        filled += n;
    }
    Some(())
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

/// A bounded, thread-safe store of [`RecoveredEntry`]s (see the module docs
/// for the memory/spill contract).
pub struct ResultsStore {
    capacity: usize,
    inner: RwLock<Inner>,
}

struct Inner {
    /// Hot entries (bounded at `capacity`).
    entries: HashMap<RecoveredEntryId, RecoveredEntry>,
    /// Insertion order of ALL ids (resident and spilled).
    order: VecDeque<RecoveredEntryId>,
    by_path: HashMap<String, RecoveredEntryId>,
    /// Active spill (None until the first eviction, or when disabled).
    spill: Option<Spill>,
    spill_path: Option<PathBuf>,
    /// Total entries currently living in the spill file.
    spilled: usize,
    /// Frames orphaned by dedup (kept for diagnostics; bytes stay until drop).
    orphans: usize,
}

impl ResultsStore {
    #[must_use]
    pub fn new(capacity: usize, spill_path: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            capacity: capacity.max(1),
            inner: RwLock::new(Inner {
                entries: HashMap::new(),
                order: VecDeque::new(),
                by_path: HashMap::new(),
                spill: None,
                spill_path,
                spilled: 0,
                orphans: 0,
            }),
        })
    }

    /// Insert an entry. Once the hot set is at capacity the OLDEST resident
    /// entry evicts to the spill file (when configured); with no spill path
    /// the store keeps everything in memory (unit tests / restore seeds).
    /// If the spill write fails the victim stays resident (fail open:
    /// memory pressure beats silent data loss).
    pub fn insert(&self, entry: RecoveredEntry) -> RecoveredEntryId {
        let id = entry.id;
        let mut g = self.inner.write();
        let path = format!("{}/{}", entry.parent_path, entry.name);
        g.by_path.insert(path, id);

        // Evict the oldest RESIDENT entry when the hot set is full and this
        // insert would grow it. (Spilled ids stay in `order` so paging over
        // the full result set keeps working.)
        if g.entries.len() >= self.capacity
            && g.spill_path.is_some()
            && !g.entries.contains_key(&id)
        {
            let victim = g
                .order
                .iter()
                .copied()
                .find(|old| g.entries.contains_key(old));
            if let Some(victim_id) = victim {
                let Some(victim_entry) = g.entries.remove(&victim_id) else {
                    unreachable!("victim was just confirmed resident")
                };
                if g.spill.is_none() {
                    g.spill = g
                        .spill_path
                        .clone()
                        .and_then(|path| Spill::create(path).ok());
                }
                let spilled_ok = match g.spill.as_mut() {
                    Some(spill) => match spill.append(&victim_entry) {
                        Ok(loc) => {
                            spill.index.insert(victim_id, loc);
                            g.spilled += 1;
                            true
                        }
                        Err(_) => false,
                    },
                    None => false,
                };
                if !spilled_ok {
                    // Spill unavailable/failed: keep the entry resident.
                    g.entries.insert(victim_id, victim_entry);
                }
            }
        }

        g.order.push_back(id);
        g.entries.insert(id, entry);
        id
    }

    /// Total entries in the store (resident + spilled).
    pub fn len(&self) -> usize {
        let g = self.inner.read();
        g.entries.len() + g.spilled
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, id: RecoveredEntryId) -> Option<RecoveredEntry> {
        let g = self.inner.read();
        if let Some(e) = g.entries.get(&id) {
            return Some(e.clone());
        }
        g.spill.as_ref()?.read(id)
    }

    /// Page through results in insertion order. `offset`/`limit` span the
    /// FULL result set (resident + spilled), so the UI paginates over
    /// everything the scan found without cloning the whole store.
    pub fn page(&self, offset: usize, limit: usize) -> Vec<RecoveredEntry> {
        if limit == 0 {
            return Vec::new();
        }
        let g = self.inner.read();
        let mut out = Vec::with_capacity(limit.min(g.order.len()));
        for id in g.order.iter().skip(offset).take(limit) {
            if let Some(e) = g.entries.get(id) {
                out.push(e.clone());
            } else if let Some(e) = g.spill.as_ref().and_then(|s| s.read(*id)) {
                out.push(e);
            }
        }
        out
    }

    pub fn snapshot(&self) -> Vec<RecoveredEntry> {
        self.page(0, self.len())
    }

    /// Collapse duplicate entries, keeping the LATEST occurrence per
    /// (parent path, name) — `by_path` already holds the latest id per
    /// path because `insert` overwrites it. The latest pass has the most
    /// complete extents/timestamps, so it wins.
    ///
    /// Used after a resumed scan re-runs the plugin pass over restored
    /// entries: identical findings appear twice (once restored, once
    /// re-walked). Returns the number of removed entries.
    pub fn dedup_by_identity(&self) -> usize {
        let mut g = self.inner.write();
        let keep: std::collections::HashSet<RecoveredEntryId> =
            g.by_path.values().copied().collect();
        let before = g.entries.len() + g.spilled;

        // Drain into a Vec first: iterating a drain while mutating other
        // fields through the same guard is a borrow violation.
        let drained: Vec<RecoveredEntryId> = g.order.drain(..).collect();
        let mut new_order = VecDeque::with_capacity(drained.len());
        for id in drained {
            if keep.contains(&id) {
                new_order.push_back(id);
            } else if g.entries.remove(&id).is_none() {
                // It lived in the spill: drop the index entry (bytes become
                // orphaned scratch, reclaimed when the spill file drops).
                if g.spill
                    .as_mut()
                    .is_some_and(|s| s.index.remove(&id).is_some())
                {
                    g.spilled = g.spilled.saturating_sub(1);
                    g.orphans += 1;
                }
            }
        }
        g.order = new_order;
        g.by_path.retain(|_, id| keep.contains(id));
        before - (g.entries.len() + g.spilled)
    }

    /// Entries currently living in the spill file.
    pub fn spilled_count(&self) -> usize {
        self.inner.read().spilled
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn entry(id: u16, name: &str, size: u64) -> RecoveredEntry {
        RecoveredEntry {
            id: RecoveredEntryId(Uuid::from_u64_pair(u64::from(id), 0)),
            fs_kind: "test".into(),
            parent_path: "/".into(),
            name: name.into(),
            size,
            start_lba: Some(0),
            mtime: None,
            ctime: None,
            atime: None,
            status: RecoveryStatus::Intact,
            extension: "txt".into(),
            mime: None,
            confidence: 1000,
            was_deleted: false,
            cluster_chain: vec![],
            extents: vec![],
            resident_data: None,
        }
    }

    fn tmpdir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[test]
    fn page_walks_insertion_order() {
        let store = ResultsStore::new(100, None);
        for i in 0..10 {
            store.insert(entry(i, &format!("f{i}"), i as u64));
        }
        assert_eq!(store.len(), 10);
        let page = store.page(0, 4);
        assert_eq!(page.len(), 4);
        assert_eq!(page[0].name, "f0");
        assert_eq!(page[3].name, "f3");
        let tail = store.page(8, 10);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[1].name, "f9");
        let past = store.page(50, 10);
        assert!(past.is_empty());
    }

    #[test]
    fn spill_evicts_oldest_and_reads_back() {
        let dir = tmpdir();
        let store = ResultsStore::new(4, Some(dir.join("s.bin")));
        for i in 0..10 {
            store.insert(entry(i, &format!("f{i}"), 100 + u64::from(i)));
        }
        // All ten visible, only 4 resident.
        assert_eq!(store.len(), 10);
        assert_eq!(store.spilled_count(), 6);

        // Oldest ids readable (from spill), newest from memory.
        let e0 = store.get(entry(0, "", 0).id).expect("oldest spilled entry");
        assert_eq!(e0.name, "f0");
        assert_eq!(e0.size, 100);
        let e9 = store.get(entry(9, "", 0).id).expect("newest resident");
        assert_eq!(e9.name, "f9");

        // Paging spans both tiers in insertion order.
        let all = store.page(0, 100);
        assert_eq!(all.len(), 10);
        assert_eq!(all[0].name, "f0");
        assert_eq!(all[9].name, "f9");
    }

    #[test]
    fn spill_file_removed_after_drop() {
        let dir = tmpdir();
        let path = dir.join("gone.bin");
        {
            let store = ResultsStore::new(2, Some(path.clone()));
            for i in 0..5 {
                store.insert(entry(i, &format!("f{i}"), 1));
            }
            assert!(path.exists());
        }
        // Last Arc reference dropped -> spill file cleaned up.
        assert!(!path.exists());
    }

    #[test]
    fn dedup_removes_orphans_across_both_tiers() {
        let dir = tmpdir();
        let store = ResultsStore::new(3, Some(dir.join("d.bin")));
        // f0 twice: first insert (evicts to spill), then a refreshed copy
        // with a NEW id and a bigger size (the "latest wins" case).
        store.insert(entry(0, "dup.txt", 10));
        for i in 1..5 {
            store.insert(entry(i, &format!("f{i}"), u64::from(i)));
        }
        let latest = entry(200, "dup.txt", 99);
        let latest_id = latest.id;
        store.insert(latest);

        let removed = store.dedup_by_identity();
        assert_eq!(removed, 1, "the older dup.txt twin must be removed");
        assert_eq!(store.len(), 5);
        assert_eq!(store.get(latest_id).unwrap().size, 99);
        assert!(store.get(entry(0, "", 0).id).is_none());
    }

    #[test]
    fn no_spill_path_keeps_everything_resident() {
        let store = ResultsStore::new(2, None);
        for i in 0..6 {
            store.insert(entry(i, &format!("f{i}"), 1));
        }
        assert_eq!(store.len(), 6);
        assert_eq!(store.spilled_count(), 0);
        assert!(store.get(entry(0, "", 0).id).is_some());
    }
}
