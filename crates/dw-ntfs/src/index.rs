//! `$I30` directory index parsing: `$INDEX_ROOT` (resident) and
//! `$INDEX_ALLOCATION` (INDX records, non-resident).
//!
//! Public on-disk layout (Linux `ntfs-3g` layouts, mirrored by every NTFS
//! implementation):
//!
//! ```text
//! INDEX_ROOT value:
//!   0x00  type u32            (0x30 = file names)
//!   0x04  collation u32
//!   0x08  bytes_per_block u32 (index block size)
//!   0x0C  clusters_per_block u8 +3 reserved
//!   0x10  <index header>       (same shape as inside INDX)
//!
//! index header (also at 0x18 inside an INDX record):
//!   +0x00 entries_offset u16   (from the header's own start)
//!   +0x02 index_length u16     (bytes in use)
//!   +0x04 allocated u32
//!   +0x08 flags u16            (0x01 = node has children)
//!   +0x0A reserved u32/u16
//!
//! index entry:
//!   0x00 file_ref u64          (low 48 bits = MFT record number)
//!   0x08 entry_length u16
//!   0x0A key_length u16
//!   0x0C flags u16             (0x01 = sub-node, VCN at entry tail;
//!                               0x02 = end marker, no key)
//!   0x0E reserved u16
//!   0x10 key ($FILE_NAME for I30)
//!   [key_length bytes]
//!   [8-byte child VCN when flags & 0x01]
//!
//! INDX record:
//!   0x00 "INDX" | usa_ofs u16 | usa_count u16 | lsn u64
//!   0x10 vcn u64
//!   0x18 <index header>
//!   ... entries at index header + entries_offset
//! ```
//!
//! The deep-scan use: when MFT records are damaged, surviving INDX blocks
//! still name files and their parent directories. The walker surfaces
//! those names as metadata-only entries (the sizes and runs live in the
//! lost record, so content recovery is honestly marked partial).

use crate::error::{NtfsError, NtfsResult};
use crate::filename::FileName;

/// "INDX" record magic.
pub const INDX_MAGIC: [u8; 4] = *b"INDX";
/// Cap on entries per index block.
const MAX_ENTRIES_PER_BLOCK: usize = 4096;

/// One parsed index entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// 48-bit MFT record number in the low half, sequence in the high.
    pub file_reference: u64,
    pub entry_length: u16,
    pub key_length: u16,
    /// 0x01 = sub-node present (child VCN at the entry tail), 0x02 = end
    /// marker (no key, terminates the entry array).
    pub flags: u16,
    pub sub_node_vcn: Option<u64>,
    /// The `$FILE_NAME` key (None for end markers / non-name indexes).
    pub key: Option<FileName>,
}

impl IndexEntry {
    /// MFT record number this entry points at.
    #[must_use]
    pub fn record_number(&self) -> u64 {
        self.file_reference & 0x0000_FFFF_FFFF_FFFF
    }
}

/// Parse the entry array from `entries[start .. start+in_use]`.
fn parse_entry_array(buf: &[u8], start: usize, in_use: usize) -> NtfsResult<Vec<IndexEntry>> {
    let mut out = Vec::new();
    let mut off = start;
    let end = start.saturating_add(in_use).min(buf.len());
    let mut hops = 0usize;
    while off + 0x10 <= end {
        hops += 1;
        if hops > MAX_ENTRIES_PER_BLOCK {
            return Err(NtfsError::Corrupt(
                "index entry count budget exceeded".into(),
            ));
        }
        let file_reference = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        let entry_length = u16::from_le_bytes(buf[off + 8..off + 10].try_into().unwrap());
        let key_length = u16::from_le_bytes(buf[off + 10..off + 12].try_into().unwrap());
        let flags = u16::from_le_bytes(buf[off + 12..off + 14].try_into().unwrap());

        if entry_length == 0 || usize::from(entry_length) > end - off {
            break; // structurally broken chain; stop honestly
        }
        if flags & 0x02 != 0 {
            break; // end marker: array is done by definition
        }
        if entry_length < 0x10 + u64::from(key_length).min(u64::from(u16::MAX)) as u16 {
            // key does not fit inside the entry
            out.clear();
            return Err(NtfsError::Corrupt(format!(
                "index entry at {off:#x}: length {entry_length} too small for key {key_length}"
            )));
        }

        let sub_node_vcn = if flags & 0x01 != 0 {
            if usize::from(entry_length) < 0x10 + usize::from(key_length) + 8 {
                out.clear();
                return Err(NtfsError::Corrupt(
                    "sub-node entry too short for VCN".into(),
                ));
            }
            let v = off + 0x10 + usize::from(key_length);
            Some(u64::from_le_bytes(buf[v..v + 8].try_into().unwrap()))
        } else {
            None
        };

        let key = if key_length >= 66 {
            FileName::parse(&buf[off + 0x10..off + 0x10 + usize::from(key_length)]).ok()
        } else {
            None
        };

        out.push(IndexEntry {
            file_reference,
            entry_length,
            key_length,
            flags,
            sub_node_vcn,
            key,
        });
        off += usize::from(entry_length);
    }
    Ok(out)
}

