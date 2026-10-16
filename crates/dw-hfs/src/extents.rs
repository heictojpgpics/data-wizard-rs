//! HFS+ extents-overflow B-tree (CNID 3, Apple TN 1150 §"Extents
//! Overflow File").
//!
//! Forks larger than the 8 inline extents continue here: each record
//! maps `(forkType, forkID, startBlock)` to the next 8 extents of that
//! fork. Two recovery uses:
//!
//! 1. **Live-file completion**: a catalog file whose data fork's
//!    logical size exceeds its inline extent coverage is completed by
//!    appending the overflow extents in fork order — the file goes
//!    from `PartiallyRecovered` to `Intact` with full content
//!    (wired in `walker.rs` via `fork_extents_with_overflow`).
//! 2. **Orphaned fragments (deep pass)**: overflow records whose
//!    forkID has no catalog record anymore are fragments of deleted
//!    files. The catalog record (and its name) is gone, but the
//!    indexed extents still point at the content — emitted as
//!    `cnid_<id>` deleted entries with real extents (the plugin's
//!    deep pass).
//!
//! Record layout (TN1150, big-endian):
//! ```text
//! key:   keyLength u16 (= 12, counts bytes after this field)
//!        forkType   u32 (0 = data fork, 0xFFFFFFFF = resource fork)
//!        forkID     u32 (CNID of the file)
//!        startBlock u32 (fork block this record's extents start at)
//! data:  8 x { startBlock u32, blockCount u32 }   (ExtentRecord, 64 B)
//! ```

use std::collections::HashMap;

use crate::btree::{BTreeGeometry, node_kind, read_node_at};
use crate::error::{HfsError, HfsResult};
use crate::volumeheader::{ForkExtent, VolumeHeader};

/// Fork type: data fork.
pub const FORK_TYPE_DATA: u32 = 0;
/// Fork type: resource fork (TN1150: 0xFFFFFFFF).
pub const FORK_TYPE_RESOURCE: u32 = 0xFFFF_FFFF;

/// One parsed extents-overflow leaf record.
#[derive(Debug, Clone)]
pub struct OverflowRecord {
    pub fork_type: u32,
    pub fork_id: u32,
    /// Fork block the first extent of this record starts at.
    pub start_block: u32,
    /// Up to 8 extents (a trailing zero-count extent ends the run).
    pub extents: Vec<ForkExtent>,
}

/// Hard cap on nodes per walk (guards against corrupt chains).
const MAX_NODES: u32 = 100_000;

/// Read the overflow B-tree geometry (node 0 of the extents fork).
pub fn read_overflow_geometry(
    read_at: &dyn dw_disk::ReadAt,
    vh: &VolumeHeader,
) -> HfsResult<BTreeGeometry> {
    let base = vh
        .fork_offset_to_volume(&vh.extents_file, 0)
        .ok_or_else(|| HfsError::Corrupt("extents node 0 beyond inline coverage".into()))?;
    let mut win = [0u8; 64];
    read_at.read_at(base, &mut win).map_err(HfsError::Disk)?;
    let node_size = u16::from_be_bytes([win[32], win[33]]);
    if node_size == 0 || !node_size.is_power_of_two() {
        return Err(HfsError::Corrupt(format!(
            "extents node size {node_size} invalid"
        )));
    }
    let node = read_node_at(read_at, vh, &vh.extents_file, 0, node_size)?;
    crate::btree::parse_geometry(&node)
}

