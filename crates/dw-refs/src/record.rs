//! Metadata records, decoded from a real Windows-formatted ReFS 3.14
//! volume and cross-checked against 32 files with known names, sizes,
//! and mtimes.
//!
//! Two record shapes are used by the signature-scoped engine:
//!
//! **Name record** (found in "directory" pages):
//! ```text
//! +0x18  u64   file id (1..65535)
//! +0x28  u16   name_chars + 2
//! +0x2a  u16   name bytes (UTF-16LE) == 2 * name_chars
//! +0x2c  ..    name (UTF-16LE, NUL-terminated in practice)
//! ```
//!
//! **File-info record** (0xa8 bytes):
//! ```text
//! +0x00  u32   record length (0xa8)
//! +0x28  FILETIME creation
//! +0x30  FILETIME modification
//! +0x38  FILETIME metadata modification
//! +0x40  FILETIME access
//! +0x58  u64   file size
//! +0x80  u64   file id (1..65535)
//! ```

use time::OffsetDateTime;

/// FILETIME -> OffsetDateTime (100ns ticks since 1601-01-01 UTC).
pub fn filetime(v: u64) -> Option<OffsetDateTime> {
    // 0x1dd0000000000000 ~ 2023; bound sanity to [1980, 2200]
    const MIN: u64 = 0x01A8_0000_0000_0000;
    const MAX: u64 = 0x02D1_0000_0000_0000;
    if !(MIN..=MAX).contains(&v) {
        return None;
    }
    // FILETIME = 100ns ticks since 1601-01-01; convert to unix nanos.
    let unix = (v / 10_000_000) as i64 - 11_644_473_600;
    let nanos = ((v % 10_000_000) * 100) as u32;
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(unix) * 1_000_000_000 + i128::from(nanos)).ok()
}

/// A parsed name record.
#[derive(Debug, Clone, PartialEq)]
pub struct NameRecord {
    pub id: u64,
    pub name: String,
}

/// A parsed file-info record.
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfoRecord {
    pub id: u64,
    pub size: u64,
    pub crtime: Option<OffsetDateTime>,
    pub mtime: Option<OffsetDateTime>,
    pub ctime: Option<OffsetDateTime>,
    pub atime: Option<OffsetDateTime>,
}

pub const FILEINFO_LEN: u32 = 0xa8;

/// Try to parse a name record at `off`. The layout above is checked
/// exactly; anything else is not a name record.
pub fn parse_name_record(buf: &[u8], off: usize) -> Option<NameRecord> {
    if off + 0x2c > buf.len() {
        return None;
    }
    let id = u64::from_le_bytes(buf[off + 0x18..off + 0x20].try_into().ok()?);
    if !(1..=65535).contains(&id) {
        return None;
    }
    let a = u16::from_le_bytes(buf[off + 0x28..off + 0x2a].try_into().ok()?);
    let b = u16::from_le_bytes(buf[off + 0x2a..off + 0x2c].try_into().ok()?);
    // a = name_chars + 2; b = UTF-16 bytes = 2 * name_chars
    if !(4..=132).contains(&a) || b != 2 * (a - 2) || b < 4 {
        return None;
    }
    let name_end = off + 0x2c + b as usize;
    if name_end > buf.len() {
        return None;
    }
    let raw = &buf[off + 0x2c..name_end];
    let name = String::from_utf16(
        raw.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect::<Vec<u16>>()
            .as_slice(),
    )
    .ok()?;
    if name.len() < 2 || name.len() > 64 {
        return None;
    }
    if !name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '(' | ')' | '~')) {
        return None;
    }
    Some(NameRecord { id, name })
}

/// Try to parse a file-info record at `off` (0xa8 bytes).
pub fn parse_fileinfo(buf: &[u8], off: usize) -> Option<FileInfoRecord> {
    if off + 0xa8 > buf.len() {
        return None;
    }
    let len = u32::from_le_bytes(buf[off..off + 4].try_into().ok()?);
    if len != FILEINFO_LEN {
        return None;
    }
    let g = |o: usize| -> Option<u64> {
        Some(u64::from_le_bytes(buf[off + o..off + o + 8].try_into().ok()?))
    };
    let id = g(0x80)?;
    if !(1..=65535).contains(&id) {
        return None;
    }
    let size = g(0x58)?;
    if size > (1 << 44) {
        return None; // > 16 TiB: not a size field
    }
    Some(FileInfoRecord {
        id,
        size,
        crtime: filetime(g(0x28)?),
        mtime: filetime(g(0x30)?),
        ctime: filetime(g(0x38)?),
        atime: filetime(g(0x40)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], off: usize, val: &[u8]) {
        buf[off..off + val.len()].copy_from_slice(val);
    }

    #[test]
    fn filetime_bounds() {
        // 2025-01-02T03:04:05Z as FILETIME
        let t = 1_735_775_045u64 + 11_644_473_600;
        let ft = t * 10_000_000;
        let dt = filetime(ft).unwrap();
        assert_eq!(dt.unix_timestamp(), 1_735_775_045);
        assert!(filetime(0).is_none());
        assert!(filetime(u64::MAX).is_none());
    }

    #[test]
    fn name_record_roundtrip() {
        // "file16.txt" (10 chars): a = 12, b = 20
        let mut buf = vec![0u8; 0x60];
        put(&mut buf, 0x18, &17u64.to_le_bytes());
        put(&mut buf, 0x28, &12u16.to_le_bytes());
        put(&mut buf, 0x2a, &20u16.to_le_bytes());
        let name16: Vec<u8> = "file16.txt"
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes().to_vec())
            .collect();
        put(&mut buf, 0x2c, &name16);
        let nr = parse_name_record(&buf, 0).unwrap();
        assert_eq!(nr.id, 17);
        assert_eq!(nr.name, "file16.txt");
    }

    #[test]
    fn name_record_rejects_garbage() {
        let buf = vec![0x41u8; 0x60];
        assert!(parse_name_record(&buf, 0).is_none());
    }

    #[test]
    fn fileinfo_roundtrip() {
        let mut buf = vec![0u8; 0xa8];
        put(&mut buf, 0, &0xa8u32.to_le_bytes());
        let mk = |y: i64| ((y + 11_644_473_600) * 10_000_000) as u64;
        put(&mut buf, 0x28, &mk(1_735_689_600).to_le_bytes()); // crtime
        put(&mut buf, 0x30, &mk(1_735_775_045).to_le_bytes()); // mtime
        put(&mut buf, 0x38, &mk(1_735_689_600).to_le_bytes());
        put(&mut buf, 0x40, &mk(1_735_689_600).to_le_bytes());
        put(&mut buf, 0x58, &5019u64.to_le_bytes());
        put(&mut buf, 0x80, &2u64.to_le_bytes());
        let fi = parse_fileinfo(&buf, 0).unwrap();
        assert_eq!(fi.id, 2);
        assert_eq!(fi.size, 5019);
        assert_eq!(fi.mtime.map(|t| t.unix_timestamp()), Some(1_735_775_045));
    }

    #[test]
    fn fileinfo_rejects_bad_length() {
        let mut buf = vec![0u8; 0xa8];
        put(&mut buf, 0, &0x48u32.to_le_bytes());
        assert!(parse_fileinfo(&buf, 0).is_none());
    }
}
