//! exFAT directory entry parser.
//!
//! exFAT uses a different directory entry format than FAT12/16/32.
//! Each entry is 32 bytes with a type byte at offset 0.
//!
//! # Entry types
//!
//! | Type  | Name              | Description |
//! |-------|-------------------|-------------|
//! | 0x81  | Bitmap            | Allocation bitmap |
//! | 0x82  | Up-case Table     | Unicode up-case table |
//! | 0x83  | Volume Label      | Volume label |
//! | 0x85  | File Directory    | Primary entry for a file/dir |
//! | 0xC0  | Stream Extension  | Data stream info for a file |
//! | 0xC1  | File Name         | File name fragment (up to 15 UTF-16 chars) |
//!
//! # File directory entry set
//!
//! A file consists of:
//! 1. One `File Directory` (0x85) entry — the primary entry
//! 2. One `Stream Extension` (0xC0) entry — data stream info
//! 3. One or more `File Name` (0xC1) entries — name fragments
//!
//! The `InUse` bit (bit 1 of the type byte) indicates whether the entry
//! is in use. A deleted file has this bit clear (0x05 instead of 0x85).

use bytemuck::{Pod, Zeroable};

use crate::error::ExfatResult;

/// Entry type constants.
///
/// In exFAT, the entry type byte encodes both the type and the InUse flag:
/// - For primary entries (Bitmap, Upcase, Volume Label, File Directory),
///   bit 7 (0x80) is the InUse flag. A deleted File Directory has type 0x05.
/// - For secondary entries (Stream Extension, File Name), the type byte is
///   always 0xC0/0xC1 — the high bit is part of the type, not InUse.
pub mod entry_type {
    // Base types (without InUse bit for primary entries)
    pub const BITMAP_BASE: u8 = 0x01;
    pub const UPCASE_TABLE_BASE: u8 = 0x02;
    pub const VOLUME_LABEL_BASE: u8 = 0x03;
    pub const FILE_DIRECTORY_BASE: u8 = 0x05;

    // Types with InUse bit set (as they appear on disk when active)
    pub const BITMAP: u8 = 0x81;
    pub const UPCASE_TABLE: u8 = 0x82;
    pub const VOLUME_LABEL: u8 = 0x83;
    pub const FILE_DIRECTORY: u8 = 0x85;
    pub const STREAM_EXTENSION: u8 = 0xC0;
    pub const FILE_NAME: u8 = 0xC1;
    /// Deleted Stream Extension (InUse bit clear). Parsed like an active
    /// stream entry; the walker treats the whole set as deleted via the
    /// primary entry's InUse bit.
    pub const STREAM_EXTENSION_DELETED: u8 = 0x40;
    /// Deleted File Name entry (InUse bit clear).
    pub const FILE_NAME_DELETED: u8 = 0x41;
    pub const END_OF_DIRECTORY: u8 = 0x00;

    /// Mask to strip the InUse bit and get the base type.
    pub const TYPE_MASK: u8 = 0x7F;
    /// InUse bit (bit 7).
    pub const IN_USE_BIT: u8 = 0x80;
}

/// Generic 32-byte directory entry raw layout.
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct DirEntryRaw {
    pub entry_type: u8,
    pub data: [u8; 31],
}

/// File Directory entry (0x85) — primary entry for a file or directory.
///
/// Layout (exFAT spec v1.00 §7.3; absolute offsets in the 32-byte entry):
/// ```text
///  0      EntryType
///  1      SecondaryCount
///  2..4   SetChecksum
///  4..6   FileAttributes
///  6..8   Reserved1
///  8..12  CreateTimestamp      (FAT format: date<<16 | time)
/// 12..16  LastModifiedTimestamp
/// 16..20  LastAccessedTimestamp
/// 20     CreateTime10ms        (10 ms units, 0..=199)
/// 21     CreateUtcOffset        (7-bit signed, 15-min units; 0x00 = unknown)
/// 22     LastModifiedUtcOffset
/// 23     LastAccessedUtcOffset
/// 24..32 Reserved2
/// ```
#[derive(Debug, Clone)]
pub struct FileDirectoryEntry {
    /// Secondary count: number of secondary entries following this one.
    pub secondary_count: u8,
    /// Set checksum: checksum of the entire entry set.
    pub set_checksum: u16,
    /// File attributes (same encoding as FAT: 0x01=read-only, 0x02=hidden,
    /// 0x04=system, 0x10=directory, 0x20=archive).
    pub file_attributes: u16,
    /// Reserved
    pub _reserved1: u16,
    /// Creation timestamp (UTC, FAT timestamp format).
    pub create_timestamp: u32,
    /// Last modified timestamp (UTC, FAT timestamp format).
    pub last_modified_timestamp: u32,
    /// Last accessed timestamp (UTC, FAT timestamp format).
    pub last_accessed_timestamp: u32,
    /// Creation time, 10 ms units (0..=199). Refines the 2 s granularity
    /// of `create_timestamp` (see dw_core fat timestamp decoding).
    pub create_time_10ms: u8,
    /// Create UTC offset (7-bit signed value in 15-min units; 0x00 = no
    /// timezone information stored).
    pub create_utc_offset: u8,
    /// Last-modified UTC offset.
    pub last_modified_utc_offset: u8,
    /// Last-accessed UTC offset.
    pub last_accessed_utc_offset: u8,
    /// Is the entry in use? (InUse bit set)
    pub in_use: bool,
}

