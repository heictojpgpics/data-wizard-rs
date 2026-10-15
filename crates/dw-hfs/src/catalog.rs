//! HFS+ catalog B-tree records (Apple TN 1150; cross-validated against
//! the Linux `hfsplus` driver `hfsplus_cat_*` structs).
//!
//! Catalog leaf record layout:
//! ```text
//! key: keyLength u16 | parentID u32 | nameLen u16 | name (nameLen x u16 BE)
//!      (keyLength counts the bytes AFTER the keyLength field:
//!       4 + 2 + 2*nameLen; keys are padded to even total length)
//! data (starts at 2 + keyLength, i.e. immediately after the padded key):
//!   recordType u16:
//!     1 = file   (hfsplus_cat_file, 204 bytes of data)
//!     2 = folder (hfsplus_cat_folder, 44 bytes)
//!     3 = folder thread
//!     4 = file thread
//! ```
//!
//! Record data sizes, EMPIRICALLY VALIDATED against a real volume
//! (2026-10-16): file record data = 248 bytes, folder record data = 88
//! bytes, with BOTH sharing an 88-byte base and the file appending the
//! two 80-byte forks:
//! ```text
//!   0   recordType u16  (1 folder / 2 file / 3 folder-thread / 4 file-thread)
//!   2   flags u16
//!   4   reserved1 u32
//!   8   ID u32 (folderID / fileID CNID)
//!  12   createDate u32       (seconds since 1904, local)
//!  16   contentModDate u32
//!  20   attributeModDate u32
//!  24   accessDate u32
//!  28   BSDInfo 16 bytes (owner u32, group u32, flags/mode, pad)
//!  44   40-byte block (Finder/text-encoding area; contains a Unix-epoch
//!        timestamp on volumes written by modern macOS - not interpreted)
//!  88   FILE ONLY: data fork ForkData (80)
//! 168   FILE ONLY: resource fork ForkData (80)
//! 248   FILE ONLY: end
//! ```
//! Cross-checks that validated this: dataFork sizes matched plausible
//! file sizes; blocks == ceil(size/blockSize); extents one-block files.
//!
//! Thread records: type@0, reserved@2, parentID@4, nameLen@8, name@10.

use time::OffsetDateTime;

use crate::error::{HfsError, HfsResult};
use crate::volumeheader::{ForkData, ForkExtent};

/// Catalog record types (Apple hfs_format.h, public Darwin source).
///
/// EMPIRICALLY VALIDATED against a real HFS+ volume: folders carry type 1
/// (root, .lproj, Contents, MacOS...), files carry type 2 (.DS_Store,
/// Info.plist, PNGs, the executable...), folder threads 3, file threads 4.
pub mod record_type {
    pub const FOLDER: u16 = 1;
    pub const FILE: u16 = 2;
    pub const FOLDER_THREAD: u16 = 3;
    pub const FILE_THREAD: u16 = 4;
}

/// Seconds between the HFS epoch (1904-01-01) and the Unix epoch.
const HFS_EPOCH_OFFSET: u64 = 2_082_844_800;

/// Convert an HFS+ date (seconds since 1904, local time) to a timestamp.
/// 0 means uninitialized -> None. Values before 1970 produce negative
/// Unix seconds (valid).
pub fn hfs_date_to_timestamp(seconds: u32) -> Option<OffsetDateTime> {
    if seconds == 0 {
        return None;
    }
    let unix = i64::from(seconds) - HFS_EPOCH_OFFSET as i64;
    OffsetDateTime::from_unix_timestamp(unix).ok()
}

/// A parsed catalog key.
#[derive(Debug, Clone)]
pub struct CatalogKey {
    pub parent_id: u32,
    pub name: String,
}

/// One catalog leaf record (key + typed data).
#[derive(Debug, Clone)]
pub enum CatalogRecord {
    File {
        key: CatalogKey,
        file_id: u32,
        create_date: Option<OffsetDateTime>,
        modify_date: Option<OffsetDateTime>,
        access_date: Option<OffsetDateTime>,
        data_fork: ForkData,
        resource_fork: ForkData,
    },
    Folder {
        key: CatalogKey,
        folder_id: u32,
        create_date: Option<OffsetDateTime>,
        modify_date: Option<OffsetDateTime>,
    },
    Thread {
        record_type: u16,
        parent_id: u32,
        name: String,
    },
}

