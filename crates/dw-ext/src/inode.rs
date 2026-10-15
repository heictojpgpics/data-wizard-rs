//! ext2/3/4 on-disk inode (128 bytes minimum; ext4 extends to
//! `s_inode_size` with extra timestamp fields).
//!
//! Layout from `fs/ext4/ext4.h` `struct ext4_inode`. Timestamps are
//! 32-bit epoch seconds; ext4 inodes with `i_extra_isize >= 0x2C` carry
//! the `*_extra` words whose top 2 bits extend the epoch to 34 bits and
//! whose low 30 bits are nanoseconds.

use crate::error::{ExtError, ExtResult};
use time::OffsetDateTime;

/// `EXT4_EXTENTS_FL` — i_block holds an extent tree root.
pub const FLAG_EXTENTS: u32 = 0x0008_0000;
/// `EXT4_INDEX_FL` — directory uses hashed indexes (htree).
pub const FLAG_INDEX: u32 = 0x0001_0000;
/// `EXT4_INLINE_DATA_FL` — small file data lives inside i_block.
pub const FLAG_INLINE_DATA: u32 = 0x4000_0000;
/// Regular file.
pub const S_IFMT: u32 = 0xF000;
pub const S_IFREG: u32 = 0x8000;
pub const S_IFDIR: u32 = 0x4000;
pub const S_IFLNK: u32 = 0xA000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inode {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    /// i_blocks in 512-byte sectors (lo + hi).
    pub sectors: u64,
    pub links_count: u32,
    pub flags: u32,
    pub generation: u32,
    pub file_acl: u64,
    pub atime: Option<OffsetDateTime>,
    pub ctime: Option<OffsetDateTime>,
    pub mtime: Option<OffsetDateTime>,
    pub dtime: Option<OffsetDateTime>,
    pub crtime: Option<OffsetDateTime>,
    /// The 60-byte i_block area (extent root, indirect map, or inline
    /// symlink target) as raw bytes.
    pub i_block: [u8; 60],
}

impl Inode {
    /// Parse from a buffer of at least `inode_size` bytes (the caller
    /// slices the table correctly; 128 is the minimum we read).
    pub fn parse(buf: &[u8]) -> ExtResult<Self> {
        if buf.len() < 128 {
            return Err(ExtError::Corrupt("truncated inode".into()));
        }
        let u16at = |o: usize| u16::from_le_bytes([buf[o], buf[o + 1]]);
        let u32at = |o: usize| u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);

        let mode = u32::from(u16at(0x00));
        let uid = u32::from(u16at(0x02));
        let size_lo = u32at(0x04);
        let atime_s = u32at(0x08) as i64;
        let ctime_s = u32at(0x0C) as i64;
        let mtime_s = u32at(0x10) as i64;
        let dtime_s = u32at(0x14) as i64;
        let gid = u32::from(u16at(0x18));
        let links_count = u32::from(u16at(0x1A));
        let blocks_lo = u32at(0x1C);
        let flags = u32at(0x20);
        let mut i_block = [0u8; 60];
        i_block.copy_from_slice(&buf[0x28..0x64]);
        let generation = u32at(0x64);
        let file_acl_lo = u32at(0x68);
        let size_high = u32at(0x6C);

        // 0x74: i_blocks_hi (u16) | 0x76: i_file_acl_hi (u16)
        let blocks_hi = u32::from(u16at(0x74));
        let file_acl_hi = u32::from(u16at(0x76));

        let size = if is_dir(mode) {
            // Directories: i_dir_acl held a du limit in ext2; treat as
            // size only when the inode is a directory and LARGE_FILE
            // marked. For directories we trust size_lo for walking and
            // cap the walk by extents; keep size = lo for dirs.
            u64::from(size_lo)
        } else {
            u64::from(size_lo) | (u64::from(size_high) << 32)
        };

        let mut atime = epoch(atime_s, 0);
        let mut ctime = epoch(ctime_s, 0);
        let mut mtime = epoch(mtime_s, 0);
        let dtime = epoch(dtime_s, 0);
        let mut crtime = None;

        // Extra fields (ext4): only when the buffer actually carries them.
        if buf.len() >= 156 {
            let extra_isize = u16at(0x80);
            if extra_isize >= 0x2C && buf.len() >= 0x98 {
                let atime_extra = u32at(0x84);
                let ctime_extra = u32at(0x88);
                let mtime_extra = u32at(0x8C);
                let crtime_s = u32at(0x90) as i64;
                let crtime_extra = u32at(0x94);
                atime = epoch_extended(atime_s, atime_extra);
                ctime = epoch_extended(ctime_s, ctime_extra);
                mtime = epoch_extended(mtime_s, mtime_extra);
                crtime = epoch_extended(crtime_s, crtime_extra);
            }
        }

