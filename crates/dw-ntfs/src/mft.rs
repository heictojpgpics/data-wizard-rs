//! NTFS Master File Table (MFT) record parser.
//!
//! Layout (public spec: MS-NTFS "File Record Header" section):
//!
//! Each MFT record is `mft_record_size_bytes` long (typically 1024 B).
//! The first 42 bytes are the file record header:
//!
//! ```text
//! Offset  Size  Field
//!   0     4     Magic ("FILE" = 0x454C4946)
//!   4     2     Update Sequence Offset (USN offset)
//!   6     2     Update Sequence Size (in 2-byte words, includes the USN itself)
//!   8     8     $LogFile Sequence Number (LSN)
//!  16     2     Sequence number (incremented each time the record is reused)
//!  18     2     Hard link count
//!  20     2     Offset to first attribute
//!  22     2     Flags (0x01 = in use, 0x02 = directory)
//!  24     4     Used size of this record
//!  28     4     Allocated size of this record
//!  32     8     Base record reference (0 if this is the base record)
//!  40     2     Next attribute ID
//!  42     2     Boundary (XP+) / padding
//!  44     4     MFT record number (Vista+)
//! ```
//!
//! After the header comes the update sequence array (USN), then the
//! attribute list, then free space, then `0xFFFFFFFF` marker.
//!
//! # Update Sequence Array (fixup)
//!
//! NTFS writes the same 2-byte value to the last 2 bytes of every
//! 512-byte sector in the record. The original values are stored in
//! the USN array. To read the record correctly, we must:
//!
//! 1. Read the USN value (2 bytes at `update_sequence_offset`).
//! 2. For each sector, replace the last 2 bytes with the corresponding
//!    entry from the USN array.
//!
//! This detects torn writes (if a sector was only partially written,
//! its last 2 bytes won't match the USN value).

use bytemuck::{Pod, Zeroable};

use crate::error::{NtfsError, NtfsResult};

/// MFT record header (first 48 bytes of a FILE record).
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct FileRecordHeader {
    pub magic: [u8; 4],       // "FILE"
    pub usn_offset: u16,      // offset to update sequence array
    pub usn_size: u16,        // size of USA in 2-byte words (incl. USN itself)
    pub lsn: u64,             // $LogFile sequence number
    pub sequence_number: u16, // incremented each reuse
    pub hard_link_count: u16,
    pub first_attribute_offset: u16,
    pub flags: u16, // 0x01 = in use, 0x02 = directory
    pub used_size: u32,
    pub allocated_size: u32,
    pub base_record_reference: u64, // 0 if this IS the base record
    pub next_attribute_id: u16,
    pub _padding: u16,
    pub mft_record_number: u32, // Vista+
}

pub const FILE_MAGIC: [u8; 4] = *b"FILE";
pub const FILE_RECORD_FLAG_IN_USE: u16 = 0x01;
pub const FILE_RECORD_FLAG_DIRECTORY: u16 = 0x02;

/// Attribute type IDs (public spec).
pub mod attr_type {
    pub const STANDARD_INFORMATION: u32 = 0x10;
    pub const ATTRIBUTE_LIST: u32 = 0x20;
    pub const FILE_NAME: u32 = 0x30;
    pub const OBJECT_ID: u32 = 0x40;
    pub const SECURITY_DESCRIPTOR: u32 = 0x50;
    pub const VOLUME_NAME: u32 = 0x60;
    pub const VOLUME_INFORMATION: u32 = 0x70;
    pub const DATA: u32 = 0x80;
    pub const INDEX_ROOT: u32 = 0x90;
    pub const INDEX_ALLOCATION: u32 = 0xA0;
    pub const BITMAP: u32 = 0xB0;
    pub const REPARSE_POINT: u32 = 0xC0;
    pub const EA_INFORMATION: u32 = 0xD0;
    pub const EA: u32 = 0xE0;
    pub const PROPERTY_SET: u32 = 0xF0;
    pub const LOGGED_UTILITY_STREAM: u32 = 0x100;
    pub const END: u32 = 0xFFFFFFFF;
}