/// Parse a catalog key at the start of `buf`. Returns the key and the
/// offset where the record DATA begins (after the even-padded key).
pub fn parse_key(buf: &[u8]) -> HfsResult<(CatalogKey, usize)> {
    if buf.len() < 10 {
        return Err(HfsError::Corrupt("record too short for a key".into()));
    }
    let key_length = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    if key_length + 2 > buf.len() {
        return Err(HfsError::Corrupt("key length beyond record".into()));
    }
    let parent_id = u32::from_be_bytes([buf[2], buf[3], buf[4], buf[5]]);
    let name_len = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    if 6 + 2 * name_len > key_length {
        return Err(HfsError::Corrupt("name length beyond key".into()));
    }
    let mut units = Vec::with_capacity(name_len);
    for i in 0..name_len {
        let o = 8 + i * 2;
        units.push(u16::from_be_bytes([buf[o], buf[o + 1]]));
    }
    let name: String = units
        .iter()
        .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
        .collect();
    // Data starts at 2 + keyLength, padded to even total key size.
    let mut data_off = 2 + key_length;
    if !data_off.is_multiple_of(2) {
        data_off += 1;
    }
    Ok((CatalogKey { parent_id, name }, data_off))
}

/// Parse one leaf record buffer (key + data).
pub fn parse_record(buf: &[u8]) -> HfsResult<Option<CatalogRecord>> {
    let (key, data_off) = parse_key(buf)?;
    let data = &buf[data_off.min(buf.len())..];
    if data.len() < 2 {
        return Ok(None); // truncated data: skip record
    }
    let rt = u16::from_be_bytes([data[0], data[1]]);
    match rt {
        record_type::FILE => {
            if data.len() < 248 {
                return Ok(None);
            }
            let rd32 =
                |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            Ok(Some(CatalogRecord::File {
                key,
                file_id: rd32(8),
                create_date: hfs_date_to_timestamp(rd32(12)),
                modify_date: hfs_date_to_timestamp(rd32(16)),
                access_date: hfs_date_to_timestamp(rd32(24)),
                data_fork: ForkData::parse(data[88..168].try_into().expect("80 bytes")),
                resource_fork: ForkData::parse(data[168..248].try_into().expect("80 bytes")),
            }))
        }
        record_type::FOLDER => {
            if data.len() < 88 {
                return Ok(None);
            }
            let rd32 =
                |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            Ok(Some(CatalogRecord::Folder {
                key,
                folder_id: rd32(8),
                create_date: hfs_date_to_timestamp(rd32(12)),
                modify_date: hfs_date_to_timestamp(rd32(16)),
            }))
        }
        record_type::FOLDER_THREAD | record_type::FILE_THREAD => {
            if data.len() < 10 {
                return Ok(None);
            }
            let parent_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            let name_len = u16::from_be_bytes([data[8], data[9]]) as usize;
            let mut units = Vec::with_capacity(name_len.min(1024));
            for i in 0..name_len {
                let o = 10 + i * 2;
                if o + 1 >= data.len() {
                    break;
                }
                units.push(u16::from_be_bytes([data[o], data[o + 1]]));
            }
            let name: String = units
                .iter()
                .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
                .collect();
            Ok(Some(CatalogRecord::Thread {
                record_type: rt,
                parent_id,
                name,
            }))
        }
        _ => Ok(None), // unknown record type: skip
    }
}