/// Stream Extension entry (0xC0) — data stream info for a file.
///
/// Layout (exFAT spec v1.00 §7.4; cross-validated against the Linux
/// kernel exfat driver `struct exfat_dentry_stream`; absolute offsets):
/// ```text
///  0      EntryType
///  1      GeneralSecondaryFlags (bit0 AllocationPossible, bit1 NoFatChain)
///  2      Reserved1
///  3      NameLength           (UTF-16 chars)
///  4..6   NameHash
///  6..8   Reserved2
///  8..16  ValidDataLength
/// 16..20  Reserved3
/// 20..24  FirstCluster
/// 24..32  DataLength
/// ```
#[derive(Debug, Clone)]
pub struct StreamExtensionEntry {
    /// General secondary flags (bit 0 = AllocationPossible, bit 1 = NoFatChain).
    pub flags: u8,
    /// Reserved1 (spec: shall be zero).
    pub reserved1: u8,
    /// Name length in UTF-16 characters.
    pub name_length: u8,
    /// Name hash (for quick name comparison).
    pub name_hash: u16,
    /// Reserved2.
    pub reserved2: u16,
    /// Valid data length (bytes).
    pub valid_data_length: u64,
    /// Reserved3.
    pub reserved3: u32,
    /// First cluster of the data stream.
    pub first_cluster: u32,
    /// Data length (bytes). May differ from valid_data_length if the
    /// file was truncated.
    pub data_length: u64,
    /// Does this stream use the FAT chain, or is it contiguous (NoFatChain)?
    pub no_fat_chain: bool,
}

/// File Name entry (0xC1) — a fragment of the file name (up to 15 UTF-16 chars).
#[derive(Debug, Clone)]
pub struct FileNameEntry {
    /// General secondary flags.
    pub flags: u8,
    /// Name fragment (up to 15 UTF-16 LE characters).
    pub name_fragment: String,
}

/// A parsed exFAT directory entry.
#[derive(Debug, Clone)]
pub enum DirEntry {
    FileDirectory(FileDirectoryEntry),
    StreamExtension(StreamExtensionEntry),
    FileName(FileNameEntry),
    Bitmap,
    UpcaseTable,
    VolumeLabel,
    /// Unknown or end-of-directory marker.
    Other(u8),
    EndOfDirectory,
}

