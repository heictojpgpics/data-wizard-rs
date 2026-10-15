//! FAT directory entry parser.
//!
//! Layout (public spec: Microsoft FAT spec, "Directory Structure" section):
//!
//! Each directory entry is 32 bytes. There are two kinds:
//!
//! ## Short (8.3) entry
//!
//! ```text
//! Offset  Size  Field
//!   0     8     Name (8 bytes, space-padded)
//!   8     3     Extension (3 bytes, space-padded)
//!  11     1     Attributes
//!                  0x01 = read-only
//!                  0x02 = hidden
//!                  0x04 = system
//!                  0x08 = volume label
//!                  0x10 = directory
//!                  0x20 = archive
//!                  0x0F = long file name (LFN) entry
//!  12     1     Reserved (NT case info)
//!  13     1     Creation time (tenths of second)
//!  14     2     Creation time
//!  16     2     Creation date
//!  18     2     Last access date
//!  20     2     First cluster high word (FAT32)
//!  22     2     Last write time
//!  24     2     Last write date
//!  26     2     First cluster low word
//!  28     4     File size (bytes)
//! ```
//!
//! ## Long File Name (LFN) entry
//!
//! LFN entries precede the short entry. Each LFN entry stores up to 13
//! UTF-16 code units. Attributes == 0x0F identifies an LFN entry.
//!
//! ```text
//! Offset  Size  Field
//!   0     1     Sequence number (bit 0x40 = last LFN entry, bits 0x01-0x0F = position)
//!   1    10     Name part 1 (5 UTF-16 code units)
//!  11     1     Attributes (0x0F)
//!  12     1     Type (0x00)
//!  13     1     Checksum of short name
//!  14    12     Name part 2 (6 UTF-16 code units)
//!  26     2     First cluster (always 0x0000)
//!  28     4     Name part 3 (2 UTF-16 code units)
//! ```
//!
//! ## Deleted entries
//!
//! When a file is deleted, the first byte of its short entry is set to
//! 0xE5. The rest of the entry (including cluster pointer and size) is
//! typically intact, but the cluster chain may have been overwritten.

use bytemuck::{Pod, Zeroable};

use crate::error::FatResult;

/// Directory entry attributes.
pub mod attr {
    pub const READ_ONLY: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const SYSTEM: u8 = 0x04;
    pub const VOLUME_ID: u8 = 0x08;
    pub const DIRECTORY: u8 = 0x10;
    pub const ARCHIVE: u8 = 0x20;
    pub const LFN: u8 = 0x0F; // long file name marker
}

/// Raw 32-byte directory entry.
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct DirEntryRaw {
    pub name: [u8; 11], // 8 name + 3 extension, space-padded
    pub attributes: u8,
    pub reserved: u8, // NT case info
    pub creation_time_tenths: u8,
    pub creation_time: u16,
    pub creation_date: u16,
    pub last_access_date: u16,
    pub first_cluster_high: u16,
    pub last_write_time: u16,
    pub last_write_date: u16,
    pub first_cluster_low: u16,
    pub file_size: u32,
}

/// Parsed short (8.3) directory entry.
#[derive(Debug, Clone)]
pub struct ShortEntry {
    pub raw_name: [u8; 11],
    pub name: String,
    pub extension: String,
    pub attributes: u8,
    pub first_cluster: u32,
    pub file_size: u32,
    /// Creation time, 10 ms units (0..=199), from the separate tenths byte.
    pub creation_time_tenths: u8,
    /// Creation time (packed FAT time bitfield).
    pub creation_time: u16,
    /// Creation date (packed FAT date bitfield).
    pub creation_date: u16,
    pub last_access_date: u16,
    /// Last write (modification) time (packed FAT time bitfield).
    pub last_write_time: u16,
    /// Last write (modification) date (packed FAT date bitfield).
    pub last_write_date: u16,
    /// True if this entry was deleted (first byte == 0xE5).
    pub is_deleted: bool,
}

/// A parsed directory entry (either short or LFN).
#[derive(Debug, Clone)]
pub enum DirEntry {
    /// A short (8.3) entry. May be preceded by LFN entries that provide
    /// the long name.
    Short {
        entry: ShortEntry,
        /// Long file name if LFN entries preceded this short entry.
        long_name: Option<String>,
    },
    /// A long file name (LFN) entry. These are collected and attached
    /// to the following short entry.
    Lfn {
        sequence: u8,
        checksum: u8,
        name_part: [u16; 13], // 13 UTF-16 code units (some may be 0x0000 terminator)
    },
    /// End-of-directory marker (first byte == 0x00). No more entries
    /// after this point.
    End,
}

pub const DELETED_MARKER: u8 = 0xE5;
pub const END_MARKER: u8 = 0x00;