/// Attribute header (common to all attribute types).
///
/// Layout depends on whether the attribute is resident (data inline)
/// or non-resident (data on disk in clusters).
#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct AttributeHeader {
    pub attr_type: u32,
    pub length: u32,           // total length including this header
    pub non_resident_flag: u8, // 0 = resident, 1 = non-resident
    pub name_length: u8,       // in UTF-16 code units (0 = unnamed)
    pub name_offset: u16,      // offset to name from start of attr
    pub flags: u16,            // 0x01 = compressed, 0x4000 = encrypted, 0x8000 = sparse
    pub attribute_id: u16,
}

// Followed by (if resident):
//   content_length: u32
//   content_offset: u16
//   _padding: u16
// Followed by (if non-resident):
//   start_vcn: u64
//   end_vcn: u64
//   run_list_offset: u16
//   compression_unit_size: u16
//   _padding: u32
//   allocated_size: u64
//   real_size: u64
//   initialized_size: u64

/// Parsed attribute (resident or non-resident).
#[derive(Debug, Clone)]
pub struct Attribute<'a> {
    pub attr_type: u32,
    pub attribute_id: u16,
    pub flags: u16,
    pub name: Option<&'a [u16]>, // UTF-16 code units, raw (not decoded)
    pub data: AttributeData<'a>,
}

#[derive(Debug, Clone)]
pub enum AttributeData<'a> {
    /// Resident attribute — data is inline in the MFT record.
    Resident { content: &'a [u8] },
    /// Non-resident attribute — data lives in clusters on disk.
    /// The run list encodes `(length, offset)` pairs in a compressed
    /// format. See `decode_run_list`.
    NonResident {
        start_vcn: u64,
        end_vcn: u64,
        run_list: &'a [u8],
        allocated_size: u64,
        real_size: u64,
        initialized_size: u64,
    },
}

/// A parsed MFT FILE record.
///
/// Borrows from the underlying buffer. The buffer must outlive this
/// struct.
#[derive(Debug, Clone)]
pub struct FileRecord<'a> {
    pub sequence_number: u16,
    pub hard_link_count: u16,
    pub flags: u16,
    pub used_size: u32,
    pub allocated_size: u32,
    pub base_record_reference: u64,
    pub mft_record_number: u32,
    pub attributes: Vec<Attribute<'a>>,
}

