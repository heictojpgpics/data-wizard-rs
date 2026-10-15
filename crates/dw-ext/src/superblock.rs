//! ext2/3/4 superblock (1024 bytes at volume offset 1024).
//!
//! Layout from the Linux kernel `fs/ext4/ext4.h` `struct ext4_super_block`
//! (public on-disk format, unchanged since ext2). Only fields the engine
//! consumes are decoded; unknown bytes are validated for bounds, not
//! interpreted.

use crate::error::ExtResult;
use time::OffsetDateTime;

/// `0xEF53` little-endian, at superblock offset 0x38.
pub const SUPER_MAGIC: u16 = 0xEF53;
/// Superblock lives at this volume offset (inside block 0 for bs > 1024).
pub const SUPER_OFFSET: u64 = 1024;

/// Incompat flags we decode (kernel values).
pub mod incompat {
    pub const FILETYPE: u32 = 0x0002;
    pub const RECOVER: u32 = 0x0004;
    pub const JOURNAL_DEV: u32 = 0x0008;
    pub const META_BG: u32 = 0x0010;
    pub const EXTENTS: u32 = 0x0040;
    pub const BITMAP_64BIT: u32 = 0x0100;
    pub const MMP: u32 = 0x0200;
    pub const FLEX_BG: u32 = 0x0400;
}

/// Compat flags we decode.
pub mod compat {
    pub const HAS_JOURNAL: u32 = 0x0004;
    pub const RESIZE_INODE: u32 = 0x0010;
    pub const INDEX_DIR: u32 = 0x0020;
}

/// RO-compat flags we decode.
pub mod ro_compat {
    pub const SPARSE_SUPER: u32 = 0x0001;
    pub const LARGE_FILE: u32 = 0x0002;
    pub const HUGE_FILE: u32 = 0x0004;
    pub const GDT_CSUM: u32 = 0x0008;
    pub const DIR_NLINK: u32 = 0x0020;
    pub const EXTRA_ISIZE: u32 = 0x0040;
    pub const BIGALLOC: u32 = 0x0200;
    pub const METADATA_CSUM: u32 = 0x0400;
}

/// Bounds guard: refuse absurd geometries before any allocation or I/O
/// plan is derived from them (mirrors the reference product's validation).
const MAX_LOG_BLOCK_SIZE: u32 = 6; // 1024 << 6 = 64 KiB blocks
const MAX_INODE_SIZE: u16 = 4096;
const MIN_INODE_SIZE: u16 = 128;
const MAX_BLOCKS_PER_GROUP: u32 = 8 << 20; // 8 * block_size hard limit is enforced separately
const MAX_INODES_PER_GROUP: u32 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperBlock {
    pub inodes_count: u32,
    pub blocks_count: u64,
    pub free_blocks_count: u64,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub block_size: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub desc_size: u16,
    pub state: u16,
    pub rev_level: u32,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
    pub uuid: [u8; 16],
    pub volume_name: String,
    pub last_mounted: String,
    pub volume_uuid_string: String,
    /// 32-bit seconds fields in the superblock (s_mtime/s_wtime).
    pub mtime: Option<OffsetDateTime>,
    /// ext4-only: block-group-descriptor count implied by geometry.
    pub group_count: u32,
}

impl SuperBlock {
    /// Parse and validate from a 1024-byte buffer read at [`SUPER_OFFSET`].
    pub fn parse(buf: &[u8; 1024]) -> ExtResult<Self> {
        let magic = u16le(buf, 0x38);
        if magic != SUPER_MAGIC {
            return Err(crate::error::ExtError::NotExt);
        }

        let inodes_count = u32le(buf, 0x00);
        let blocks_count_lo = u32le(buf, 0x04);
        let free_blocks_lo = u32le(buf, 0x0C);
        let free_inodes_count = u32le(buf, 0x10);
        let first_data_block = u32le(buf, 0x14);
        let log_block_size = u32le(buf, 0x18);
        let blocks_per_group = u32le(buf, 0x20);
        let inodes_per_group = u32le(buf, 0x28);
        let state = u16le(buf, 0x3A);
        let rev_level = u32le(buf, 0x4C);
        let inode_size = u16le(buf, 0x58);
        let feature_compat = u32le(buf, 0x5C);
        let feature_incompat = u32le(buf, 0x60);
        let feature_ro_compat = u32le(buf, 0x64);
        let desc_size = if rev_level >= 1 { u16le(buf, 0xFE) } else { 0 };

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&buf[0x68..0x78]);
        let volume_name = latin1(&buf[0x78..0x88]);
        let last_mounted = latin1(&buf[0x88..0xC8]);

