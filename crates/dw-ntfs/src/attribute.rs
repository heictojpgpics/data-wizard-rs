//! `$ATTRIBUTE_LIST` handling (MS-NTFS §2.3, "AttributeList").
//!
//! When a file's attributes do not fit in one MFT record (typically a
//! heavily fragmented large file), the base record carries an
//! `$ATTRIBUTE_LIST` enumerating every attribute and the MFT record it
//! lives in. The base record itself then holds only the first part of
//! `$DATA`; the continuation run lists live in *extension records*.
//!
//! Ignoring the list truncates such files to their first record's
//! coverage — exactly the class of "large file recovers partially"
//! bug this module exists to prevent.
//!
//! Entry layout (26 bytes + name, packed):
//! ```text
//!  0  type          u32   attribute type the entry points at
//!  4  length        u16   length of this entry
//!  6  name_length   u8    name length in UTF-16 chars
//!  7  name_offset   u8    offset of the name from the entry start
//!  8  lowest_vcn    u64   lowest VCN this part covers
//! 16  mft_reference u64   low 48 bits: MFT record number; high 16:
//!                        sequence number
//! 24  attribute_id  u16
//! ```

use crate::error::{NtfsError, NtfsResult};
use crate::mft::{AttributeData, DataRun, FileRecord, attr_type, decode_run_list};
use dw_disk::ReadAt;

/// One `$ATTRIBUTE_LIST` entry.
#[derive(Debug, Clone)]
pub struct AttributeListEntry {
    pub attr_type: u32,
    pub lowest_vcn: u64,
    /// MFT record number the attribute lives in (low 48 bits of the
    /// reference).
    pub record_number: u64,
    /// Sequence part of the reference (upper 16 bits).
    pub sequence: u16,
    pub attribute_id: u16,
    /// Attribute name, when the entry is named.
    pub name: Option<Vec<u16>>,
}

/// Parse the (resident) content of an `$ATTRIBUTE_LIST` attribute.
pub fn parse_attribute_list(content: &[u8]) -> NtfsResult<Vec<AttributeListEntry>> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 26 <= content.len() {
        let attr_type = u32::from_le_bytes(
            content[off..off + 4]
                .try_into()
                .map_err(|_| NtfsError::Corrupt("list type".into()))?,
        );
        if attr_type == attr_type::END {
            break;
        }
        let length = u16::from_le_bytes([content[off + 4], content[off + 5]]) as usize;
        if length < 26 || off + length > content.len() {
            return Err(NtfsError::Corrupt(format!(
                "attribute-list entry length {length} invalid at {off}"
            )));
        }
        let name_length = content[off + 6] as usize;
        let name_offset = content[off + 7] as usize;
        let lowest_vcn = u64::from_le_bytes(
            content[off + 8..off + 16]
                .try_into()
                .map_err(|_| NtfsError::Corrupt("list vcn".into()))?,
        );
        let mft_reference = u64::from_le_bytes(
            content[off + 16..off + 24]
                .try_into()
                .map_err(|_| NtfsError::Corrupt("list ref".into()))?,
        );
        let attribute_id = u16::from_le_bytes([content[off + 24], content[off + 25]]);
        let name = if name_length > 0 && name_offset >= 8 && name_offset + name_length * 2 <= length
        {
            let mut chars = Vec::with_capacity(name_length);
            for i in 0..name_length {
                let at = off + name_offset + i * 2;
                chars.push(u16::from_le_bytes([content[at], content[at + 1]]));
            }
            Some(chars)
        } else {
            None
        };
        out.push(AttributeListEntry {
            attr_type,
            lowest_vcn,
            record_number: mft_reference & 0x0000_FFFF_FFFF_FFFF,
            sequence: (mft_reference >> 48) as u16,
            attribute_id,
            name,
        });
        off += length;
    }
    Ok(out)
}

/// Read one MFT record's raw bytes through the run-position index
/// (the same mapping `walk_mft` uses: `run_boundaries` maps MFT byte
/// ranges to disk offsets, `mft_offset` is the $MFT start LBA byte).
pub fn read_mft_record(
    read_at: &dyn ReadAt,
    run_boundaries: &[(u64, u64, u64)],
    mft_offset: u64,
    record_size: usize,
    record_number: u64,
) -> Option<Vec<u8>> {
    let mft_byte = record_number * record_size as u64;
    let (.., disk) = run_boundaries
        .iter()
        .find(|(start, end, _)| mft_byte >= *start && mft_byte < *end)?;
    let mut buf = vec![0u8; record_size];
    read_at.read_at(disk + mft_offset, &mut buf).ok()?;
    Some(buf)
}

/// The merged result of following an `$ATTRIBUTE_LIST`.
#[derive(Debug, Clone)]
pub struct MergedData {
    /// Runs from every part, in VCN order.
    pub runs: Vec<DataRun>,
    /// The attribute's whole size (max across parts; NTFS repeats it).
    pub real_size: u64,
    /// How many extension records were followed.
    pub extension_records: usize,
}