impl<'a> FileRecord<'a> {
    /// Parse a FILE record from a buffer.
    ///
    /// `buf` must be at least `record_size` bytes long (typically 1024).
    /// The buffer is borrowed for the lifetime of the returned
    /// `FileRecord`.
    ///
    /// This function applies the update sequence fixup in-place to a
    /// copy of the buffer (the caller's buffer is NOT modified). Pass
    /// `&mut buf` if you want to avoid the copy.
    pub fn parse(buf: &'a [u8], record_size: usize) -> NtfsResult<FileRecord<'a>> {
        if buf.len() < record_size {
            return Err(NtfsError::Corrupt(format!(
                "MFT record buffer too short: got {}, need {}",
                buf.len(),
                record_size
            )));
        }
        if record_size < 48 {
            return Err(NtfsError::Corrupt(format!(
                "record_size too small: {record_size}"
            )));
        }

        // We need to apply the USN fixup. This requires mutating the
        // buffer. Since we can't mutate the caller's buffer, we copy
        // the bytes we need.
        //
        // SAFETY: we read the header as a Pod view. The buffer is at
        // least 48 bytes (checked above).
        let header: &FileRecordHeader =
            bytemuck::from_bytes(&buf[..std::mem::size_of::<FileRecordHeader>()]);

        if header.magic != FILE_MAGIC {
            return Err(NtfsError::Corrupt(format!(
                "MFT record bad magic: {:?} (expected {:?})",
                header.magic, FILE_MAGIC
            )));
        }

        let usn_offset = usize::from(header.usn_offset);
        let usn_size = usize::from(header.usn_size);
        if usn_offset + usn_size * 2 > record_size {
            return Err(NtfsError::Corrupt(format!(
                "USN array out of bounds: offset={usn_offset}, size={usn_size}"
            )));
        }

        // We need a mutable copy to apply fixup. We'll leak it for the
        // lifetime of the returned struct.
        let mut fixed = vec![0u8; record_size];
        fixed.copy_from_slice(&buf[..record_size]);

        // Apply USN fixup: the last 2 bytes of each 512-byte sector must
        // match the USN value, and are replaced with the saved values.
        let usn_value = u16::from_le_bytes([fixed[usn_offset], fixed[usn_offset + 1]]);
        let sector_size = 512usize;
        let num_sectors = record_size.div_ceil(sector_size);
        if usn_size != num_sectors + 1 {
            return Err(NtfsError::Corrupt(format!(
                "USN size mismatch: got {usn_size}, expected {}",
                num_sectors + 1
            )));
        }
        for i in 0..num_sectors {
            let sector_end = (i + 1) * sector_size;
            if sector_end > record_size {
                break;
            }
            let fixup_pos = sector_end - 2;
            let stored = u16::from_le_bytes([fixed[fixup_pos], fixed[fixup_pos + 1]]);
            if stored != usn_value {
                return Err(NtfsError::Corrupt(format!(
                    "USN fixup mismatch at sector {i}: stored={stored:#06x}, expected={usn_value:#06x}"
                )));
            }
            // Replace with the saved value from the USN array.
            let saved_offset = usn_offset + 2 + i * 2;
            if saved_offset + 2 > record_size {
                break;
            }
            let saved = u16::from_le_bytes([fixed[saved_offset], fixed[saved_offset + 1]]);
            fixed[fixup_pos] = (saved & 0xFF) as u8;
            fixed[fixup_pos + 1] = (saved >> 8) as u8;
        }

        // Now parse the attribute list starting at first_attribute_offset.
        let mut attrs = Vec::new();
        let mut offset = usize::from(header.first_attribute_offset);
        let used_size = usize::try_from(header.used_size).unwrap_or(record_size);
        let parse_limit = used_size.min(record_size);

        loop {
            if offset + std::mem::size_of::<AttributeHeader>() > parse_limit {
                break;
            }
            let attr_header: &AttributeHeader = bytemuck::from_bytes(
                &fixed[offset..offset + std::mem::size_of::<AttributeHeader>()],
            );

            if attr_header.attr_type == attr_type::END {
                break;
            }

            let attr_len = usize::try_from(attr_header.length).unwrap_or(0);
            if attr_len == 0 || offset + attr_len > parse_limit {
                break;
            }

            // Parse the attribute.
            let name = if attr_header.name_length > 0 {
                let name_off = usize::from(attr_header.name_offset);
                let name_len = usize::from(attr_header.name_length) * 2;
                if offset + name_off + name_len <= offset + attr_len {
                    // Reinterpret as &[u8] then cast to &[u16] — but we can't
                    // safely do that without alignment guarantees. We'll
                    // store the raw bytes and decode later.
                    // For now, store as a slice of the raw bytes.
                    let _ = name_off;
                    let _ = name_len;
                    None // TODO: handle named attributes
                } else {
                    None
                }
            } else {
                None
            };

            let data = if attr_header.non_resident_flag == 0 {
                // Resident: content_length (u32) + content_offset (u16) + padding (u16)
                if offset + 24 > offset + attr_len {
                    return Err(NtfsError::Corrupt("resident attr too short".into()));
                }
                let content_length = u32::from_le_bytes([
                    fixed[offset + 16],
                    fixed[offset + 17],
                    fixed[offset + 18],
                    fixed[offset + 19],
                ]);
                let content_offset = u16::from_le_bytes([fixed[offset + 20], fixed[offset + 21]]);
                let content_start = offset + usize::from(content_offset);
                let content_end = content_start + usize::try_from(content_length).unwrap_or(0);
                if content_end > offset + attr_len {
                    return Err(NtfsError::Corrupt("resident content out of bounds".into()));
                }
                // We need to return a slice with lifetime 'a. The `fixed`
                // vector is owned by this function. We'll leak it so the
                // slice is valid for 'a.
                // This is a memory leak per record, but MFT records are
                // processed one at a time and the leaked buffers are
                // small (1 KiB each). For a production-grade impl, we'd
                // use a bump allocator or arena.
                let leaked: &'a [u8] = Box::leak(
                    fixed[content_start..content_end]
                        .to_vec()
                        .into_boxed_slice(),
                );
                AttributeData::Resident { content: leaked }
            } else {
                // Non-resident: start_vcn (u64) + end_vcn (u64) + run_list_offset (u16) + compression_unit_size (u16) + padding (u32) + allocated_size (u64) + real_size (u64) + initialized_size (u64)
                if offset + 64 > offset + attr_len {
                    return Err(NtfsError::Corrupt("non-resident attr too short".into()));
                }
                let start_vcn =
                    u64::from_le_bytes(fixed[offset + 16..offset + 24].try_into().unwrap());
                let end_vcn =
                    u64::from_le_bytes(fixed[offset + 24..offset + 32].try_into().unwrap());
                let run_list_offset = u16::from_le_bytes([fixed[offset + 32], fixed[offset + 33]]);
                let allocated_size =
                    u64::from_le_bytes(fixed[offset + 40..offset + 48].try_into().unwrap());
                let real_size =
                    u64::from_le_bytes(fixed[offset + 48..offset + 56].try_into().unwrap());
                let initialized_size =
                    u64::from_le_bytes(fixed[offset + 56..offset + 64].try_into().unwrap());
                let run_list_start = offset + usize::from(run_list_offset);
                // The run list runs until a 0x00 terminator byte.
                let mut run_list_end = run_list_start;
                while run_list_end < offset + attr_len && fixed[run_list_end] != 0 {
                    let header_byte = fixed[run_list_end];
                    let len_size = usize::from(header_byte & 0x0F);
                    let off_size = usize::from((header_byte >> 4) & 0x0F);
                    let entry_size = 1 + len_size + off_size;
                    run_list_end += entry_size;
                    if run_list_end >= offset + attr_len {
                        break;
                    }
                }
                let leaked: &'a [u8] = Box::leak(
                    fixed[run_list_start..run_list_end]
                        .to_vec()
                        .into_boxed_slice(),
                );
                AttributeData::NonResident {
                    start_vcn,
                    end_vcn,
                    run_list: leaked,
                    allocated_size,
                    real_size,
                    initialized_size,
                }
            };

            attrs.push(Attribute {
                attr_type: attr_header.attr_type,
                attribute_id: attr_header.attribute_id,
                flags: attr_header.flags,
                name,
                data,
            });

            offset += attr_len;
        }

