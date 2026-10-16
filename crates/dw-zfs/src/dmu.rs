//! DMU primitives: verified block reads, dnode access, objset parsing.
//!
//! Layout constants (OpenZFS headers, extracted verbatim):
//! - `dnode_phys_t` = 512 B: core 64 B + `dn_blkptr[3]` @64, `dn_bonus`
//!   @192 (320 B), `dn_spill` @480 (32 B). Object ids address a 16 KiB
//!   dnode array: 32 slots per block (`DNODES_PER_BLOCK = 2^(14-9)`).
//! - `objset_phys_t` = 4096 B: `os_meta_dnode` @0, zil header @512 (192 B),
//!   `os_type` @704, `os_flags` @712, `os_userused_dnode` @1024,
//!   `os_groupused_dnode` @1536, `os_projectused_dnode` @2048.
//! - `dsl_dataset_phys_t` bonus: `ds_bp` (blkptr to the filesystem objset)
//!   @128.
//!
//! Indirect trees: a node of level k covers `per^k` data blocks where
//! `per = 2^(indblkshift-7)` (an indirect block of `2^indblkshift` bytes
//! holds `2^indblkshift/128` blkptrs). Navigation and full-L0 collection
//! both use this span arithmetic.

use crate::blkptr::{BLKPTR_SIZE, BlkPtr, dmu_ot};
use crate::checksum;
use crate::compress;
use crate::error::{ZfsError, ZfsResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DNODE_SIZE: usize = 512;
pub const DNODES_PER_BLOCK: u64 = 32; // 16 KiB / 512
pub const OBJSET_SIZE: usize = 4096;
/// `dn_bonus` offset inside a 512 B dnode.
pub const DNODE_BONUS_OFF: usize = 192;
/// Bonus capacity of a minimum-size dnode (`DN_OLD_MAX_BONUSLEN`).
pub const DNODE_BONUS_CAP: usize = 320;
/// `dn_spill` offset (tail 32 B).
pub const DNODE_SPILL_OFF: usize = 480;
/// `ds_bp` offset inside a dsl_dataset bonus buffer.
pub const DS_BP_OFF: usize = 128;

/// Cap on indirect-tree levels (ZFS uses 1..7 for data).
const MAX_LEVELS: u8 = 7;

/// Exact synchronous read into a fresh buffer.
pub fn read_exact(read: &dyn dw_disk::ReadAt, off: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    read.read_at(off, &mut buf).map_err(std::io::Error::other)?;
    Ok(buf)
}
/// The DMU reader: owns the source and provides verified block access.
pub struct Dmu {
    read: Arc<dyn dw_disk::ReadAt>,
    src_size: u64,
    cancel: Arc<AtomicBool>,
    /// Blocks read / checksum-verified (for progress reporting).
    pub blocks_read: u64,
}

impl Dmu {
    pub fn new(read: Arc<dyn dw_disk::ReadAt>, src_size: u64, cancel: Arc<AtomicBool>) -> Dmu {
        Dmu {
            read,
            src_size,
            cancel,
            blocks_read: 0,
        }
    }

    pub fn src_size(&self) -> u64 {
        self.src_size
    }

    /// Raw (unverified) read used for label/uberblock structures.
    pub fn read_raw(&self, off: u64, len: usize) -> std::io::Result<Vec<u8>> {
        read_exact(self.read.as_ref(), off, len)
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Read, verify, and decompress the block a blkptr points to.
    /// Returns the logical (lsize) content. Holes map to zeros; embedded
    /// and encrypted blocks are explicit errors.
    pub fn read_bp(&mut self, bp: &BlkPtr) -> ZfsResult<Vec<u8>> {
        if bp.is_hole() {
            return Ok(vec![0u8; bp.lsize() as usize]);
        }
        if bp.is_embedded() {
            return Err(ZfsError::EmbeddedBp { off: 0 });
        }
        if bp.uses_crypt() {
            return Err(ZfsError::Corrupt(
                "encrypted block: native encryption is out of scope".into(),
            ));
        }
        let dva = bp
            .dva(0)
            .ok_or_else(|| ZfsError::Corrupt("blkptr with no DVA".into()))?;
        if dva.gang {
            return Err(ZfsError::Corrupt(
                "gang block: gang headers are out of scope for this engine".into(),
            ));
        }
        let psize = bp.psize() as usize;
        let lsize = bp.lsize();
        if dva.offset + psize as u64 > self.src_size {
            return Err(ZfsError::ReadOutOfBounds {
                off: dva.offset,
                size: psize,
                src: self.src_size,
            });
        }
        let raw = read_exact(self.read.as_ref(), dva.offset, psize)?;
        self.blocks_read += 1;
        checksum::verify_block(bp, dva.offset, &raw)?;
        compress::decompress(bp.compression(), &raw, lsize, dva.offset)
    }

    /// Navigate the dnode's indirect tree to data block `block_id`.
    /// Single-level dnodes address blocks through their inline pointers
    /// (block ids beyond `nblkptr` do not exist for level-1 dnodes: ZFS
    /// switches to an indirect root as soon as the file grows past 3
    /// blocks, with `dn_blkptr[0]` as the tree root).
    pub fn read_dnode_block(&mut self, dn: &Dnode, block_id: u64) -> ZfsResult<Option<Vec<u8>>> {
        let levels = dn.nlevels();
        if levels <= 1 {
            return match dn.blkptr(block_id as usize) {
                Some(bp) => Ok(Some(self.read_bp(&bp)?)),
                None => Ok(None),
            };
        }
        let root = match dn.blkptr(0) {
            Some(bp) => bp,
            None => return Ok(None),
        };
        self.tree_read(&root, levels, dn.children_per_node(), block_id)
            .map(Some)
    }

    fn tree_read(&mut self, bp: &BlkPtr, level: u8, per: u64, block_id: u64) -> ZfsResult<Vec<u8>> {
        if self.cancelled() {
            return Ok(Vec::new());
        }
        if level == 1 {
            // Children of this node are data blocks.
            if block_id >= per {
                return Ok(Vec::new()); // beyond this node: caller handles
            }
            let node = self.read_bp(bp)?;
            let idx = block_id as usize;
            let child = match BlkPtr::from_slice(&node[idx * BLKPTR_SIZE..]) {
                Some(c) => c,
                None => return Ok(Vec::new()),
            };
            return self.read_bp(&child);
        }
        // Upper node: child at level-1 covers per^(level-1) data blocks.
        let node = self.read_bp(bp)?;
        let child_span = per.pow(u32::from(level - 1));
        let idx = block_id / child_span;
        if idx >= per {
            return Ok(Vec::new());
        }
        let child = match BlkPtr::from_slice(&node[idx as usize * BLKPTR_SIZE..]) {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };
        // Holes carry their lsize: read_bp returns the zero span.
        self.tree_read(&child, level - 1, per, block_id % child_span)
    }

    /// Collect every L0 blkptr of a dnode's data, in file order. Holes are
    /// included as hole bps so file order stays aligned; the walker skips
    /// them when building extents.
    pub fn file_block_ptrs(&mut self, dn: &Dnode, max_blocks: u64) -> ZfsResult<Vec<BlkPtr>> {
        let levels = dn.nlevels();
        let count = max_blocks.min(dn.maxblkid().saturating_add(1));
        if levels <= 1 {
            let mut out = Vec::new();
            for i in 0..count.min(dn.nblkptr() as u64) {
                if self.cancelled() {
                    break;
                }
                if let Some(bp) = dn.blkptr(i as usize) {
                    out.push(bp);
                }
            }
            Ok(out)
        } else {
            let root = match dn.blkptr(0) {
                Some(bp) => bp,
                None => return Ok(Vec::new()),
            };
            let per = dn.children_per_node();
            let mut out = Vec::new();
            self.tree_collect(&root, levels, per, &mut out, count)?;
            Ok(out)
        }
    }

    fn tree_collect(
        &mut self,
        bp: &BlkPtr,
        level: u8,
        per: u64,
        acc: &mut Vec<BlkPtr>,
        limit: u64,
    ) -> ZfsResult<()> {
        if self.cancelled() || acc.len() as u64 >= limit {
            return Ok(());
        }
        if level as usize > MAX_LEVELS as usize {
            return Err(ZfsError::IndirectTooDeep {
                levels: level,
                obj: 0,
            });
        }
        let node = self.read_bp(bp)?;
        if level == 1 {
            let entries = node.len() / BLKPTR_SIZE;
            for i in 0..entries {
                if acc.len() as u64 >= limit {
                    break;
                }
                if let Some(c) = BlkPtr::from_slice(&node[i * BLKPTR_SIZE..]) {
                    acc.push(c);
                }
            }
            return Ok(());
        }
        // Upper node: each child at level-1 covers per^(level-1) blocks.
        // Holes at internal levels stand for whole zero spans: emit the
        // child as a hole bp once (the walker treats holes as zeros).
        let entries = node.len() / BLKPTR_SIZE;
        for i in 0..entries {
            if self.cancelled() || acc.len() as u64 >= limit {
                return Ok(());
            }
            let child = match BlkPtr::from_slice(&node[i * BLKPTR_SIZE..]) {
                Some(c) => c,
                None => continue,
            };
            if child.is_hole() {
                // A zero span of per^(level-1) data blocks: emit hole bps
                // to keep ordering (capped by the limit).
                let span = per.pow(u32::from(level - 1));
                let mut emitted = 0u64;
                while emitted < span && (acc.len() as u64) < limit {
                    acc.push(child.clone());
                    emitted += 1;
                }
                continue;
            }
            self.tree_collect(&child, level - 1, per, acc, limit)?;
        }
        Ok(())
    }
}

/// An owned dnode (copy of its 512 on-disk bytes).
#[derive(Debug, Clone)]
pub struct Dnode {
    raw: [u8; DNODE_SIZE],
}

impl Dnode {
    pub fn from_slice(buf: &[u8]) -> Option<Dnode> {
        if buf.len() < DNODE_SIZE {
            return None;
        }
        let mut raw = [0u8; DNODE_SIZE];
        raw.copy_from_slice(&buf[..DNODE_SIZE]);
        Some(Dnode { raw })
    }

    fn u16(&self, off: usize) -> u16 {
        u16::from_le_bytes([self.raw[off], self.raw[off + 1]])
    }

    pub fn type_(&self) -> u8 {
        self.raw[0]
    }
    pub fn indblkshift(&self) -> u8 {
        self.raw[1]
    }
    pub fn nlevels(&self) -> u8 {
        self.raw[2]
    }
    pub fn nblkptr(&self) -> u8 {
        self.raw[3]
    }
    pub fn bonustype(&self) -> u8 {
        self.raw[4]
    }
    pub fn datablksz(&self) -> u64 {
        u64::from(self.u16(8)) * 512
    }
    pub fn bonuslen(&self) -> u16 {
        self.u16(10)
    }
    pub fn maxblkid(&self) -> u64 {
        crate::blkptr::le_u64(&self.raw, 16)
    }
    pub fn used(&self) -> u64 {
        crate::blkptr::le_u64(&self.raw, 24)
    }

    /// Blkptrs per indirect node for this dnode's tree.
    pub fn children_per_node(&self) -> u64 {
        1u64 << (self.indblkshift().saturating_sub(7))
    }

    /// The i-th inline blkptr (0-based) from the tail region @64.
    pub fn blkptr(&self, i: usize) -> Option<BlkPtr> {
        if i >= self.nblkptr() as usize {
            return None;
        }
        BlkPtr::from_slice(&self.raw[64 + i * BLKPTR_SIZE..])
    }

    /// Bonus buffer bytes (`dn_bonus` @192), length `dn_bonuslen` capped
    /// at the physical 320 B tail.
    pub fn bonus(&self) -> &[u8] {
        let len = self.bonuslen() as usize;
        &self.raw[DNODE_BONUS_OFF..DNODE_BONUS_OFF + len.min(DNODE_BONUS_CAP)]
    }

    pub fn is_free(&self) -> bool {
        self.type_() == dmu_ot::NONE && self.bonustype() == dmu_ot::NONE
    }
}

/// Parsed objset_phys (the 4096-B block a rootbp points to).
pub struct Objset {
    raw: Vec<u8>,
}

impl Objset {
    pub fn from_block(block: &[u8]) -> Option<Objset> {
        if block.len() < OBJSET_SIZE {
            return None;
        }
        Some(Objset {
            raw: block[..OBJSET_SIZE].to_vec(),
        })
    }

    pub fn from_vec(block: Vec<u8>) -> Option<Objset> {
        if block.len() < OBJSET_SIZE {
            return None;
        }
        Some(Objset { raw: block })
    }

    /// The meta dnode (dnode array root) at offset 0.
    pub fn meta_dnode(&self) -> Dnode {
        Dnode::from_slice(&self.raw[..DNODE_SIZE]).expect("objset >= 4096")
    }

    pub fn os_type(&self) -> u64 {
        crate::blkptr::le_u64(&self.raw, 704)
    }
}

/// Object-array access for one objset: resolves object ids to owned
/// dnodes, caching the dnode-array blocks.
pub struct ObjectTable {
    cache: HashMap<u64, Vec<u8>>,
}

impl Default for ObjectTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectTable {
    pub fn new() -> ObjectTable {
        ObjectTable {
            cache: HashMap::new(),
        }
    }

    /// Fetch the dnode for `objid` (1-based). Returns None when the
    /// array block or the slot is absent.
    pub fn dnode(&mut self, dmu: &mut Dmu, os: &Objset, objid: u64) -> ZfsResult<Option<Dnode>> {
        if objid == 0 {
            return Ok(None);
        }
        let block_id = (objid - 1) / DNODES_PER_BLOCK;
        let slot = ((objid - 1) % DNODES_PER_BLOCK) as usize;
        if let std::collections::hash_map::Entry::Vacant(e) = self.cache.entry(block_id) {
            let meta = os.meta_dnode();
            let block = match dmu.read_dnode_block(&meta, block_id)? {
                Some(b) => b,
                None => return Ok(None),
            };
            e.insert(block);
        }
        let block = self.cache.get(&block_id).expect("just inserted");
        Ok(Dnode::from_slice(&block[slot * DNODE_SIZE..]))
    }

    /// Iterate object ids 1..=max, yielding dnodes until the array ends.
    pub fn iter_until(
        &mut self,
        dmu: &mut Dmu,
        os: &Objset,
        max: u64,
    ) -> ZfsResult<Vec<(u64, Dnode)>> {
        let mut out = Vec::new();
        for objid in 1..=max {
            if dmu.cancelled() {
                break;
            }
            match self.dnode(dmu, os, objid)? {
                Some(dn) => out.push((objid, dn)),
                None => break,
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnode_field_offsets() {
        let mut raw = vec![0u8; 512];
        raw[0] = 19; // PLAIN_FILE_CONTENTS
        raw[1] = 17; // indblkshift
        raw[2] = 1;
        raw[3] = 3;
        raw[4] = 44; // SA bonus
        raw[8..10].copy_from_slice(&32u16.to_le_bytes()); // 16 KiB
        raw[10..12].copy_from_slice(&64u16.to_le_bytes());
        raw[16..24].copy_from_slice(&7u64.to_le_bytes());
        let dn = Dnode::from_slice(&raw).unwrap();
        assert_eq!(dn.type_(), 19);
        assert_eq!(dn.indblkshift(), 17);
        assert_eq!(dn.datablksz(), 16384);
        assert_eq!(dn.bonuslen(), 64);
        assert_eq!(dn.maxblkid(), 7);
        assert_eq!(dn.bonus().len(), 64);
        assert!(dn.blkptr(0).is_some());
        assert!(dn.blkptr(3).is_none());
        // indblkshift 17 => per = 2^10 = 1024 ptrs per indirect node
        assert_eq!(dn.children_per_node(), 1024);
        assert!(!dn.is_free());
    }

    #[test]
    fn objset_offsets() {
        let mut raw = vec![0u8; 4096];
        raw[704..712].copy_from_slice(&u64::MAX.to_le_bytes());
        raw[0] = 10; // meta dnode type = DNODE
        let os = Objset::from_block(&raw).unwrap();
        assert_eq!(os.os_type(), u64::MAX);
        assert_eq!(os.meta_dnode().type_(), 10);
    }
}
