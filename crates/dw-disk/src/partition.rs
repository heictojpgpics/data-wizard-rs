//! Partition-table discovery: MBR (with EBR chains) and GPT.
//!
//! All layouts are public specifications:
//! - MBR/EBR: MS-DOS/BIOS partition scheme; entries at LBA0 offset 446,
//!   16 bytes each; signature `55 AA` at offset 510. Extended partitions
//!   (types 0x05/0x0F/0x85) chain through EBRs.
//! - GPT: UEFI Specification, chapter 5. Header at LBA1 (`EFI PART`
//!   signature), CRC32 of the header (92 bytes, checksum field zeroed)
//!   and of the partition-entry array; 128-byte entries by default.
//!
//! Design rules (from the validation plan, docs/03 + open-recovery-wizard
//! 03-validation-and-safety):
//! - malformed tables produce typed errors / empty results, never panics;
//! - CRC mismatches reject the GPT header (we never fall back to using a
//!   table that fails its own checksum);
//! - EBR chains are bounded (128 links) and cycle-safe.

use crate::{DiskError, DiskResult};

/// One discovered partition, in absolute disk offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionEntry {
    pub index: u32,
    /// Absolute byte offset of the partition start on the source.
    pub offset_bytes: u64,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Where the entry came from: `"mbr"`, `"ebr"`, or `"gpt"`.
    pub scheme: &'static str,
    /// Type description (MBR type name or known GPT GUID name).
    pub type_name: String,
    /// GPT partition name (UTF-16 from the entry), or MBR volume label
    /// position (unused for MBR — None).
    pub label: Option<String>,
}

/// Rich parse result: full detail per entry (scheme, type name,
/// GPT label). Convert to the canonical [`crate::geometry::PartitionTable`]
/// with [`DiscoveredTable::into_geometry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveredTable {
    Gpt {
        entries: Vec<PartitionEntry>,
        /// 16-byte disk GUID.
        disk_guid: [u8; 16],
    },
    Mbr {
        entries: Vec<PartitionEntry>,
    },
    /// No recognizable table (raw volume or garbage LBA0).
    None,
}

const SECTOR: u64 = 512;

/// Discover the partition table on a source.
pub fn discover_partitions(read_at: &dyn crate::ReadAt) -> DiskResult<DiscoveredTable> {
    let mut mbr = [0u8; 512];
    // A source smaller than one sector has no table.
    if read_at.size() < 512 {
        return Ok(DiscoveredTable::None);
    }
    read_at.read_at(0, &mut mbr)?;

    // MBR signature.
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Ok(DiscoveredTable::None);
    }

    // Protective GPT? (type 0xEE in any of the four slots.)
    let protective = (0..4).any(|i| mbr[446 + i * 16 + 4] == 0xEE);
    if protective {
        return parse_gpt(read_at);
    }

    parse_mbr(mbr, read_at)
}

impl DiscoveredTable {
    /// Convert to the canonical geometry representation used by the app.
    pub fn into_geometry(self) -> crate::geometry::PartitionTable {
        use crate::geometry::{
            Partition, PartitionTable as GeoTable, PartitionType,
        };

        let (kind, raw): (PartitionType, Vec<PartitionEntry>) = match self {
            DiscoveredTable::Gpt { entries, .. } => (PartitionType::Gpt, entries),
            DiscoveredTable::Mbr { entries } => (PartitionType::Mbr, entries),
            DiscoveredTable::None => (PartitionType::Unknown, Vec::new()),
        };
        let partitions = raw
            .into_iter()
            .map(|e| {
                let (fs_hint, is_system) = fs_hint_and_system(&e.type_name);
                Partition {
                    index: e.index,
                    start_byte: e.offset_bytes,
                    size_bytes: e.size_bytes,
                    fs_hint,
                    label: e.label,
                    is_system,
                }
            })
            .collect();
        GeoTable { kind, partitions }
    }
}