/// Convert a fork's inline extents to absolute volume-byte extents,
/// trimmed to the fork's logical size.
///
/// HFS+ block N sits at `N * blockSize` from the VOLUME start. The caller
/// adds the volume start (partition offset) when the source is a whole
/// disk rather than the volume itself.
pub fn fork_extents_to_bytes(
    block_size: u32,
    fork: &ForkData,
    logical_size: u64,
) -> Vec<(u64, u64)> {
    let bs = u64::from(block_size);
    if bs == 0 || logical_size == 0 {
        return Vec::new();
    }
    let mut out: Vec<(u64, u64)> = Vec::new();
    let mut remaining = logical_size;
    for ForkExtent {
        start_block,
        block_count,
    } in &fork.extents
    {
        if remaining == 0 {
            break;
        }
        let want = (u64::from(*block_count) * bs).min(remaining);
        let start = u64::from(*start_block) * bs;
        if let Some(last) = out.last_mut()
            && last.0 + last.1 == start
        {
            last.1 += want;
        } else {
            out.push((start, want));
        }
        remaining -= want;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    fn make_key(parent: u32, name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        // keyLength = 4 (parent) + 2 (nameLen) + 2*chars
        let key_len = (6 + 2 * units.len()) as u16;
        let mut v = Vec::new();
        v.extend_from_slice(&key_len.to_be_bytes());
        v.extend_from_slice(&parent.to_be_bytes());
        v.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for &u in &units {
            v.extend_from_slice(&u.to_be_bytes());
        }
        if v.len() % 2 != 0 {
            v.push(0); // even padding
        }
        v
    }

    fn make_file_data(file_id: u32, size: u64, start_block: u32) -> Vec<u8> {
        // 248-byte file record (validated layout: forks at 88/168).
        let mut d = vec![0u8; 248];
        d[0..2].copy_from_slice(&record_type::FILE.to_be_bytes());
        d[8..12].copy_from_slice(&file_id.to_be_bytes());
        // dates @12,16,24
        d[12..16].copy_from_slice(&(3_500_000_000u32).to_be_bytes()); // ~2015
        d[16..20].copy_from_slice(&3_600_000_000u32.to_be_bytes());
        d[24..28].copy_from_slice(&3_700_000_000u32.to_be_bytes());
        // data fork @88: logicalSize u64, clumpSize u32, totalBlocks u32,
        // then extents (start, count) x8.
        d[88..96].copy_from_slice(&size.to_be_bytes());
        d[100..104].copy_from_slice(&1u32.to_be_bytes()); // totalBlocks
        d[104..108].copy_from_slice(&start_block.to_be_bytes());
        d[108..112].copy_from_slice(&1u32.to_be_bytes());
        d
    }

    #[test]
    fn parses_file_record() {
        let mut buf = make_key(2, "readme.txt");
        let data = make_file_data(42, 1234, 30);
        buf.extend_from_slice(&data);
        match parse_record(&buf).unwrap().unwrap() {
            CatalogRecord::File {
                key,
                file_id,
                data_fork,
                create_date,
                ..
            } => {
                assert_eq!(key.parent_id, 2);
                assert_eq!(key.name, "readme.txt");
                assert_eq!(file_id, 42);
                assert_eq!(data_fork.logical_size, 1234);
                assert_eq!(data_fork.extents.len(), 1);
                assert_eq!(data_fork.extents[0].start_block, 30);
                // 3_500_000_000 - 2_082_844_800 = 1_417_155_200 -> 2014-10-15
                let ts = create_date.unwrap();
                assert_eq!(ts.year(), 2014);
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn hfs_epoch_conversion_edges() {
        // 0 -> None
        assert!(hfs_date_to_timestamp(0).is_none());
        // 1970-01-01 00:00:00 = HFS 2_082_844_800
        let ts = hfs_date_to_timestamp(2_082_844_800).unwrap();
        assert_eq!(ts.year(), 1970);
        // 1904-01-01 = HFS 1 (negative unix)
        let ts = hfs_date_to_timestamp(1).unwrap();
        assert_eq!(ts.year(), 1904);
    }

    #[test]
    fn fork_extents_merge_and_trim() {
        let fork = ForkData {
            logical_size: 10_000,
            clump_size: 4096,
            total_blocks: 3,
            extents: vec![
                ForkExtent {
                    start_block: 10,
                    block_count: 2,
                },
                ForkExtent {
                    start_block: 30,
                    block_count: 1,
                },
            ],
        };
        let bs = 4096u32;
        // logical 10000: extent1 8192 contiguous (10,11), extent2 partial 1808
        let out = fork_extents_to_bytes(bs, &fork, 10_000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], (10 * 4096, 8192));
        assert_eq!(out[1], (30 * 4096, 1808));
        // total coverage < logical size -> honest partial
        let short = fork_extents_to_bytes(bs, &fork, 100_000);
        assert_eq!(short.iter().map(|e| e.1).sum::<u64>(), 12_288);
    }

    #[test]
    fn thread_record_parses() {
        // type=4 (file thread), parentID=2, name "a.app"
        let mut d = Vec::new();
        d.extend_from_slice(&record_type::FILE_THREAD.to_be_bytes());
        d.extend_from_slice(&0u16.to_be_bytes()); // reserved
        d.extend_from_slice(&2u32.to_be_bytes()); // parentID @4
        let units: Vec<u16> = "a.app".encode_utf16().collect();
        d.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for &u in &units {
            d.extend_from_slice(&u.to_be_bytes());
        }
        // key prefix (the record slice begins with the key)
        let key = make_key(2, "a.app");
        let mut buf = key;
        buf.extend_from_slice(&d);
        match parse_record(&buf).unwrap().unwrap() {
            CatalogRecord::Thread {
                record_type,
                parent_id,
                name,
            } => {
                assert_eq!(record_type, record_type::FILE_THREAD);
                assert_eq!(parent_id, 2);
                assert_eq!(name, "a.app");
            }
            other => panic!("expected Thread, got {other:?}"),
        }
    }
}
