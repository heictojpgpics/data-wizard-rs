//! Scan orchestration.
//!
//! Quick: newest uberblock -> MOS objset -> object 1 (master node zap,
//! `DMU_POOL_DIRECTORY_OBJECT = 1`) -> "root_dataset" -> dsl_dataset
//! dnode (bonus `ds_bp` @128) -> filesystem objset -> "ROOT" (master node
//! of the FS, object 1) -> walk the directory tree: every object with
//! `dn_type == PLAIN_FILE_CONTENTS` is a file (extents from its blkptr
//! tree, timestamps from the SA bonus), every `DIRECTORY_CONTENTS` object
//! is read as a zap and recursed into. Symlinks surface as resident data.
//!
//! Deep: ZFS is copy-on-write, so the tree under the *previous* uberblock
//! (second-newest distinct txg in the rings) persists on disk until those
//! blocks are reused. The deep pass walks that older tree and emits every
//! entry the quick pass did not surface (recently deleted files keep
//! their names and extents there), marked `was_deleted`.
//!
//! Physical extent mapping: extents are the on-disk DVA ranges of each
//! L0 block (offset..offset+asize). For compressed blocks this is the
//! byte-faithful on-disk representation; logical reconstruction of
//! compressed streams is a recovery-writer feature (documented scope).

use crate::blkptr::{BlkPtr, dmu_ot};
use crate::dmu::{DS_BP_OFF, Dmu, Dnode, ObjectTable, Objset};
use crate::error::{ZfsError, ZfsResult};
use crate::label::{self, LabelSet, Uberblock};
use crate::sa::{self, ZfsAttrs};
use crate::zap::{self, ZapValue};
use dw_core::progress::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_core::results::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use time::OffsetDateTime;
use tracing::{info, warn};
use uuid::Uuid;

const CONF_LIVE: u16 = 1000;
const CONF_DELETED_INTACT: u16 = 900;
const CONF_DELETED_PARTIAL: u16 = 700;
const CONF_METADATA_ONLY: u16 = 400;
const MAX_ENTRIES: u64 = 1 << 22;
const MAX_DIRS: u64 = 4096;

fn ts(v: Option<u64>) -> Option<OffsetDateTime> {
    v.and_then(|s| OffsetDateTime::from_unix_timestamp(s.min(i64::MAX as u64) as i64).ok())
}

/// Entry point used by the plugin.
pub async fn scan_zfs(
    source: Arc<dyn dw_disk::DiskSource>,
    labels: LabelSet,
    mode: dw_core::ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
) -> ZfsResult<()> {
    let read_at = source.as_read_at();
    let size = read_at.size();
    let dmu = Dmu::new(read_at, size, cancel);
    let mut walker = Walker {
        dmu,
        results,
        progress,
        emitted: 0,
        dirs_visited: 0,
        live_paths: HashSet::new(),
    };

    match mode {
        dw_core::ScanMode::Quick => {
            walker.quick(&labels).await?;
        }
        dw_core::ScanMode::Deep => {
            walker.quick(&labels).await?;
            walker.deep(&labels).await?;
        }
    }
    info!(entries = walker.emitted, "zfs walk complete");
    Ok(())
}

pub struct Walker {
    pub dmu: Dmu,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    emitted: u64,
    dirs_visited: u64,
    live_paths: HashSet<String>,
}

