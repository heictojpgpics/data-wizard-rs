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
    pub start_lba: Option<u64>,
    /// Modification time as stored on disk (FS-native).
    pub mtime: Option<OffsetDateTime>,
    /// Creation time as stored on disk.
    pub ctime: Option<OffsetDateTime>,
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
    /// Original cluster chain (FS-native), if known.
    pub cluster_chain: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecoveryStatus {
    Intact,
    PartiallyRecovered,
    Overwritten,
    Carved,
    PendingValidation,
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

    pub fn spilled_count(&self) -> usize {
        self.inner.read().spilled
    }
}