/// Parse a resident `$INDEX_ROOT` attribute value.
pub fn parse_index_root(content: &[u8]) -> NtfsResult<Vec<IndexEntry>> {
    if content.len() < 0x20 {
        return Err(NtfsError::Corrupt("INDEX_ROOT truncated".into()));
    }
    let type_code = u32::from_le_bytes(content[0x00..0x04].try_into().unwrap());
    if type_code != 0x30 {
        return Err(NtfsError::Corrupt(format!(
            "INDEX_ROOT type {type_code:#x} is not a file-name index"
        )));
    }
    let entries_offset = u16::from_le_bytes(content[0x20..0x22].try_into().unwrap()) as usize;
    let index_length = u16::from_le_bytes(content[0x22..0x24].try_into().unwrap()) as usize;
    // The index header begins at 0x10; entries_offset is relative to it.
    let abs = 0x10 + entries_offset;
    if abs > content.len() || abs + index_length > content.len() {
        return Err(NtfsError::Corrupt(
            "INDEX_ROOT entry window out of bounds".into(),
        ));
    }
    parse_entry_array(content, abs, index_length)
}

/// Parse one INDX record (applies the update-sequence fixup first).
/// `buf` must be exactly one index record (`index_record_size` bytes).
pub fn parse_indx_block(buf: &[u8]) -> NtfsResult<Vec<IndexEntry>> {
    if buf.len() < 0x28 {
        return Err(NtfsError::Corrupt("INDX record truncated".into()));
    }
    if buf[0..4] != INDX_MAGIC {
        return Err(NtfsError::Corrupt("INDX bad magic".into()));
    }
    let usa_ofs = u16::from_le_bytes(buf[0x04..0x06].try_into().unwrap()) as usize;
    let usa_count = u16::from_le_bytes(buf[0x06..0x08].try_into().unwrap()) as usize;
    if usa_ofs == 0 || usa_count < 2 || usa_ofs + usa_count * 2 > buf.len() {
        return Err(NtfsError::Corrupt("INDX USN array out of bounds".into()));
    }
    let sectors = buf.len() / 512;
    if usa_count != sectors + 1 {
        return Err(NtfsError::Corrupt(format!(
            "INDX USN count {usa_count} does not cover {} sectors",
            sectors + 1
        )));
    }

    // Apply the fixup on a copy: last 2 bytes of each sector are replaced
    // by the saved values; a mismatch means the block was torn mid-write.
    let mut fixed = buf.to_vec();
    let usn = u16::from_le_bytes([fixed[usa_ofs], fixed[usa_ofs + 1]]);
    for i in 0..sectors {
        let end = (i + 1) * 512;
        if end > fixed.len() {
            break;
        }
        let pos = end - 2;
        let stored = u16::from_le_bytes([fixed[pos], fixed[pos + 1]]);
        if stored != usn {
            return Err(NtfsError::Corrupt(format!(
                "INDX USN mismatch at sector {i}: {stored:#06x} != {usn:#06x}"
            )));
        }
        let saved = usa_ofs + 2 + i * 2;
        let value = u16::from_le_bytes([fixed[saved], fixed[saved + 1]]);
        fixed[pos] = value as u8;
        fixed[pos + 1] = (value >> 8) as u8;
    }

    let entries_offset = u16::from_le_bytes(fixed[0x20..0x22].try_into().unwrap()) as usize;
    let index_length = u16::from_le_bytes(fixed[0x22..0x24].try_into().unwrap()) as usize;
    let abs = 0x18 + entries_offset;
    if abs > fixed.len() || abs + index_length > fixed.len() {
        return Err(NtfsError::Corrupt("INDX entry window out of bounds".into()));
    }
    parse_entry_array(&fixed, abs, index_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal $FILE_NAME key (66+ bytes): parent ref, times,
    /// name length + UTF-16LE name.
    fn name_key(name: &str) -> Vec<u8> {
        let mut k = vec![0u8; 66];
        k[0x00..0x08].copy_from_slice(&5u64.to_le_bytes()); // parent ref
        k[0x40] = name.encode_utf16().count() as u8; // UTF-16 units
        k[0x41] = 0; // namespace: posix
        for unit in name.encode_utf16() {
            k.extend_from_slice(&unit.to_le_bytes());
        }
        // pad to 8
        while !k.len().is_multiple_of(8) {
            k.push(0);
        }
        k
    }

    fn entry_bytes(file_ref: u64, key: &[u8], subnode_vcn: Option<u64>, is_last: bool) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&file_ref.to_le_bytes());
        let key_len = key.len() as u16;
        let extra = if subnode_vcn.is_some() { 8 } else { 0 };
        let last_flag = if is_last { 2 } else { 0 };
        let node_flag = if subnode_vcn.is_some() { 1 } else { 0 };
        let len = (0x10 + key_len as usize + extra) as u16;
        e.extend_from_slice(&len.to_le_bytes());
        e.extend_from_slice(&key_len.to_le_bytes());
        e.extend_from_slice(&((node_flag | last_flag) as u16).to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes());
        e.extend_from_slice(key);
        if let Some(v) = subnode_vcn {
            e.extend_from_slice(&v.to_le_bytes());
        }
        e
    }

    /// INDEX_ROOT: index header at 0x10; entries_offset from there.
    fn index_root(entries: &[Vec<u8>]) -> Vec<u8> {
        // 0x20 fixed part + 0x10 pad so entries start at 0x30 (entries_offset
        // is relative to the index header at 0x10, so entries_offset = 0x20).
        let mut r = vec![0u8; 0x30];
        r[0x00..0x04].copy_from_slice(&0x30u32.to_le_bytes());
        r[0x20..0x22].copy_from_slice(&0x20u16.to_le_bytes()); // entries_offset (rel to 0x10)
        let total: usize = entries.iter().map(|e| e.len()).sum();
        r[0x22..0x24].copy_from_slice(&(total as u16).to_le_bytes());
        r[0x24..0x28].copy_from_slice(&0x100u32.to_le_bytes()); // allocated
        r[0x28..0x2A].copy_from_slice(&0u16.to_le_bytes()); // flags: leaf
        for e in entries {
            r.extend_from_slice(e);
        }
        r
    }

    /// One INDX block (4096 bytes) with a valid USN fixup over 8 sectors.
    fn indx_block(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 4096];
        b[0x00..0x04].copy_from_slice(&INDX_MAGIC);
        // USN array: offset 0x28 (after header), count = 8 sectors + 1 = 9
        b[0x04..0x06].copy_from_slice(&0x28u16.to_le_bytes());
        b[0x06..0x08].copy_from_slice(&9u16.to_le_bytes());
        b[0x08..0x10].copy_from_slice(&0u64.to_le_bytes()); // lsn
        b[0x10..0x18].copy_from_slice(&0u64.to_le_bytes()); // vcn
        // index header at 0x18: entries_offset (from 0x18) = 0x28 => abs 0x40
        b[0x20..0x22].copy_from_slice(&0x28u16.to_le_bytes());
        let total: usize = entries.iter().map(|e| e.len()).sum();
        b[0x22..0x24].copy_from_slice(&(total as u16).to_le_bytes());
        b[0x24..0x28].copy_from_slice(&0x100u32.to_le_bytes());
        b[0x28..0x2A].copy_from_slice(&1u16.to_le_bytes()); // has children
        // entries at 0x40
        let mut off = 0x40;
        for e in entries {
            b[off..off + e.len()].copy_from_slice(e);
            off += e.len();
        }
        // USN fixup: at 0x28..(0x28+18): USN value then 8 saved pairs.
        // Sectors end at 0x200,0x400,...,0x1000; the last 2 bytes of each
        // are currently ZERO (the entries never reach that far) - set the
        // USN value to 0 so the check passes, and store real saved values.
        for i in 0..9 {
            b[0x28 + i * 2..0x28 + i * 2 + 2].copy_from_slice(&0u16.to_le_bytes());
        }
        // saved values for sectors: last 2 bytes of each sector are zeros;
        // the fixup writes zeros back - identity. Keep 0 so parse passes.
        b
    }

    #[test]
    fn index_root_parses_leaf_entries() {
        let k1 = name_key("hello.txt");
        let k2 = name_key("inner.bin");
        let e1 = entry_bytes(100, &k1, None, false);
        let e2 = entry_bytes(101, &k2, Some(7), false);
        let end = entry_bytes(0, &[], None, true);
        let root = index_root(&[e1, e2, end]);
        let out = parse_index_root(&root).unwrap();
        assert_eq!(out.len(), 2); // end marker excluded
        assert_eq!(out[0].record_number(), 100);
        assert_eq!(out[1].record_number(), 101);
        assert_eq!(out[1].sub_node_vcn, Some(7));
        assert_eq!(
            out[0].key.as_ref().map(|f| f.name.as_str()),
            Some("hello.txt")
        );
    }

    #[test]
    fn indx_block_parses_with_fixup() {
        let k = name_key("from indx.bin");
        let e1 = entry_bytes(200, &k, None, false);
        let end = entry_bytes(0, &[], None, true);
        let blk = indx_block(&[e1, end]);
        let out = parse_indx_block(&blk).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].record_number(), 200);
        assert_eq!(
            out[0].key.as_ref().map(|f| f.name.as_str()),
            Some("from indx.bin")
        );
    }

    #[test]
    fn bad_magic_rejected() {
        let mut blk = indx_block(&[]);
        blk[0] = b'X';
        assert!(parse_indx_block(&blk).is_err());
    }

    #[test]
    fn wrong_index_type_rejected() {
        let mut root = index_root(&[]);
        root[0x00..0x04].copy_from_slice(&0x40u32.to_le_bytes()); // $VIEW_INDEX
        assert!(parse_index_root(&root).is_err());
    }

    #[test]
    fn zero_entry_length_stops_cleanly() {
        // First entry claims length 0: the array breaks without panic.
        let mut root = index_root(&[]);
        // overwrite the first entry area with zeros (entry_length at +8)
        // root currently has zero entries; append a broken one.
        let broken = vec![0u8; 0x10]; // file_ref 0, len 0
        root.extend_from_slice(&broken);
        // bump index_length so the window covers it
        let total = root[0x22..0x24].read_u16le();
        root[0x22..0x24].copy_from_slice(&(total + 0x10).to_le_bytes());
        let out = parse_index_root(&root).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn fixup_mismatch_rejected() {
        let mut blk = indx_block(&[]);
        // Corrupt sector 0's last 2 bytes (0x1FE..0x200) to a wrong USN.
        blk[0x1FE] = 0xAA;
        blk[0x1FF] = 0xBB;
        assert!(parse_indx_block(&blk).is_err());
    }

    trait ReadU16le {
        fn read_u16le(&self) -> u16;
    }
    impl ReadU16le for [u8] {
        fn read_u16le(&self) -> u16 {
            u16::from_le_bytes([self[0], self[1]])
        }
    }
}
