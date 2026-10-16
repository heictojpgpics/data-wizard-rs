//! Block mapping: extent trees (ext4) and legacy indirect maps (ext2/3,
//! plus ext4 inodes without the EXTENTS flag).
//!
//! Both walks return physical FS block numbers in *logical* order so the
//! shared extent helper can build recovery extents that reproduce file
//! content order for fragmented files.
//!
//! Hard safety rules (all walks):
//! - every physical block is bounds-checked against the volume before use;
//! - child-node blocks and indirect blocks go in a visited set (cycle
//!   guard) with a total node-visit cap;
//! - unwritten extents (ee_len high bit) and zero pointers (holes) yield
//!   no data: their logical slot is skipped (documented behavior for
//!   reconstructing deleted files from partial maps).

use crate::Inode;
use crate::error::{ExtError, ExtResult};
use crate::superblock::SuperBlock;
use dw_disk::ReadAt;
use std::collections::HashSet;

/// `0xF30A` — ext4 extent node magic (`eh_magic`).
pub const EXTENT_MAGIC: u16 = 0xF30A;
/// Maximum extent tree depth (root included).
const MAX_DEPTH: u16 = 5;
/// Hard cap on mapped blocks per inode (1 MiB entries).
const MAX_BLOCKS_PER_INODE: usize = 1 << 20;
/// Cap on node/indirect-block visits per inode.
const MAX_NODE_VISITS: usize = 4096;

/// Result of mapping an inode's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockMap {
    /// Physical FS block numbers, logical order, data-bearing only.
    pub blocks: Vec<u64>,
    /// True when the on-disk map was shorter than the file size requires.
    pub truncated: bool,
}

/// Map `ino`'s data blocks. Chooses extent-tree vs indirect by the inode
/// flags; inodes with neither (and no inline data) yield an empty map.
pub fn map_inode_blocks(read: &dyn ReadAt, sb: &SuperBlock, ino: &Inode) -> ExtResult<BlockMap> {
    let wanted = data_blocks_needed(ino, sb);
    let mut st = WalkerState {
        read,
        sb,
        visited: HashSet::new(),
        node_visits: 0,
    };

    let blocks = if ino.uses_extents() {
        st.walk_extent_root(&ino.i_block)?
    } else {
        st.walk_indirect(&ino.i_block)?
    };

    let truncated = blocks.len() < wanted;
    Ok(BlockMap { blocks, truncated })
}

/// How many blocks the file's size needs (0 for symlinks sized by target).
fn data_blocks_needed(ino: &Inode, sb: &SuperBlock) -> usize {
    let bs = u64::from(sb.block_size);
    if ino.size == 0 || bs == 0 {
        return 0;
    }
    ino.size.div_ceil(bs).min(usize::MAX as u64) as usize
}

struct WalkerState<'a> {
    read: &'a dyn ReadAt,
    sb: &'a SuperBlock,
    visited: HashSet<u64>,
    node_visits: usize,
}