/// Follow a base record's `$ATTRIBUTE_LIST` and merge every unnamed
/// `$DATA` part into one run list.
///
/// Only entries for the unnamed `$DATA` (type 0x80, no name) are
/// followed — streams (named $DATA) stay with the contiguous tier;
/// their names are surfaced but their content recovery is a separate
/// concern. The base record's own run list (covering VCN 0) comes
/// first; extension parts follow sorted by `lowest_vcn`.
pub fn merge_attribute_list_data(
    base: &FileRecord,
    list: &[AttributeListEntry],
    read_at: &dyn ReadAt,
    run_boundaries: &[(u64, u64, u64)],
    mft_offset: u64,
    record_size: usize,
) -> NtfsResult<MergedData> {
    let mut merged = MergedData {
        runs: Vec::new(),
        real_size: 0,
        extension_records: 0,
    };

    // The base record's own unnamed $DATA (VCN 0 part), when present.
    if let Some(attr) = base.data_attribute()
        && let AttributeData::NonResident {
            run_list,
            real_size,
            ..
        } = &attr.data
    {
        merged.runs = decode_run_list(run_list)?;
        merged.real_size = *real_size;
    }

    // Extension parts, in VCN order.
    let mut parts: Vec<&AttributeListEntry> = list
        .iter()
        .filter(|e| e.attr_type == attr_type::DATA && e.name.is_none() && e.lowest_vcn > 0)
        .collect();
    parts.sort_by_key(|e| e.lowest_vcn);

    for entry in parts {
        let Some(buf) = read_mft_record(
            read_at,
            run_boundaries,
            mft_offset,
            record_size,
            entry.record_number,
        ) else {
            continue; // unreadable extension: keep what we have
        };
        let Ok(record) = FileRecord::parse(&buf, record_size) else {
            continue;
        };
        // The extension must point back at our base record.
        if record.base_record_reference & 0x0000_FFFF_FFFF_FFFF != u64::from(base.mft_record_number)
        {
            continue;
        }
        let Some(attr) = record
            .attributes
            .iter()
            .find(|a| a.attr_type == attr_type::DATA && a.name.is_none())
        else {
            continue;
        };
        let AttributeData::NonResident {
            run_list,
            real_size,
            start_vcn,
            ..
        } = &attr.data
        else {
            continue;
        };
        if *start_vcn != entry.lowest_vcn {
            continue; // inconsistent with the list: refuse the part
        }
        let runs = decode_run_list(run_list)?;
        merged.runs.extend(runs);
        merged.real_size = merged.real_size.max(*real_size);
        merged.extension_records += 1;
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parses_entries_with_names() {
        // Two entries: an unnamed $DATA part at VCN 0x10 in record 0x33,
        // and a $FILE_NAME in record 5.
        let mut v = Vec::new();
        let mut push_entry = |ty: u32, vcn: u64, mft_ref: u64, name: Option<&str>| {
            let name_len = name.map(|n| n.encode_utf16().count()).unwrap_or(0);
            let name_off = if name_len > 0 { 26u8 } else { 0u8 };
            let len = 26 + name_len * 2;
            v.extend_from_slice(&ty.to_le_bytes());
            v.extend_from_slice(&(len as u16).to_le_bytes());
            v.push(name_len as u8);
            v.push(name_off);
            v.extend_from_slice(&vcn.to_le_bytes());
            v.extend_from_slice(&mft_ref.to_le_bytes());
            v.extend_from_slice(&7u16.to_le_bytes());
            if let Some(n) = name {
                for c in n.encode_utf16() {
                    v.extend_from_slice(&c.to_le_bytes());
                }
            }
        };
        push_entry(0x80, 0x10, 0x33, None);
        push_entry(0x30, 0, 5, Some("x"));
        push_entry(0xFFFFFFFF, 0, 0, None); // end

        let list = parse_attribute_list(&v).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].attr_type, 0x80);
        assert_eq!(list[0].lowest_vcn, 0x10);
        assert_eq!(list[0].record_number, 0x33);
        assert!(list[0].name.is_none());
        assert_eq!(list[1].attr_type, 0x30);
        assert_eq!(
            list[1].name.as_deref().map(|n| n.to_vec()),
            Some(vec![u16::from(b'x')])
        );
    }

    #[test]
    fn rejects_truncated_entries() {
        assert!(parse_attribute_list(&[0u8; 10]).unwrap().is_empty());
        // Entry claims 100 bytes but only 26 exist.
        let mut v = vec![0u8; 26];
        v[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        v[4..6].copy_from_slice(&100u16.to_le_bytes());
        assert!(parse_attribute_list(&v).is_err());
    }

    #[test]
    fn mft_reference_splits_record_and_sequence() {
        let mut v = vec![0u8; 26];
        v[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        v[4..6].copy_from_slice(&26u16.to_le_bytes());
        // record 0x1234, sequence 0x0002
        let reference: u64 = 0x0002_0000_0000_1234;
        v[16..24].copy_from_slice(&reference.to_le_bytes());
        let list = parse_attribute_list(&v).unwrap();
        assert_eq!(list[0].record_number, 0x1234);
        assert_eq!(list[0].sequence, 2);
    }
}