/// Map a human type name to (fs hint, is-system).
fn fs_hint_and_system(type_name: &str) -> (crate::geometry::PartitionFsHint, bool) {
    use crate::geometry::PartitionFsHint;
    match type_name {
        "FAT12" => (PartitionFsHint::Other(0x01), false),
        "FAT16" | "FAT16 (LBA)" => (PartitionFsHint::Fat16, false),
        "FAT32" | "FAT32 (LBA)" => (PartitionFsHint::Fat32, false),
        "NTFS/exFAT" => (PartitionFsHint::Ntfs, false),
        "Linux" => (PartitionFsHint::Other(0x83), false),
        "Linux swap" => (PartitionFsHint::Swap, false),
        "EFI System Partition" => (PartitionFsHint::Other(0xEF), true),
        _ => (PartitionFsHint::Unknown, false),
    }
}

// ---------------------------------------------------------------------------
// MBR (+ EBR chains)
// ---------------------------------------------------------------------------

fn parse_mbr(mbr: [u8; 512], read_at: &dyn crate::ReadAt) -> DiskResult<DiscoveredTable> {
    let mut entries: Vec<PartitionEntry> = Vec::new();
    let mut extended_base: Option<u64> = None; // absolute LBA of the extended partition
    let mut next_index: u32 = 0;

    for i in 0..4 {
        let e = &mbr[446 + i * 16..446 + i * 16 + 16];
        let ptype = e[4];
        let lba = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64;
        let sectors = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as u64;
        if ptype == 0 || sectors == 0 {
            continue;
        }
        if matches!(ptype, 0x05 | 0x0F | 0x85) {
            if extended_base.is_none() {
                extended_base = Some(lba);
            }
            continue;
        }
        entries.push(PartitionEntry {
            index: next_index,
            offset_bytes: lba * SECTOR,
            size_bytes: sectors * SECTOR,
            scheme: "mbr",
            type_name: mbr_type_name(ptype).to_string(),
            label: None,
        });
        next_index += 1;
    }

    // Walk the EBR chain of the extended partition.
    if let Some(base) = extended_base {
        let logical = walk_ebr_chain(read_at, base)?;
        for mut e in logical {
            e.index = next_index;
            next_index += 1;
            entries.push(e);
        }
    }

    Ok(DiscoveredTable::Mbr { entries })
}

/// Walk an EBR chain starting at `base` (absolute LBA of the extended
/// partition's first EBR). Bounded to 128 links; cycle-safe.
fn walk_ebr_chain(read_at: &dyn crate::ReadAt, base: u64) -> DiskResult<Vec<PartitionEntry>> {
    const MAX_LINKS: usize = 128;
    let mut out = Vec::new();
    let mut visited: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut ebr_lba: u64 = base;

    for _ in 0..MAX_LINKS {
        if !visited.insert(ebr_lba) {
            break; // cycle
        }
        let mut ebr = [0u8; 512];
        if read_at.read_at(ebr_lba * SECTOR, &mut ebr).is_err() {
            break; // unreadable EBR: stop the chain here
        }
        if ebr[510] != 0x55 || ebr[511] != 0xAA {
            break; // not a valid EBR
        }

        // Entry 0: this logical partition, start relative to THIS EBR.
        let e0 = &ebr[446..462];
        let ptype0 = e0[4];
        let rel0 = u32::from_le_bytes([e0[8], e0[9], e0[10], e0[11]]) as u64;
        let sectors0 = u32::from_le_bytes([e0[12], e0[13], e0[14], e0[15]]) as u64;
        if ptype0 != 0 && sectors0 != 0 && rel0 != 0 {
            let abs = ebr_lba + rel0;
            out.push(PartitionEntry {
                index: 0, // reassigned by the caller
                offset_bytes: abs * SECTOR,
                size_bytes: sectors0 * SECTOR,
                scheme: "ebr",
                type_name: mbr_type_name(ptype0).to_string(),
                label: None,
            });
        }

        // Entry 1: the next EBR, start relative to the EXTENDED BASE.
        let e1 = &ebr[462..478];
        let ptype1 = e1[4];
        let rel1 = u32::from_le_bytes([e1[8], e1[9], e1[10], e1[11]]) as u64;
        let sectors1 = u32::from_le_bytes([e1[12], e1[13], e1[14], e1[15]]);
        if ptype1 == 0 || rel1 == 0 || sectors1 == 0 {
            break; // end of chain
        }
        ebr_lba = base + rel1;
    }

    Ok(out)
}