/// Parse one leaf record (key + extent data) from `data`.
///
/// `data` is the record slice: `keyLength u16 | key body | 64-byte
/// ExtentRecord`. Layout per TN1150; the key length field counts the
/// bytes after itself.
pub fn parse_overflow_record(data: &[u8]) -> HfsResult<OverflowRecord> {
    if data.len() < 2 + 12 {
        return Err(HfsError::Corrupt("overflow record too short".into()));
    }
    let key_len = u16::from_be_bytes([data[0], data[1]]) as usize;
    if key_len != 12 {
        // Threads and older formats pad keys; 12 is the fixed extent
        // key body (forkType + forkID + startBlock).
        return Err(HfsError::Corrupt(format!(
            "overflow key length {key_len} != 12"
        )));
    }
    if data.len() < 2 + key_len + 64 {
        return Err(HfsError::Corrupt("overflow record data truncated".into()));
    }
    let k = &data[2..2 + key_len];
    let fork_type = u32::from_be_bytes([k[0], k[1], k[2], k[3]]);
    let fork_id = u32::from_be_bytes([k[4], k[5], k[6], k[7]]);
    let start_block = u32::from_be_bytes([k[8], k[9], k[10], k[11]]);
    let body = &data[2 + key_len..2 + key_len + 64];
    let mut extents = Vec::with_capacity(8);
    for i in 0..8 {
        let o = i * 8;
        let start = u32::from_be_bytes([body[o], body[o + 1], body[o + 2], body[o + 3]]);
        let count = u32::from_be_bytes([body[o + 4], body[o + 5], body[o + 6], body[o + 7]]);
        if count == 0 {
            break; // trailing descriptors are zero
        }
        extents.push(ForkExtent {
            start_block: start,
            block_count: count,
        });
    }
    if extents.is_empty() {
        return Err(HfsError::Corrupt("overflow record has no extents".into()));
    }
    Ok(OverflowRecord {
        fork_type,
        fork_id,
        start_block,
        extents,
    })
}

/// Walk the extents-overflow leaf chain and return every record.
///
/// Records are returned in chain order; use [`overflow_map`] to
/// assemble per-fork extent lists.
pub fn walk_overflow(
    read_at: &dyn dw_disk::ReadAt,
    vh: &VolumeHeader,
) -> HfsResult<Vec<OverflowRecord>> {
    let geometry = read_overflow_geometry(read_at, vh)?;
    let mut out = Vec::new();
    let mut visited: HashMap<u32, ()> = HashMap::new();
    let mut current = geometry.first_leaf_node;
    let mut nodes: u32 = 0;
    while current != 0 && nodes < MAX_NODES {
        if visited.insert(current, ()).is_some() {
            break; // cycle guard
        }
        let node = match read_node_at(read_at, vh, &vh.extents_file, current, geometry.node_size) {
            Ok(n) => n,
            Err(_) => break,
        };
        if node.kind != node_kind::LEAF && node.kind != node_kind::INDEX {
            break;
        }
        for i in 0..node.num_records {
            let Ok(slice) = node.record_slice(i) else {
                continue;
            };
            match parse_overflow_record(slice) {
                Ok(r) => out.push(r),
                Err(_) => continue, // foreign/malformed record in the node
            }
        }
        if node.kind != node_kind::LEAF {
            break;
        }
        current = node.f_link;
        nodes += 1;
    }
    Ok(out)
}

/// Assemble `fork_id -> extents` (data forks only) with records sorted
/// by `start_block`, descriptors concatenated in fork order.
pub fn overflow_map(records: &[OverflowRecord]) -> HashMap<u32, Vec<ForkExtent>> {
    let mut by_fork: HashMap<u32, Vec<&OverflowRecord>> = HashMap::new();
    for r in records {
        if r.fork_type != FORK_TYPE_DATA {
            continue;
        }
        by_fork.entry(r.fork_id).or_default().push(r);
    }
    let mut out = HashMap::with_capacity(by_fork.len());
    for (fork, mut recs) in by_fork {
        recs.sort_by_key(|r| r.start_block);
        let mut extents = Vec::new();
        for r in recs {
            extents.extend(r.extents.iter().copied());
        }
        out.insert(fork, extents);
    }
    out
}