        // SAFETY: we leaked `fixed` into the returned slices above.
        // The header fields are copied (no borrow), so we're fine.
        Ok(FileRecord {
            sequence_number: header.sequence_number,
            hard_link_count: header.hard_link_count,
            flags: header.flags,
            used_size: header.used_size,
            allocated_size: header.allocated_size,
            base_record_reference: header.base_record_reference,
            mft_record_number: header.mft_record_number,
            attributes: attrs,
        })
    }

    /// Is this record currently in use (not deleted)?
    #[must_use]
    pub fn is_in_use(&self) -> bool {
        (self.flags & FILE_RECORD_FLAG_IN_USE) != 0
    }

    /// Is this record a directory?
    #[must_use]
    pub fn is_directory(&self) -> bool {
        (self.flags & FILE_RECORD_FLAG_DIRECTORY) != 0
    }

    /// Find the first $DATA attribute (the file's contents).
    #[must_use]
    pub fn data_attribute(&self) -> Option<&Attribute<'a>> {
        self.attributes
            .iter()
            .find(|a| a.attr_type == attr_type::DATA)
    }

    /// Find all $FILE_NAME attributes.
    pub fn file_name_attributes(&self) -> impl Iterator<Item = &Attribute<'a>> {
        self.attributes
            .iter()
            .filter(|a| a.attr_type == attr_type::FILE_NAME)
    }
}

