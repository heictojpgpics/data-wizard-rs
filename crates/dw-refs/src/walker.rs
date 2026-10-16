//! Scan orchestration.
//!
//! Quick pass: locate headed `MSB+` metadata pages (cluster-aligned)
//! and parse the two validated record shapes inside them. Records are
//! joined by file id: a file surfaces only when both its name record
//! and its file-info record are present, with the exact size and the
//! four FILETIME timestamps.
//!
//! Deep pass, three sources of recovery:
//!  1. Headed-page records (same as quick — emitted as live).
//!  2. Headless old-generation pages: ReFS is copy-on-write at the
//!     page level, so superseded page generations persist as headerless
//!     cluster copies. Records found there for ids the live tree never
//!     surfaced are emitted with `was_deleted`.
//!  3. Signature carving over every non-metadata region (the free/
//!     overwritten space where deleted content actually lives), giving
//!     content-backed entries for files whose metadata is gone — the
//!     same `header_footer` engine the ext plugin uses for free runs.
//!
//! Extents are not decoded (ReFS stores data runs behind the container
//! / block-allocation indirection; that walk is future work), so
//! record-based entries carry metadata-only status and content
//! recovery flows through the carving pass.

use crate::error::RefsResult;
use crate::page::find_headed_pages;
use crate::record::{FileInfoRecord, NameRecord, parse_fileinfo, parse_name_record};
use dw_core::progress::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_core::results::{RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use uuid::Uuid;

const CONF_LIVE: u16 = 1000;
const CONF_DELETED: u16 = 800;
const CONF_CARVED: u16 = 700;
const MAX_ENTRIES: u64 = 1 << 22;
const MAX_RECORDS: usize = 1 << 20;
/// Scan window (1 MiB); a cluster multiple for both 4 KiB and 64 KiB
/// cluster sizes, so pages never straddle window boundaries.
const WINDOW: usize = 1 << 20;
/// Validated page header size: records start at +0x80 inside an
/// `MSB+` page.
const PAGE_HEADER: usize = 0x80;

struct Collected {
    names: HashMap<u64, NameRecord>,
    infos: HashMap<u64, FileInfoRecord>,
}

impl Collected {
    fn new() -> Self {
        Self {
            names: HashMap::new(),
            infos: HashMap::new(),
        }
    }
}

pub struct Walker {
    src: Arc<dyn dw_disk::ReadAt>,
    size: u64,
    cluster: u64,
    cancel: Arc<AtomicBool>,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    emitted: u64,
    records_seen: usize,
    live_ids: HashSet<u64>,
    /// Headed-page cluster indices, cached from the quick pass for the
    /// deep pass's region exclusion.
    headed_clusters: HashSet<u64>,
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

    /// Parse the two record shapes inside `region`, joining by id.
    fn parse_region(&mut self, region: &[u8], out: &mut Collected) {
        let mut off = 0usize;
        while off + 4 <= region.len() && self.records_seen < MAX_RECORDS {
            if let Some(nr) = parse_name_record(region, off) {
                let adv = nr.record_len;
                out.names.insert(nr.id, nr);
                self.records_seen += 1;
                off += adv;
                continue;
            }
            if let Some(fi) = parse_fileinfo(region, off) {
                out.infos.insert(fi.id, fi);
                self.records_seen += 1;
                off += crate::record::FILEINFO_LEN as usize;
                continue;
            }
            off += 2;
        }
    }

    /// One windowed read pass collecting records from every headed
    /// page, and (optionally) recording which clusters are covered by
    /// headed pages so the deep pass can exclude them.
    fn scan_headed_pages(&mut self, mark_clusters: bool) -> Collected {
        let mut out = Collected::new();
        let cluster = self.cluster as usize;
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
            let mut hits = Vec::new();
            find_headed_pages(&buf, win_off, self.cluster, &mut hits);
            for hit in hits {
                let local = (hit.offset - win_off) as usize;
                if mark_clusters {
                    self.headed_clusters.insert(hit.offset / self.cluster);
                }
                // Records live in [page + 0x80, page + cluster).
                let start = local + PAGE_HEADER;
                let end = local + cluster;
                if start < end && end <= buf.len() {
                    self.parse_region(&buf[start..end], &mut out);
                }
            }
            win_off += len as u64;
        }
        out
    }

    /// Parse records in every cluster NOT covered by a headed page —
    /// the headless old-generation copies.
    fn scan_headless_regions(&mut self) -> Collected {
        let mut out = Collected::new();
        let cluster = self.cluster as usize;
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
            // Every cluster in the window that is not a headed page is
            // a headless-region candidate.
            let mut local = 0usize;
            while local + cluster <= buf.len() {
                let global_cluster = (win_off + local as u64) / self.cluster;
                if !self.headed_clusters.contains(&global_cluster) {
                    self.parse_region(&buf[local..local + cluster], &mut out);
                }
                local += cluster;
            }
            win_off += len as u64;
        }
        out
    }

    /// Signature carving over every non-headed region: content-backed
    /// entries for deleted files whose records are gone. Mirrors the
    /// ext plugin's free-run carve.
    fn carve_regions(&self) -> u64 {
        let db = dw_signatures::SignatureDb::from_embedded();
        let cluster = self.cluster;
        let mut carved = 0u64;
        let mut off = 0u64;
        while off < self.size {
            if self.cancelled() {
                break;
            }
            let len = WINDOW.min((self.size - off) as usize) as u64;
            let region_cluster = off / cluster;
            let end_cluster = (off + len).div_ceil(cluster);
            // Skip leading covered clusters; carve the longest
            // uncovered run in this window.
            let mut run_start = None;
            for c in region_cluster..end_cluster {
                let covered = self.headed_clusters.contains(&c);
                match (covered, run_start) {
                    (false, None) => run_start = Some(c),
                    (true, Some(s)) => {
                        carved += self.carve_run(s, c, &db);
                        run_start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = run_start {
                carved += self.carve_run(s, end_cluster, &db);
            }
            off += len;
        }
        carved
    }

    fn carve_run(
        &self,
        first_cluster: u64,
        end_cluster: u64,
        db: &dw_signatures::SignatureDb,
    ) -> u64 {
        let start = first_cluster * self.cluster;
        let len = (end_cluster - first_cluster) * self.cluster;
        if len == 0 || start >= self.size {
            return 0;
        }
        let len = len.min(self.size - start);
        dw_carver::header_footer::carve_region(
            self.src.as_ref(),
            start,
            len,
            db,
            &self.results,
            Some(&self.cancel),
            None,
        )
    }

    pub async fn quick(&mut self) -> RefsResult<()> {
        self.tick(ProgressPhase::QuickScan, "refs: scanning metadata pages")
            .await;
        let c = self.scan_headed_pages(true);
        info!(
            pages = self.headed_clusters.len(),
            names = c.names.len(),
            infos = c.infos.len(),
            "refs: quick pass"
        );
        self.emit(&c.names, &c.infos, false);
        Ok(())
    }

    pub async fn deep(&mut self) -> RefsResult<()> {
        self.tick(
            ProgressPhase::DeepScan,
            "refs: scanning old-generation records",
        )
        .await;
        let mut c = self.scan_headless_regions();
        // Only ids never seen live surface as deleted.
        c.names.retain(|id, _| !self.live_ids.contains(id));
        c.infos.retain(|id, _| !self.live_ids.contains(id));
        info!(
            names = c.names.len(),
            infos = c.infos.len(),
            "refs: deep pass delta"
        );
        self.emit(&c.names, &c.infos, true);

        // Content carving over the non-metadata regions.
        self.tick(ProgressPhase::Carving, "refs: carving deleted content")
            .await;
        let carved = self.carve_regions();
        info!(carved, "refs: content carving complete");
        Ok(())
    }

    /// Emit joined entries (name + info both required).
    fn emit(
        &mut self,
        names: &HashMap<u64, NameRecord>,
        infos: &HashMap<u64, FileInfoRecord>,
        deleted: bool,
    ) {
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
                // Metadata recovered; content flows through the
                // carving pass (ReFS data runs are container-indirect).
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
    let size = read.size();

    // Cluster size from the boot sector (4 KiB or 64 KiB); the scan
    // grid must match the volume's page alignment.
    let mut boot = vec![0u8; 512];
    let cluster = if size >= 512 && read.read_at(0, &mut boot).is_ok() {
        crate::boot::BootSector::parse(&boot)
            .map(|b| b.cluster_size())
            .unwrap_or(4096)
    } else {
        4096
    };

    let mut w = Walker {
        src: read.clone(),
        size,
        cluster,
        cancel,
        results,
        progress,
        emitted: 0,
        records_seen: 0,
        live_ids: HashSet::new(),
        headed_clusters: HashSet::new(),
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

/// Confidence assigned to carved content entries (test channel).
#[allow(dead_code)]
pub const CARVED_CONFIDENCE: u16 = CONF_CARVED;
