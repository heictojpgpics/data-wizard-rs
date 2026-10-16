//! Scan orchestration.
//!
//! Quick pass: every window of the volume is scanned for the two
//! validated record shapes (name records, file-info records). Records
//! are joined by file id into entries: a file surfaces only when both
//! shapes are present, with the exact size and the four FILETIME
//! timestamps.
//!
//! Deep pass: the same scan looks for ids the quick pass did not see.
//! ReFS is copy-on-write at the page level: older page generations
//! persist as headless cluster copies, so an id that only appears there
//! is emitted with `was_deleted`.
//!
//! Extents are not decoded in v1 (the data-run table walk is the
//! documented roadmap); entries carry metadata-only status.

use crate::error::RefsResult;
use crate::record::{parse_fileinfo, parse_name_record, FileInfoRecord, NameRecord};
use dw_core::progress::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_core::results::{RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use uuid::Uuid;

const CONF_LIVE: u16 = 1000;
const CONF_DELETED: u16 = 800;
const MAX_ENTRIES: u64 = 1 << 22;
const MAX_RECORDS: usize = 1 << 20;
/// Scan window (1 MiB), cluster-multiple.
const WINDOW: usize = 1 << 20;

struct Collected {
    names: HashMap<u64, NameRecord>,
    infos: HashMap<u64, FileInfoRecord>,
}

pub struct Walker {
    src: Arc<dyn dw_disk::ReadAt>,
    size: u64,
    cancel: Arc<AtomicBool>,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    emitted: u64,
    live_ids: HashSet<u64>,
}

impl Walker {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    async fn tick(&self, phase: ProgressPhase, msg: &str) {
        self.progress
            .emit(ProgressEvent {
                scan_id: Uuid::nil(),
                phase,
                current: self.emitted,
                total: 0,
                message: msg.to_string(),
            })
            .await;
    }

    fn scan_all(&self) -> Collected {
        let mut names = HashMap::new();
        let mut infos = HashMap::new();
        let mut records = 0usize;
        let mut win_off = 0u64;
        while win_off < self.size {
            if self.cancelled() {
                break;
            }
            let len = WINDOW.min((self.size - win_off) as usize);
            let mut buf = vec![0u8; len];
            if self.src.read_at(win_off, &mut buf).is_err() {
                win_off += len as u64;
                continue;
            }
            let mut off = 0usize;
            while off + 0xa8 <= buf.len() && records < MAX_RECORDS {
                if let Some(nr) = parse_name_record(&buf, off) {
                    names.insert(nr.id, nr);
                    records += 1;
                    off += 0x30;
                    continue;
                }
                if let Some(fi) = parse_fileinfo(&buf, off) {
                    infos.insert(fi.id, fi);
                    records += 1;
                    off += 0xa8;
                    continue;
                }
                off += 2;
            }
            win_off += len as u64;
        }
        Collected { names, infos }
    }

    pub async fn quick(&mut self) -> RefsResult<()> {
        self.tick(ProgressPhase::QuickScan, "refs: scanning for records")
            .await;
        let c = self.scan_all();
        info!(names = c.names.len(), infos = c.infos.len(), "refs: quick pass");
        self.emit(&c.names, &c.infos, false);
        Ok(())
    }

    pub async fn deep(&mut self) -> RefsResult<()> {
        self.tick(
            ProgressPhase::DeepScan,
            "refs: scanning for old-generation records",
        )
        .await;
        let mut c = self.scan_all();
        // Only ids never seen live surface as deleted.
        c.names.retain(|id, _| !self.live_ids.contains(id));
        let new_infos: HashMap<u64, FileInfoRecord> = c
            .infos
            .into_iter()
            .filter(|(id, _)| !self.live_ids.contains(id))
            .collect();
        info!(names = c.names.len(), "refs: deep pass delta");
        self.emit(&c.names, &new_infos, true);
        Ok(())
    }

    /// Emit joined entries (name + info both required).
    fn emit(&mut self, names: &HashMap<u64, NameRecord>, infos: &HashMap<u64, FileInfoRecord>, deleted: bool) {
        let mut ids: Vec<u64> = names
            .keys()
            .filter(|id| infos.contains_key(id))
            .copied()
            .collect();
        ids.sort_unstable();
        for id in ids {
            if self.emitted >= MAX_ENTRIES {
                return;
            }
            self.live_ids.insert(id);
            let nr = &names[&id];
            let fi = &infos[&id];
            let ext = nr.name.rsplit('.').next().unwrap_or("").to_lowercase();
            let extension = if nr.name.contains('.') && !ext.is_empty() && ext.len() <= 10 {
                ext
            } else {
                String::new()
            };
            let _ = self.results.insert(RecoveredEntry {
                id: RecoveredEntryId(Uuid::now_v7()),
                fs_kind: "refs".to_string(),
                parent_path: "/".to_string(),
                name: nr.name.clone(),
                size: fi.size,
                start_lba: None,
                mtime: fi.mtime,
                ctime: fi.ctime.or(fi.crtime),
                atime: fi.atime,
                // Metadata recovered, extents pending (v1 roadmap).
                status: RecoveryStatus::PartiallyRecovered,
                extension,
                mime: None,
                confidence: if deleted { CONF_DELETED } else { CONF_LIVE },
                was_deleted: deleted,
                cluster_chain: Vec::new(),
                extents: Vec::new(),
                resident_data: None,
            });
            self.emitted += 1;
        }
    }
}

/// Entry point used by the plugin.
pub async fn scan_refs(
    source: Arc<dyn dw_disk::DiskSource>,
    mode: dw_core::ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
) -> RefsResult<()> {
    let read = source.as_read_at();
    let mut w = Walker {
        src: read.clone(),
        size: read.size(),
        cancel,
        results,
        progress,
        emitted: 0,
        live_ids: HashSet::new(),
    };
    match mode {
        dw_core::ScanMode::Quick => w.quick().await?,
        dw_core::ScanMode::Deep => {
            w.quick().await?;
            w.deep().await?;
        }
    }
    info!(entries = w.emitted, "refs walk complete");
    Ok(())
}