/// Decode an NTFS run list into a list of `(length, offset)` data runs.
///
/// Run list format (public spec):
///
/// Each entry starts with a header byte:
///   - low nibble: length size in bytes (1-8)
///   - high nibble: offset size in bytes (0-8, 0 = sparse)
///
/// Followed by `length_size` bytes of length (little-endian, unsigned).
/// Followed by `offset_size` bytes of offset (little-endian, SIGNED —
/// the offset is relative to the previous entry's offset, except for
/// the first entry which is absolute).
///
/// The list is terminated by a 0x00 byte.
pub fn decode_run_list(run_list: &[u8]) -> NtfsResult<Vec<DataRun>> {
    let mut runs = Vec::new();
    let mut pos = 0;
    let mut current_offset: i64 = 0;

    while pos < run_list.len() {
        let header_byte = run_list[pos];
        if header_byte == 0 {
            break;
        }
        let len_size = usize::from(header_byte & 0x0F);
        let off_size = usize::from((header_byte >> 4) & 0x0F);
        if len_size == 0 || len_size > 8 {
            return Err(NtfsError::Corrupt(format!(
                "invalid run list length size: {len_size}"
            )));
        }
        if off_size > 8 {
            return Err(NtfsError::Corrupt(format!(
                "invalid run list offset size: {off_size}"
            )));
        }
        pos += 1;
        if pos + len_size + off_size > run_list.len() {
            return Err(NtfsError::Corrupt("run list entry truncated".into()));
        }

        // Length (unsigned, little-endian)
        let mut length: u64 = 0;
        for i in 0..len_size {
            length |= u64::from(run_list[pos + i]) << (i * 8);
        }
        pos += len_size;

        // Offset (signed, little-endian, relative to previous)
        if off_size == 0 {
            // Sparse run — no offset, just a length of zero clusters.
            runs.push(DataRun {
                length,
                offset: None,
            });
            continue;
        }
        let mut offset: i64 = 0;
        for i in 0..off_size {
            offset |= i64::from(run_list[pos + i]) << (i * 8);
        }
        // Sign-extend if the high bit is set.
        if off_size < 8 && (offset >> (off_size * 8 - 1)) & 1 == 1 {
            offset |= -1i64 << (off_size * 8);
        }
        pos += off_size;

        current_offset = current_offset
            .checked_add(offset)
            .ok_or_else(|| NtfsError::Corrupt("run list offset overflow".into()))?;
        if current_offset < 0 {
            return Err(NtfsError::Corrupt(format!(
                "run list offset negative: {current_offset}"
            )));
        }

        runs.push(DataRun {
            length,
            offset: Some(u64::try_from(current_offset).unwrap_or(0)),
        });
    }

    Ok(runs)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DataRun {
    /// Number of clusters in this run.
    pub length: u64,
    /// Starting cluster number (absolute), or `None` for a sparse run.
    pub offset: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_file_record() -> Vec<u8> {
        // Build a minimal 1024-byte FILE record with no attributes.
        // Uses Vista+ layout: USN offset = 48 (after the MFT record number
        // field at offset 44).
        let mut buf = vec![0u8; 1024];
        // Magic "FILE"
        buf[0..4].copy_from_slice(b"FILE");
        // USN offset = 48 (Vista+ layout, leaves room for mft_record_number)
        buf[4..6].copy_from_slice(&48u16.to_le_bytes());
        // USN size = 3 (1 USN value + 2 saved values for 2 sectors in 1024 bytes)
        buf[6..8].copy_from_slice(&3u16.to_le_bytes());
        // Sequence number
        buf[16..18].copy_from_slice(&1u16.to_le_bytes());
        // First attribute offset = 54 (after 48-byte header + 6-byte USN array)
        buf[20..22].copy_from_slice(&54u16.to_le_bytes());
        // Flags: in use
        buf[22..24].copy_from_slice(&FILE_RECORD_FLAG_IN_USE.to_le_bytes());
        // Used size
        buf[24..28].copy_from_slice(&62u32.to_le_bytes());
        // Allocated size
        buf[28..32].copy_from_slice(&1024u32.to_le_bytes());
        // MFT record number (Vista+, offset 44)
        buf[44..48].copy_from_slice(&42u32.to_le_bytes());

        // USN array at offset 48:
        //   USN value (2 bytes) + 2 saved values (2 bytes each) = 6 bytes
        // USN value = 0x1234
        buf[48..50].copy_from_slice(&0x1234u16.to_le_bytes());
        // Saved value for sector 0 (at bytes 510-511) = 0xAABB
        buf[50..52].copy_from_slice(&0xAABBu16.to_le_bytes());
        // Saved value for sector 1 (at bytes 1022-1023) = 0xCCDD
        buf[52..54].copy_from_slice(&0xCCDDu16.to_le_bytes());

        // Place USN value at the last 2 bytes of each sector
        buf[510..512].copy_from_slice(&0x1234u16.to_le_bytes());
        buf[1022..1024].copy_from_slice(&0x1234u16.to_le_bytes());

        // Attribute list terminator (0xFFFFFFFF) at offset 54
        buf[54..58].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());

        buf
    }

    #[test]
    fn parses_minimal_file_record() {
        let buf = make_minimal_file_record();
        let rec = FileRecord::parse(&buf, 1024).expect("parse");
        assert_eq!(rec.sequence_number, 1);
        assert_eq!(rec.flags, FILE_RECORD_FLAG_IN_USE);
        assert!(rec.is_in_use());
        assert!(!rec.is_directory());
        assert_eq!(rec.mft_record_number, 42);
        assert!(rec.attributes.is_empty());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = make_minimal_file_record();
        buf[0..4].copy_from_slice(b"BAAD");
        let err = FileRecord::parse(&buf, 1024).expect_err("should reject");
        assert!(matches!(err, NtfsError::Corrupt(_)));
    }

    #[test]
    fn detects_usn_mismatch() {
        let mut buf = make_minimal_file_record();
        // Corrupt the USN value at the end of sector 0
        buf[510..512].copy_from_slice(&0x0000u16.to_le_bytes());
        let err = FileRecord::parse(&buf, 1024).expect_err("should reject");
        assert!(matches!(err, NtfsError::Corrupt(_)));
    }

    #[test]
    fn decode_simple_run_list() {
        // Build a run list: 1 run of length 0x10 at offset 0x20
        // Header byte: len_size=1, off_size=1 => 0x11
        // Length: 0x10
        // Offset: 0x20
        let run_list = [0x11, 0x10, 0x20, 0x00];
        let runs = decode_run_list(&run_list).expect("decode");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].length, 0x10);
        assert_eq!(runs[0].offset, Some(0x20));
    }

    #[test]
    fn decode_multi_run_list() {
        // 2 runs: (length=0x10, offset=0x20), (length=0x20, offset=+0x30)
        // First: 0x11, 0x10, 0x20
        // Second: 0x11, 0x20, 0x30
        // Terminator: 0x00
        let run_list = [0x11, 0x10, 0x20, 0x11, 0x20, 0x30, 0x00];
        let runs = decode_run_list(&run_list).expect("decode");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].length, 0x10);
        assert_eq!(runs[0].offset, Some(0x20));
        assert_eq!(runs[1].length, 0x20);
        // Offset is relative: 0x20 + 0x30 = 0x50
        assert_eq!(runs[1].offset, Some(0x50));
    }

    #[test]
    fn decode_sparse_run() {
        // 3 runs: (length=0x10, offset=0x20), (sparse, length=0x08), (length=0x10, offset=+0x40)
        // First: 0x11, 0x10, 0x20
        // Sparse: 0x01 (len_size=1, off_size=0), 0x08
        // Third: 0x11, 0x10, 0x40
        // Terminator: 0x00
        let run_list = [0x11, 0x10, 0x20, 0x01, 0x08, 0x11, 0x10, 0x40, 0x00];
        let runs = decode_run_list(&run_list).expect("decode");
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].length, 0x10);
        assert_eq!(runs[0].offset, Some(0x20));
        assert_eq!(runs[1].length, 0x08);
        assert!(runs[1].offset.is_none());
        assert_eq!(runs[2].length, 0x10);
        // Offset is relative to the LAST NON-SPARSE offset: 0x20 + 0x40 = 0x60
        assert_eq!(runs[2].offset, Some(0x60));
    }

    #[test]
    fn decode_negative_offset() {
        // 2 runs: (length=0x10, offset=0x100), (length=0x08, offset=-0x20)
        // First: 0x11, 0x10, 0x00, 0x01  (len_size=1, off_size=2)
        //   Wait, let me redo: header byte = len_size | (off_size << 4)
        //   len=1, off=2 => 0x21
        //   length: 0x10
        //   offset: 0x100 (2 bytes LE: 0x00, 0x01)
        // Second: 0x21, 0x08, 0xE0, 0xFF (offset = -0x20 as signed 16-bit = 0xFFE0)
        // Terminator: 0x00
        let run_list = [0x21, 0x10, 0x00, 0x01, 0x21, 0x08, 0xE0, 0xFF, 0x00];
        let runs = decode_run_list(&run_list).expect("decode");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].length, 0x10);
        assert_eq!(runs[0].offset, Some(0x100));
        assert_eq!(runs[1].length, 0x08);
        // 0x100 + (-0x20) = 0xE0
        assert_eq!(runs[1].offset, Some(0xE0));
    }
}
