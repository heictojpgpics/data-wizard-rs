//! HFS+ B-tree structures (Apple TN 1150; cross-validated against the
//! Linux `hfsplus` driver).
//!
//! BTNodeDescriptor (14 bytes, BIG-endian):
//! ```text
//!  0  fLink      u32   (next node of same kind; 0 = last)
//!  4  bLink      u32   (previous node; 0 = first)
//!  8  kind       u8    (see kinds below)
//!  9  height     u8
//! 10  numRecords u16
//! 12  reserved   u16
//! ```
//!
//! Node kinds (TN1150):
//! ```text
//!  0x00 invalid/free
//!  0x01 leaf
//!  0x02 index
//!  0x03 header
//!  0xFD map (-3)
//! ```
//!
//! BTreeHeaderRecord (node 0, starting at byte 14 of the node):
//! ```text
//! 14  treeDepth   u16
//! 16  rootNode    u32
//! 20  leafRecords u32
//! 24  firstLeafNode u32
//! 28  lastLeafNode  u32
//! 32  nodeSize    u16   ("blockSize" in TN1150; the node size)
//! 34  totalNodes  u32
//! 38  freeNodes   u32
//! 42  nextNodeID? u32   (further fields unused here)
//! ```
//!
//! Record offsets: entry for record i is a u16 (offset from node start)
//! stored at `node_end - 2*(i+1)`. Records grow up from the node start;
//! the offsets array grows down from the node end.

use crate::error::{HfsError, HfsResult};
use crate::volumeheader::VolumeHeader;

/// Node kind constants.
///
/// EMPIRICALLY VALIDATED against a real HFS+ volume (the reference
/// product's macOS stub DMG, reconstructed 2026-10-16): leaf nodes carry
/// kind 0xFF (-1 as s8) and the header node (node 0) carries 0x01.
/// The chain firstLeaf..fLink walked with 0xFF leaves and the
/// per-node numRecords summed exactly to the header's leafRecords.
pub mod node_kind {
    pub const INVALID: u8 = 0x00;
    /// Empirical: leaf nodes on real volumes.
    pub const LEAF: u8 = 0xFF;
    /// Empirical: the header node (node 0) on real volumes.
    pub const HEADER: u8 = 0x01;
    /// Not observed on this volume; conventional index value.
    pub const INDEX: u8 = 0x02;
    /// Map node: -3 as s8.
    pub const MAP: u8 = 0xFD;
}

/// Parsed B-tree node header info (without the record payloads).
#[derive(Debug, Clone)]
pub struct BTreeNode {
    /// Node number within the tree.
    pub number: u32,
    pub f_link: u32,
    pub b_link: u32,
    pub kind: u8,
    pub height: u8,
    pub num_records: u16,
    /// The full node bytes (node_size long).
    pub data: Vec<u8>,
}

impl BTreeNode {
    /// Parse the descriptor fields from a node buffer.
    pub fn parse(number: u32, data: Vec<u8>) -> HfsResult<Self> {
        if data.len() < 14 {
            return Err(HfsError::Corrupt("node shorter than a descriptor".into()));
        }
        let f_link = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let b_link = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let kind = data[8];
        let height = data[9];
        let num_records = u16::from_be_bytes([data[10], data[11]]);
        Ok(Self {
            number,
            f_link,
            b_link,
            kind,
            height,
            num_records,
            data,
        })
    }

    /// Absolute offset (within the node) of record `i`'s start.
    pub fn record_offset(&self, i: u16) -> HfsResult<usize> {
        if i >= self.num_records {
            return Err(HfsError::Corrupt(format!(
                "record index {i} >= numRecords {}",
                self.num_records
            )));
        }
        let node_end = self.data.len();
        let entry_pos = node_end - 2 * (i as usize + 1);
        if entry_pos + 1 >= node_end {
            return Err(HfsError::Corrupt("record offset entry out of node".into()));
        }
        let off = u16::from_be_bytes([self.data[entry_pos], self.data[entry_pos + 1]]) as usize;
        if off >= node_end {
            return Err(HfsError::Corrupt(format!(
                "record {i} offset {off} beyond the node"
            )));
        }
        Ok(off)
    }

    /// Slice of record `i` (start offset + content parse determines the
    /// end; here we return from the record start to the next record's
    /// start, or to the free-space boundary for the last record).
    pub fn record_slice(&self, i: u16) -> HfsResult<&[u8]> {
        let start = self.record_offset(i)?;
        // End: the next record's offset when present, else the offsets
        // array start (node_end - 2*numRecords).
        let node_end = self.data.len();
        let end = if i + 1 < self.num_records {
            self.record_offset(i + 1)?
        } else {
            node_end.saturating_sub(2 * self.num_records as usize)
        };
        if end <= start || end > self.data.len() {
            return Err(HfsError::Corrupt(format!(
                "record {i} range {start}..{end} invalid"
            )));
        }
        Ok(&self.data[start..end])
    }
}

/// B-tree geometry from the header node (node 0).
#[derive(Debug, Clone)]
pub struct BTreeGeometry {
    pub node_size: u16,
    pub total_nodes: u32,
    pub free_nodes: u32,
    pub first_leaf_node: u32,
    pub last_leaf_node: u32,
    pub leaf_records: u32,
    pub root_node: u32,
}