/// Human-readable names for common MBR type bytes.
fn mbr_type_name(t: u8) -> &'static str {
    match t {
        0x01 => "FAT12",
        0x04 | 0x06 => "FAT16",
        0x05 | 0x0F | 0x85 => "Extended",
        0x07 => "NTFS/exFAT",
        0x0B => "FAT32",
        0x0C => "FAT32 (LBA)",
        0x0E => "FAT16 (LBA)",
        0x82 => "Linux swap",
        0x83 => "Linux",
        0xA5 => "FreeBSD",
        0xEE => "GPT protective",
        0xEF => "EFI System",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------
// GPT
// ---------------------------------------------------------------------------

const GPT_HEADER_SIZE: usize = 92;
const GPT_MAX_ENTRIES: usize = 128;
const GPT_MAX_ENTRY_ARRAY_BYTES: usize = 64 * 1024;

fn parse_gpt(read_at: &dyn crate::ReadAt) -> DiskResult<DiscoveredTable> {
    // Header at LBA 1.
    let mut header = [0u8; 512];
    if read_at.read_at(SECTOR, &mut header).is_err() {
        return Err(DiskError::Corrupt("GPT header unreadable".into()));
    }
    if &header[0..8] != b"EFI PART" {
        return Err(DiskError::Corrupt("GPT header signature missing".into()));
    }
    // Revision (offset 8..12): UEFI 1.0 is 0x00010000; accept 1.x.
    let revision = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    if !(0x0001_0000..=0x0001_FFFF).contains(&revision) {
        return Err(DiskError::Corrupt(format!(
            "GPT revision {revision:08X} is not 1.x"
        )));
    }
    // Header size (offset 12..16): 92 per UEFI.
    let header_size = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    if header_size != GPT_HEADER_SIZE {
        return Err(DiskError::Corrupt(format!(
            "GPT header size {header_size} != {GPT_HEADER_SIZE}"
        )));
    }
    // my_lba must be 1.
    let my_lba = u64::from_le_bytes(header[24..32].try_into().expect("8 bytes"));
    if my_lba != 1 {
        return Err(DiskError::Corrupt(format!(
            "GPT header my_lba {my_lba} != 1"
        )));
    }
    // Header CRC32: over bytes 0..92 with the CRC field (16..20) zeroed.
    let mut crcbuf = [0u8; GPT_HEADER_SIZE];
    crcbuf.copy_from_slice(&header[..GPT_HEADER_SIZE]);
    crcbuf[16..20].copy_from_slice(&0u32.to_le_bytes());
    let expected_crc = u32::from_le_bytes(header[16..20].try_into().expect("4 bytes"));
    let actual_crc = crc32_iso(&crcbuf);
    if actual_crc != expected_crc {
        return Err(DiskError::Corrupt(format!(
            "GPT header CRC32 mismatch: stored {expected_crc:08X}, computed {actual_crc:08X}"
        )));
    }

    let entries_lba = u64::from_le_bytes(header[72..80].try_into().expect("8 bytes"));
    let num_entries = u32::from_le_bytes(header[80..84].try_into().expect("4 bytes")) as usize;
    let entry_size = u32::from_le_bytes(header[84..88].try_into().expect("4 bytes")) as usize;
    let expected_entries_crc = u32::from_le_bytes(header[88..92].try_into().expect("4 bytes"));

    if num_entries > GPT_MAX_ENTRIES {
        return Err(DiskError::Corrupt(format!(
            "GPT claims {num_entries} entries (max {GPT_MAX_ENTRIES})"
        )));
    }
    if entry_size < 128 {
        return Err(DiskError::Corrupt(format!(
            "GPT entry size {entry_size} < 128"
        )));
    }
    let array_bytes = num_entries * entry_size;
    if array_bytes == 0 || array_bytes > GPT_MAX_ENTRY_ARRAY_BYTES {
        return Err(DiskError::Corrupt(format!(
            "GPT entry array size {array_bytes} out of bounds"
        )));
    }

    // Read + validate the entry array.
    let mut array = vec![0u8; array_bytes];
    read_at.read_at(entries_lba * SECTOR, &mut array)?;
    let actual_entries_crc = crc32_iso(&array);
    if actual_entries_crc != expected_entries_crc {
        return Err(DiskError::Corrupt(format!(
            "GPT entries CRC32 mismatch: stored {expected_entries_crc:08X}, computed {actual_entries_crc:08X}"
        )));
    }

    let disk_guid: [u8; 16] = header[56..72].try_into().expect("16 bytes");
    let mut entries: Vec<PartitionEntry> = Vec::new();
    let mut index: u32 = 0;
    for i in 0..num_entries {
        let e = &array[i * entry_size..(i + 1) * entry_size];
        let type_guid: [u8; 16] = e[..16].try_into().expect("16 bytes");
        if type_guid == [0u8; 16] {
            continue; // unused entry
        }
        let first_lba = u64::from_le_bytes(e[32..40].try_into().expect("8 bytes"));
        let last_lba = u64::from_le_bytes(e[40..48].try_into().expect("8 bytes"));
        if first_lba == 0 || last_lba < first_lba {
            continue; // corrupt entry: skip, do not fail the table
        }
        let name = utf16_le_string(&e[56..128]);
        entries.push(PartitionEntry {
            index,
            offset_bytes: first_lba * SECTOR,
            size_bytes: (last_lba - first_lba + 1) * SECTOR,
            scheme: "gpt",
            type_name: gpt_type_name(&type_guid).to_string(),
            label: name,
        });
        index += 1;
    }

    Ok(DiscoveredTable::Gpt { entries, disk_guid })
}

/// Decode a UTF-16LE string (NUL-terminated) from `buf`.
fn utf16_le_string(buf: &[u8]) -> Option<String> {
    let mut units = Vec::with_capacity(buf.len() / 2);
    let mut i = 0;
    while i + 1 < buf.len() {
        let unit = u16::from_le_bytes([buf[i], buf[i + 1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
        i += 2;
    }
    if units.is_empty() {
        return None;
    }
    let s: String = units
        .iter()
        .map(|&u| char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'))
        .collect();
    Some(s).filter(|s| !s.is_empty())
}

/// Known GPT type GUIDs (UEFI spec ch. 5).
fn gpt_type_name(guid: &[u8; 16]) -> &'static str {
    // GUIDs are stored mixed-endian: first 3 groups LE, last 2 BE.
    // Compare against pre-encoded byte forms.
    match guid {
        b"\x28\x32\x2a\xc1\x1f\xf8\xd2\x11\xba\x4b\x00\xa0\xc9\x3e\xc9\x3b" => {
            "EFI System Partition"
        }
        b"\xa2\xa0\xd0\xeb\xe5\xb9\x33\x44\x87\xc0\x68\xb6\xb7\x26\x99\xc7" => "Microsoft Reserved",
        b"\x35\xd7\xd0\xeb\x5d\x9e\x33\x44\x87\xc0\x68\xb6\xb7\x26\x99\xc7" => {
            "Basic data partition"
        }
        b"\xaf\x3d\xc6\x0f\x83\x84\x72\x47\x8e\x79\x3d\x69\xd8\x47\x77\xde" => "Linux filesystem",
        b"\xa4\xbb\x94\xde\xd1\x06\x40\x4a\xa1\x6a\xbf\xd5\x01\x79\xd6\xac" => "Windows recovery",
        b"\x68\xd2\xb7\x2b\x06\x3e\x41\x44\xb9\x25\x2c\xc5\x9d\x5b\x8d\x47" => "Linux RAID",
        b"\x39\x8b\xd1\x06\x4a\x84\x4d\x4b\x9c\x2f\x3a\x1e\x9e\x6f\x2e\xf4" => "Linux LVM",
        b"\xdb\x59\x6d\x93\x1f\x86\x47\x45\xb9\x25\x2c\xc5\x9d\x5b\x8d\x47" => "Linux swap",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------
// CRC-32 (ISO-HDLC, reflected, poly 0xEDB88320) — public domain table
// method (Ross Williams, "A Painless Guide to CRC Error Detection").
// ---------------------------------------------------------------------------

fn crc32_iso(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use crate::source::ReadAt;

    /// In-memory ReadAt over a Vec.
    struct MemSource {
        data: Vec<u8>,
    }
    impl ReadAt for MemSource {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> DiskResult<()> {
            let start = offset as usize;
            let end = start + buf.len();
            if end > self.data.len() {
                return Err(DiskError::OutOfRange {
                    offset,
                    len: buf.len() as u64,
                    source_size: self.data.len() as u64,
                });
            }
            buf.copy_from_slice(&self.data[start..end]);
            Ok(())
        }
        fn size(&self) -> u64 {
            self.data.len() as u64
        }
    }

    fn crc32_test_vector() {
        // The classic check value for CRC-32/ISO-HDLC.
        assert_eq!(crc32_iso(b"123456789"), 0xCBF4_3926);
    }

    // ---- GPT image builder ----

    fn build_gpt_image(
        num_parts: usize,
        corrupt_header_crc: bool,
        corrupt_entries_crc: bool,
    ) -> Vec<u8> {
        // Layout: LBA0 protective MBR, LBA1 GPT header, LBA2.. entry array,
        // partitions start at LBA 34.
        let entry_size = 128usize;
        let array_bytes = 128 * entry_size;
        let entries_lba: u64 = 2;
        let mut img = vec![0u8; (34 * 512) as usize + array_bytes + 1024 * 1024];

        // Protective MBR @0
        img[446 + 4] = 0xEE; // type
        img[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes()); // start LBA
        let last_lba = u32::try_from(img.len() as u64 / 512 - 1).unwrap_or(1);
        img[446 + 12..446 + 16].copy_from_slice(&last_lba.to_le_bytes());
        img[510..512].copy_from_slice(&[0x55, 0xAA]);

        // Partition entries @ LBA2
        let mut array = vec![0u8; array_bytes];
        for i in 0..num_parts {
            let e = &mut array[i * entry_size..(i + 1) * entry_size];
            // Basic data partition GUID (mixed-endian encoded)
            e[..16].copy_from_slice(
                b"\x35\xd7\xd0\xeb\x5d\x9e\x33\x44\x87\xc0\x68\xb6\xb7\x26\x99\xc7",
            );
            e[16..32].copy_from_slice(&[0x11u8; 16]); // unique GUID (arbitrary)
            let first: u64 = 34 + i as u64 * 2048;
            let last: u64 = first + 1023;
            e[32..40].copy_from_slice(&first.to_le_bytes());
            e[40..48].copy_from_slice(&last.to_le_bytes());
            e[48..56].copy_from_slice(&0u64.to_le_bytes());
            // name "Part N"
            let name: Vec<u16> = format!("Part {i}").encode_utf16().collect();
            for (j, &u) in name.iter().enumerate() {
                e[56 + j * 2] = (u & 0xFF) as u8;
                e[57 + j * 2] = (u >> 8) as u8;
            }
        }
        let entries_crc = crc32_iso(&array);
        if !corrupt_entries_crc {
            img[(entries_lba * 512) as usize..(entries_lba * 512) as usize + array_bytes]
                .copy_from_slice(&array);
        } else {
            // Copy with a flipped byte -> entries CRC mismatch.
            let mut corrupted = array.clone();
            corrupted[10] ^= 0xFF;
            img[(entries_lba * 512) as usize..(entries_lba * 512) as usize + array_bytes]
                .copy_from_slice(&corrupted);
        }

        // GPT header @ LBA1
        let mut h = [0u8; 512];
        h[0..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes()); // revision 1.0
        h[12..16].copy_from_slice(&(GPT_HEADER_SIZE as u32).to_le_bytes());
        h[20..24].copy_from_slice(&0u32.to_le_bytes()); // reserved
        h[24..32].copy_from_slice(&1u64.to_le_bytes()); // my_lba
        h[32..40].copy_from_slice(&2u64.to_le_bytes()); // backup lba (arbitrary)
        h[40..48].copy_from_slice(&2u64.to_le_bytes()); // first usable (arbitrary)
        h[48..56].copy_from_slice(&1000u64.to_le_bytes()); // last usable
        h[56..72].copy_from_slice(&[0xABu8; 16]); // disk guid
        h[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        h[80..84].copy_from_slice(&128u32.to_le_bytes()); // 128 entries
        h[84..88].copy_from_slice(&(entry_size as u32).to_le_bytes());
        h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        // header CRC (over 92 bytes with field zeroed)
        let mut crcbuf = [0u8; GPT_HEADER_SIZE];
        crcbuf.copy_from_slice(&h[..GPT_HEADER_SIZE]);
        let h_crc = crc32_iso(&crcbuf);
        h[16..20].copy_from_slice(&h_crc.to_le_bytes());
        if corrupt_header_crc {
            h[16..20].copy_from_slice(&(h_crc ^ 0xDEAD).to_le_bytes());
        }
        img[512..1024].copy_from_slice(&h);
        img
    }

    #[test]
    fn crc32_iso_check_value() {
        crc32_test_vector();
    }

    #[test]
    fn gpt_image_roundtrip() {
        let img = build_gpt_image(3, false, false);
        let src = MemSource { data: img };
        let table = discover_partitions(&src).unwrap();
        match table {
            DiscoveredTable::Gpt { entries, disk_guid } => {
                assert_eq!(entries.len(), 3);
                assert_eq!(disk_guid, [0xABu8; 16]);
                let p0 = &entries[0];
                assert_eq!(p0.offset_bytes, 34 * 512);
                assert_eq!(p0.size_bytes, 1024 * 512);
                assert_eq!(p0.type_name, "Basic data partition");
                assert_eq!(p0.label.as_deref(), Some("Part 0"));
                assert_eq!(entries[2].label.as_deref(), Some("Part 2"));
            }
            other => panic!("expected Gpt, got {other:?}"),
        }
    }

    #[test]
    fn gpt_rejects_bad_header_crc() {
        let img = build_gpt_image(2, true, false);
        let src = MemSource { data: img };
        let err = discover_partitions(&src).unwrap_err();
        assert!(err.to_string().contains("header CRC32 mismatch"), "{err}");
    }

    #[test]
    fn gpt_rejects_bad_entries_crc() {
        let img = build_gpt_image(2, false, true);
        let src = MemSource { data: img };
        let err = discover_partitions(&src).unwrap_err();
        assert!(err.to_string().contains("entries CRC32 mismatch"), "{err}");
    }

    // ---- MBR image builder ----

    fn build_mbr_image(with_extended: bool) -> Vec<u8> {
        // LBA0 MBR: p1 = FAT32 @2048 (2048 sectors), p2 = NTFS @4096 (4096)
        // extended @8192 (2 EBR links if with_extended).
        let mut img = vec![0u8; 8 * 1024 * 1024]; // 8 MiB: EBR1 @4MiB, EBR2 @6MiB

        fn mbr_entry(buf: &mut [u8], slot: usize, ptype: u8, lba: u32, sectors: u32) {
            let o = 446 + slot * 16;
            buf[o] = 0x80; // active
            buf[o + 1..o + 4].copy_from_slice(&[0xFE, 0xFF, 0xFF]); // CHS start (unused)
            buf[o + 4] = ptype;
            buf[o + 5..o + 8].copy_from_slice(&[0xFE, 0xFF, 0xFF]); // CHS end
            buf[o + 8..o + 12].copy_from_slice(&lba.to_le_bytes());
            buf[o + 12..o + 16].copy_from_slice(&sectors.to_le_bytes());
        }

        mbr_entry(&mut img, 0, 0x0B, 2048, 2048); // FAT32
        mbr_entry(&mut img, 1, 0x07, 4096, 4096); // NTFS
        if with_extended {
            mbr_entry(&mut img, 2, 0x0F, 8192, 8192); // extended container
        }
        img[510..512].copy_from_slice(&[0x55, 0xAA]);

        if with_extended {
            // EBR1 @8192: logical (0x83) at +63 (relative to EBR), next EBR @ +4096
            // (relative to extended base 8192).
            let ebr1 = (8192 * 512) as usize;
            img[ebr1 + 510..ebr1 + 512].copy_from_slice(&[0x55, 0xAA]);
            mbr_entry(&mut img[ebr1..], 0, 0x83, 63, 2048 - 63);
            mbr_entry(&mut img[ebr1..], 1, 0x05, 4096, 4096);

            // EBR2 @8192+4096=12288: logical (0x07) at +63, no next.
            let ebr2 = (12_288 * 512) as usize;
            img[ebr2 + 510..ebr2 + 512].copy_from_slice(&[0x55, 0xAA]);
            mbr_entry(&mut img[ebr2..], 0, 0x07, 63, 1024);
        }
        img
    }

    #[test]
    fn mbr_image_roundtrip() {
        let img = build_mbr_image(false);
        let src = MemSource { data: img };
        match discover_partitions(&src).unwrap() {
            DiscoveredTable::Mbr { entries } => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].type_name, "FAT32");
                assert_eq!(entries[0].offset_bytes, 2048 * 512);
                assert_eq!(entries[0].size_bytes, 2048 * 512);
                assert_eq!(entries[1].type_name, "NTFS/exFAT");
            }
            other => panic!("expected Mbr, got {other:?}"),
        }
    }

    #[test]
    fn mbr_ebr_chain_walks_both_links() {
        let img = build_mbr_image(true);
        let src = MemSource { data: img };
        match discover_partitions(&src).unwrap() {
            DiscoveredTable::Mbr { entries } => {
                // 2 primary + 2 logical.
                assert_eq!(entries.len(), 4, "entries: {entries:#?}");
                // Logical 1: EBR1 @8192 + 63
                assert_eq!(entries[2].scheme, "ebr");
                assert_eq!(entries[2].offset_bytes, (8192 + 63) * 512);
                assert_eq!(entries[2].type_name, "Linux");
                // Logical 2: EBR2 @12288 + 63
                assert_eq!(entries[3].offset_bytes, (12_288 + 63) * 512);
                assert_eq!(entries[3].type_name, "NTFS/exFAT");
            }
            other => panic!("expected Mbr, got {other:?}"),
        }
    }

    #[test]
    fn no_signature_yields_none() {
        let src = MemSource {
            data: vec![0u8; 4096],
        };
        assert_eq!(discover_partitions(&src).unwrap(), DiscoveredTable::None);
    }

    #[test]
    fn tiny_source_yields_none() {
        let src = MemSource {
            data: vec![0u8; 100],
        };
        assert_eq!(discover_partitions(&src).unwrap(), DiscoveredTable::None);
    }
}