impl DirEntry {
    /// Parse a 32-byte buffer as a directory entry.
    pub fn parse(buf: &[u8; 32]) -> ExfatResult<Self> {
        let raw: &DirEntryRaw = bytemuck::from_bytes(buf);
        let entry_type = raw.entry_type;

        if entry_type == entry_type::END_OF_DIRECTORY {
            return Ok(Self::EndOfDirectory);
        }

        // Match on the raw type byte. For primary entries, the InUse bit
        // (0x80) distinguishes active (0x85) from deleted (0x05). For
        // secondary entries, the type byte is always 0xC0/0xC1.
        match entry_type {
            entry_type::BITMAP => Ok(Self::Bitmap),
            entry_type::UPCASE_TABLE => Ok(Self::UpcaseTable),
            entry_type::VOLUME_LABEL => Ok(Self::VolumeLabel),
            entry_type::FILE_DIRECTORY | entry_type::FILE_DIRECTORY_BASE => {
                let in_use = entry_type & entry_type::IN_USE_BIT != 0;
                let data = &raw.data;
                Ok(Self::FileDirectory(FileDirectoryEntry {
                    secondary_count: data[0],
                    set_checksum: u16::from_le_bytes([data[1], data[2]]),
                    file_attributes: u16::from_le_bytes([data[3], data[4]]),
                    _reserved1: u16::from_le_bytes([data[5], data[6]]),
                    create_timestamp: u32::from_le_bytes([data[7], data[8], data[9], data[10]]),
                    last_modified_timestamp: u32::from_le_bytes([
                        data[11], data[12], data[13], data[14],
                    ]),
                    last_accessed_timestamp: u32::from_le_bytes([
                        data[15], data[16], data[17], data[18],
                    ]),
                    create_time_10ms: data[19],
                    create_utc_offset: data[20],
                    last_modified_utc_offset: data[21],
                    last_accessed_utc_offset: data[22],
                    in_use,
                }))
            }
            entry_type::STREAM_EXTENSION | entry_type::STREAM_EXTENSION_DELETED => {
                // Spec layout (absolute offsets): 1 flags, 2 Reserved1,
                // 3 NameLength, 4..6 NameHash, 6..8 Reserved2,
                // 8..16 ValidDataLength, 16..20 Reserved3, 20..24
                // FirstCluster, 24..32 DataLength.
                // `data` = bytes 1..32, so data[i] = absolute offset i+1.
                let data = &raw.data;
                let flags = data[0];
                let no_fat_chain = (flags & 0x02) != 0;
                Ok(Self::StreamExtension(StreamExtensionEntry {
                    flags,
                    reserved1: data[1],
                    name_length: data[2],
                    name_hash: u16::from_le_bytes([data[3], data[4]]),
                    reserved2: u16::from_le_bytes([data[5], data[6]]),
                    valid_data_length: u64::from_le_bytes([
                        data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14],
                    ]),
                    reserved3: u32::from_le_bytes([data[15], data[16], data[17], data[18]]),
                    first_cluster: u32::from_le_bytes([data[19], data[20], data[21], data[22]]),
                    data_length: u64::from_le_bytes([
                        data[23], data[24], data[25], data[26], data[27], data[28], data[29],
                        data[30],
                    ]),
                    no_fat_chain,
                }))
            }
            entry_type::FILE_NAME | entry_type::FILE_NAME_DELETED => {
                let data = &raw.data;
                let flags = data[0];
                // File name fragment: bytes 1..31 = 30 bytes = up to 15 UTF-16 chars
                let name_bytes = &data[1..31];
                let mut chars = Vec::with_capacity(15);
                let mut i = 0;
                while i + 1 < name_bytes.len() {
                    let unit = u16::from_le_bytes([name_bytes[i], name_bytes[i + 1]]);
                    if unit == 0 {
                        break;
                    }
                    chars.push(unit);
                    i += 2;
                }
                let name_fragment: String = chars
                    .iter()
                    .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
                    .collect();
                Ok(Self::FileName(FileNameEntry {
                    flags,
                    name_fragment,
                }))
            }
            _ => Ok(Self::Other(entry_type)),
        }
    }
}

/// A complete file entry set: File Directory + Stream Extension + File Name(s).
#[derive(Debug, Clone)]
pub struct FileEntrySet {
    pub primary: FileDirectoryEntry,
    pub stream: StreamExtensionEntry,
    pub name: String,
    pub is_directory: bool,
    pub is_deleted: bool,
}

impl FileEntrySet {
    /// Is this a directory?
    #[must_use]
    pub fn is_directory(&self) -> bool {
        self.is_directory
    }

    /// First cluster of the data stream.
    #[must_use]
    pub fn first_cluster(&self) -> u32 {
        self.stream.first_cluster
    }

    /// Data length in bytes.
    #[must_use]
    pub fn data_length(&self) -> u64 {
        self.stream.data_length
    }
}