/// Merge a fork's inline extents with its overflow extents, truncating
/// to `logical_size` bytes of coverage (the caller's size field is the
/// authority on total length).
pub fn fork_extents_with_overflow(
    inline: &[ForkExtent],
    overflow: Option<&Vec<ForkExtent>>,
    block_size: u32,
    logical_size: u64,
) -> Vec<ForkExtent> {
    let mut merged: Vec<ForkExtent> = inline.to_vec();
    if let Some(ov) = overflow {
        merged.extend(ov.iter().copied());
    }
    // Truncate to logical size.
    let bs = u64::from(block_size);
    let mut covered: u64 = 0;
    let mut out = Vec::with_capacity(merged.len());
    for e in merged {
        if covered >= logical_size {
            break;
        }
        let span = u64::from(e.block_count) * bs;
        let need = logical_size - covered;
        if span <= need {
            out.push(e);
            covered += span;
        } else {
            let blocks = u32::try_from(need.div_ceil(bs)).unwrap_or(e.block_count);
            out.push(ForkExtent {
                start_block: e.start_block,
                block_count: blocks,
            });
            covered += u64::from(blocks) * bs;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn record_bytes(fork_id: u32, start_block: u32, extents: &[(u32, u32)]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&12u16.to_be_bytes());
        v.extend_from_slice(&FORK_TYPE_DATA.to_be_bytes());
        v.extend_from_slice(&fork_id.to_be_bytes());
        v.extend_from_slice(&start_block.to_be_bytes());
        for i in 0..8 {
            if let Some((s, c)) = extents.get(i) {
                v.extend_from_slice(&s.to_be_bytes());
                v.extend_from_slice(&c.to_be_bytes());
            } else {
                v.extend_from_slice(&0u32.to_be_bytes());
                v.extend_from_slice(&0u32.to_be_bytes());
            }
        }
        v
    }

    #[test]
    fn parses_a_record() {
        let b = record_bytes(42, 8, &[(100, 4), (200, 2)]);
        let r = parse_overflow_record(&b).unwrap();
        assert_eq!(r.fork_id, 42);
        assert_eq!(r.start_block, 8);
        assert_eq!(r.extents.len(), 2);
        assert_eq!(r.extents[0].start_block, 100);
        assert_eq!(r.extents[0].block_count, 4);
        assert_eq!(r.extents[1].start_block, 200);
    }

    #[test]
    fn rejects_short_and_foreign_records() {
        assert!(parse_overflow_record(&[0u8; 8]).is_err());
        // keyLength != 12
        let mut b = record_bytes(1, 0, &[(5, 1)]);
        b[0..2].copy_from_slice(&14u16.to_be_bytes());
        assert!(parse_overflow_record(&b).is_err());
        // no extents at all
        let b2 = record_bytes(1, 0, &[]);
        assert!(parse_overflow_record(&b2).is_err());
    }

    #[test]
    fn overflow_map_orders_and_merges() {
        let recs = vec![
            OverflowRecord {
                fork_type: FORK_TYPE_DATA,
                fork_id: 7,
                start_block: 16,
                extents: vec![ForkExtent {
                    start_block: 300,
                    block_count: 2,
                }],
            },
            OverflowRecord {
                fork_type: FORK_TYPE_DATA,
                fork_id: 7,
                start_block: 8,
                extents: vec![ForkExtent {
                    start_block: 200,
                    block_count: 2,
                }],
            },
            OverflowRecord {
                fork_type: FORK_TYPE_RESOURCE, // must be excluded
                fork_id: 9,
                start_block: 0,
                extents: vec![ForkExtent {
                    start_block: 900,
                    block_count: 1,
                }],
            },
        ];
        let map = overflow_map(&recs);
        assert_eq!(map.len(), 1);
        let e7 = &map[&7];
        assert_eq!(e7.len(), 2);
        assert_eq!(e7[0].start_block, 200); // start_block 8 record first
        assert_eq!(e7[1].start_block, 300);
    }

    /// End-to-end: a synthetic volume image (header + extents fork
    /// with header node + leaf chain) walked by `walk_overflow`.
    #[test]
    fn walks_a_synthetic_overflow_btree() {
        const BS: u32 = 4096;
        // Layout: VH at 1024; catalog fork unused by this test (one
        // inline extent far away, zero size); extents fork at volume
        // block 10 (header node) and 11 (leaf node).
        let mut img = vec![0u8; 64 * 1024];

        fn fork_bytes(logical: u64, blocks: u32, extents: &[(u32, u32)]) -> [u8; 80] {
            let mut b = [0u8; 80];
            b[0..8].copy_from_slice(&logical.to_be_bytes());
            b[8..12].copy_from_slice(&4096u32.to_be_bytes());
            b[12..16].copy_from_slice(&blocks.to_be_bytes());
            for (i, (s, c)) in extents.iter().enumerate().take(8) {
                b[16 + i * 8..20 + i * 8].copy_from_slice(&s.to_be_bytes());
                b[20 + i * 8..24 + i * 8].copy_from_slice(&c.to_be_bytes());
            }
            b
        }

        // Volume header at 1024.
        let vh_at = 1024usize;
        img[vh_at..vh_at + 4].copy_from_slice(&[0x48, 0x2B, 0x00, 0x04]);
        img[vh_at + 40..vh_at + 44].copy_from_slice(&BS.to_be_bytes());
        img[vh_at + 44..vh_at + 48].copy_from_slice(&16u32.to_be_bytes());
        // catalog fork @272: zero size, one dummy extent.
        img[vh_at + 272..vh_at + 352].copy_from_slice(&fork_bytes(0, 0, &[(14, 1)]));
        // extents fork @192: logical 2 nodes (512-byte nodes), extents
        // {10, 1} and {11, 1}.
        img[vh_at + 192..vh_at + 272].copy_from_slice(&fork_bytes(1024, 2, &[(10, 1), (11, 1)]));

        // Extents B-tree header node at volume block 10 (node 0):
        // descriptor kind=HEADER(0x01), BTreeHeaderRecord with
        // firstLeafNode=1, nodeSize=512.
        let n0 = (10 * BS) as usize;
        img[n0 + 8] = 0x01; // HEADER
        img[n0 + 24..n0 + 28].copy_from_slice(&1u32.to_be_bytes()); // firstLeaf
        img[n0 + 32..n0 + 34].copy_from_slice(&512u16.to_be_bytes()); // nodeSize
        img[n0 + 34..n0 + 38].copy_from_slice(&2u32.to_be_bytes()); // totalNodes

        // Leaf node (node 1) at fork byte 512 — still inside fork
        // BLOCK 0 (blocks are 4096 B, nodes 512 B): volume block 10,
        // byte 512. kind=LEAF(0xFF), 2 records, fLink=0 (last).
        let n1 = (10 * BS) as usize + 512;
        img[n1 + 8] = 0xFF; // LEAF
        img[n1 + 10..n1 + 12].copy_from_slice(&2u16.to_be_bytes());
        let r0 = record_bytes(20, 8, &[(100, 2)]); // live file id 20
        let r1 = record_bytes(21, 8, &[(300, 3)]); // orphan id 21
        img[n1 + 14..n1 + 14 + r0.len()].copy_from_slice(&r0);
        img[n1 + 14 + r0.len()..n1 + 14 + r0.len() + r1.len()].copy_from_slice(&r1);
        // Record offset entries (from node end).
        let node_end = n1 + 512;
        img[node_end - 2..node_end].copy_from_slice(&14u16.to_be_bytes());
        img[node_end - 4..node_end - 2].copy_from_slice(&((14 + r0.len()) as u16).to_be_bytes());

        // Write to a file source and walk.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v.img");
        std::fs::write(&p, &img).unwrap();
        let src = dw_disk::FileBackedSource::open(&p).unwrap();
        let read: std::sync::Arc<dyn dw_disk::ReadAt> = std::sync::Arc::new(src);
        let vh = crate::walker::read_volume_header(read.as_ref()).unwrap();

        let records = walk_overflow(read.as_ref(), &vh).unwrap();
        assert_eq!(records.len(), 2);
        let map = overflow_map(&records);
        assert_eq!(map.len(), 2);
        assert_eq!(map[&20][0].start_block, 100);
        assert_eq!(map[&21][0].block_count, 3);

        // Fork completion: inline 1 block + overflow 2 blocks, logical
        // 3 blocks -> full coverage.
        let merged = fork_extents_with_overflow(
            &[ForkExtent {
                start_block: 9,
                block_count: 1,
            }],
            map.get(&20),
            BS,
            3 * 4096,
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged.iter().map(|e| e.block_count).sum::<u32>(),
            3,
            "full logical coverage"
        );
    }

    #[test]
    fn merge_truncates_to_logical_size() {
        let inline = [ForkExtent {
            start_block: 10,
            block_count: 2,
        }];
        let overflow = vec![ForkExtent {
            start_block: 50,
            block_count: 4,
        }];
        // logical size 3 blocks: 2 inline + 1 of the overflow record.
        let merged = fork_extents_with_overflow(&inline, Some(&overflow), 4096, 3 * 4096);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].block_count, 1);
        // logical size below inline coverage: truncated inline.
        let merged2 = fork_extents_with_overflow(&inline, Some(&overflow), 4096, 4096);
        assert_eq!(merged2.len(), 1);
        assert_eq!(merged2[0].block_count, 1);
    }
}
