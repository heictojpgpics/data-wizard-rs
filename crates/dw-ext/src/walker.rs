//! Scan orchestration for ext2/3/4 volumes.
//!
//! Phases (all cancellation-aware, all bounded):
//!
//! 1. **Live walk** (quick + deep): DFS from root inode 2 through the
//!    live dirent chain. Emits every reachable file, directory and
//!    symlink; extents come from the extent tree / indirect map so
//!    fragmented files recover in content order.
//! 2. **Shadow pass** (quick + deep): every directory block visited by
//!    the live walk is also scanned for structurally valid dirents the
//!    live chain skips over (recently deleted names). A shadow entry is
//!    surfaced only when its inode is unallocated; one name per inode.
//! 3. **Orphan pass** (deep only): every unallocated inode in every group
//!    whose on-disk record still describes a file, and that no dirent
//!    named, is surfaced as `inode_<n>`.
//! 4. **Free-space carving** (deep only): the block bitmaps' free runs
//!    are carved through the shared carver (header/footer DB). Allocated
//!    blocks are never carved - carving them would duplicate live files.

use crate::dir::{live_chain, shadowed_entries};
use crate::error::{ExtError, ExtResult};
use crate::extent::map_inode_blocks;
use crate::group::{Bitmap, GroupDesc};
use crate::inode::Inode;
use crate::superblock::{SUPER_OFFSET, SuperBlock};
use dw_core::extents::merge_and_trim_offsets;
use dw_core::progress::{ProgressEvent, ProgressPhase, ProgressSink};
use dw_core::results::{RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use dw_disk::{DiskSource, ReadAt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use time::OffsetDateTime;
use tracing::{info, warn};
use uuid::Uuid;

/// Directory nesting cap (defense against corrupt dirent loops).
const MAX_DIR_DEPTH: usize = 64;
/// Cap on blocks read per directory.
const MAX_DIR_BLOCKS: usize = 1 << 16;
/// Global cap on entries emitted per scan.
const MAX_TOTAL_ENTRIES: u64 = 1 << 22;
/// First non-reserved inode number (kernel FIRST_INO).
const FIRST_INO: u32 = 11;
/// Confidence: live entry, map intact.
const CONF_LIVE: u16 = 1000;
/// Confidence: deleted entry, content map covers the file.
const CONF_DELETED_INTACT: u16 = 900;
/// Confidence: deleted entry, partial map.
const CONF_DELETED_PARTIAL: u16 = 700;
/// Confidence: deleted entry, name/metadata only (no data map).
const CONF_METADATA_ONLY: u16 = 400;

/// Parsed volume: superblock + group descriptors.
pub struct ExtVolume {
    pub sb: SuperBlock,
    pub groups: Vec<GroupDesc>,
}

impl ExtVolume {
    /// Read and validate the superblock + every group descriptor.
    pub fn read(read: &dyn ReadAt) -> ExtResult<Self> {
        let mut sb_buf = [0u8; 1024];
        read.read_at(SUPER_OFFSET, &mut sb_buf)
            .map_err(ExtError::from)?;
        let sb = SuperBlock::parse(&sb_buf)?;
        let desc64 = sb.is_64bit();

        let mut groups = Vec::with_capacity(sb.group_count as usize);
        for g in 0..sb.group_count {
            let off = sb
                .group_desc_offset(g)
                .ok_or_else(|| ExtError::Corrupt("group index out of range".into()))?;
            let mut d = vec![0u8; sb.desc_size as usize];
            read.read_at(off, &mut d).map_err(ExtError::from)?;
            groups.push(GroupDesc::parse(&d, desc64)?);
        }
        Ok(Self { sb, groups })
    }

    /// First global block number of group `g`'s data range.
    #[must_use]
    pub fn group_first_block(&self, g: u32) -> u64 {
        u64::from(g) * u64::from(self.sb.blocks_per_group) + u64::from(self.sb.first_data_block)
    }

    /// Absolute byte offset of inode `no`'s record.
    #[must_use]
    pub fn inode_offset(&self, no: u32) -> Option<u64> {
        let (g, idx) = self.sb.locate_inode(no)?;
        let desc = self.groups.get(g as usize)?;
        Some(
            desc.inode_table_block * u64::from(self.sb.block_size)
                + u64::from(idx) * u64::from(self.sb.inode_size),
        )
    }

    /// Read one inode.
    pub fn read_inode(&self, read: &dyn ReadAt, no: u32) -> ExtResult<Inode> {
        let off = self
            .inode_offset(no)
            .ok_or_else(|| ExtError::Corrupt(format!("inode {no} out of range")))?;
        let mut buf = vec![0u8; self.sb.inode_size as usize];
        read.read_at(off, &mut buf).map_err(ExtError::from)?;
        Inode::parse(&buf)
    }

    /// Read group `g`'s inode bitmap.
    pub fn inode_bitmap(&self, read: &dyn ReadAt, g: u32) -> ExtResult<Bitmap> {
        let desc = self
            .groups
            .get(g as usize)
            .ok_or_else(|| ExtError::Corrupt("group out of range".into()))?;
        if desc.inode_bitmap_block == 0 {
            return Err(ExtError::Corrupt("inode bitmap block is zero".into()));
        }
        if desc.inode_bitmap_block >= self.sb.blocks_count {
            return Err(ExtError::Corrupt("inode bitmap block out of volume".into()));
        }
        let off = desc.inode_bitmap_block * u64::from(self.sb.block_size);
        let mut raw = vec![0u8; self.sb.block_size as usize];
        read.read_at(off, &mut raw).map_err(ExtError::from)?;
        Bitmap::new(raw, self.sb.inodes_per_group as usize)
    }

    /// Read group `g`'s block bitmap.
    pub fn block_bitmap(&self, read: &dyn ReadAt, g: u32) -> ExtResult<Bitmap> {
        let desc = self
            .groups
            .get(g as usize)
            .ok_or_else(|| ExtError::Corrupt("group out of range".into()))?;
        if desc.block_bitmap_block == 0 {
            return Err(ExtError::Corrupt("block bitmap block is zero".into()));
        }
        if desc.block_bitmap_block >= self.sb.blocks_count {
            return Err(ExtError::Corrupt("block bitmap out of volume".into()));
        }
        let off = desc.block_bitmap_block * u64::from(self.sb.block_size);
        let mut raw = vec![0u8; self.sb.block_size as usize];
        read.read_at(off, &mut raw).map_err(ExtError::from)?;
        let blocks_in_group = blocks_in_group(&self.sb, g);
        Bitmap::new(raw, blocks_in_group as usize)
    }
}

impl ExtVolume {
    /// Read one group's whole inode table in a single sequential read and
    /// parse every record (fast orphan pass: one I/O per group instead of
    /// one per inode). Table span is bounds-checked against the volume.
    pub fn read_group_inodes(&self, read: &dyn ReadAt, g: u32) -> ExtResult<Vec<Option<Inode>>> {
        let desc = self
            .groups
            .get(g as usize)
            .ok_or_else(|| ExtError::Corrupt("group out of range".into()))?;
        if desc.inode_table_block == 0 {
            return Err(ExtError::Corrupt("inode table block is zero".into()));
        }
        let table_bytes = u64::from(self.sb.inodes_per_group) * u64::from(self.sb.inode_size);
        if table_bytes > 32 << 20 {
            return Err(ExtError::Corrupt("inode table span implausible".into()));
        }
        let start = desc.inode_table_block * u64::from(self.sb.block_size);
        let end = start + table_bytes;
        if end > self.sb.blocks_count * u64::from(self.sb.block_size) {
            return Err(ExtError::Corrupt("inode table runs past volume".into()));
        }
        let mut buf = vec![0u8; table_bytes as usize];
        read.read_at(start, &mut buf).map_err(ExtError::from)?;
        let mut out = Vec::with_capacity(self.sb.inodes_per_group as usize);
        for i in 0..self.sb.inodes_per_group as usize {
            let b = &buf[i * self.sb.inode_size as usize..(i + 1) * self.sb.inode_size as usize];
            out.push(Inode::parse(b).ok());
        }
        Ok(out)
    }
}

/// Blocks actually addressable in group `g` (the last group is short).
fn blocks_in_group(sb: &SuperBlock, g: u32) -> u64 {
    let first = u64::from(g) * u64::from(sb.blocks_per_group) + u64::from(sb.first_data_block);
    let end = sb.blocks_count;
    let span = u64::from(sb.blocks_per_group);
    (first + span).min(end).saturating_sub(first)
}

/// Shared state for one scan run.
struct ScanCtx {
    scan_id: Uuid,
    read: Arc<dyn ReadAt>,
    vol: ExtVolume,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
    /// inode bitmaps, cached per group.
    inode_bitmaps: HashMap<u32, Bitmap>,
    /// Inode numbers surfaced by the live walk.
    live_inodes: HashSet<u32>,
    /// Inode numbers surfaced by name (live or shadow).
    named: HashSet<u32>,
    emitted: u64,
    deleted_emitted: u64,
    fs_kind: &'static str,
}

impl ScanCtx {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    async fn tick(&self, msg: &str) {
        self.progress
            .emit(ProgressEvent {
                scan_id: self.scan_id,
                phase: ProgressPhase::QuickScan,
                current: self.emitted,
                total: 0,
                message: msg.to_string(),
            })
            .await;
    }

    fn inode_allocated(&mut self, no: u32) -> ExtResult<bool> {
        let (g, idx) = self
            .vol
            .sb
            .locate_inode(no)
            .ok_or_else(|| ExtError::Corrupt(format!("inode {no} out of range")))?;
        if !self.inode_bitmaps.contains_key(&g) {
            let bm = self.vol.inode_bitmap(self.read.as_ref(), g)?;
            self.inode_bitmaps.insert(g, bm);
        }
        Ok(self
            .inode_bitmaps
            .get(&g)
            .map(|bm| bm.is_used(idx as usize))
            .unwrap_or(false))
    }

    /// Emit one entry from a parsed inode.
    async fn emit_inode(
        &mut self,
        parent: &str,
        name: &str,
        ino: &Inode,
        deleted: bool,
    ) -> ExtResult<()> {
        if self.cancelled() {
            return Ok(());
        }
        if self.emitted >= MAX_TOTAL_ENTRIES {
            return Err(ExtError::Corrupt("global entry budget exceeded".into()));
        }

        let bs = u64::from(self.vol.sb.block_size);
        let extension = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .unwrap_or_default();

        // Symlinks: fast symlinks (<60 bytes, no map) carry the target in
        // i_block - surface it as resident data.
        let resident = if ino.is_symlink() && ino.size <= 60 && !ino.uses_extents() {
            Some(ino.i_block[..ino.size as usize].to_vec())
        } else {
            None
        };

        let (extents, status, confidence) = if ino.is_symlink() && resident.is_some() {
            (Vec::new(), RecoveryStatus::Intact, CONF_LIVE)
        } else {
            let map = map_inode_blocks(self.read.as_ref(), &self.vol.sb, ino)?;
            let offsets: Vec<u64> = map.blocks.iter().map(|&b| b * bs).collect();
            let extents = merge_and_trim_offsets(&offsets, bs, ino.size);
            let want = if ino.size == 0 {
                0
            } else {
                ino.size.div_ceil(bs)
            } as usize;
            let have = map.blocks.len();
            let (st, cf) = if have >= want || ino.size == 0 {
                (
                    RecoveryStatus::Intact,
                    if deleted {
                        CONF_DELETED_INTACT
                    } else {
                        CONF_LIVE
                    },
                )
            } else if have > 0 {
                (RecoveryStatus::PartiallyRecovered, CONF_DELETED_PARTIAL)
            } else {
                (RecoveryStatus::PartiallyRecovered, CONF_METADATA_ONLY)
            };
            (extents, st, cf)
        };

        let entry = RecoveredEntry {
            id: RecoveredEntryId(Uuid::now_v7()),
            fs_kind: self.fs_kind.to_string(),
            parent_path: parent.to_string(),
            name: name.to_string(),
            size: ino.size,
            start_lba: None,
            mtime: ino.mtime,
            ctime: ino.crtime.or(ino.ctime),
            atime: ino.atime,
            status,
            extension,
            mime: None,
            confidence,
            was_deleted: deleted,
            cluster_chain: Vec::new(),
            extents,
            resident_data: resident,
        };
        self.results.insert(entry);
        self.emitted += 1;
        if deleted {
            self.deleted_emitted += 1;
        }
        Ok(())
    }

    /// Directory data blocks in logical order, capped.
    fn dir_blocks(&self, ino: &Inode) -> ExtResult<Vec<u64>> {
        if !ino.is_dir() {
            return Err(ExtError::Corrupt("not a directory".into()));
        }
        let map = map_inode_blocks(self.read.as_ref(), &self.vol.sb, ino)?;
        if map.blocks.len() > MAX_DIR_BLOCKS {
            return Err(ExtError::Corrupt("directory block budget exceeded".into()));
        }
        Ok(map.blocks)
    }

    /// Read one fs block.
    fn read_block(&self, b: u64) -> ExtResult<Vec<u8>> {
        if b >= self.vol.sb.blocks_count {
            return Err(ExtError::Corrupt(format!("block {b} beyond volume")));
        }
        let off = b * u64::from(self.vol.sb.block_size);
        let mut buf = vec![0u8; self.vol.sb.block_size as usize];
        self.read.read_at(off, &mut buf).map_err(ExtError::from)?;
        Ok(buf)
    }
}

/// Full scan entry point (used by the plugin).
pub async fn scan_ext(
    scan_id: Uuid,
    source: Arc<dyn DiskSource>,
    mode: dw_core::ScanMode,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
) -> ExtResult<()> {
    let read: Arc<dyn ReadAt> = source.as_read_at();
    let vol = ExtVolume::read(read.as_ref())?;
    let fs_kind = vol.sb.fs_kind();

    info!(
        fs = fs_kind,
        groups = vol.sb.group_count,
        blocks = vol.sb.blocks_count,
        inodes = vol.sb.inodes_count,
        volume_name = %vol.sb.volume_name,
        "ext volume opened"
    );

    let mut ctx = ScanCtx {
        scan_id,
        read,
        vol,
        results,
        progress,
        cancel,
        inode_bitmaps: HashMap::new(),
        live_inodes: HashSet::new(),
        named: HashSet::new(),
        emitted: 0,
        deleted_emitted: 0,
        fs_kind,
    };

    // ---- Phase 1+2: live walk with shadow collection ----
    let root = 2u32;
    if ctx.vol.sb.inodes_count < 2 {
        return Err(ExtError::Corrupt("volume has no root inode slot".into()));
    }
    let mut shadow_names: HashMap<u32, String> = HashMap::new();
    let mut shadow_parents: HashMap<u32, String> = HashMap::new();
    walk_dir(
        &mut ctx,
        root,
        "/".to_string(),
        0,
        &mut shadow_names,
        &mut shadow_parents,
    )
    .await?;

    // Emit shadow entries (deleted, named by surviving dirent bytes).
    let mut sorted: Vec<(u32, String, String)> = shadow_names
        .into_iter()
        .map(|(no, name)| {
            let parent = shadow_parents.remove(&no).unwrap_or_else(|| "/".into());
            (no, name, parent)
        })
        .collect();
    sorted.sort_unstable();
    for (no, name, parent) in sorted {
        if ctx.cancelled() {
            break;
        }
        // Only unallocated inodes are "deleted"; if the inode is live the
        // stale dirent is from a pre-move copy - the live walk already
        // surfaced the file under its current name.
        if ctx.inode_allocated(no).unwrap_or(false) || ctx.live_inodes.contains(&no) {
            continue;
        }
        if let Ok(ino) = ctx.vol.read_inode(ctx.read.as_ref(), no)
            && ino.is_recoverable_orphan()
        {
            ctx.emit_inode(&parent, &name, &ino, true).await?;
            ctx.named.insert(no);
        }
    }

    info!(
        live = ctx.emitted - ctx.deleted_emitted,
        deleted_named = ctx.deleted_emitted,
        "ext quick phases complete"
    );

    // ---- Phase 3+4: deep-only passes ----
    if mode == dw_core::ScanMode::Deep {
        orphan_pass(&mut ctx).await?;
        carve_free_runs(&ctx).await?;
    }
    Ok(())
}

/// DFS one directory; collects shadow dirents along the way.
async fn walk_dir(
    ctx: &mut ScanCtx,
    dir_no: u32,
    path: String,
    depth: usize,
    shadow_names: &mut HashMap<u32, String>,
    shadow_parents: &mut HashMap<u32, String>,
) -> ExtResult<()> {
    if ctx.cancelled() {
        return Ok(());
    }
    if depth > MAX_DIR_DEPTH {
        warn!(dir = dir_no, "directory nesting cap hit; skipping subtree");
        return Ok(());
    }
    if ctx.live_inodes.contains(&dir_no) {
        warn!(dir = dir_no, "directory loop; skipping");
        return Ok(());
    }
    ctx.live_inodes.insert(dir_no);

    let ino = ctx.vol.read_inode(ctx.read.as_ref(), dir_no)?;
    if !ino.is_dir() {
        return Err(ExtError::Corrupt(format!(
            "inode {dir_no} is not a directory"
        )));
    }
    let blocks = ctx.dir_blocks(&ino)?;

    ctx.tick(&format!(
        "scanning directory {path} ({} blocks)",
        blocks.len()
    ))
    .await;

    let mut subdirs: Vec<(u32, String)> = Vec::new();
    for b in blocks {
        if ctx.cancelled() {
            return Ok(());
        }
        let data = match ctx.read_block(b) {
            Ok(d) => d,
            Err(e) => {
                warn!(dir = dir_no, block = b, error = %e, "unreadable dir block; partial walk");
                continue;
            }
        };
        let live = live_chain(&data);
        let shadows = shadowed_entries(&data, &live);

        for d in &live {
            if d.inode == 0 || d.inode == dir_no {
                continue;
            }
            if d.name == "." || d.name == ".." {
                continue;
            }
            // Read the child once; classify by its mode (the dirent's
            // file_type byte is absent on volumes without FILETYPE).
            let child = match ctx.vol.read_inode(ctx.read.as_ref(), d.inode) {
                Ok(c) => c,
                Err(e) => {
                    warn!(inode = d.inode, error = %e, "unreadable child inode; skipping");
                    continue;
                }
            };
            if child.is_dir() {
                subdirs.push((d.inode, join(&path, &d.name)));
            } else {
                ctx.emit_inode(&path, &d.name, &child, false).await?;
                ctx.named.insert(d.inode);
            }
        }
        // Shadows for this block: remember the first name per inode.
        for s in shadows {
            if s.inode == 0 || ctx.live_inodes.contains(&s.inode) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = shadow_names.entry(s.inode) {
                e.insert(s.name.clone());
                shadow_parents.insert(s.inode, path.clone());
            }
        }
    }

    for (no, p) in subdirs {
        if ctx.cancelled() {
            return Ok(());
        }
        // Async recursion: box the recursive future.
        Box::pin(walk_dir(
            ctx,
            no,
            p,
            depth + 1,
            shadow_names,
            shadow_parents,
        ))
        .await?;
    }
    Ok(())
}

/// Deep pass: unallocated inodes no dirent named.
async fn orphan_pass(ctx: &mut ScanCtx) -> ExtResult<()> {
    let total_inodes = ctx.vol.sb.inodes_count;
    let groups = ctx.vol.sb.group_count;

    let mut surfaced = 0u64;
    for g in 0..groups {
        if ctx.cancelled() {
            return Ok(());
        }
        let bm = match ctx.vol.inode_bitmap(ctx.read.as_ref(), g) {
            Ok(bm) => bm,
            Err(e) => {
                warn!(group = g, error = %e, "unreadable inode bitmap; skipping group");
                continue;
            }
        };
        let table = match ctx.vol.read_group_inodes(ctx.read.as_ref(), g) {
            Ok(t) => t,
            Err(e) => {
                warn!(group = g, error = %e, "unreadable inode table; skipping group");
                continue;
            }
        };
        for (idx, maybe) in table.iter().enumerate() {
            if ctx.cancelled() {
                return Ok(());
            }
            let no = g * ctx.vol.sb.inodes_per_group + idx as u32 + 1;
            if no > total_inodes || no < FIRST_INO {
                continue; // reserved bookkeeping inodes (journal, resize, bad)
            }
            if bm.is_used(idx) {
                continue; // allocated - live walk's territory
            }
            if ctx.named.contains(&no) || ctx.live_inodes.contains(&no) {
                continue; // already surfaced by name
            }
            let ino = match maybe {
                Some(i) => i,
                None => continue,
            };
            if !ino.is_recoverable_orphan() {
                continue;
            }
            // Name by inode number: no surviving dirent named it, and
            // inventing a filename would be dishonest.
            let name = format!("inode_{no}");
            ctx.emit_inode("/", &name, ino, true).await?;
            ctx.named.insert(no);
            surfaced += 1;
            if surfaced.is_multiple_of(1000) {
                ctx.tick(&format!("orphan pass: {surfaced} inodes surfaced"))
                    .await;
            }
        }
    }
    info!(surfaced, "ext orphan pass complete");
    Ok(())
}

/// Deep pass: carve the block bitmaps' free runs.
async fn carve_free_runs(ctx: &ScanCtx) -> ExtResult<()> {
    let db = dw_signatures::SignatureDb::from_embedded();
    let mut carved_total = 0u64;
    for g in 0..ctx.vol.sb.group_count {
        if ctx.cancelled() {
            return Ok(());
        }
        let bm = match ctx.vol.block_bitmap(ctx.read.as_ref(), g) {
            Ok(bm) => bm,
            Err(e) => {
                warn!(group = g, error = %e, "unreadable block bitmap; skipping");
                continue;
            }
        };
        let first = ctx.vol.group_first_block(g);
        let bs = u64::from(ctx.vol.sb.block_size);
        for (start, len) in bm.free_runs() {
            if ctx.cancelled() {
                return Ok(());
            }
            let start_byte = (first + start) * bs;
            let length = len * bs;
            if length == 0 {
                continue;
            }
            carved_total += dw_carver::header_footer::carve_region(
                ctx.read.as_ref(),
                start_byte,
                length,
                &db,
                &ctx.results,
                Some(&ctx.cancel),
                None,
            );
        }
    }
    info!(carved = carved_total, "ext free-space carving complete");
    Ok(())
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Epoch helper used by tests (kept public for the integration harness).
#[must_use]
pub fn epoch_of(secs: i64) -> Option<OffsetDateTime> {
    if secs <= 0 {
        None
    } else {
        OffsetDateTime::from_unix_timestamp(secs).ok()
    }
}
