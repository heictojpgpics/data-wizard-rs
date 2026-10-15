//! Scan orchestration.
//!
//! Quick: walk the root tree -> every live subvolume's FS tree -> files,
//! dirs, symlinks. Fragmented files recover in content order because
//! each EXTENT_DATA item carries its own file-logical offset; inline
//! files surface as resident data.
//!
//! Deep: quick + the orphan-tree-block pass. Btrfs is copy-on-write, so
//! nodes from older tree generations persist on disk until overwritten.
//! The deep pass scans every nodesize-aligned offset inside
//! SYSTEM/METADATA chunks for nodes that (a) verify (csum + fsid +
//! bytenr), (b) belong to the FS tree (owner 5), and (c) were NOT
//! visited by the live walk. All their items are collected into an
//! orphan tree and emitted like a normal tree (deleted = true), so
//! deleted files keep their names (DIR_ITEMs survive in old nodes) and
//! their extents (EXTENT_DATA items survive too).
//!
//! Files that are still live (emitted by the quick pass) are never
//! duplicated by the orphan pass.

use crate::btree::{self, Node, NodeItem};
use crate::chunk::ChunkMap;
use crate::error::BtrfsResult;
use crate::items::{
    DIR_ITEM_KEY, DirEntry, EXTENT_DATA_KEY, EXTENT_INLINE, EXTENT_REG, FS_TREE_OBJECTID,
    FileExtent, INODE_ITEM_KEY, InodeItem, ROOT_ITEM_KEY, RootItem,
};
use crate::superblock::SuperBlock;
use dw_core::progress::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_core::results::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};
use uuid::Uuid;

const CONF_LIVE: u16 = 1000;
const CONF_DELETED_INTACT: u16 = 900;
const CONF_DELETED_PARTIAL: u16 = 700;
const CONF_METADATA_ONLY: u16 = 400;
const MAX_ENTRIES: u64 = 1 << 22;
const MAX_DIRS: usize = 4096;

/// Per-tree collected state (one FS tree or the orphan tree).
struct FsTree {
    owner: u64,
    inodes: HashMap<u64, InodeItem>,
    /// ino -> (file-logical offset, extent), logical order applied later.
    extents: HashMap<u64, Vec<(u64, FileExtent)>>,
    /// parent ino -> entries.
    dirents: HashMap<u64, Vec<DirEntry>>,
}

struct Ctx<'a> {
    read: Arc<dyn dw_disk::ReadAt>,
    sb: &'a SuperBlock,
    map: ChunkMap,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
    emitted: u64,
    /// Inodes surfaced by the quick pass (orphan pass never duplicates).
    live_inodes: HashSet<u64>,
}

pub async fn scan_btrfs(
    source: Arc<dyn dw_disk::DiskSource>,
    sb: SuperBlock,
    mode: dw_core::ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
) -> BtrfsResult<()> {
    let read: Arc<dyn dw_disk::ReadAt> = source.as_read_at();
    let map = ChunkMap::build(read.as_ref(), &sb)?;

    info!(
        chunks = map.all().len(),
        nodesize = sb.nodesize,
        label = %sb.label,
        generation = sb.generation,
        "btrfs volume opened"
    );

    let mut ctx = Ctx {
        read,
        sb: &sb,
        map,
        results,
        progress,
        cancel,
        emitted: 0,
        live_inodes: HashSet::new(),
    };

    // ---- quick: live trees ----
    let (_, root_items) =
        btree::collect_reachable(ctx.read.as_ref(), ctx.sb, &ctx.map, ctx.sb.root)?;
    let mut live_nodes: HashSet<u64> = HashSet::new();
    live_nodes.insert(ctx.sb.root);

    let mut trees: Vec<FsTree> = Vec::new();
    for it in &root_items {
        if ctx.cancelled() {
            return Ok(());
        }
        if it.key_type != ROOT_ITEM_KEY {
            continue;
        }
        let Some(ri) = RootItem::parse(&it.data) else {
            continue;
        };
        if ri.refs == 0 || ri.is_dead() || ri.bytenr == 0 {
            continue;
        }
        if it.objectid == FS_TREE_OBJECTID || it.objectid >= 256 {
            let (visited, items) =
                btree::collect_reachable(ctx.read.as_ref(), ctx.sb, &ctx.map, ri.bytenr)?;
            live_nodes.extend(visited);
            trees.push(collect_tree(items, it.objectid));
        }
    }
    if trees.is_empty() {
        warn!("no live filesystem tree found (root tree has no FS_TREE ROOT_ITEM)");
        return Ok(());
    }
    let total_dirs: usize = trees.iter().map(|t| t.dirents.len()).sum();
    if total_dirs > MAX_DIRS {
        warn!(
            total_dirs,
            "directory budget exceeded; truncating to the main tree"
        );
        trees.truncate(1);
    }

    for tree in &trees {
        if ctx.cancelled() {
            return Ok(());
        }
        ctx.tick("walking live btrfs tree").await;
        let tag = if tree.owner == FS_TREE_OBJECTID {
            String::new()
        } else {
            format!("/@subvol_{}", tree.owner)
        };
        emit_tree(&mut ctx, tree, &tag, false)?;
    }
    info!(entries = ctx.emitted, "btrfs quick walk complete");

    // ---- deep: orphan tree-block scan ----
    if mode == dw_core::ScanMode::Deep {
        orphan_pass(&mut ctx, &live_nodes).await?;
    }
    Ok(())
}

