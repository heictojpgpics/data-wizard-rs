//! B-tree node reading and tree walking.
//!
//! Node layout (kernel `btrfs_tree.h`, offsets compiled against the
//! header): header 101 bytes (csum[32]@0, fsid@0x20, bytenr@0x30,
//! flags@0x38, chunk_tree_uuid@0x40, generation@0x50, owner@0x58,
//! nritems@0x60, level@0x64); leaf items are 25 bytes each at 101
//! (key@0, offset@0x11, size@0x15); internal key pointers are 33 bytes
//! each at 101 (key@0, blockptr@0x11, generation@0x19).
//!
//! Every node is verified: crc32c over bytes 32..nodesize must match
//! `csum[0..4]`, fsid must equal the superblock's, and bytenr must equal
//! the requested logical address. A node that fails any check is
//! corrupt by definition - the walker skips it and reports, it never
//! guesses.

use crate::chunk::ChunkMap;
use crate::error::{BtrfsError, BtrfsResult};
use crate::superblock::{SuperBlock, crc32c};
use dw_disk::ReadAt;

pub const HEADER_SIZE: usize = 101;
/// Item array cap per node: (nodesize - 101) / 25 rounded hard-down.
fn max_items(nodesize: u32) -> usize {
    ((nodesize as usize).saturating_sub(HEADER_SIZE)) / 25
}
/// Total node-visit budget per tree walk.
const MAX_NODE_VISITS: usize = 65536;
/// Total item budget per tree walk.
const MAX_ITEMS: usize = 1 << 22;

/// One flattened item from a leaf, in key order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeItem {
    pub objectid: u64,
    pub key_type: u8,
    pub offset: u64,
    pub data: Vec<u8>,
}

/// A parsed node (either level, headers only plus raw bytes kept for
/// item extraction).
pub struct Node {
    pub logical: u64,
    pub level: u8,
    pub owner: u64,
    pub generation: u64,
    pub nritems: u32,
    pub raw: Vec<u8>,
}

impl Node {
    /// Read and verify the node at `logical`. Errors on any integrity
    /// failure (csum, fsid, bytenr) or unmappable logical.
    pub fn read(
        read: &dyn ReadAt,
        sb: &SuperBlock,
        map: &ChunkMap,
        logical: u64,
    ) -> BtrfsResult<Self> {
        let physical = map
            .map(logical)
            .ok_or_else(|| BtrfsError::Corrupt(format!("logical {logical:#x} in no chunk")))?;
        if physical + u64::from(sb.nodesize) > read.size() {
            return Err(BtrfsError::Corrupt(format!(
                "node {logical:#x} physical {physical:#x} beyond volume"
            )));
        }
        let mut raw = vec![0u8; sb.nodesize as usize];
        read.read_at(physical, &mut raw).map_err(BtrfsError::Disk)?;

        // Integrity: csum over 32..nodesize.
        let expect = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let actual = crc32c(&raw[32..]);
        if expect != actual {
            return Err(BtrfsError::Corrupt(format!(
                "node {logical:#x} csum mismatch (stored {expect:#010x}, computed {actual:#010x})"
            )));
        }
        // fsid (first 4 fields of a node must match the super's).
        if raw[0x20..0x30] != sb.fsid {
            return Err(BtrfsError::Corrupt(format!(
                "node {logical:#x} fsid mismatch"
            )));
        }
        let bytenr = u64le(&raw, 0x30);
        if bytenr != logical {
            return Err(BtrfsError::Corrupt(format!(
                "node bytenr {bytenr:#x} != requested {logical:#x}"
            )));
        }
        let nritems = u32le(&raw, 0x60);
        let level = raw[0x64];
        if level > 8 {
            return Err(BtrfsError::Corrupt(format!("node level {level} > 8")));
        }
        if nritems as usize > max_items(sb.nodesize) {
            return Err(BtrfsError::Corrupt(format!(
                "nritems {nritems} exceeds node capacity"
            )));
        }
        Ok(Self {
            logical,
            level,
            owner: u64le(&raw, 0x58),
            generation: u64le(&raw, 0x50),
            nritems,
            raw,
        })
    }

