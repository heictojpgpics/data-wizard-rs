//! HFS+ volume header (Apple TN 1150 "HFS Plus Volume Format", public).
//!
//! The volume header sits at offset 1024 within the volume and is
//! BIG-endian. Layout (TN1150 Table 1, cross-validated against the Linux
//! kernel `hfsplus` driver `hfsplus_vh`):
//!
//! ```text
//!   0   signature u16 ('H+' = 0x482B, 'HX' = 0x4858 HFSX)
//!   2   version u16
//!   4   attributes u32
//!   8   lastMountedVersion u32
//!  12   journalInfoBlock u32
//!  16   createDate u32        (seconds since 1904-01-01, local)
//!  20   modifyDate u32
//!  24   backupDate u32
//!  28   checkedDate u32
//!  32   fileCount u32
//!  36   folderCount u32
//!  40   blockSize u32
//!  44   totalBlocks u32
//!  48   freeBlocks u32
//!  52   nextAllocation u32
//!  56   rsrcClumpSize u32
//!  60   dataClumpSize u32
//!  64   nextCatalogID u32
//!  68   writeCount u32
//!  72   encodingsBitmap u64
//!  80   finderInfo 8 x u32
//! 112   allocationFile  ForkData (80 bytes)
//! 192   extentsFile     ForkData
//! 272   catalogFile     ForkData
//! 352   attributesFile  ForkData
//! 432   startupFile     ForkData
//! 512   (end of the header area)
//! ```
//!
//! ForkData (80 bytes, `hfsplus_fork` in the Linux driver):
//! ```text
//!   0   logicalSize u64
//!   8   clumpSize u32
//!  12   totalBlocks u32
//!  16   8 extents: { startBlock u32, blockCount u32 }  (inline; further
//!       extents live in the extents-overflow B-tree, not parsed in v1)
//! ```

use bytemuck::{Pod, Zeroable};

use crate::error::{HfsError, HfsResult};

/// Raw 512-byte header area.
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VolumeHeaderRaw {
    pub data: [u8; 512],
}

/// One inline extent of a fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForkExtent {
    pub start_block: u32,
    pub block_count: u32,
}

/// Parsed fork data (allocation/extents/catalog/attributes/startup file).
#[derive(Debug, Clone)]
pub struct ForkData {
    pub logical_size: u64,
    pub clump_size: u32,
    pub total_blocks: u32,
    /// Up to 8 inline extents (TN1150); files larger than the inline
    /// coverage continue in the extents-overflow B-tree (v1 limitation:
    /// not followed).
    pub extents: Vec<ForkExtent>,
}

impl ForkData {
    pub fn parse(buf: &[u8; 80]) -> Self {
        let logical_size = u64::from_be_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let clump_size = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let total_blocks = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let mut extents = Vec::with_capacity(8);
        for i in 0..8 {
            let o = 16 + i * 8;
            let start_block = u32::from_be_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
            let block_count = u32::from_be_bytes([buf[o + 4], buf[o + 5], buf[o + 6], buf[o + 7]]);
            if block_count > 0 {
                extents.push(ForkExtent {
                    start_block,
                    block_count,
                });
            }
        }
        Self {
            logical_size,
            clump_size,
            total_blocks,
            extents,
        }
    }

    /// Total inline coverage in blocks (diagnostic + partial detection).
    pub fn inline_block_coverage(&self) -> u32 {
        self.extents.iter().map(|e| e.block_count).sum()
    }
}

/// Parsed HFS+ volume header.
#[derive(Debug, Clone)]
pub struct VolumeHeader {
    pub signature: u16,
    pub version: u16,
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub file_count: u32,
    pub folder_count: u32,
    pub create_date: u32,
    pub modify_date: u32,
    pub next_catalog_id: u32,
    pub allocation_file: ForkData,
    pub extents_file: ForkData,
    pub catalog_file: ForkData,
    pub attributes_file: ForkData,
    pub startup_file: ForkData,
}

/// CNIDs with special meaning (TN1150).
pub mod cnid {
    pub const ROOT: u32 = 2;
    pub const EXTENTS: u32 = 3;
    pub const CATALOG: u32 = 4;
    pub const ATTRIBUTES: u32 = 5;
    pub const STARTUP: u32 = 6;
    pub const USER_FIRST: u32 = 16;
}

