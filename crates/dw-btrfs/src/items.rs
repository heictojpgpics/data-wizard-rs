//! Typed views over raw b-tree item data. All offsets match the kernel
//! `btrfs_tree.h` structs (compiled table in the research workspace).

use crate::superblock::btrfs_time;
use time::OffsetDateTime;

/// `BTRFS_INODE_ITEM_KEY`.
pub const INODE_ITEM_KEY: u8 = 1;
/// `BTRFS_DIR_ITEM_KEY`.
pub const DIR_ITEM_KEY: u8 = 84;
/// `BTRFS_EXTENT_DATA_KEY`.
pub const EXTENT_DATA_KEY: u8 = 108;
/// `BTRFS_ROOT_ITEM_KEY`.
pub const ROOT_ITEM_KEY: u8 = 132;
/// Main filesystem tree objectid (`BTRFS_FS_TREE_OBJECTID`).
pub const FS_TREE_OBJECTID: u64 = 5;
/// Directory key type bits (`BTRFS_FT_*`).
pub const FT_REG: u8 = 1;
pub const FT_DIR: u8 = 2;
pub const FT_SYMLINK: u8 = 7;
/// File extent types.
pub const EXTENT_INLINE: u8 = 0;
pub const EXTENT_REG: u8 = 1;
/// Inline data begins at `offsetof(btrfs_file_extent_item, disk_bytenr)`.
pub const INLINE_START: usize = 0x15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InodeItem {
    pub size: u64,
    pub nlink: u32,
    pub mode: u32,
    pub flags: u64,
    pub atime: Option<OffsetDateTime>,
    pub ctime: Option<OffsetDateTime>,
    pub mtime: Option<OffsetDateTime>,
    pub otime: Option<OffsetDateTime>,
}

impl InodeItem {
    /// `btrfs_inode_item` is 160 bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 160 {
            return None;
        }
        let ts = |off: usize| -> Option<OffsetDateTime> {
            btrfs_time(u64le(data, off), u32le(data, off + 8))
        };
        Some(Self {
            size: u64le(data, 0x10),
            nlink: u32le(data, 0x28),
            mode: u32le(data, 0x34),
            flags: u64le(data, 0x40),
            atime: ts(0x70),
            ctime: ts(0x7C),
            mtime: ts(0x88),
            otime: ts(0x94),
        })
    }
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.mode & 0xF000 == 0x4000
    }
    #[must_use]
    pub fn is_regular(&self) -> bool {
        self.mode & 0xF000 == 0x8000
    }
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.mode & 0xF000 == 0xA000
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Child location from the dir item's key.
    pub child_objectid: u64,
    /// Child item type from the location key (INODE_ITEM or ROOT_ITEM).
    pub child_key_type: u8,
    pub name: String,
    pub ft: u8,
}