    /// Leaf item data at `i`, fully bounds-checked.
    pub fn leaf_item(&self, i: usize, nodesize: u32) -> BtrfsResult<(u64, u8, u64, u64, u32)> {
        if i >= self.nritems as usize {
            return Err(BtrfsError::Corrupt("leaf item index out of range".into()));
        }
        let base = HEADER_SIZE + 25 * i;
        let objectid = u64le(&self.raw, base);
        let key_type = self.raw[base + 8];
        let offset = u64le(&self.raw, base + 9);
        let item_off = u32le(&self.raw, base + 0x11);
        let item_size = u32le(&self.raw, base + 0x15);
        let n = nodesize as usize;
        if (item_off as usize) < HEADER_SIZE
            || (item_off as usize).saturating_add(item_size as usize) > n
        {
            return Err(BtrfsError::Corrupt(format!(
                "leaf item {i} window {item_off}+{item_size} out of node"
            )));
        }
        Ok((objectid, key_type, offset, item_off as u64, item_size))
    }

    /// Internal child pointer at `i`.
    pub fn child_ptr(&self, i: usize) -> BtrfsResult<u64> {
        if i >= self.nritems as usize {
            return Err(BtrfsError::Corrupt("key_ptr index out of range".into()));
        }
        let base = HEADER_SIZE + 33 * i;
        Ok(u64le(&self.raw, base + 0x11))
    }
}

/// Walk a whole tree from `root` (logical), flattening every leaf item in
/// key order. Internal-level children are visited in order; corrupt
/// nodes are skipped with a warning (partial trees surface partial
/// results - the alternative, failing the whole walk, would hide
/// recoverable data on damaged volumes).
pub fn walk_tree(
    read: &dyn ReadAt,
    sb: &SuperBlock,
    map: &ChunkMap,
    root: u64,
) -> BtrfsResult<(std::collections::HashSet<u64>, Vec<NodeItem>)> {
    let mut out = Vec::new();
    let mut visited: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut visits = 0usize;
    walk_node(read, sb, map, root, &mut visited, &mut visits, &mut out)?;
    Ok((visited, out))
}

fn walk_node(
    read: &dyn ReadAt,
    sb: &SuperBlock,
    map: &ChunkMap,
    logical: u64,
    visited: &mut std::collections::HashSet<u64>,
    visits: &mut usize,
    out: &mut Vec<NodeItem>,
) -> BtrfsResult<()> {
    if *visits >= MAX_NODE_VISITS {
        return Err(BtrfsError::Corrupt("node visit budget exceeded".into()));
    }
    *visits += 1;
    if !visited.insert(logical) {
        return Err(BtrfsError::Corrupt(format!("tree cycle at {logical:#x}")));
    }
    if out.len() > MAX_ITEMS {
        return Err(BtrfsError::Corrupt("item budget exceeded".into()));
    }
    if visited.len() > 4096 {
        return Err(BtrfsError::Corrupt("tree too wide".into()));
    }
    if visited.len() > 4096 {
        return Err(BtrfsError::Corrupt("tree too wide".into()));
    }
    let node = match Node::read(read, sb, map, logical) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(node = logical, error = %e, "skipping unreadable tree node");
            return Ok(());
        }
    };
    if node.level == 0 {
        for i in 0..node.nritems as usize {
            let (objectid, key_type, offset, item_off, item_size) =
                node.leaf_item(i, sb.nodesize)?;
            let data =
                node.raw[item_off as usize..(item_off + u64::from(item_size)) as usize].to_vec();
            out.push(NodeItem {
                objectid,
                key_type,
                offset,
                data,
            });
            if out.len() > MAX_ITEMS {
                return Err(BtrfsError::Corrupt("item budget exceeded".into()));
            }
        }
    } else {
        for i in 0..node.nritems as usize {
            let child = node.child_ptr(i)?;
            if child == 0 {
                continue;
            }
            // Depth guard: the level byte caps recursion in practice.
            walk_node(read, sb, map, child, visited, visits, out)?;
        }
    }
    Ok(())
}

/// Collect the set of node logical addresses visited by a walk plus the
/// flattened items (used by the deep pass to know which nodes are live).
pub fn collect_reachable(
    read: &dyn ReadAt,
    sb: &SuperBlock,
    map: &ChunkMap,
    root: u64,
) -> BtrfsResult<(std::collections::HashSet<u64>, Vec<NodeItem>)> {
    walk_tree(read, sb, map, root)
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([
        b[o],
        b[o + 1],
        b[o + 2],
        b[o + 3],
        b[o + 4],
        b[o + 5],
        b[o + 6],
        b[o + 7],
    ])
}