impl WalkerState<'_> {
    fn read_fs_block(&self, block: u64) -> ExtResult<Vec<u8>> {
        if block >= self.sb.blocks_count {
            return Err(ExtError::Corrupt(format!(
                "block {block} beyond volume ({})",
                self.sb.blocks_count
            )));
        }
        let off = block * u64::from(self.sb.block_size);
        let len = self.sb.block_size as usize;
        let mut buf = vec![0u8; len];
        self.read.read_at(off, &mut buf).map_err(ExtError::Disk)?;
        Ok(buf)
    }

    fn note_visit(&mut self, block: u64) -> ExtResult<()> {
        self.node_visits += 1;
        if self.node_visits > MAX_NODE_VISITS {
            return Err(ExtError::Corrupt("node visit budget exceeded".into()));
        }
        if !self.visited.insert(block) {
            return Err(ExtError::Corrupt(format!(
                "cycle detected at block {block}"
            )));
        }
        Ok(())
    }

    /// Walk the extent tree rooted in i_block (depth 0..5).
    fn walk_extent_root(&mut self, i_block: &[u8; 60]) -> ExtResult<Vec<u64>> {
        let magic = u16::from_le_bytes([i_block[0], i_block[1]]);
        if magic != EXTENT_MAGIC {
            // Extent flag set but root is not an extent header: corrupt.
            return Err(ExtError::Corrupt(
                "extent flag set but root magic bad".into(),
            ));
        }
        let entries = u16::from_le_bytes([i_block[2], i_block[3]]);
        let depth = u16::from_le_bytes([i_block[6], i_block[7]]);
        if depth > MAX_DEPTH {
            return Err(ExtError::Corrupt(format!(
                "extent depth {depth} > {MAX_DEPTH}"
            )));
        }
        if usize::from(entries) > 4 {
            // i_block root holds at most 4 entries (60-12)/12.
            return Err(ExtError::Corrupt("root eh_entries exceeds capacity".into()));
        }
        self.walk_extent_node(&i_block[..12 + 4 * 12], depth, 0)
    }

    /// Walk one extent node buffer (header + entries); returns the data
    /// blocks under it in logical order.
    fn walk_extent_node(
        &mut self,
        node: &[u8],
        depth: u16,
        _logical_base: u64,
    ) -> ExtResult<Vec<u64>> {
        if node.len() < 12 {
            return Err(ExtError::Corrupt("truncated extent node".into()));
        }
        let magic = u16::from_le_bytes([node[0], node[1]]);
        if magic != EXTENT_MAGIC {
            return Err(ExtError::Corrupt("extent node magic mismatch".into()));
        }
        let entries = u16::from_le_bytes([node[2], node[3]]);
        let capacity = (node.len() - 12) / 12;
        if usize::from(entries) > capacity {
            return Err(ExtError::Corrupt("eh_entries exceeds node capacity".into()));
        }

        let mut out = Vec::new();
        if depth == 0 {
            for i in 0..usize::from(entries) {
                let e = &node[12 + i * 12..12 + i * 12 + 12];
                let ee_len = u16::from_le_bytes([e[4], e[5]]);
                let unwritten = ee_len & 0x8000 != 0;
                let run_len = u64::from(ee_len & 0x7FFF);
                if run_len == 0 || unwritten {
                    continue;
                }
                let start_hi = u16::from_le_bytes([e[6], e[7]]) as u64;
                let start_lo = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64;
                let start = start_lo | (start_hi << 32);
                for b in 0..run_len {
                    let phys = start + b;
                    if phys >= self.sb.blocks_count {
                        return Err(ExtError::Corrupt(format!(
                            "extent points past volume: block {phys}"
                        )));
                    }
                    if out.len() >= MAX_BLOCKS_PER_INODE {
                        return Err(ExtError::Corrupt("block budget exceeded".into()));
                    }
                    out.push(phys);
                }
            }
        } else {
            // Index level: each entry points to a child node block.
            for i in 0..usize::from(entries) {
                let e = &node[12 + i * 12..12 + i * 12 + 12];
                let leaf_lo = u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as u64;
                let leaf_hi = u16::from_le_bytes([e[8], e[9]]) as u64;
                let child_block = leaf_lo | (leaf_hi << 32);
                if child_block == 0 {
                    continue;
                }
                self.note_visit(child_block)?;
                let child = self.read_fs_block(child_block)?;
                // Full child node; recurse with depth-1.
                let sub = self.walk_extent_node(&child, depth - 1, 0)?;
                out.extend(sub);
                if out.len() >= MAX_BLOCKS_PER_INODE {
                    return Err(ExtError::Corrupt("block budget exceeded".into()));
                }
            }
        }
        Ok(out)
    }

    /// Legacy direct/indirect map (ext2/3 and non-extent ext4).
    fn walk_indirect(&mut self, i_block: &[u8; 60]) -> ExtResult<Vec<u64>> {
        let u32at = |b: &[u8], o: usize| -> u32 {
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        };
        let mut out = Vec::new();
        // Direct blocks 0..11.
        for i in 0..12 {
            let p = u64::from(u32at(i_block, i * 4));
            if p == 0 {
                continue;
            }
            if p >= self.sb.blocks_count {
                return Err(ExtError::Corrupt(format!("direct block {p} beyond volume")));
            }
            out.push(p);
        }
        // Single indirect at i_block[12] (byte 48).
        if let Some(blk) = nonzero(u32at(i_block, 48)) {
            let singles = self.read_indirect_block(blk as u64)?;
            for p in singles {
                if p != 0 {
                    out.push(p);
                }
            }
        }
        // Double indirect at i_block[13] (byte 52).
        if let Some(blk) = nonzero(u32at(i_block, 52)) {
            let doubles = self.read_indirect_block(blk as u64)?;
            for pb in doubles {
                if pb == 0 {
                    continue;
                }
                let singles = self.read_indirect_block(pb)?;
                for p in singles {
                    if p != 0 {
                        out.push(p);
                    }
                }
            }
        }
        // Triple indirect at i_block[14] (byte 56).
        if let Some(blk) = nonzero(u32at(i_block, 56)) {
            let triples = self.read_indirect_block(blk as u64)?;
            for tb in triples {
                if tb == 0 {
                    continue;
                }
                let doubles = self.read_indirect_block(tb)?;
                for pb in doubles {
                    if pb == 0 {
                        continue;
                    }
                    let singles = self.read_indirect_block(pb)?;
                    for p in singles {
                        if p != 0 {
                            out.push(p);
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Read one indirect block and return its pointers (unfiltered, 0s
    /// included as holes) after bounds-checking nonzero values.
    fn read_indirect_block(&mut self, block: u64) -> ExtResult<Vec<u64>> {
        self.note_visit(block)?;
        let raw = self.read_fs_block(block)?;
        let n = raw.len() / 4;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let p = u64::from(u32::from_le_bytes([
                raw[i * 4],
                raw[i * 4 + 1],
                raw[i * 4 + 2],
                raw[i * 4 + 3],
            ]));
            if p != 0 && p >= self.sb.blocks_count {
                return Err(ExtError::Corrupt(format!(
                    "indirect pointer {p} beyond volume"
                )));
            }
            out.push(p);
        }
        Ok(out)
    }
}

fn nonzero(v: u32) -> Option<u32> {
    if v == 0 { None } else { Some(v) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_disk::{DiskSource, FileBackedSource};

    /// Test volume: 256 blocks x 4 KiB (1 MiB), 1 group, ext4-ish geometry.
    struct TestVol {
        sb: SuperBlock,
        src: FileBackedSource,
        _tmp: tempfile::NamedTempFile,
    }

    fn fake_sb() -> SuperBlock {
        SuperBlock {
            inodes_count: 8,
            blocks_count: 256,
            free_blocks_count: 240,
            free_inodes_count: 3,
            first_data_block: 0,
            block_size: 4096,
            blocks_per_group: 256,
            inodes_per_group: 8,
            inode_size: 256,
            desc_size: 32,
            state: 1,
            rev_level: 1,
            feature_compat: 0,
            feature_incompat: 0x42,
            feature_ro_compat: 0,
            uuid: [0; 16],
            volume_name: String::new(),
            last_mounted: String::new(),
            volume_uuid_string: String::new(),
            mtime: None,
            group_count: 1,
        }
    }

    /// Build a volume with `patch` applied to the raw image before open.
    fn with_image(patch: impl FnOnce(&mut [u8])) -> TestVol {
        let mut img = vec![0u8; 256 * 4096];
        patch(&mut img);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &img).unwrap();
        let src = FileBackedSource::open(tmp.path()).unwrap();
        TestVol {
            sb: fake_sb(),
            src,
            _tmp: tmp,
        }
    }

    fn extent_root(entries: &[(u32, u64, u16)]) -> [u8; 60] {
        // (logical, phys_start, len)
        let mut ib = [0u8; 60];
        ib[0..2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());
        ib[2..4].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        ib[4..6].copy_from_slice(&4u16.to_le_bytes()); // eh_max
        ib[6..8].copy_from_slice(&0u16.to_le_bytes()); // depth 0
        for (i, &(lg, start, len)) in entries.iter().enumerate() {
            let o = 12 + i * 12;
            ib[o..o + 4].copy_from_slice(&lg.to_le_bytes());
            ib[o + 4..o + 6].copy_from_slice(&len.to_le_bytes());
            ib[o + 6..o + 8].copy_from_slice(&((start >> 32) as u16).to_le_bytes());
            ib[o + 8..o + 12].copy_from_slice(&(start as u32).to_le_bytes());
        }
        ib
    }

    fn inode_with(i_block: [u8; 60], size: u64, flags: u32) -> Inode {
        Inode {
            mode: 0x81A4,
            uid: 0,
            gid: 0,
            size,
            sectors: 8,
            links_count: 1,
            flags,
            generation: 0,
            file_acl: 0,
            atime: None,
            ctime: None,
            mtime: None,
            dtime: None,
            crtime: None,
            i_block,
        }
    }

    #[test]
    fn single_extent_leaf_maps_contiguous_run() {
        let tv = with_image(|_| {});
        let read = tv.src.as_read_at();
        let ino = inode_with(extent_root(&[(0, 100, 3)]), 12 * 1024, 0x80000);
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert_eq!(map.blocks, vec![100, 101, 102]);
        assert!(!map.truncated);
    }

    #[test]
    fn fragmented_file_keeps_logical_order() {
        let tv = with_image(|_| {});
        let read = tv.src.as_read_at();
        // logical 0..2 -> phys 200; logical 2..4 -> phys 202 (gap at 201,203)
        let ino = inode_with(extent_root(&[(0, 200, 2), (2, 202, 2)]), 16 * 1024, 0x80000);
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert_eq!(map.blocks, vec![200, 201, 202, 203]);
        assert!(!map.truncated);
    }

    #[test]
    fn unwritten_extents_are_skipped() {
        let tv = with_image(|_| {});
        let read = tv.src.as_read_at();
        // entry 0 normal (0,100,2); entry 1 unwritten (2,300,2|0x8000)
        let mut ib = extent_root(&[(0, 100, 2), (2, 300, 2)]);
        let len2 = u16::from_le_bytes([ib[24 + 4], ib[24 + 5]]);
        ib[24 + 4..24 + 6].copy_from_slice(&(len2 | 0x8000).to_le_bytes());
        let ino = inode_with(ib, 16 * 1024, 0x80000);
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert_eq!(map.blocks, vec![100, 101]); // unwritten slot dropped
        assert!(map.truncated); // 16 KiB wants 4 blocks; map has 2
    }

    #[test]
    fn out_of_volume_extent_rejected() {
        let tv = with_image(|_| {});
        let read = tv.src.as_read_at();
        let ino = inode_with(extent_root(&[(0, 300, 3)]), 12 * 1024, 0x80000); // 300+3 > 256
        assert!(map_inode_blocks(read.as_ref(), &tv.sb, &ino).is_err());
    }

    #[test]
    fn depth_two_tree_traverses_children() {
        // Child leaf in block 60: 1 entry mapping logical 0 -> phys 150 len 2.
        let tv = with_image(|img| {
            let base = 60 * 4096;
            img[base..base + 2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());
            img[base + 2..base + 4].copy_from_slice(&1u16.to_le_bytes());
            img[base + 6..base + 8].copy_from_slice(&0u16.to_le_bytes());
            img[base + 12..base + 16].copy_from_slice(&0u32.to_le_bytes());
            img[base + 16..base + 18].copy_from_slice(&2u16.to_le_bytes());
            img[base + 18..base + 20].copy_from_slice(&0u16.to_le_bytes());
            img[base + 20..base + 24].copy_from_slice(&150u32.to_le_bytes());
        });
        let read = tv.src.as_read_at();
        // Root: depth 1, one index entry -> child at block 60.
        let mut root = [0u8; 60];
        root[0..2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());
        root[2..4].copy_from_slice(&1u16.to_le_bytes());
        root[4..6].copy_from_slice(&4u16.to_le_bytes());
        root[6..8].copy_from_slice(&1u16.to_le_bytes()); // depth 1
        root[12..16].copy_from_slice(&0u32.to_le_bytes());
        root[16..20].copy_from_slice(&60u32.to_le_bytes());
        root[20..22].copy_from_slice(&0u16.to_le_bytes());

        let ino = inode_with(root, 8 * 1024, 0x80000);
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert_eq!(map.blocks, vec![150, 151]);
    }

    #[test]
    fn indirect_walk_maps_direct_and_single() {
        // Direct 0..2 at 30,31; single indirect block 40 with [50,51,0,53].
        let tv = with_image(|img| {
            let base = 40 * 4096;
            for (i, p) in [50u32, 51, 0, 53].iter().enumerate() {
                img[base + i * 4..base + i * 4 + 4].copy_from_slice(&p.to_le_bytes());
            }
        });
        let read = tv.src.as_read_at();
        let mut ib = [0u8; 60];
        ib[0..4].copy_from_slice(&30u32.to_le_bytes());
        ib[4..8].copy_from_slice(&31u32.to_le_bytes());
        ib[48..52].copy_from_slice(&40u32.to_le_bytes()); // i_block[12]
        let ino = inode_with(ib, 8 * 1024, 0); // no EXTENTS flag
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert_eq!(map.blocks, vec![30, 31, 50, 51, 53]); // hole skipped
    }

    #[test]
    fn cycle_detection_trips() {
        // The same block 70 hangs off both the single- and the
        // double-indirect slots: the node-visit guard must trip.
        let tv = with_image(|img| {
            let base = 70 * 4096;
            img[base..base + 4].copy_from_slice(&70u32.to_le_bytes());
        });
        let read = tv.src.as_read_at();
        let mut ib = [0u8; 60];
        ib[48..52].copy_from_slice(&70u32.to_le_bytes()); // i_block[12]
        ib[52..56].copy_from_slice(&70u32.to_le_bytes()); // i_block[13]
        let ino = inode_with(ib, 8 * 1024, 0);
        assert!(map_inode_blocks(read.as_ref(), &tv.sb, &ino).is_err());
    }

    #[test]
    fn zero_inode_yields_empty_map_not_truncated_for_zero_size() {
        let tv = with_image(|_| {});
        let read = tv.src.as_read_at();
        let ino = inode_with([0u8; 60], 0, 0); // no flags: zero indirect map
        let map = map_inode_blocks(read.as_ref(), &tv.sb, &ino).unwrap();
        assert!(map.blocks.is_empty());
        assert!(!map.truncated); // zero size: nothing missing
    }
}
