//! `$FILE_NAME` attribute parser.
//!
//! Layout (public spec: MS-NTFS "$FILE_NAME (0x30)" section):
//!
//! The attribute's resident content (after the common attribute header)
//! has the following layout:
//!
//! ```text
//! Offset  Size  Field
//!   0     8     Parent directory reference (low 6 bytes = MFT record number,
//!                high 2 bytes = sequence number)
//!   8     8     Creation time (FILETIME, 100ns intervals since 1601-01-01)
//!  16     8     Modification time (FILETIME)
//!  24     8     MFT change time (FILETIME)
//!  32     8     Read time (FILETIME)
//!  40     8     Allocated size (bytes)
//!  48     8     Real size (bytes)
//!  56     4     Flags (0x01 = read-only, 0x02 = hidden, etc. — same as
//!                short dir entry attributes; 0x1000 = directory)
//!  60     4     Reparse value (0 unless reparse point)
//!  64     1     Name length (in UTF-16 code units, NOT bytes)
//!  65     1     Namespace (0 = POSIX, 1 = Win32, 2 = DOS, 3 = Win32+DOS)
//!  66     2*N   Name (N UTF-16 LE code units)
//! ```
//!
//! # Namespace
//!
//! NTFS stores up to 3 names per file: a POSIX name (case-sensitive),
//! a Win32 name (case-insensitive), and a DOS name (8.3). Most files
//! have only one (Win32) or two (Win32 + DOS). The namespace byte tells
//! us which.

use time::OffsetDateTime;

use crate::error::{NtfsError, NtfsResult};

/// Namespace constants (public spec).
pub mod namespace {
    pub const POSIX: u8 = 0x00;
    pub const WIN32: u8 = 0x01;
    pub const DOS: u8 = 0x02;
    pub const WIN32_AND_DOS: u8 = 0x03;
}

/// Parsed `$FILE_NAME` attribute content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    /// Parent directory MFT record number (low 48 bits of the 64-bit
    /// parent reference).
    pub parent_record_number: u64,
    /// Parent directory sequence number (high 16 bits).
    pub parent_sequence_number: u16,
    pub creation_time: Option<OffsetDateTime>,
    pub modification_time: Option<OffsetDateTime>,
    pub mft_change_time: Option<OffsetDateTime>,
    pub read_time: Option<OffsetDateTime>,
    pub allocated_size: u64,
    pub real_size: u64,
    pub flags: u32,
    pub reparse_value: u32,
    /// Name length in UTF-16 code units.
    pub name_length: u8,
    pub namespace: u8,
    /// The file name, decoded from UTF-16 LE to a Rust `String`.
    /// Unpaired surrogates are replaced with U+FFFD.
    pub name: String,
}

impl FileName {
    /// Parse the resident content of a `$FILE_NAME` attribute.
    ///
    /// `content` is the bytes after the common attribute header — i.e.
    /// what `Attribute::resident_content()` returns for a `$FILE_NAME`
    /// attribute.
    pub fn parse(content: &[u8]) -> NtfsResult<Self> {
        if content.len() < 66 {
            return Err(NtfsError::Corrupt(format!(
                "$FILE_NAME content too short: got {}, need 66",
                content.len()
            )));
        }

        let parent_reference = u64::from_le_bytes(content[0..8].try_into().unwrap());
        let parent_record_number = parent_reference & 0x0000_FFFF_FFFF_FFFF;
        let parent_sequence_number = (parent_reference >> 48) as u16;

        let creation_time =
            filetime_to_datetime(u64::from_le_bytes(content[8..16].try_into().unwrap()));
        let modification_time =
            filetime_to_datetime(u64::from_le_bytes(content[16..24].try_into().unwrap()));
        let mft_change_time =
            filetime_to_datetime(u64::from_le_bytes(content[24..32].try_into().unwrap()));
        let read_time =
            filetime_to_datetime(u64::from_le_bytes(content[32..40].try_into().unwrap()));

        let allocated_size = u64::from_le_bytes(content[40..48].try_into().unwrap());
        let real_size = u64::from_le_bytes(content[48..56].try_into().unwrap());
        let flags = u32::from_le_bytes(content[56..60].try_into().unwrap());
        let reparse_value = u32::from_le_bytes(content[60..64].try_into().unwrap());

        let name_length = content[64];
        let namespace = content[65];

        let name_bytes_needed = usize::from(name_length) * 2;
        if content.len() < 66 + name_bytes_needed {
            return Err(NtfsError::Corrupt(format!(
                "$FILE_NAME name truncated: need {name_bytes_needed} bytes, have {}",
                content.len() - 66
            )));
        }

        let name: String = {
            let mut units = Vec::with_capacity(name_bytes_needed / 2);
            let name_bytes = &content[66..66 + name_bytes_needed];
            let mut i = 0;
            while i + 1 < name_bytes.len() {
                units.push(u16::from_le_bytes([name_bytes[i], name_bytes[i + 1]]));
                i += 2;
            }
            units
                .iter()
                .map(|&unit| char::from_u32(u32::from(unit)).unwrap_or('\u{FFFD}'))
                .collect()
        };

        Ok(Self {
            parent_record_number,
            parent_sequence_number,
            creation_time,
            modification_time,
            mft_change_time,
            read_time,
            allocated_size,
            real_size,
            flags,
            reparse_value,
            name_length,
            namespace,
            name,
        })
    }

    /// Is this a DOS (8.3) namespace name? These are auto-generated
    /// short names; we usually want the Win32 name instead.
    #[must_use]
    pub fn is_dos_namespace(&self) -> bool {
        self.namespace == namespace::DOS
    }