impl Ctx<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    async fn tick(&self, msg: &str) {
        self.progress
            .emit(ProgressEvent {
                scan_id: Uuid::nil(),
                phase: ProgressPhase::QuickScan,
                current: self.emitted,
                total: 0,
                message: msg.to_string(),
            })
            .await;
    }
}

fn collect_tree(items: Vec<NodeItem>, owner: u64) -> FsTree {
    let mut inodes: HashMap<u64, InodeItem> = HashMap::new();
    let mut extents: HashMap<u64, Vec<(u64, FileExtent)>> = HashMap::new();
    let mut dirents: HashMap<u64, Vec<DirEntry>> = HashMap::new();
    for it in items {
        match it.key_type {
            INODE_ITEM_KEY => {
                if let Some(ino) = InodeItem::parse(&it.data) {
                    inodes.insert(it.objectid, ino);
                }
            }
            EXTENT_DATA_KEY => {
                if let Some(fe) = FileExtent::parse(&it.data) {
                    extents
                        .entry(it.objectid)
                        .or_default()
                        .push((it.offset, fe));
                }
            }
            DIR_ITEM_KEY => {
                if let Some(de) = DirEntry::parse(&it.data) {
                    dirents.entry(it.objectid).or_default().push(de);
                }
            }
            _ => {}
        }
    }
    FsTree {
        owner,
        inodes,
        extents,
        dirents,
    }
}

/// Emit one tree's entries (live or orphan).
fn emit_tree(ctx: &mut Ctx<'_>, tree: &FsTree, tag: &str, deleted: bool) -> BtrfsResult<()> {
    // child ino -> (parent ino, name)
    let mut parent_of: HashMap<u64, (u64, String)> = HashMap::new();
    for (parent, entries) in &tree.dirents {
        for e in entries {
            if e.child_key_type == INODE_ITEM_KEY && !parent_of.contains_key(&e.child_objectid) {
                parent_of.insert(e.child_objectid, (*parent, e.name.clone()));
            }
        }
    }
    let dir_path = |ino: u64| -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = ino;
        let mut guard = 0;
        while guard < 32 {
            guard += 1;
            if cur == 256 || !parent_of.contains_key(&cur) {
                break;
            }
            let (p, name) = &parent_of[&cur];
            parts.push(name.as_str());
            cur = *p;
        }
        let joined: Vec<String> = parts.iter().rev().map(|s| s.to_string()).collect();
        // The tree root (256) itself is "/"; subvolumes prefix their tag.
        if joined.is_empty() {
            if tag.is_empty() {
                "/".to_string()
            } else {
                tag.to_string()
            }
        } else if tag.is_empty() {
            format!("/{}", joined.join("/"))
        } else {
            format!("{}/{}", tag, joined.join("/"))
        }
    };

    for (ino, inode) in &tree.inodes {
        if ctx.cancelled() {
            return Ok(());
        }
        if ctx.emitted >= MAX_ENTRIES {
            return Err(crate::error::BtrfsError::Corrupt("entry budget".into()));
        }
        if !inode.is_regular() && !inode.is_symlink() {
            continue;
        }
        if deleted && ctx.live_inodes.contains(ino) {
            continue; // still live: quick pass already surfaced it better
        }
        let (parent_path, name) = match parent_of.get(ino) {
            Some((p, n)) => (dir_path(*p), n.clone()),
            None => {
                if deleted {
                    // Deleted with no surviving dirent: name by inode.
                    ("/".to_string(), format!("inode_{ino}"))
                } else {
                    continue; // unreferenced live inode: metadata noise
                }
            }
        };
        let entry = build_entry(ctx, inode, parent_path, &name, tree, *ino, deleted)?;
        ctx.results.insert(entry);
        ctx.emitted += 1;
        if !deleted {
            ctx.live_inodes.insert(*ino);
        }
    }
    Ok(())
}

