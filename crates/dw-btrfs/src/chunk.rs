//! Logical->physical mapping: the chunk tree plus the superblock's
//! bootstrap `sys_chunk_array`.
//!
//! v1 scope, stated honestly in the plugin: single-device volumes with
//! non-RAID profiles only (any RAID bit or num_stripes > 1 makes the
//! probe return Unsupported). Multi-device and RAID need the stripe
//! geometry to pick the right mirror/stripe, which is future work.

use crate::btree;
use crate::error::{BtrfsError, BtrfsResult};
use crate::superblock::SuperBlock;

/// `BTRFS_FIRST_CHUNK_TREE_OBJECTID`.
pub const FIRST_CHUNK_OBJECTID: u64 = 256;
/// Key type `BTRFS_CHUNK_ITEM_KEY`.
pub const CHUNK_ITEM_KEY: u8 = 228;
/// RAID/profile bits we refuse (kernel `BTRFS_BLOCK_GROUP_RAID*`).
pub const RAID_MASK: u64 =
    (1 << 3) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Logical start of this chunk (the key's offset).
    pub logical: u64,
    /// `btrfs_chunk.length`.
    pub length: u64,
    /// `btrfs_chunk.type` (profile bits).
    pub profile: u64,
    /// Physical start on this device (stripe[0].offset for
    /// single-stripe chunks).
    pub physical: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ChunkMap {
    chunks: Vec<Chunk>,
}

impl ChunkMap {
    pub fn from_array_only(sys: &[u8], sb: &SuperBlock) -> BtrfsResult<Self> {
        let mut chunks = Vec::new();
        let mut off = 0usize;
        while off + 17 + 80 <= sys.len() {
            let objectid = u64le(sys, off);
            let key_type = sys[off + 8];
            let logical = u64le(sys, off + 9);
            if objectid != FIRST_CHUNK_OBJECTID || key_type != CHUNK_ITEM_KEY {
                return Err(BtrfsError::Corrupt(format!(
                    "sys array entry at {off}: not a chunk item"
                )));
            }
            let mut c = parse_chunk(&sys[off + 17..off + 17 + 80])?;
            c.logical = logical; // the key offset IS the logical start
            if c.physical >= sb.total_bytes {
                return Err(BtrfsError::Corrupt(
                    "sys chunk physical beyond volume".into(),
                ));
            }
            chunks.push(c);
            off += 17 + 80;
        }
        Ok(Self { chunks })
    }

    /// Full map: sys array for bootstrap, then the chunk tree walk adds
    /// every chunk (deduplicated by logical start).
    pub fn build(read: &dyn dw_disk::ReadAt, sb: &SuperBlock) -> BtrfsResult<Self> {
        let mut map = Self::from_array_only(&sb.sys_chunk_array, sb)?;
        if map.chunks.is_empty() {
            return Err(BtrfsError::Corrupt(
                "no bootstrap chunk in sys array".into(),
            ));
        }
        // Walk the chunk tree through the bootstrap map (its nodes are in
        // SYSTEM/METADATA chunks by definition).
        let (_, items) = btree::walk_tree(read, sb, &map, sb.chunk_root)?;
        for it in items {
            if it.key_type != CHUNK_ITEM_KEY || it.objectid != FIRST_CHUNK_OBJECTID {
                continue;
            }
            if it.data.len() < 80 {
                continue;
            }
            let mut c = parse_chunk(&it.data[..80])?;
            c.logical = it.offset; // key offset = logical start
            if !map.chunks.iter().any(|e| e.logical == c.logical) && c.physical < sb.total_bytes {
                map.chunks.push(c);
            }
        }
        map.chunks.sort_unstable_by_key(|c| c.logical);
        Ok(map)
    }

    /// Map a logical byte to its physical byte on this device.
    #[must_use]
    pub fn map(&self, logical: u64) -> Option<u64> {
        for c in &self.chunks {
            if logical >= c.logical && logical < c.logical + c.length {
                let into = logical - c.logical;
                return Some(c.physical + into);
            }
        }
        None
    }

    /// All chunks (for tests and for the deep metadata scan).
    #[must_use]
    pub fn all(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Chunks covering metadata/system profiles (where b-tree nodes live).
    #[must_use]
    pub fn metadata_chunks(&self) -> Vec<&Chunk> {
        self.chunks
            .iter()
            .filter(|c| c.profile & 0b110 != 0) // SYSTEM(2) | METADATA(4)
            .collect()
    }
}

/// Parse one `btrfs_chunk` (80 bytes: header + one stripe).
fn parse_chunk(raw: &[u8]) -> BtrfsResult<Chunk> {
    let length = u64le(raw, 0x00);
    let profile = u64le(raw, 0x18);
    let num_stripes = u16le(raw, 0x2C);
    let sub_stripes = u16le(raw, 0x2E);
    if length == 0 || length > (1 << 44) {
        return Err(BtrfsError::Corrupt(format!(
            "chunk length {length} invalid"
        )));
    }
    if profile & RAID_MASK != 0 {
        return Err(BtrfsError::Unsupported(format!(
            "RAID profile {profile:#x} needs stripe geometry (v1 is single-stripe only)"
        )));
    }
    if num_stripes != 1 || sub_stripes != 0 {
        return Err(BtrfsError::Unsupported(format!(
            "num_stripes {num_stripes}/sub_stripes {sub_stripes}: single-stripe only in v1"
        )));
    }
    let physical = u64le(raw, 0x38); // stripe.offset (stripe at 0x30, offset at +8)
    Ok(Chunk {
        logical: 0, // caller sets from the key
        length,
        profile,
        physical,
    })
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
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

#[cfg(test)]
mod tests {
    use super::*;

    /// sys-array entry bytes: disk_key(17) + chunk(80).
    fn chunk_entry(logical: u64, length: u64, physical: u64, profile: u64) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&FIRST_CHUNK_OBJECTID.to_le_bytes());
        e.push(CHUNK_ITEM_KEY);
        e.extend_from_slice(&logical.to_le_bytes());
        let mut c = [0u8; 80];
        c[0x00..0x08].copy_from_slice(&length.to_le_bytes());
        c[0x18..0x20].copy_from_slice(&profile.to_le_bytes());
        c[0x2C..0x2E].copy_from_slice(&1u16.to_le_bytes()); // num_stripes 1
        c[0x2E..0x30].copy_from_slice(&0u16.to_le_bytes());
        c[0x38..0x40].copy_from_slice(&physical.to_le_bytes()); // stripe.offset
        e.extend_from_slice(&c);
        e
    }

    fn sb_for(total: u64) -> SuperBlock {
        SuperBlock {
            bytenr: 65_536,
            generation: 1,
            root: 0x2000,
            chunk_root: 0x1000,
            total_bytes: total,
            bytes_used: 0,
            num_devices: 1,
            sectorsize: 4096,
            nodesize: 4096,
            csum_type: 0,
            root_level: 0,
            chunk_root_level: 0,
            incompat_flags: 0,
            fsid: [0; 16],
            label: String::new(),
            sys_chunk_array: Vec::new(),
            sys_chunk_array_size: 0,
        }
    }

    #[test]
    fn array_only_map_maps_identity_and_offset() {
        let mut sys = Vec::new();
        sys.extend_from_slice(&chunk_entry(0, 1 << 20, 0, 0x6)); // SYSTEM|METADATA
        sys.extend_from_slice(&chunk_entry(1 << 20, 1 << 20, 0x1_80000, 0x1)); // DATA
        let sb = sb_for(4 << 20);
        let map = ChunkMap::from_array_only(&sys, &sb).unwrap();
        assert_eq!(map.map(0x1000), Some(0x1000));
        assert_eq!(map.map(0x1_00000 + 0x1000), Some(0x1_80000 + 0x1000));
        assert_eq!(map.map(0x2_10000), None); // outside both chunks
    }

    #[test]
    fn raid_profile_rejected() {
        let mut sys = Vec::new();
        sys.extend_from_slice(&chunk_entry(0, 1 << 20, 0, 0x8)); // RAID0 bit
        let sb = sb_for(4 << 20);
        assert!(matches!(
            ChunkMap::from_array_only(&sys, &sb),
            Err(BtrfsError::Unsupported(_))
        ));
    }

    #[test]
    fn multi_stripe_rejected() {
        let mut e = chunk_entry(0, 1 << 20, 0, 0x2);
        e[17 + 0x2C] = 2; // num_stripes = 2
        let mut sys = Vec::new();
        sys.extend_from_slice(&e);
        let sb = sb_for(4 << 20);
        assert!(matches!(
            ChunkMap::from_array_only(&sys, &sb),
            Err(BtrfsError::Unsupported(_))
        ));
    }

    #[test]
    fn metadata_chunks_filter() {
        let mut sys = Vec::new();
        sys.extend_from_slice(&chunk_entry(0, 1 << 20, 0, 0x6));
        sys.extend_from_slice(&chunk_entry(1 << 20, 1 << 20, 0x1_80000, 0x1));
        let sb = sb_for(4 << 20);
        let map = ChunkMap::from_array_only(&sys, &sb).unwrap();
        assert_eq!(map.metadata_chunks().len(), 1);
        assert_eq!(map.metadata_chunks()[0].profile, 0x6);
    }
}