/// Reassemble a file name from multiple File Name entries.
///
/// File Name entries are numbered 1..N and each holds up to 15 UTF-16 chars.
/// The entries appear in order (entry 1 first, entry 2 second, etc.).
/// We concatenate all fragments until we hit a null terminator.
#[must_use]
pub fn reassemble_file_name(name_entries: &[FileNameEntry]) -> String {
    let mut name = String::new();
    for entry in name_entries {
        name.push_str(&entry.name_fragment);
    }
    name
}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_file_dir_entry(in_use: bool, attrs: u16, secondary_count: u8) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0] = if in_use {
            entry_type::FILE_DIRECTORY // 0x85 (InUse bit already set)
        } else {
            entry_type::FILE_DIRECTORY_BASE // 0x05 (deleted)
        };
        buf[1] = secondary_count; // secondary count
        buf[2..4].copy_from_slice(&0u16.to_le_bytes()); // set checksum
        buf[4..6].copy_from_slice(&attrs.to_le_bytes()); // file attributes
        // timestamps (all zeros for now)
        buf
    }

    fn make_stream_ext_entry(
        first_cluster: u32,
        data_length: u64,
        name_length: u8,
        no_fat_chain: bool,
    ) -> [u8; 32] {
        // Spec layout (absolute offsets): 1 flags, 2 Reserved1, 3 NameLength,
        // 4..6 NameHash, 6..8 Reserved2, 8..16 ValidDataLength, 16..20
        // Reserved3, 20..24 FirstCluster, 24..32 DataLength.
        let mut buf = [0u8; 32];
        buf[0] = entry_type::STREAM_EXTENSION | entry_type::IN_USE_BIT;
        let flags: u8 = if no_fat_chain { 0x02 } else { 0x01 }; // AllocationPossible
        buf[1] = flags;
        buf[2] = 0; // Reserved1
        buf[3] = name_length;
        buf[4..6].copy_from_slice(&0u16.to_le_bytes()); // NameHash
        buf[8..16].copy_from_slice(&data_length.to_le_bytes()); // ValidDataLength
        buf[20..24].copy_from_slice(&first_cluster.to_le_bytes()); // FirstCluster
        buf[24..32].copy_from_slice(&data_length.to_le_bytes()); // DataLength
        buf
    }

    fn make_file_name_entry(fragment: &str, index: u8) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0] = entry_type::FILE_NAME | entry_type::IN_USE_BIT;
        buf[1] = index; // GeneralSecondaryFlags (lower 5 bits = entry index)
        let units: Vec<u16> = fragment.encode_utf16().collect();
        for (i, &unit) in units.iter().take(15).enumerate() {
            buf[2 + i * 2] = (unit & 0xFF) as u8;
            buf[2 + i * 2 + 1] = (unit >> 8) as u8;
        }
        buf
    }

    #[test]
    fn parses_file_directory_entry() {
        let buf = make_file_dir_entry(true, 0x0010, 2); // directory, 2 secondaries
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileDirectory(fd) => {
                assert!(fd.in_use);
                assert_eq!(fd.file_attributes, 0x0010);
                assert_eq!(fd.secondary_count, 2);
            }
            _ => panic!("expected FileDirectory"),
        }
    }

    #[test]
    fn parses_deleted_file_directory_entry() {
        let buf = make_file_dir_entry(false, 0x0020, 3); // deleted file
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileDirectory(fd) => {
                assert!(!fd.in_use);
                assert_eq!(fd.file_attributes, 0x0020);
                assert_eq!(fd.secondary_count, 3);
            }
            _ => panic!("expected FileDirectory"),
        }
    }

    #[test]
    fn parses_stream_extension_entry() {
        let buf = make_stream_ext_entry(100, 4096, 10, false);
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::StreamExtension(se) => {
                assert_eq!(se.first_cluster, 100);
                assert_eq!(se.data_length, 4096);
                assert_eq!(se.name_length, 10);
                assert!(!se.no_fat_chain);
            }
            _ => panic!("expected StreamExtension"),
        }
    }

    #[test]
    fn parses_stream_extension_no_fat_chain() {
        let buf = make_stream_ext_entry(200, 8192, 5, true);
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::StreamExtension(se) => {
                assert!(se.no_fat_chain);
                assert_eq!(se.first_cluster, 200);
            }
            _ => panic!("expected StreamExtension"),
        }
    }

    #[test]
    fn parses_file_name_entry() {
        let buf = make_file_name_entry("test.txt", 1);
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileName(fn_) => {
                assert_eq!(fn_.name_fragment, "test.txt");
            }
            _ => panic!("expected FileName"),
        }
    }

    #[test]
    fn parses_unicode_file_name() {
        let buf = make_file_name_entry("héllo 世界", 1);
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileName(fn_) => {
                assert_eq!(fn_.name_fragment, "héllo 世界");
            }
            _ => panic!("expected FileName"),
        }
    }

    #[test]
    fn reassembles_multi_fragment_name() {
        let entries = vec![
            FileNameEntry {
                flags: 1,
                name_fragment: "very_long_file_".to_string(),
            },
            FileNameEntry {
                flags: 2,
                name_fragment: "name_part2.txt".to_string(),
            },
        ];
        let name = reassemble_file_name(&entries);
        assert_eq!(name, "very_long_file_name_part2.txt");
    }

    #[test]
    fn parses_deleted_stream_extension_entry() {
        // Deleted secondaries keep their type with the InUse bit clear
        // (0xC0 -> 0x40).
        let buf = make_stream_ext_entry(100, 4096, 10, false);
        let mut buf = buf;
        buf[0] = entry_type::STREAM_EXTENSION_DELETED;
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::StreamExtension(se) => {
                assert_eq!(se.first_cluster, 100);
                assert_eq!(se.data_length, 4096);
            }
            _ => panic!("expected deleted StreamExtension"),
        }
    }

    #[test]
    fn parses_deleted_file_name_entry() {
        let buf = make_file_name_entry("gone.txt", 1);
        let mut buf = buf;
        buf[0] = entry_type::FILE_NAME_DELETED;
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileName(fn_) => assert_eq!(fn_.name_fragment, "gone.txt"),
            _ => panic!("expected deleted FileName"),
        }
    }

    #[test]
    fn stream_extension_spec_offsets_roundtrip() {
        // Byte-exact spec-layout round-trip: every field at its spec offset.
        let buf = make_stream_ext_entry(0x00ABCDEF, 0x1122334455667788, 0x21, true);
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::StreamExtension(se) => {
                assert_eq!(se.flags, 0x02); // NoFatChain set, AllocationPossible clear
                assert_eq!(se.reserved1, 0);
                assert_eq!(se.name_length, 0x21);
                assert_eq!(se.name_hash, 0);
                assert_eq!(se.reserved2, 0);
                assert_eq!(se.valid_data_length, 0x1122334455667788);
                assert_eq!(se.reserved3, 0);
                assert_eq!(se.first_cluster, 0x00ABCDEF);
                assert_eq!(se.data_length, 0x1122334455667788);
                assert!(se.no_fat_chain);
            }
            _ => panic!("expected StreamExtension"),
        }
    }

    #[test]
    fn file_directory_entry_parses_10ms_and_utc_offsets() {
        let mut buf = make_file_dir_entry(true, 0x0020, 2);
        // CreateTime10ms at absolute offset 20; UTC offsets at 21..24.
        buf[20] = 137;
        buf[21] = 0x11; // +1:45h (11 * 15min)
        buf[22] = 0x0F; // +3:45h
        buf[23] = 0x91; // -3:45h (sign bit set)
        let entry = DirEntry::parse(&buf).unwrap();
        match entry {
            DirEntry::FileDirectory(fd) => {
                assert_eq!(fd.create_time_10ms, 137);
                assert_eq!(fd.create_utc_offset, 0x11);
                assert_eq!(fd.last_modified_utc_offset, 0x0F);
                assert_eq!(fd.last_accessed_utc_offset, 0x91);
            }
            _ => panic!("expected FileDirectory"),
        }
    }

    #[test]
    fn parses_end_of_directory() {
        let buf = [0u8; 32]; // all zeros = end of directory
        let entry = DirEntry::parse(&buf).unwrap();
        assert!(matches!(entry, DirEntry::EndOfDirectory));
    }

    #[test]
    fn parses_bitmap_entry() {
        let mut buf = [0u8; 32];
        buf[0] = entry_type::BITMAP | entry_type::IN_USE_BIT;
        let entry = DirEntry::parse(&buf).unwrap();
        assert!(matches!(entry, DirEntry::Bitmap));
    }

    #[test]
    fn parses_upcase_table_entry() {
        let mut buf = [0u8; 32];
        buf[0] = entry_type::UPCASE_TABLE | entry_type::IN_USE_BIT;
        let entry = DirEntry::parse(&buf).unwrap();
        assert!(matches!(entry, DirEntry::UpcaseTable));
    }

    #[test]
    fn parses_volume_label_entry() {
        let mut buf = [0u8; 32];
        buf[0] = entry_type::VOLUME_LABEL | entry_type::IN_USE_BIT;
        let entry = DirEntry::parse(&buf).unwrap();
        assert!(matches!(entry, DirEntry::VolumeLabel));
    }

    #[test]
    fn parses_unknown_entry_type() {
        let mut buf = [0u8; 32];
        buf[0] = 0xFF; // unknown type
        let entry = DirEntry::parse(&buf).unwrap();
        assert!(matches!(entry, DirEntry::Other(0xFF)));
    }
}
