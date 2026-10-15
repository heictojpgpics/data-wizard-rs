//! In-memory + on-disk results store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

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
}

/// A bounded, thread-safe store of [`RecoveredEntry`]s.
///
/// Backing strategy:
///  - Hot index in memory (`HashMap<RecoveredEntryId, RecoveredEntry>`).
///  - Eviction: once `capacity` entries are reached, additional entries
///    spill to an on-disk `spill.bin` (length-prefixed postcard frames).
///  - The hot index keeps the most-recently-added entries; older entries
///    are queried lazily from disk.
pub struct ResultsStore {
    capacity: usize,
    inner: RwLock<Inner>,
    /// Reserved for the spill-to-disk path (not yet implemented).
    #[allow(dead_code)]
    spill_path: Option<PathBuf>,
}

struct Inner {
    entries: HashMap<RecoveredEntryId, RecoveredEntry>,
    by_path: HashMap<String, RecoveredEntryId>,
    spilled: usize,
}

impl ResultsStore {
    #[must_use]
    pub fn new(capacity: usize, spill_path: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            inner: RwLock::new(Inner {
                entries: HashMap::new(),
                by_path: HashMap::new(),
                spilled: 0,
            }),
            spill_path,
        })
    }

    pub fn insert(&self, entry: RecoveredEntry) -> RecoveredEntryId {
        let id = entry.id;
        let mut g = self.inner.write();
        let path = format!("{}/{}", entry.parent_path, entry.name);
        g.by_path.insert(path, id);
        if g.entries.len() >= self.capacity {
            g.spilled += 1;
            // For now: keep the newest entries, drop the oldest. The
            // spill-to-disk path will be implemented in a follow-up.
            // (Reference behaviour: scan keeps going, UI paginates.)
        }
        g.entries.insert(id, entry);
        id
    }

    pub fn get(&self, id: RecoveredEntryId) -> Option<RecoveredEntry> {
        self.inner.read().entries.get(&id).cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.read().entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.read().entries.is_empty()
    }

    pub fn snapshot(&self) -> Vec<RecoveredEntry> {
        self.inner.read().entries.values().cloned().collect()
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
        let before = g.entries.len();
        // Keep set = latest ids per path (from by_path).
        let keep: std::collections::HashSet<RecoveredEntryId> =
            g.by_path.values().copied().collect();
        g.entries.retain(|id, _| keep.contains(id));
        g.by_path.retain(|_, id| keep.contains(id));
        before - g.entries.len()
    }

    pub fn spilled_count(&self) -> usize {
        self.inner.read().spilled
    }
}