impl VolumeHeader {
    /// Parse the 512-byte header area (volume offset 1024).
    pub fn parse(buf: &[u8; 512]) -> HfsResult<Self> {
        let rd = |o: usize| -> u16 { u16::from_be_bytes([buf[o], buf[o + 1]]) };
        let rd32 =
            |o: usize| -> u32 { u32::from_be_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]) };

        let signature = rd(0);
        if signature != 0x482B && signature != 0x4858 {
            return Err(HfsError::NotHfsPlus);
        }
        let block_size = rd32(40);
        if !block_size.is_power_of_two() || block_size < 512 {
            return Err(HfsError::Corrupt(format!(
                "invalid block size {block_size}"
            )));
        }

        let fork = |o: usize| ForkData::parse(buf[o..o + 80].try_into().expect("80 bytes"));

        Ok(Self {
            signature,
            version: rd(2),
            block_size,
            total_blocks: rd32(44),
            free_blocks: rd32(48),
            file_count: rd32(32),
            folder_count: rd32(36),
            create_date: rd32(16),
            modify_date: rd32(20),
            next_catalog_id: rd32(64),
            allocation_file: fork(112),
            extents_file: fork(192),
            catalog_file: fork(272),
            attributes_file: fork(352),
            startup_file: fork(432),
        })
    }

    /// Map a block offset inside a FORK's logical space to the absolute
    /// byte offset in the volume, using the inline extents.
    ///
    /// Returns None when the fork offset is beyond the inline coverage
    /// (extents-overflow B-tree territory - v1 limitation).
    pub fn fork_offset_to_volume(&self, fork: &ForkData, fork_offset: u64) -> Option<u64> {
        let block_size = u64::from(self.block_size);
        if block_size == 0 {
            return None;
        }
        // Which volume block holds this byte? Extents map CONSECUTIVE
        // fork-block runs to ARBITRARY volume-block runs: fork blocks
        // [acc, acc+count) -> volume blocks [start, start+count). The
        // run offset within the extent is (fork_block - acc) — NOT
        // fork_block itself (extents are rarely physically contiguous).
        let mut remaining = fork_offset;
        let mut acc_blocks: u32 = 0;
        for e in &fork.extents {
            let span = u64::from(e.block_count) * block_size;
            if remaining < span {
                let within = remaining / block_size;
                return Some(
                    u64::from(e.start_block + within as u32) * block_size
                        + (remaining % block_size),
                );
            }
            remaining -= span;
            acc_blocks = acc_blocks.saturating_add(e.block_count);
        }
        None // beyond inline extents
    }

    /// Resolve a fork offset through the inline extents plus overflow
    /// records from the extents-overflow B-tree (TN1150: the first 8
    /// extents live in the fork; the rest are (fork_id, fork_index,
    /// start_block, block_count) records in the overflow file).
    ///
    /// `overflow` yields the overflow extents in fork order after the
    /// inline ones (see [`crate::extents`]).
    pub fn fork_offset_to_volume_with_overflow(
        &self,
        fork: &ForkData,
        fork_offset: u64,
        fork_id: u32,
        overflow: &dyn Fn(u32) -> Vec<ForkExtent>,
    ) -> Option<u64> {
        if let Some(off) = self.fork_offset_to_volume(fork, fork_offset) {
            return Some(off);
        }
        // Beyond inline coverage: continue into the overflow records.
        let block_size = u64::from(self.block_size);
        let mut remaining = fork_offset
            - fork
                .extents
                .iter()
                .map(|e| u64::from(e.block_count) * block_size)
                .sum::<u64>();
        for e in overflow(fork_id) {
            let span = u64::from(e.block_count) * block_size;
            if remaining < span {
                let within = remaining / block_size;
                return Some(
                    u64::from(e.start_block + within as u32) * block_size
                        + (remaining % block_size),
                );
            }
            remaining -= span;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn build_vh() -> [u8; 512] {
        let mut b = [0u8; 512];
        // 'H+' signature
        b[0] = 0x48;
        b[1] = 0x2B;
        b[2] = 0x00;
        b[3] = 0x04; // version 4
        // blockSize @40 = 4096
        b[40..44].copy_from_slice(&4096u32.to_be_bytes());
        // totalBlocks @44
        b[44..48].copy_from_slice(&100u32.to_be_bytes());
        // freeBlocks @48 = 40
        b[48..52].copy_from_slice(&40u32.to_be_bytes());
        // fileCount @32 = 5, folderCount @36 = 3
        b[32..36].copy_from_slice(&5u32.to_be_bytes());
        b[36..40].copy_from_slice(&3u32.to_be_bytes());
        // catalog fork @272: logical 8192, clump 4096, blocks 2,
        // extent {start=10, count=2}
        b[272..280].copy_from_slice(&8192u64.to_be_bytes());
        b[280..284].copy_from_slice(&4096u32.to_be_bytes());
        b[284..288].copy_from_slice(&2u32.to_be_bytes());
        b[288..292].copy_from_slice(&10u32.to_be_bytes());
        b[292..296].copy_from_slice(&2u32.to_be_bytes());
        b
    }

    #[test]
    fn parses_volume_header() {
        let vh = VolumeHeader::parse(&build_vh()).unwrap();
        assert_eq!(vh.signature, 0x482B);
        assert_eq!(vh.block_size, 4096);
        assert_eq!(vh.total_blocks, 100);
        assert_eq!(vh.free_blocks, 40);
        assert_eq!(vh.file_count, 5);
        assert_eq!(vh.folder_count, 3);
        assert_eq!(vh.catalog_file.logical_size, 8192);
        assert_eq!(vh.catalog_file.extents.len(), 1);
        assert_eq!(vh.catalog_file.extents[0].start_block, 10);
        assert_eq!(vh.catalog_file.extents[0].block_count, 2);
    }

    #[test]
    fn rejects_non_hfs_signature() {
        let mut b = build_vh();
        b[0] = 0x12;
        b[1] = 0x34;
        assert!(matches!(VolumeHeader::parse(&b), Err(HfsError::NotHfsPlus)));
    }

    #[test]
    fn rejects_bad_block_size() {
        let mut b = build_vh();
        // 3000: not a power of two
        b[40..44].copy_from_slice(&3000u32.to_be_bytes());
        assert!(VolumeHeader::parse(&b).is_err());
    }

    #[test]
    fn fork_offset_mapping() {
        let vh = VolumeHeader::parse(&build_vh()).unwrap();
        let fork = &vh.catalog_file;
        // extent {start=10, count=2}, blockSize 4096: covers fork bytes 0..8192
        assert_eq!(vh.fork_offset_to_volume(fork, 0), Some(10 * 4096));
        assert_eq!(vh.fork_offset_to_volume(fork, 5000), Some(10 * 4096 + 5000));
        assert_eq!(vh.fork_offset_to_volume(fork, 8191), Some(11 * 4096 + 4095));
        // beyond inline coverage
        assert_eq!(vh.fork_offset_to_volume(fork, 8192), None);
    }

    #[test]
    fn fork_offset_maps_noncontiguous_extents_correctly() {
        // Two inline extents, physically disjoint: fork blocks 0..2 ->
        // volume 100..102, fork blocks 2..5 -> volume 200..203.
        // (Regression guard: an earlier mapping added the accumulated
        // fork-block count to the second extent's start block.)
        let mut b = build_vh();
        // catalog fork @272: logical 5 blocks, extent0 {100,2},
        // extent1 {200,3}
        b[272..280].copy_from_slice(&(5 * 4096u64).to_be_bytes());
        b[280..284].copy_from_slice(&4096u32.to_be_bytes());
        b[284..288].copy_from_slice(&5u32.to_be_bytes());
        b[288..292].copy_from_slice(&100u32.to_be_bytes());
        b[292..296].copy_from_slice(&2u32.to_be_bytes());
        b[296..300].copy_from_slice(&200u32.to_be_bytes());
        b[300..304].copy_from_slice(&3u32.to_be_bytes());
        let vh = VolumeHeader::parse(&b).unwrap();
        let fork = &vh.catalog_file;
        // Fork block 0 -> volume block 100.
        assert_eq!(vh.fork_offset_to_volume(fork, 0), Some(100 * 4096));
        // Fork block 1 (last of extent 0) -> volume 101.
        assert_eq!(vh.fork_offset_to_volume(fork, 4096), Some(101 * 4096));
        // Fork block 2 (first of extent 1) -> volume 200, NOT 202.
        assert_eq!(vh.fork_offset_to_volume(fork, 2 * 4096), Some(200 * 4096));
        // Fork block 4 (last of extent 1) -> volume 202.
        assert_eq!(vh.fork_offset_to_volume(fork, 4 * 4096), Some(202 * 4096));
        // Beyond both extents.
        assert_eq!(vh.fork_offset_to_volume(fork, 5 * 4096), None);
    }
}