        let blocks_count_hi = u32le(buf, 0x150) as u64;
        let free_blocks_hi = u32le(buf, 0x158) as u64;
        let blocks_count = u64::from(blocks_count_lo) | (blocks_count_hi << 32);
        let free_blocks_count = u64::from(free_blocks_lo) | (free_blocks_hi << 32);

        // --- geometry sanity (fail closed on nonsense) ---
        if log_block_size > MAX_LOG_BLOCK_SIZE {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_log_block_size {log_block_size} exceeds sane bound"
            )));
        }
        let block_size = 1024u32 << log_block_size;
        if block_size < 1024 {
            return Err(crate::error::ExtError::Corrupt("block size < 1024".into()));
        }
        if !(MIN_INODE_SIZE..=MAX_INODE_SIZE).contains(&inode_size)
            || !(inode_size as u32).is_multiple_of(4)
        {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_inode_size {inode_size} invalid"
            )));
        }
        if inodes_per_group == 0 || inodes_per_group > MAX_INODES_PER_GROUP {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_inodes_per_group {inodes_per_group} invalid"
            )));
        }
        if u64::from(inodes_per_group) > u64::from(block_size) * 8 {
            return Err(crate::error::ExtError::Corrupt(
                "inode bitmap cannot cover s_inodes_per_group".into(),
            ));
        }
        if blocks_per_group == 0
            || blocks_per_group > MAX_BLOCKS_PER_GROUP
            || !blocks_per_group.is_multiple_of(8)
        {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_blocks_per_group {blocks_per_group} invalid"
            )));
        }
        if u64::from(blocks_per_group) > u64::from(block_size) * 8 {
            return Err(crate::error::ExtError::Corrupt(
                "block bitmap cannot cover s_blocks_per_group".into(),
            ));
        }
        if first_data_block != 0 && block_size == 1024 && first_data_block != 1 {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_first_data_block {first_data_block} invalid for 1 KiB blocks"
            )));
        }
        if blocks_count == 0 || inodes_count == 0 {
            return Err(crate::error::ExtError::Corrupt(
                "empty volume counters".into(),
            ));
        }
        if blocks_count < u64::from(first_data_block) + 8 {
            return Err(crate::error::ExtError::Corrupt(
                "volume too small for metadata blocks".into(),
            ));
        }
        // desc_size: 0 -> 32 on rev >= 1; must cover the fields we read.
        let desc_size = if desc_size == 0 { 32 } else { desc_size };
        if desc_size < 32 || desc_size % 8 != 0 || desc_size as u32 > block_size {
            return Err(crate::error::ExtError::Corrupt(format!(
                "s_desc_size {desc_size} invalid"
            )));
        }
        if (feature_incompat & incompat::BITMAP_64BIT) != 0 && desc_size < 64 {
            return Err(crate::error::ExtError::Corrupt(
                "64BIT feature set but s_desc_size < 64".into(),
            ));
        }

        // Group count: ceil(blocks_count / blocks_per_group), bounded.
        let group_count = u32::try_from(
            ((blocks_count - u64::from(first_data_block)) + u64::from(blocks_per_group) - 1)
                / u64::from(blocks_per_group),
        )
        .map_err(|_| crate::error::ExtError::Corrupt("group count overflow".into()))?;
        if group_count == 0 || group_count > 1 << 16 {
            return Err(crate::error::ExtError::Corrupt(format!(
                "group count {group_count} out of range"
            )));
        }
        if u64::from(group_count) * u64::from(inodes_per_group) < u64::from(inodes_count) {
            return Err(crate::error::ExtError::Corrupt(
                "s_inodes_count exceeds per-group capacity".into(),
            ));
        }

        let mtime = epoch(u32le(buf, 0x2C) as i64);

        Ok(Self {
            inodes_count,
            blocks_count,
            free_blocks_count,
            free_inodes_count,
            first_data_block,
            block_size,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            desc_size,
            state,
            rev_level,
            feature_compat,
            feature_incompat,
            feature_ro_compat,
            uuid,
            volume_name,
            last_mounted,
            volume_uuid_string: format_uuid(&uuid),
            mtime,
            group_count,
        })
    }

    /// Volume-relative byte offset of the group-descriptor table.
    #[must_use]
    pub fn gdt_offset(&self) -> u64 {
        // The GDT follows the superblock: on 1 KiB volumes that is block 1,
        // otherwise the start of block 1 (bs>1024) or `first_data_block`+1.
        u64::from(self.first_data_block + 1) * u64::from(self.block_size)
    }

    /// Byte offset of group `g`'s descriptors inside the GDT.
    #[must_use]
    pub fn group_desc_offset(&self, group: u32) -> Option<u64> {
        if group >= self.group_count {
            return None;
        }
        Some(self.gdt_offset() + u64::from(group) * u64::from(self.desc_size))
    }

    /// Global inode number -> (group index, index within group).
    #[must_use]
    pub fn locate_inode(&self, inode_no: u32) -> Option<(u32, u32)> {
        if inode_no == 0 || inode_no > self.inodes_count {
            return None;
        }
        Some((
            (inode_no - 1) / self.inodes_per_group,
            (inode_no - 1) % self.inodes_per_group,
        ))
    }

    /// ext-family classification for `fs_kind`.
    #[must_use]
    pub fn fs_kind(&self) -> &'static str {
        let ext4_marks = incompat::EXTENTS
            | incompat::FLEX_BG
            | incompat::BITMAP_64BIT
            | incompat::META_BG
            | incompat::MMP;
        let ext4_ro = ro_compat::HUGE_FILE
            | ro_compat::GDT_CSUM
            | ro_compat::DIR_NLINK
            | ro_compat::EXTRA_ISIZE
            | ro_compat::BIGALLOC
            | ro_compat::METADATA_CSUM;
        if (self.feature_incompat & ext4_marks) != 0 || (self.feature_ro_compat & ext4_ro) != 0 {
            "ext4"
        } else if (self.feature_compat & compat::HAS_JOURNAL) != 0 {
            "ext3"
        } else {
            "ext2"
        }
    }

    /// Does the volume use 64-bit block/inode numbers in descriptors?
    #[must_use]
    pub fn is_64bit(&self) -> bool {
        (self.feature_incompat & incompat::BITMAP_64BIT) != 0 && self.desc_size >= 64
    }
}

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Decode a NUL-padded latin-1 string field (volume names are latin-1
/// by spec; anything non-printable is dropped).
fn latin1(raw: &[u8]) -> String {
    let s: Vec<u8> = raw.iter().copied().take_while(|&c| c != 0).collect();
    String::from_utf8_lossy(&s).trim().to_string()
}

fn epoch(secs: i64) -> Option<OffsetDateTime> {
    if secs <= 0 {
        return None;
    }
    OffsetDateTime::from_unix_timestamp(secs).ok()
}

fn format_uuid(u: &[u8; 16]) -> String {
    let h = |r: std::ops::Range<usize>| -> String {
        u[r].iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    format!(
        "{}-{}-{}-{}-{}",
        h(0..4),
        h(4..6),
        h(6..8),
        h(8..10),
        h(10..16)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero superblock with required non-zero fields set; tests patch it.
    fn base_sb() -> [u8; 1024] {
        let mut b = [0u8; 1024];
        b[0x38..0x3A].copy_from_slice(&SUPER_MAGIC.to_le_bytes());
        // 4096-byte blocks, 256 blocks, 1 group, 8 inodes of 256 bytes.
        b[0x00..0x04].copy_from_slice(&8u32.to_le_bytes()); // inodes_count
        b[0x04..0x08].copy_from_slice(&256u32.to_le_bytes()); // blocks_count
        b[0x0C..0x10].copy_from_slice(&240u32.to_le_bytes()); // free blocks
        b[0x10..0x14].copy_from_slice(&3u32.to_le_bytes()); // free inodes
        b[0x14..0x18].copy_from_slice(&0u32.to_le_bytes()); // first_data_block (bs>1024)
        b[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes()); // log bs (4096)
        b[0x20..0x24].copy_from_slice(&256u32.to_le_bytes()); // blocks_per_group
        b[0x28..0x2C].copy_from_slice(&8u32.to_le_bytes()); // inodes_per_group
        b[0x3A..0x3C].copy_from_slice(&1u16.to_le_bytes()); // state clean
        b[0x4C..0x50].copy_from_slice(&1u32.to_le_bytes()); // rev 1
        b[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes()); // inode size
        b[0x78..0x80].copy_from_slice(b"TESTEXT4"); // volume name
        b
    }

    #[test]
    fn parses_valid_superblock() {
        let sb = SuperBlock::parse(&base_sb()).expect("valid");
        assert_eq!(sb.block_size, 4096);
        assert_eq!(sb.group_count, 1);
        assert_eq!(sb.volume_name, "TESTEXT4");
        assert_eq!(sb.desc_size, 32); // 0 -> default 32
        assert_eq!(sb.gdt_offset(), 4096);
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut b = base_sb();
        b[0x38..0x3A].copy_from_slice(&0x1234u16.to_le_bytes());
        assert!(matches!(
            SuperBlock::parse(&b),
            Err(crate::error::ExtError::NotExt)
        ));
    }

    #[test]
    fn rejects_insanity() {
        for (off, val, desc) in [
            (0x18, 9u32, "log bs"),        // > 64 KiB blocks
            (0x58, 96u32, "inode size"),   // < 128
            (0x58, 8192u32, "inode size"), // > 4096
            (0x20, 257u32, "bpg"),         // not multiple of 8
            (0x04, 2u32, "blocks_count"),  // too few blocks
        ] {
            let mut b = base_sb();
            match off {
                0x18 | 0x20 | 0x04 => b[off..off + 4].copy_from_slice(&val.to_le_bytes()),
                0x58 => b[off..off + 2].copy_from_slice(&(val as u16).to_le_bytes()),
                _ => unreachable!(),
            }
            let r = SuperBlock::parse(&b);
            assert!(r.is_err(), "should reject {desc}");
        }
    }

    #[test]
    fn fs_kind_classification() {
        let mut b = base_sb();
        // ext2: no journal, no ext4 markers.
        assert_eq!(SuperBlock::parse(&b).unwrap().fs_kind(), "ext2");
        // ext3: journal, still no ext4 markers.
        b[0x5C..0x60].copy_from_slice(&compat::HAS_JOURNAL.to_le_bytes());
        assert_eq!(SuperBlock::parse(&b).unwrap().fs_kind(), "ext3");
        // ext4: extents flag.
        b[0x60..0x64].copy_from_slice(&incompat::EXTENTS.to_le_bytes());
        assert_eq!(SuperBlock::parse(&b).unwrap().fs_kind(), "ext4");
    }

    #[test]
    fn uuid_formats() {
        let mut b = base_sb();
        b[0x68..0x78].copy_from_slice(&[
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 1, 2, 3, 4, 5, 6, 7, 8,
        ]);
        let sb = SuperBlock::parse(&b).unwrap();
        assert_eq!(
            sb.volume_uuid_string,
            "12345678-9abc-def0-0102-030405060708"
        );
    }

    #[test]
    fn s_state_zero_is_not_clean_but_parses() {
        let mut b = base_sb();
        b[0x3A..0x3C].copy_from_slice(&0u16.to_le_bytes());
        let sb = SuperBlock::parse(&b).unwrap();
        assert_eq!(sb.state, 0);
    }
}