    /// Is this a Win32 namespace name? This is the "normal" long name.
    #[must_use]
    pub fn is_win32_namespace(&self) -> bool {
        self.namespace == namespace::WIN32 || self.namespace == namespace::WIN32_AND_DOS
    }

    /// Is the parent directory the root directory (MFT record 5)?
    #[must_use]
    pub fn is_in_root(&self) -> bool {
        self.parent_record_number == 5
    }
}

/// Convert a Windows FILETIME (100ns intervals since 1601-01-01 UTC) to
/// an `OffsetDateTime`.
///
/// FILETIME epoch (1601-01-01) is 11644473600 seconds before Unix epoch
/// (1970-01-01). A value of 0 means "no time set" per NTFS convention.
pub fn filetime_to_datetime(filetime: u64) -> Option<OffsetDateTime> {
    if filetime == 0 {
        return None;
    }
    // 100ns intervals since 1601-01-01 → nanoseconds since 1601-01-01
    let nanos_since_1601 = u128::from(filetime) * 100;
    // Offset from 1601-01-01 to 1970-01-01 in nanoseconds
    const EPOCH_OFFSET_NANOS: u128 = 11_644_473_600 * 1_000_000_000;
    if nanos_since_1601 < EPOCH_OFFSET_NANOS {
        // Before 1970 — rare but valid
        return None;
    }
    let nanos_since_1970 = nanos_since_1600_to_unix_nanos(nanos_since_1601);
    let secs = (nanos_since_1970 / 1_000_000_000) as i64;
    let nanos = (nanos_since_1970 % 1_000_000_000) as u32;
    // Build the datetime directly from a Unix timestamp.
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(secs) * 1_000_000_000 + i128::from(nanos))
        .ok()
}

fn nanos_since_1600_to_unix_nanos(nanos_since_1601: u128) -> u128 {
    const EPOCH_OFFSET_NANOS: u128 = 11_644_473_600 * 1_000_000_000;
    nanos_since_1601 - EPOCH_OFFSET_NANOS
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn make_file_name_content(name: &str, parent_record: u64) -> Vec<u8> {
        let name_units: Vec<u16> = name.encode_utf16().collect();
        let mut buf = Vec::with_capacity(66 + name_units.len() * 2);

        // Parent reference (8 bytes)
        buf.extend_from_slice(&parent_record.to_le_bytes());

        // Times (8 bytes each, all zeros = "not set")
        buf.extend_from_slice(&0u64.to_le_bytes()); // creation
        buf.extend_from_slice(&0u64.to_le_bytes()); // modification
        buf.extend_from_slice(&0u64.to_le_bytes()); // mft change
        buf.extend_from_slice(&0u64.to_le_bytes()); // read

        // Sizes
        buf.extend_from_slice(&0u64.to_le_bytes()); // allocated
        buf.extend_from_slice(&0u64.to_le_bytes()); // real

        // Flags + reparse
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Name length (in code units)
        buf.push(name_units.len() as u8);
        // Namespace: Win32
        buf.push(namespace::WIN32);

        // Name (UTF-16 LE)
        for unit in name_units {
            buf.extend_from_slice(&unit.to_le_bytes());
        }

        buf
    }

    #[test]
    fn parses_simple_file_name() {
        let content = make_file_name_content("README.txt", 5);
        let fn_attr = FileName::parse(&content).expect("parse");
        assert_eq!(fn_attr.name, "README.txt");
        assert_eq!(fn_attr.parent_record_number, 5);
        assert_eq!(fn_attr.namespace, namespace::WIN32);
        assert!(fn_attr.is_win32_namespace());
        assert!(!fn_attr.is_dos_namespace());
        assert!(fn_attr.is_in_root());
        assert_eq!(fn_attr.name_length, 10);
    }

    #[test]
    fn parses_unicode_name() {
        let content = make_file_name_content("héllo 世界.txt", 42);
        let fn_attr = FileName::parse(&content).expect("parse");
        assert_eq!(fn_attr.name, "héllo 世界.txt");
        assert_eq!(fn_attr.parent_record_number, 42);
        assert!(!fn_attr.is_in_root());
    }

    #[test]
    fn rejects_too_short_content() {
        let content = vec![0u8; 60];
        let err = FileName::parse(&content).expect_err("should reject");
        assert!(matches!(err, NtfsError::Corrupt(_)));
    }

    #[test]
    fn handles_truncated_name() {
        let mut content = make_file_name_content("README.txt", 5);
        // Truncate the name bytes
        content.truncate(70);
        let err = FileName::parse(&content).expect_err("should reject");
        assert!(matches!(err, NtfsError::Corrupt(_)));
    }

    #[test]
    fn filetime_zero_is_none() {
        assert!(filetime_to_datetime(0).is_none());
    }

    #[test]
    fn filetime_known_value() {
        // FILETIME for 2020-01-01T00:00:00Z
        // 2020-01-01 is 50 years (and a bit) after 1970-01-01.
        // Unix timestamp: 1577836800
        // FILETIME = (unix_ts + 11644473600) * 10_000_000
        let unix_ts = 1_577_836_800i64;
        let filetime = (unix_ts + 11_644_473_600) as u64 * 10_000_000;
        let dt = filetime_to_datetime(filetime).expect("some datetime");
        assert_eq!(dt.year(), 2020);
        assert_eq!(dt.month(), Month::January);
        assert_eq!(dt.day(), 1);
    }
}