impl DirEntry {
    /// `btrfs_dir_item` is 30 bytes followed by `name_len` name bytes
    /// (`data_len` xattr bytes come after the name; we ignore them).
    pub fn parse(item_data: &[u8]) -> Option<Self> {
        if item_data.len() < 30 {
            return None;
        }
        let child_objectid = u64le(item_data, 0);
        let child_key_type = item_data[8];
        let name_len = u16le(item_data, 0x1B) as usize;
        let ft = item_data[0x1D] & 0x7F;
        let data_len = u16le(item_data, 0x19) as usize;
        if item_data.len() < 30 + name_len + data_len {
            return None;
        }
        if ft > 8 {
            return None;
        }
        let name = String::from_utf8_lossy(&item_data[30..30 + name_len]).to_string();
        if name.is_empty() || name.len() > 255 {
            return None;
        }
        Some(Self {
            child_objectid,
            child_key_type,
            name,
            ft,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExtent {
    /// 0 = inline, 1 = regular, 2 = prealloc.
    pub kind: u8,
    /// Compression algorithm (0 = none). Nonzero compression is surfaced
    /// as metadata-only: decompression is out of v1 scope, honestly.
    pub compression: u8,
    pub disk_bytenr: u64,
    pub disk_num_bytes: u64,
    pub data_offset: u64,
    pub num_bytes: u64,
    /// Inline payload (kind 0 only).
    pub inline: Vec<u8>,
}

impl FileExtent {
    /// `btrfs_file_extent_item` is 53 bytes; inline data starts at 0x15.
    pub fn parse(item_data: &[u8]) -> Option<Self> {
        if item_data.len() < INLINE_START + 8 {
            return None;
        }
        let compression = item_data[0x10];
        let kind = item_data[0x14];
        match kind {
            EXTENT_REG | 2 => {
                if item_data.len() < 53 {
                    return None;
                }
                Some(Self {
                    kind,
                    compression,
                    disk_bytenr: u64le(item_data, 0x15),
                    disk_num_bytes: u64le(item_data, 0x1D),
                    data_offset: u64le(item_data, 0x25),
                    num_bytes: u64le(item_data, 0x2D),
                    inline: Vec::new(),
                })
            }
            EXTENT_INLINE => {
                // Inline: data occupies item_data[0x15..]. Empty inline
                // (size 0) is legal for empty files.
                let inline = item_data[INLINE_START..].to_vec();
                if inline.len() > 64 * 1024 {
                    return None; // inline caps at ~one node in practice
                }
                Some(Self {
                    kind,
                    compression,
                    disk_bytenr: 0,
                    disk_num_bytes: inline.len() as u64,
                    data_offset: 0,
                    num_bytes: inline.len() as u64,
                    inline,
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootItem {
    pub bytenr: u64,
    pub bytes_used: u64,
    pub refs: u32,
    pub flags: u64,
    pub level: u8,
}

/// `BTRFS_ROOT_SUBVOL_DEAD` (root item flags): subvol marked deleted but
/// still visible.
pub const ROOT_SUBVOL_DEAD: u64 = 1 << 48;

impl RootItem {
    /// `btrfs_root_item` is 439 bytes (legacy part ends at
    /// generation_v2 @0xEF; we only read the legacy fields).
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 0xEF {
            return None;
        }
        Some(Self {
            bytenr: u64le(data, 0xB0),
            bytes_used: u64le(data, 0xC0),
            refs: u32le(data, 0xD8),
            flags: u64le(data, 0xD0),
            level: data[0xEE],
        })
    }
    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.flags & ROOT_SUBVOL_DEAD != 0
    }
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inode_item_roundtrip() {
        let mut d = vec![0u8; 160];
        d[0x10..0x18].copy_from_slice(&12u64.to_le_bytes()); // size
        d[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes()); // nlink
        d[0x34..0x38].copy_from_slice(&0x81A4u32.to_le_bytes()); // mode reg
        d[0x88..0x90].copy_from_slice(&1_735_689_600u64.to_le_bytes()); // mtime sec
        d[0x90..0x94].copy_from_slice(&500u32.to_le_bytes()); // mtime nsec
        let ino = InodeItem::parse(&d).unwrap();
        assert!(ino.is_regular());
        assert_eq!(ino.size, 12);
        assert_eq!(ino.mtime.map(|t| t.unix_timestamp()), Some(1_735_689_600));
        assert_eq!(ino.mtime.map(|t| t.nanosecond()), Some(500));
    }

    #[test]
    fn dir_entry_parses_name_after_struct() {
        let mut d = Vec::new();
        // location: objectid 257, type INODE_ITEM(1), offset 0
        d.extend_from_slice(&257u64.to_le_bytes());
        d.push(INODE_ITEM_KEY);
        d.extend_from_slice(&0u64.to_le_bytes());
        // transid
        d.extend_from_slice(&7u64.to_le_bytes());
        // data_len, name_len
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&9u16.to_le_bytes()); // "hello.txt"
        d.push(FT_REG);
        d.extend_from_slice(b"hello.txt");
        let de = DirEntry::parse(&d).unwrap();
        assert_eq!(de.name, "hello.txt");
        assert_eq!(de.child_objectid, 257);
        assert_eq!(de.ft, FT_REG);
    }

    #[test]
    fn regular_extent_parses() {
        let mut d = vec![0u8; 53];
        d[0x10] = 0; // no compression
        d[0x14] = EXTENT_REG;
        d[0x15..0x1D].copy_from_slice(&0x88_0000u64.to_le_bytes()); // disk_bytenr
        d[0x1D..0x25].copy_from_slice(&4096u64.to_le_bytes()); // disk_num_bytes
        d[0x25..0x2D].copy_from_slice(&0u64.to_le_bytes()); // offset
        d[0x2D..0x35].copy_from_slice(&8192u64.to_le_bytes()); // num_bytes
        let fe = FileExtent::parse(&d).unwrap();
        assert_eq!(fe.kind, EXTENT_REG);
        assert_eq!(fe.disk_bytenr, 0x88_0000);
        assert_eq!(fe.num_bytes, 8192);
    }

    #[test]
    fn inline_extent_carries_payload() {
        let mut d = vec![0u8; 0x15];
        d[0x10] = 0;
        d[0x14] = EXTENT_INLINE;
        d.extend_from_slice(b"tinydata");
        let fe = FileExtent::parse(&d).unwrap();
        assert_eq!(fe.kind, EXTENT_INLINE);
        assert_eq!(fe.inline, b"tinydata".to_vec());
    }

    #[test]
    fn bad_ft_rejected() {
        let mut d = Vec::new();
        d.extend_from_slice(&257u64.to_le_bytes());
        d.push(INODE_ITEM_KEY);
        d.extend_from_slice(&0u64.to_le_bytes());
        d.extend_from_slice(&7u64.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&4u16.to_le_bytes());
        d.push(0x7F); // FT_ENCRYPTED bit + 0x7F -> base ft 127 > 8
        d.extend_from_slice(b"name");
        assert!(DirEntry::parse(&d).is_none());
    }

    #[test]
    fn root_item_dead_flag() {
        let mut d = vec![0u8; 0xEF];
        d[0xB0..0xB8].copy_from_slice(&0x3000u64.to_le_bytes());
        d[0xD0..0xD8].copy_from_slice(&ROOT_SUBVOL_DEAD.to_le_bytes());
        let ri = RootItem::parse(&d).unwrap();
        assert!(ri.is_dead());
        assert_eq!(ri.bytenr, 0x3000);
    }
}