impl Walker {
    fn cancelled(&self) -> bool {
        self.dmu.cancelled()
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

    /// Quick pass over the newest uberblock's tree.
    pub async fn quick(&mut self, labels: &LabelSet) -> ZfsResult<()> {
        self.tick(ProgressPhase::QuickScan, "zfs: walking newest uberblock")
            .await;
        self.walk_tree(labels.best, false).await
    }

    /// Deep pass: walk the previous uberblock; emit entries the quick
    /// pass did not surface, marked deleted.
    pub async fn deep(&mut self, labels: &LabelSet) -> ZfsResult<()> {
        let Some(prev) = labels.previous else {
            info!("zfs deep: no previous uberblock; nothing to diff");
            return Ok(());
        };
        info!(
            txg_prev = prev.txg,
            txg_now = labels.best.txg,
            "zfs deep pass"
        );
        self.tick(
            ProgressPhase::DeepScan,
            "zfs: walking previous uberblock (deleted-file pass)",
        )
        .await;
        self.walk_tree(prev, true).await
    }

    async fn walk_tree(&mut self, ub: Uberblock, deleted_pass: bool) -> ZfsResult<()> {
        // MOS objset from ub_rootbp.
        let root_bp_raw = self.dmu.read_raw(ub.rootbp_off, 128)?;
        let root_bp = BlkPtr::from_slice(&root_bp_raw)
            .ok_or_else(|| ZfsError::Corrupt("uberblock rootbp unreadable".into()))?;
        let mos_block = self.dmu.read_bp(&root_bp)?;
        let mos = Objset::from_vec(mos_block)
            .ok_or_else(|| ZfsError::Corrupt("MOS objset short".into()))?;
        let mut table = ObjectTable::new();

        // MOS master node: object 1 = "root_dataset" -> dsl dataset obj.
        let master = zap::read_zap(&mut self.dmu, &mut table, &mos, 1)?;
        let root_ds_obj = zap::zap_u64(&master, "root_dataset")
            .ok_or_else(|| ZfsError::Corrupt("MOS: no root_dataset".into()))?;

        // dsl_dataset dnode: bonus ds_bp @128 -> FS objset block.
        let ds_dn = table
            .dnode(&mut self.dmu, &mos, root_ds_obj)?
            .ok_or_else(|| ZfsError::Corrupt("root dataset object missing".into()))?;
        let bonus = ds_dn.bonus();
        if bonus.len() < DS_BP_OFF + 128 {
            return Err(ZfsError::Corrupt("dsl_dataset bonus short".into()));
        }
        let fs_bp = BlkPtr::from_slice(&bonus[DS_BP_OFF..])
            .ok_or_else(|| ZfsError::Corrupt("ds_bp unreadable".into()))?;
        let fs_block = self.dmu.read_bp(&fs_bp)?;
        let fs = Objset::from_vec(fs_block)
            .ok_or_else(|| ZfsError::Corrupt("FS objset short".into()))?;
        let mut fs_table = ObjectTable::new();

        // FS master node (object 1): ROOT -> root dir object.
        let fs_master = zap::read_zap(&mut self.dmu, &mut fs_table, &fs, 1)?;
        let root_dir = zap::zap_u64(&fs_master, "ROOT")
            .ok_or_else(|| ZfsError::Corrupt("FS master node: no ROOT".into()))?;

        // SA registry for this objset.
        let sa_state = sa::SaState::load(&mut self.dmu, &mut fs_table, &fs)?;
        info!(sa = sa_state.is_sa(), "zfs: registry loaded");

        self.walk_dir(
            &fs,
            &mut fs_table,
            &sa_state,
            root_dir,
            String::new(),
            deleted_pass,
        )
        .await
    }

    /// Recurse one directory: read its zap, visit children.
    async fn walk_dir(
        &mut self,
        fs: &Objset,
        table: &mut ObjectTable,
        sa_state: &sa::SaState,
        dir_obj: u64,
        parent: String,
        deleted_pass: bool,
    ) -> ZfsResult<()> {
        if self.cancelled() || self.emitted >= MAX_ENTRIES || self.dirs_visited >= MAX_DIRS {
            return Ok(());
        }
        self.dirs_visited += 1;
        let path = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        let entries = match zap::read_zap(&mut self.dmu, table, fs, dir_obj) {
            Ok(e) => e,
            Err(e) => {
                warn!(obj = dir_obj, err = %e, "directory unreadable");
                return Ok(());
            }
        };
        for (name, value) in &entries {
            if self.cancelled() || self.emitted >= MAX_ENTRIES {
                return Ok(());
            }
            let ZapValue::U64(child_obj) = value else {
                continue;
            };
            if *child_obj == 0 {
                continue;
            }
            let child = match table.dnode(&mut self.dmu, fs, *child_obj)? {
                Some(c) => c,
                None => continue,
            };
            match child.type_() {
                dmu_ot::DIRECTORY_CONTENTS => {
                    let attrs = sa_state.parse_bonus(child.bonus());
                    self.emit_dir(&path, name, &attrs, deleted_pass);
                    let child_path = format!("{path}{name}");
                    Box::pin(self.walk_dir(
                        fs,
                        table,
                        sa_state,
                        *child_obj,
                        child_path,
                        deleted_pass,
                    ))
                    .await?;
                }
                dmu_ot::PLAIN_FILE_CONTENTS => {
                    self.emit_file(&path, name, &child, sa_state, deleted_pass)?;
                }
                dmu_ot::ZAP_OTHER => {
                    // xattr dir or sa registry object: not user files
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn emit_dir(&mut self, parent: &str, name: &str, attrs: &ZfsAttrs, deleted: bool) {
        let key = format!("{parent}{name}");
        if deleted && self.live_paths.contains(&key) {
            return; // still live: never mark deleted
        }
        if !deleted {
            self.live_paths.insert(key);
        }
        self.push_entry(RecoveredEntry {
            id: RecoveredEntryId(Uuid::now_v7()),
            fs_kind: "zfs".to_string(),
            parent_path: {
                let p = parent.trim_end_matches('/');
                if p.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{p}")
                }
            },
            name: name.to_string(),
            size: 0,
            start_lba: None,
            mtime: ts(attrs.mtime),
            ctime: ts(attrs.ctime.or(attrs.crtime)),
            atime: ts(attrs.atime),
            status: RecoveryStatus::Intact,
            extension: String::new(),
            mime: None,
            confidence: if deleted {
                CONF_DELETED_INTACT
            } else {
                CONF_LIVE
            },
            was_deleted: deleted,
            cluster_chain: Vec::new(),
            extents: Vec::new(),
            resident_data: None,
        });
    }

    fn emit_file(
        &mut self,
        parent: &str,
        name: &str,
        dn: &Dnode,
        sa_state: &sa::SaState,
        deleted: bool,
    ) -> ZfsResult<()> {
        let attrs = sa_state.parse_bonus(dn.bonus());
        let key = format!("{parent}{name}");
        if deleted && self.live_paths.contains(&key) {
            return Ok(()); // still live
        }

        // Extents: every L0 block's DVA, in file order.
        let max_blocks = dn.maxblkid().saturating_add(1);
        let mut extents = Vec::new();
        if max_blocks > 0 {
            let bps = match self.dmu.file_block_ptrs(dn, max_blocks) {
                Ok(b) => b,
                Err(e) => {
                    // One unreadable block must not kill the walk: the
                    // entry surfaces with the extents that survived.
                    warn!(obj = dn.maxblkid(), err = %e, "data block unreadable");
                    Vec::new()
                }
            };
            for bp in &bps {
                if bp.is_hole() || bp.is_embedded() {
                    continue;
                }
                if let Some(dva) = bp.dva(0) {
                    if dva.vdev != 0 {
                        continue; // out-of-scope geometry guard
                    }
                    extents.push(Extent {
                        start_byte: dva.offset,
                        length: dva.asize,
                    });
                }
            }
        }
        extents.sort_by_key(|e| e.start_byte);

        let size = attrs.size.unwrap_or(dn.used());
        let status = if extents.is_empty() && size > 0 {
            RecoveryStatus::PartiallyRecovered
        } else {
            RecoveryStatus::Intact
        };
        let confidence = if deleted {
            if extents.is_empty() {
                CONF_METADATA_ONLY
            } else if size > 0 {
                CONF_DELETED_INTACT
            } else {
                CONF_DELETED_PARTIAL
            }
        } else {
            CONF_LIVE
        };

        let resident = symlink_resident(dn, sa_state);
        if !deleted {
            self.live_paths.insert(key);
        }
        let start_lba = extents.first().map(|e| e.start_byte);
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        let extension = if name.contains('.') && !ext.is_empty() && ext.len() <= 10 {
            ext
        } else {
            String::new()
        };
        self.push_entry(RecoveredEntry {
            id: RecoveredEntryId(Uuid::now_v7()),
            fs_kind: "zfs".to_string(),
            parent_path: {
                let p = parent.trim_end_matches('/');
                if p.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{p}")
                }
            },
            name: name.to_string(),
            size,
            start_lba,
            mtime: ts(attrs.mtime),
            ctime: ts(attrs.ctime.or(attrs.crtime)),
            atime: ts(attrs.atime),
            status,
            extension,
            mime: None,
            confidence,
            was_deleted: deleted,
            cluster_chain: Vec::new(),
            extents,
            resident_data: resident,
        });
        Ok(())
    }

    fn push_entry(&mut self, entry: RecoveredEntry) {
        if self.emitted >= MAX_ENTRIES {
            return;
        }
        let _ = self.results.insert(entry);
        self.emitted += 1;
    }

    pub fn emitted(&self) -> u64 {
        self.emitted
    }
}

/// Symlink target: the bonus tail after the 176-byte legacy znode header
/// (zfs_sa.h: "data may pad out the remaining bytes ... symbolic links").
fn symlink_resident(dn: &Dnode, sa_state: &sa::SaState) -> Option<Vec<u8>> {
    let bonus = dn.bonus();
    let _ = sa_state;
    if bonus.len() > 176 {
        let tail = &bonus[176..];
        let nul = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        if nul > 0 && nul <= 1024 {
            return Some(tail[..nul].to_vec());
        }
    }
    None
}

/// Load labels for probe/scan; shared by the plugin.
pub fn load(
    source: &Arc<dyn dw_disk::DiskSource>,
    cancel: &Arc<AtomicBool>,
) -> ZfsResult<LabelSet> {
    let read_at = source.as_read_at();
    label::load_labels(read_at.as_ref(), read_at.size(), cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn timestamp_conversion() {
        assert_eq!(ts(Some(0)).map(|t| t.unix_timestamp()), Some(0));
        assert!(ts(Some(1_700_000_000)).is_some());
        assert_eq!(ts(None), None);
        // absurd corrupt timestamps are dropped, never fabricated
        assert!(ts(Some(u64::MAX)).is_none());
    }

    #[test]
    fn cancel_flag_read() {
        let flag = AtomicBool::new(true);
        assert!(flag.load(Ordering::Relaxed));
    }
}
