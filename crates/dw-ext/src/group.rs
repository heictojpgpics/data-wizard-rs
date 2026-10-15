//! Block group descriptors (32/64 bytes) and the two bitmaps each group
//! carries (block bitmap, inode bitmap).
//!
//! Layout from `fs/ext4/ext4.h` `struct ext4_group_desc`. With the 64BIT
//! feature the high words follow at +0x20..+0x40 and `s_desc_size >= 64`.

use crate::error::{ExtError, ExtResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupDesc {
    pub block_bitmap_block: u64,
    pub inode_bitmap_block: u64,
    pub inode_table_block: u64,
    pub free_blocks: u32,
    pub free_inodes: u32,
    pub used_dirs: u32,
}

impl GroupDesc {
    /// Parse one descriptor. `desc64` must be true when the 64BIT feature
    /// is active (caller enforces `s_desc_size >= 64` at superblock parse).
    pub fn parse(raw: &[u8], desc64: bool) -> ExtResult<Self> {
        if raw.len() < 32 {
            return Err(ExtError::Corrupt("truncated group descriptor".into()));
        }
        let lo =
            |o: usize| -> u32 { u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]) };
        let u16at = |o: usize| -> u16 { u16::from_le_bytes([raw[o], raw[o + 1]]) };

        let mut block_bitmap = u64::from(lo(0x00));
        let mut inode_bitmap = u64::from(lo(0x04));
        let mut inode_table = u64::from(lo(0x08));
        let free_blocks = u32::from(u16at(0x0C));
        let free_inodes = u32::from(u16at(0x0E));
        let used_dirs = u32::from(u16at(0x10));

        if desc64 && raw.len() >= 64 {
            block_bitmap |= u64::from(lo(0x20)) << 32;
            inode_bitmap |= u64::from(lo(0x24)) << 32;
            inode_table |= u64::from(lo(0x28)) << 32;
        }
        Ok(Self {
            block_bitmap_block: block_bitmap,
            inode_bitmap_block: inode_bitmap,
            inode_table_block: inode_table,
            free_blocks,
            free_inodes,
            used_dirs,
        })
    }
}

/// A parsed block/inode bitmap (one FS block worth of bits, LSB-first per
/// byte: bit `n` set means block/inode `group_base + n` is IN USE).
#[derive(Debug, Clone)]
pub struct Bitmap {
    data: Vec<u8>,
    valid_bits: usize,
}

impl Bitmap {
    /// `raw` is the bitmap block as read from disk; `count` is the number
    /// of bits that are meaningful (blocks_in_group / inodes_per_group).
    pub fn new(raw: Vec<u8>, count: usize) -> ExtResult<Self> {
        let need = count.div_ceil(8);
        if raw.len() < need {
            return Err(ExtError::Corrupt(format!(
                "bitmap block too small: {need} bytes needed, {} present",
                raw.len()
            )));
        }
        Ok(Self {
            data: raw,
            valid_bits: count,
        })
    }

    /// Bit test for index `n` within this group (0-based).
    #[must_use]
    pub fn is_used(&self, n: usize) -> bool {
        if n >= self.valid_bits {
            return false;
        }
        self.data[n / 8] & (1 << (n % 8)) != 0
    }

    /// Iterate the runs of FREE bits as `(start_bit, run_len)`; runs are
    /// clamped to `valid_bits`. Used for deep-scan carving targets and
    /// (inverted) for sanity-checking superblock free counts.
    #[must_use]
    pub fn free_runs(&self) -> Vec<(u64, u64)> {
        let mut runs = Vec::new();
        let mut i = 0usize;
        while i < self.valid_bits {
            if self.is_used(i) {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.valid_bits && !self.is_used(i) {
                i += 1;
            }
            runs.push((start as u64, (i - start) as u64));
        }
        runs
    }

    /// Count of used bits (for cross-checks).
    #[must_use]
    pub fn used_count(&self) -> u64 {
        let mut n = 0u64;
        for i in 0..self.valid_bits {
            if self.is_used(i) {
                n += 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc_bytes(bb: u32, ib: u32, it: u32, fb: u16, fi: u16) -> [u8; 64] {
        let mut b = [0u8; 64];
        b[0x00..0x04].copy_from_slice(&bb.to_le_bytes());
        b[0x04..0x08].copy_from_slice(&ib.to_le_bytes());
        b[0x08..0x0C].copy_from_slice(&it.to_le_bytes());
        b[0x0C..0x0E].copy_from_slice(&fb.to_le_bytes());
        b[0x0E..0x10].copy_from_slice(&fi.to_le_bytes());
        b[0x10..0x12].copy_from_slice(&2u16.to_le_bytes()); // used dirs
        b
    }

    #[test]
    fn parses_32bit_descriptor() {
        let b = desc_bytes(2, 3, 4, 240, 3);
        let d = GroupDesc::parse(&b[..32], false).unwrap();
        assert_eq!(d.block_bitmap_block, 2);
        assert_eq!(d.inode_bitmap_block, 3);
        assert_eq!(d.inode_table_block, 4);
        assert_eq!(d.free_blocks, 240);
        assert_eq!(d.free_inodes, 3);
        assert_eq!(d.used_dirs, 2);
    }

    #[test]
    fn parses_64bit_descriptor_high_words() {
        let mut b = desc_bytes(2, 3, 4, 240, 3);
        // High words: block bitmap at block 0x1_00000002 style offsets.
        b[0x20..0x24].copy_from_slice(&1u32.to_le_bytes()); // <<32
        b[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes());
        let d = GroupDesc::parse(&b, true).unwrap();
        assert_eq!(d.block_bitmap_block, (1u64 << 32) | 2);
        assert_eq!(d.inode_table_block, (1u64 << 32) | 4);
    }

    #[test]
    fn truncated_descriptor_rejected() {
        let b = desc_bytes(2, 3, 4, 240, 3);
        assert!(GroupDesc::parse(&b[..16], false).is_err());
    }

    #[test]
    fn bitmap_runs_and_counts() {
        // intent: bits 0,1 used; 2..6 free; 6 used; 7,8 free (9 bits)
        // byte0 = 0b0100_0011 (1,2,64) ; byte1 bit0 (global bit 8) = 0
        let raw = vec![0b0100_0011u8, 0b0000_0000u8];
        let bm = Bitmap::new(raw, 9).unwrap();
        assert_eq!(bm.used_count(), 3); // bits 0,1,6
        let runs = bm.free_runs();
        assert_eq!(runs, vec![(2, 4), (7, 2)]);
    }

    #[test]
    fn bitmap_rejects_short_block() {
        assert!(Bitmap::new(vec![0u8; 1], 32).is_err());
    }

    #[test]
    fn all_used_has_no_runs() {
        let raw = vec![0xFFu8; 4];
        let bm = Bitmap::new(raw, 32).unwrap();
        assert!(bm.free_runs().is_empty());
        assert_eq!(bm.used_count(), 32);
    }
}