/// Read node `n` of a B-tree in `fork` using `read_at`.
pub fn read_node_at(
    read_at: &dyn dw_disk::ReadAt,
    vh: &VolumeHeader,
    fork: &crate::volumeheader::ForkData,
    n: u32,
    node_size: u16,
) -> HfsResult<BTreeNode> {
    let node_size = node_size as usize;
    if node_size < 14 {
        return Err(HfsError::Corrupt("node size below descriptor size".into()));
    }
    let base = vh
        .fork_offset_to_volume(fork, u64::from(n) * node_size as u64)
        .ok_or_else(|| {
            HfsError::Corrupt(format!(
                "node {n} beyond the fork's inline extents (extents-overflow not supported in v1)"
            ))
        })?;
    let mut buf = vec![0u8; node_size];
    read_at.read_at(base, &mut buf).map_err(HfsError::Disk)?;
    BTreeNode::parse(n, buf)
}

/// Parse the B-tree geometry from the header node (node 0).
pub fn parse_geometry(header_node: &BTreeNode) -> HfsResult<BTreeGeometry> {
    let d = &header_node.data;
    if d.len() < 48 {
        return Err(HfsError::Corrupt("header node too short".into()));
    }
    let g = BTreeGeometry {
        node_size: u16::from_be_bytes([d[32], d[33]]),
        total_nodes: u32::from_be_bytes([d[34], d[35], d[36], d[37]]),
        free_nodes: u32::from_be_bytes([d[38], d[39], d[40], d[41]]),
        first_leaf_node: u32::from_be_bytes([d[24], d[25], d[26], d[27]]),
        last_leaf_node: u32::from_be_bytes([d[28], d[29], d[30], d[31]]),
        leaf_records: u32::from_be_bytes([d[20], d[21], d[22], d[23]]),
        root_node: u32::from_be_bytes([d[16], d[17], d[18], d[19]]),
    };
    if g.node_size == 0 || !g.node_size.is_power_of_two() {
        return Err(HfsError::Corrupt(format!(
            "invalid node size {}",
            g.node_size
        )));
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn make_node(num_records: u16, records: &[&[u8]], f_link: u32, kind: u8) -> Vec<u8> {
        let node_size = 512usize;
        let mut d = vec![0u8; node_size];
        // descriptor
        d[0..4].copy_from_slice(&f_link.to_be_bytes());
        d[8] = kind;
        d[10..12].copy_from_slice(&num_records.to_be_bytes());
        // records from the start (after descriptor area conceptually; use 14
        // as the first possible start)
        let mut off = 14usize;
        for (i, r) in records.iter().enumerate() {
            d[off..off + r.len()].copy_from_slice(r);
            // offsets array entry for record i at end - 2*(i+1)
            let entry = node_size - 2 * (i + 1);
            d[entry..entry + 2].copy_from_slice(&(off as u16).to_be_bytes());
            off += r.len();
        }
        d
    }

    #[test]
    fn parses_node_descriptor() {
        let rec: Vec<u8> = vec![0xAB; 40];
        let data = make_node(1, &[&rec], 7, node_kind::LEAF);
        let n = BTreeNode::parse(5, data).unwrap();
        assert_eq!(n.f_link, 7);
        assert_eq!(n.kind, node_kind::LEAF);
        assert_eq!(n.num_records, 1);
        assert_eq!(n.record_offset(0).unwrap(), 14);
        let slice = n.record_slice(0).unwrap();
        assert_eq!(&slice[..8], &[0xAB; 8]);
    }

    #[test]
    fn record_out_of_range_is_rejected() {
        let data = make_node(1, &[&[0xAB; 40]], 0, node_kind::LEAF);
        let n = BTreeNode::parse(0, data).unwrap();
        assert!(n.record_offset(1).is_err());
        assert!(n.record_slice(5).is_err());
    }

    #[test]
    fn geometry_from_header_node() {
        // Build a header node: kind=HEADER, then the BTreeHeaderRecord
        // fields at 14+.
        let mut d = vec![0u8; 512];
        d[8] = node_kind::HEADER;
        d[10..12].copy_from_slice(&0u16.to_be_bytes()); // header has no user records
        // treeDepth@14=3, rootNode@16=5, leafRecords@20=100,
        // firstLeafNode@24=6, lastLeafNode@28=9, nodeSize@32=512,
        // totalNodes@34=16, freeNodes@38=2
        d[14..16].copy_from_slice(&3u16.to_be_bytes());
        d[16..20].copy_from_slice(&5u32.to_be_bytes());
        d[20..24].copy_from_slice(&100u32.to_be_bytes());
        d[24..28].copy_from_slice(&6u32.to_be_bytes());
        d[28..32].copy_from_slice(&9u32.to_be_bytes());
        d[32..34].copy_from_slice(&512u16.to_be_bytes());
        d[34..38].copy_from_slice(&16u32.to_be_bytes());
        d[38..42].copy_from_slice(&2u32.to_be_bytes());
        let n = BTreeNode::parse(0, d).unwrap();
        let g = parse_geometry(&n).unwrap();
        assert_eq!(g.node_size, 512);
        assert_eq!(g.total_nodes, 16);
        assert_eq!(g.free_nodes, 2);
        assert_eq!(g.first_leaf_node, 6);
        assert_eq!(g.last_leaf_node, 9);
        assert_eq!(g.leaf_records, 100);
        assert_eq!(g.root_node, 5);
    }

    #[test]
    fn invalid_node_size_rejected() {
        let mut d = vec![0u8; 512];
        d[8] = node_kind::HEADER;
        d[32..34].copy_from_slice(&300u16.to_be_bytes()); // not a power of two
        let n = BTreeNode::parse(0, d).unwrap();
        assert!(parse_geometry(&n).is_err());
    }
}