        Ok(Self {
            mode,
            uid,
            gid,
            size,
            sectors: u64::from(blocks_lo) | (u64::from(blocks_hi) << 32),
            links_count,
            flags,
            generation,
            file_acl: u64::from(file_acl_lo) | (u64::from(file_acl_hi) << 32),
            atime,
            ctime,
            mtime,
            dtime,
            crtime,
            i_block,
        })
    }

    #[must_use]
    pub fn is_regular(&self) -> bool {
        self.mode & S_IFMT == S_IFREG && self.mode != 0
    }
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.mode & S_IFMT == S_IFLNK
    }
    #[must_use]
    pub fn uses_extents(&self) -> bool {
        self.flags & FLAG_EXTENTS != 0
    }

    /// An inode is a plausibly-deleted-but-recoverable candidate when it
    /// is unallocated (caller knows that) but its on-disk record still
    /// describes a file: nonzero mode, sane links, valid dtime or intact
    /// block map. Pure-zero inodes (never used / wiped) fail here.
    #[must_use]
    pub fn is_recoverable_orphan(&self) -> bool {
        if self.mode == 0 || self.mode & S_IFMT == 0 {
            return false;
        }
        if self.size == 0 && self.sectors == 0 {
            return false;
        }
        self.is_regular() || self.is_dir() || self.is_symlink()
    }
}

fn is_dir(mode: u32) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// Plain 32-bit epoch; <= 0 means unset.
fn epoch(secs: i64, nanos: i128) -> Option<OffsetDateTime> {
    if secs <= 0 {
        return None;
    }
    OffsetDateTime::from_unix_timestamp_nanos(secs as i128 * 1_000_000_000 + nanos).ok()
}

/// ext4 extended epoch: extra word bits 31..30 extend seconds to 34 bits,
/// bits 29..0 are nanoseconds.
fn epoch_extended(secs: i64, extra: u32) -> Option<OffsetDateTime> {
    if secs <= 0 {
        return None;
    }
    let epoch_hi = i64::from(extra >> 30);
    let nanos = i128::from(extra & 0x3FFF_FFFF);
    let total_secs = secs + (epoch_hi << 32);
    if total_secs <= 0 {
        return None;
    }
    OffsetDateTime::from_unix_timestamp_nanos(total_secs as i128 * 1_000_000_000 + nanos).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zeroed 256-byte inode buffer for patching.
    fn blank() -> [u8; 256] {
        [0u8; 256]
    }

    #[test]
    fn parses_regular_file_inode() {
        let mut b = blank();
        b[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes()); // -rw-r--r--
        b[0x04..0x08].copy_from_slice(&12u32.to_le_bytes()); // size lo
        b[0x08..0x0C].copy_from_slice(&1_735_689_600u32.to_le_bytes()); // atime
        b[0x0C..0x10].copy_from_slice(&1_735_689_601u32.to_le_bytes()); // ctime
        b[0x10..0x14].copy_from_slice(&1_735_689_602u32.to_le_bytes()); // mtime
        b[0x1A..0x1C].copy_from_slice(&1u16.to_le_bytes()); // links
        b[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // blocks (4KiB)
        b[0x20..0x24].copy_from_slice(&FLAG_EXTENTS.to_le_bytes());
        b[0x6C..0x70].copy_from_slice(&0u32.to_le_bytes()); // size hi
        b[0x80..0x82].copy_from_slice(&0x30u16.to_le_bytes()); // extra_isize 48

        let ino = Inode::parse(&b).unwrap();
        assert!(ino.is_regular());
        assert!(!ino.is_dir());
        assert_eq!(ino.size, 12);
        assert_eq!(ino.links_count, 1);
        assert!(ino.uses_extents());
        assert_eq!(ino.mtime.map(|t| t.unix_timestamp()), Some(1_735_689_602));
    }

    #[test]
    fn extra_epoch_bits_and_nanos() {
        let mut b = blank();
        b[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        b[0x04..0x08].copy_from_slice(&10u32.to_le_bytes());
        b[0x08..0x0C].copy_from_slice(&1u32.to_le_bytes()); // atime base
        b[0x80..0x82].copy_from_slice(&0x30u16.to_le_bytes());
        // mtime: base 1, extra = (epoch_hi=1 <<30) | 500ms nanos
        b[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
        b[0x8C..0x90].copy_from_slice(&(1u32 << 30 | 500_000_000u32).to_le_bytes());
        let ino = Inode::parse(&b).unwrap();
        let m = ino.mtime.unwrap();
        assert_eq!(m.unix_timestamp(), 1 + (1 << 32));
        assert_eq!(m.nanosecond(), 500_000_000);
    }

    #[test]
    fn dtime_present_on_deleted_inode() {
        let mut b = blank();
        b[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        b[0x04..0x08].copy_from_slice(&20u32.to_le_bytes());
        b[0x14..0x18].copy_from_slice(&1_735_776_000u32.to_le_bytes()); // dtime
        let ino = Inode::parse(&b).unwrap();
        assert_eq!(ino.dtime.map(|t| t.unix_timestamp()), Some(1_735_776_000));
        assert!(ino.is_recoverable_orphan());
    }

    #[test]
    fn zero_inode_not_recoverable() {
        let ino = Inode::parse(&blank()).unwrap();
        assert!(!ino.is_recoverable_orphan());
        assert!(!ino.is_regular());
    }

    #[test]
    fn size_uses_high_word_for_regular_files() {
        let mut b = blank();
        b[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        b[0x04..0x08].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        b[0x6C..0x70].copy_from_slice(&1u32.to_le_bytes());
        let ino = Inode::parse(&b).unwrap();
        assert_eq!(ino.size, (1u64 << 32) + u64::from(0xFFFF_FFFFu32));
    }

    #[test]
    fn truncated_inode_rejected() {
        assert!(Inode::parse(&[0u8; 64]).is_err());
    }
}