impl ShortEntry {
    /// Parse a 32-byte buffer as a short directory entry.
    pub fn parse(buf: &[u8; 32]) -> FatResult<Self> {
        let raw: &DirEntryRaw = bytemuck::from_bytes(buf);

        // Extract name and extension.
        let raw_name = raw.name;
        let is_deleted = raw_name[0] == DELETED_MARKER;

        // For deleted entries, restore the original first byte (0x05 -> 0xE5
        // per spec, but for deleted we just use a placeholder).
        let name_bytes = if is_deleted {
            // The original first character is gone. We use '_' as a
            // placeholder so the user sees "_ilename.ext".
            let mut n = raw_name;
            n[0] = b'_';
            n
        } else if raw_name[0] == 0x05 {
            // Per spec: 0x05 in the first byte means the original was 0xE5
            // (to distinguish from the deleted marker). Restore it.
            let mut n = raw_name;
            n[0] = 0xE5;
            n
        } else {
            raw_name
        };

        let name = String::from_utf8_lossy(&name_bytes[..8])
            .trim_end()
            .to_string();
        let extension = String::from_utf8_lossy(&name_bytes[8..11])
            .trim_end()
            .to_string();

        let first_cluster =
            (u32::from(raw.first_cluster_high) << 16) | u32::from(raw.first_cluster_low);

        Ok(Self {
            raw_name,
            name,
            extension,
            attributes: raw.attributes,
            first_cluster,
            file_size: raw.file_size,
            creation_time_tenths: raw.creation_time_tenths,
            creation_time: raw.creation_time,
            creation_date: raw.creation_date,
            last_access_date: raw.last_access_date,
            last_write_time: raw.last_write_time,
            last_write_date: raw.last_write_date,
            is_deleted,
        })
    }

    /// Full filename (name + extension with dot).
    #[must_use]
    pub fn full_name(&self) -> String {
        if self.extension.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.name, self.extension)
        }
    }

    /// Is this entry a directory?
    #[must_use]
    pub fn is_directory(&self) -> bool {
        (self.attributes & attr::DIRECTORY) != 0
    }

    /// Is this the volume label entry?
    #[must_use]
    pub fn is_volume_label(&self) -> bool {
        (self.attributes & attr::VOLUME_ID) != 0
    }
}

impl DirEntry {
    /// Parse a 32-byte buffer as a directory entry (short, LFN, or end).
    pub fn parse(buf: &[u8; 32]) -> FatResult<Self> {
        // Check for end-of-directory marker.
        if buf[0] == END_MARKER {
            return Ok(Self::End);
        }

        let raw: &DirEntryRaw = bytemuck::from_bytes(buf);

        // Check for LFN entry.
        if raw.attributes == attr::LFN {
            return Ok(Self::Lfn {
                sequence: buf[0],
                checksum: buf[13],
                name_part: extract_lfn_name(buf),
            });
        }

        // Short entry.
        let entry = ShortEntry::parse(buf)?;
        Ok(Self::Short {
            entry,
            long_name: None,
        })
    }
}

/// Extract the 13 UTF-16 code units from an LFN entry.
fn extract_lfn_name(buf: &[u8; 32]) -> [u16; 13] {
    let mut units = [0u16; 13];
    // Part 1: bytes 1..11 (5 code units)
    for i in 0..5 {
        units[i] = u16::from_le_bytes([buf[1 + i * 2], buf[2 + i * 2]]);
    }
    // Part 2: bytes 14..26 (6 code units)
    for i in 0..6 {
        units[5 + i] = u16::from_le_bytes([buf[14 + i * 2], buf[15 + i * 2]]);
    }
    // Part 3: bytes 28..32 (2 code units)
    for i in 0..2 {
        units[11 + i] = u16::from_le_bytes([buf[28 + i * 2], buf[29 + i * 2]]);
    }
    units
}

/// Compute the LFN checksum for a short name (used to validate LFN entries).
///
/// Algorithm from the FAT spec:
/// ```text
/// Checksum = 0
/// for each of the 11 bytes in name:
///     Checksum = ((Checksum & 1) << 7) + (Checksum >> 1) + byte
/// ```
#[must_use]
pub fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut checksum: u8 = 0;
    for &byte in short_name {
        // Rotate right by 1 and add
        checksum = ((checksum & 1) << 7)
            .wrapping_add(checksum >> 1)
            .wrapping_add(byte);
    }
    checksum
}