fn build_entry(
    ctx: &Ctx<'_>,
    inode: &InodeItem,
    parent_path: String,
    name: &str,
    tree: &FsTree,
    ino: u64,
    deleted: bool,
) -> BtrfsResult<RecoveredEntry> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    let mut extents: Vec<Extent> = Vec::new();
    let mut resident: Option<Vec<u8>> = None;
    let mut status = RecoveryStatus::Intact;
    let mut confidence = if deleted {
        CONF_DELETED_INTACT
    } else {
        CONF_LIVE
    };

    if let Some(list) = tree.extents.get(&ino) {
        let mut sorted = list.clone();
        sorted.sort_unstable_by_key(|(lo, _)| *lo);
        let mut covered: u64 = 0;
        for (file_off, fe) in sorted {
            if file_off != covered && covered < inode.size {
                status = RecoveryStatus::PartiallyRecovered;
                confidence = CONF_DELETED_PARTIAL;
                break;
            }
            match fe.kind {
                EXTENT_INLINE => {
                    if fe.compression == 0 {
                        if resident.is_none() {
                            resident = Some(fe.inline.clone());
                        }
                    } else {
                        status = RecoveryStatus::PartiallyRecovered;
                        confidence = CONF_METADATA_ONLY;
                    }
                    covered += fe.num_bytes;
                }
                EXTENT_REG | 2 => {
                    if fe.compression != 0 {
                        status = RecoveryStatus::PartiallyRecovered;
                        confidence = CONF_METADATA_ONLY;
                        covered += fe.num_bytes;
                        continue;
                    }
                    if let Some(phys) = ctx.map.map(fe.disk_bytenr) {
                        let want = inode.size.saturating_sub(file_off).min(fe.num_bytes);
                        if want > 0 {
                            extents.push(Extent {
                                start_byte: phys + fe.data_offset,
                                length: want,
                            });
                        }
                    } else {
                        status = RecoveryStatus::PartiallyRecovered;
                        confidence = CONF_DELETED_PARTIAL;
                    }
                    covered += fe.num_bytes;
                }
                _ => {}
            }
        }
        if covered < inode.size {
            status = RecoveryStatus::PartiallyRecovered;
            if !extents.is_empty() {
                confidence = confidence.min(CONF_DELETED_PARTIAL);
            } else if resident.is_none() {
                confidence = CONF_METADATA_ONLY;
            }
        }
    } else if inode.size > 0 {
        status = RecoveryStatus::PartiallyRecovered;
        confidence = CONF_METADATA_ONLY;
    }

    Ok(RecoveredEntry {
        id: RecoveredEntryId(Uuid::now_v7()),
        fs_kind: "btrfs".to_string(),
        parent_path,
        name: name.to_string(),
        size: inode.size,
        start_lba: None,
        mtime: inode.mtime,
        ctime: inode.otime.or(inode.ctime),
        atime: inode.atime,
        status,
        extension,
        mime: None,
        confidence,
        was_deleted: deleted,
        cluster_chain: Vec::new(),
        extents,
        resident_data: resident,
    })
}

/// Deep pass: scan SYSTEM/METADATA chunks for verified-but-unreachable
/// FS-tree leaves; collect their items into one orphan tree and emit it.
async fn orphan_pass(ctx: &mut Ctx<'_>, live_nodes: &HashSet<u64>) -> BtrfsResult<()> {
    let ns = u64::from(ctx.sb.nodesize);
    let mut orphan_items: Vec<NodeItem> = Vec::new();
    let mut scanned = 0u64;

    for chunk in ctx.map.metadata_chunks() {
        if ctx.cancelled() {
            return Ok(());
        }
        let end = (chunk.physical + chunk.length).min(ctx.read.size());
        let mut off = chunk.physical;
        while off + ns <= end {
            if ctx.cancelled() {
                return Ok(());
            }
            scanned += 1;
            // Candidate's logical address is its own bytenr field; a node
            // that fails csum/fsid/bytenr never reaches the map check.
            let mut probe = vec![0u8; ns as usize];
            if ctx.read.read_at(off, &mut probe).is_err() {
                break;
            }
            if probe.len() < 0x38 {
                break;
            }
            let bytenr = u64::from_le_bytes([
                probe[0x30],
                probe[0x31],
                probe[0x32],
                probe[0x33],
                probe[0x34],
                probe[0x35],
                probe[0x36],
                probe[0x37],
            ]);
            if bytenr == 0 {
                off += ns;
                continue;
            }
            let node = match Node::read(ctx.read.as_ref(), ctx.sb, &ctx.map, bytenr) {
                Ok(n) => n,
                Err(_) => {
                    off += ns;
                    continue;
                }
            };
            if node.owner != FS_TREE_OBJECTID || node.level != 0 {
                off += ns;
                continue;
            }
            if live_nodes.contains(&node.logical) {
                off += ns;
                continue;
            }
            for i in 0..node.nritems as usize {
                let Ok((objectid, key_type, offset, item_off, item_size)) =
                    node.leaf_item(i, ctx.sb.nodesize)
                else {
                    break;
                };
                let data = node.raw[item_off as usize..(item_off + u64::from(item_size)) as usize]
                    .to_vec();
                orphan_items.push(NodeItem {
                    objectid,
                    key_type,
                    offset,
                    data,
                });
            }
            off += ns;
        }
    }

    let orphan_tree = collect_tree(orphan_items, FS_TREE_OBJECTID);
    info!(
        scanned,
        entries_before = ctx.emitted,
        "btrfs deep: orphan scan done, emitting"
    );
    ctx.tick("walking orphan btrfs nodes").await;
    emit_tree(ctx, &orphan_tree, "", true)?;
    info!(entries = ctx.emitted, "btrfs deep complete");
    Ok(())
}