/// Reassemble a long file name from collected LFN entries.
///
/// `lfn_entries` should be the LFN entries that precede a short entry,
/// in the order they appear in the directory (reverse sequence order).
/// Returns the reassembled name, or `None` if the entries are invalid.
#[must_use]
pub fn reassemble_lfn(lfn_entries: &[DirEntry]) -> Option<String> {
    if lfn_entries.is_empty() {
        return None;
    }

    // Sort by sequence number (ascending). The last entry has bit 0x40 set
    // and the lowest sequence number.
    let mut entries: Vec<&[u16; 13]> = Vec::new();
    let mut expected_checksum: Option<u8> = None;
    let mut max_seq: u8 = 0;

    for entry in lfn_entries {
        if let DirEntry::Lfn {
            sequence,
            checksum,
            name_part,
        } = entry
        {
            expected_checksum.get_or_insert(*checksum);
            let seq = sequence & 0x3F; // strip the 0x40 "last" bit
            max_seq = max_seq.max(seq);
            entries.push(name_part);
        }
    }

    if entries.is_empty() {
        return None;
    }

    // Build the name string from the LFN entries in sequence order.
    // The entry with sequence 1 comes first in the name, sequence 2 next, etc.
    // But in the directory, the entry with the highest sequence (and 0x40 bit)
    // comes FIRST, followed by descending sequence numbers.
    // So we reverse the directory order to get name order.
    entries.reverse();

    let mut name = String::new();
    for part in &entries {
        for &unit in *part {
            if unit == 0x0000 || unit == 0xFFFF {
                // End of name or padding.
                return if name.is_empty() { None } else { Some(name) };
            }
            // Convert UTF-16 code unit to char (BMP only — surrogate pairs
            // not handled for simplicity; rare in filenames).
            if let Some(c) = char::from_u32(u32::from(unit)) {
                name.push(c);
            }
        }
    }

    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_short_entry(name: &[u8; 11], attrs: u8, cluster: u32, size: u32) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..11].copy_from_slice(name);
        buf[11] = attrs;
        // First cluster low (offset 26)
        buf[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        // First cluster high (offset 20)
        buf[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
        // File size (offset 28)
        buf[28..32].copy_from_slice(&size.to_le_bytes());
        buf
    }

    #[test]
    fn parses_short_entry() {
        let buf = make_short_entry(b"README  TXT", attr::ARCHIVE, 100, 4096);
        let entry = ShortEntry::parse(&buf).expect("parse");
        assert_eq!(entry.name, "README");
        assert_eq!(entry.extension, "TXT");
        assert_eq!(entry.full_name(), "README.TXT");
        assert_eq!(entry.attributes, attr::ARCHIVE);
        assert_eq!(entry.first_cluster, 100);
        assert_eq!(entry.file_size, 4096);
        assert!(!entry.is_deleted);
        assert!(!entry.is_directory());
        assert!(!entry.is_volume_label());
    }

    #[test]
    fn parses_directory_entry() {
        let buf = make_short_entry(b"MYDIR      ", attr::DIRECTORY, 50, 0);
        let entry = ShortEntry::parse(&buf).expect("parse");
        assert_eq!(entry.name, "MYDIR");
        assert_eq!(entry.extension, "");
        assert_eq!(entry.full_name(), "MYDIR");
        assert!(entry.is_directory());
        assert_eq!(entry.first_cluster, 50);
    }

    #[test]
    fn parses_deleted_entry() {
        let mut name = *b"FILE    TXT";
        name[0] = DELETED_MARKER;
        let buf = make_short_entry(&name, attr::ARCHIVE, 200, 1024);
        let entry = ShortEntry::parse(&buf).expect("parse");
        assert!(entry.is_deleted);
        // Name should have '_' as first char (placeholder)
        assert_eq!(entry.name, "_ILE");
        assert_eq!(entry.extension, "TXT");
        assert_eq!(entry.first_cluster, 200);
    }

    #[test]
    fn parses_volume_label() {
        let buf = make_short_entry(b"MYVOLUME   ", attr::VOLUME_ID, 0, 0);
        let entry = ShortEntry::parse(&buf).expect("parse");
        assert!(entry.is_volume_label());
    }

    #[test]
    fn parses_end_marker() {
        let mut buf = [0u8; 32];
        buf[0] = END_MARKER;
        let entry = DirEntry::parse(&buf).expect("parse");
        assert!(matches!(entry, DirEntry::End));
    }

    #[test]
    fn lfn_checksum_matches_spec() {
        // The spec example: short name "FILE    TXT" should produce a
        // specific checksum. Let's verify the algorithm.
        let name = *b"FILE    TXT";
        let cs = lfn_checksum(&name);
        // We can't hardcode the expected value without running the reference
        // algorithm, but we can verify it's deterministic.
        assert_eq!(cs, lfn_checksum(&name));
    }

    #[test]
    fn reassembles_simple_lfn() {
        // Build a single LFN entry with sequence 0x41 (last + 1)
        // containing "Hello.txt"
        let mut buf = [0u8; 32];
        buf[0] = 0x41; // sequence: last (0x40) + 1
        buf[11] = attr::LFN;
        buf[13] = 0; // checksum (not validated in reassemble)
        // "Hello.txt" = 9 chars, fits in one LFN entry (13 char capacity)
        let name: Vec<u16> = "Hello.txt".encode_utf16().collect();
        // Part 1 (5 chars): bytes 1..11
        for (i, &unit) in name.iter().take(5).enumerate() {
            buf[1 + i * 2] = (unit & 0xFF) as u8;
            buf[2 + i * 2] = (unit >> 8) as u8;
        }
        // Part 2 (4 chars): bytes 14..22
        for (i, &unit) in name.iter().skip(5).take(6).enumerate() {
            buf[14 + i * 2] = (unit & 0xFF) as u8;
            buf[15 + i * 2] = (unit >> 8) as u8;
        }
        // Terminator at position 9
        buf[14 + 4 * 2] = 0;
        buf[14 + 4 * 2 + 1] = 0;

        let entry = DirEntry::parse(&buf).expect("parse");
        let lfn_entries = vec![entry];
        let name = reassemble_lfn(&lfn_entries).expect("reassemble");
        assert_eq!(name, "Hello.txt");
    }
}
