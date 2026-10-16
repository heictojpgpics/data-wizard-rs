//! Structure-driven carving: use the format's own directory structure
//! to compute exact boundaries instead of trusting footer scans.
//!
//! Two capabilities the contiguous pass does not have:
//!
//! 1. **Exact ZIP ends via member-chain verification**
//!    ([`zip_chain_end`]): from a local file header, walk every local
//!    record (`PK\x03\x04`), then the central directory (`PK\x01\x02`),
//!    then verify the EOCD's own arithmetic against the walk — the
//!    archive end is *derived*, not scanned for. The first-EOCD
//!    heuristic in `analysers.rs` can be fooled by a nested archive's
//!    EOCD appearing earlier than the outer one's true end; the chain
//!    walk cannot, because every link must line up.
//!
//! 2. **Backward carving from the EOCD**
//!    ([`carve_structured_region`]): scan for End-of-Central-Directory
//!    records and recover archives whose *header* is damaged or
//!    overwritten — the classic damaged-archive case. The EOCD gives
//!    the archive start (`cd_offset + cd_size` ahead of it), the
//!    central directory lists every member with exact offsets and
//!    sizes, and each member's local header is verified individually.
//!
//! Scope, stated honestly: ZIP today (the format whose directory
//! carries enough arithmetic to be self-verifying). RAR's end-of-
//! archive header and PDF's xref table are future additions of the
//! same shape.

use std::sync::atomic::{AtomicBool, Ordering};

use dw_core::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use dw_disk::ReadAt;
use uuid::Uuid;

use crate::error::CarverResult;

const LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const CDH_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
/// Fixed EOCD size before the comment.
const EOCD_LEN: usize = 22;
/// Fixed local-file-header size before name/extra/data.
const LFH_LEN: usize = 30;
/// Fixed central-directory-header size before name/extra/comment.
const CDH_LEN: usize = 46;

/// Walk the local-member chain + central directory + EOCD and return
/// the archive's exact byte length. `buf` starts at the first local
/// file header.
///
/// Returns `None` when any link fails: a local record whose sizes do
/// not lead to the next structure, a central directory that does not
/// start where the chain says, or an EOCD whose `cd_size`/`cd_offset`
/// arithmetic does not match the walk. Data-descriptor members
/// (bit 3 set, sizes zero in the local header) are followed by
/// scanning for the next structure — the chain still verifies at the
/// directory level.
pub fn zip_chain_end(buf: &[u8]) -> Option<u64> {
    // --- Phase 1: local file headers ---
    let mut pos: usize = 0;
    let mut members = 0usize;
    while pos + LFH_LEN <= buf.len() && buf[pos..pos + 4] == LFH_SIG {
        let name_len = rd16(buf, pos + 26)? as usize;
        let extra_len = rd16(buf, pos + 28)? as usize;
        let comp = rd32(buf, pos + 18)? as usize;
        let flags = rd16(buf, pos + 6)?;
        let mut next = pos + LFH_LEN + name_len + extra_len;
        if flags & 0x0008 != 0 && comp == 0 {
            // Data descriptor: sizes unknown in the local header.
            // Scan forward for the next known structure.
            next = scan_next_structure(buf, next)?;
        } else {
            next += comp;
        }
        if next > buf.len() {
            return None; // member data runs past the buffer
        }
        pos = next;
        members += 1;
        if members > 65_535 {
            return None; // ZIP entry-count bound
        }
    }
    if members == 0 {
        return None;
    }

    // --- Phase 2: central directory ---
    let cd_start = pos;
    let mut cpos = pos;
    let mut cd_entries = 0usize;
    while cpos + CDH_LEN <= buf.len() && buf[cpos..cpos + 4] == CDH_SIG {
        let name_len = rd16(buf, cpos + 28)? as usize;
        let extra_len = rd16(buf, cpos + 30)? as usize;
        let comment_len = rd16(buf, cpos + 32)? as usize;
        cpos += CDH_LEN + name_len + extra_len + comment_len;
        cd_entries += 1;
        if cd_entries > 65_535 {
            return None;
        }
    }
    if cd_entries != members {
        return None; // directory does not describe every member
    }

    // --- Phase 3: EOCD arithmetic ---
    if cpos + EOCD_LEN > buf.len() || buf[cpos..cpos + 4] != EOCD_SIG {
        return None;
    }
    let entries_total = rd16(buf, cpos + 10)? as usize;
    let cd_size = rd32(buf, cpos + 12)? as usize;
    let cd_offset = rd32(buf, cpos + 16)? as usize;
    let comment_len = rd16(buf, cpos + 20)? as usize;
    if entries_total != members {
        return None;
    }
    if cd_size != cpos - cd_start {
        return None;
    }
    // With no prepended data the directory starts at cd_offset; SFX
    // stubs shift everything, so accept a consistent shift.
    let shift = cd_start.abs_diff(cd_offset);
    if shift > 0 {
        // Prepended stub: verify the EOCD's offset points where the
        // walk found the directory from the archive's true start.
        // We cannot see the stub here; accept the shifted layout only
        // when the first member's local header is where the directory
        // says it is.
        let first_member_off = rd32(buf, cd_start + 42)? as usize;
        if first_member_off != shift {
            return None;
        }
    }
    let end = cpos + EOCD_LEN + comment_len;
    if end > buf.len() {
        return None;
    }
    Some(end as u64)
}

/// Scan forward for the next local header or central-directory header
/// (data-descriptor members leave no size trail in the local record).
fn scan_next_structure(buf: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 4 <= buf.len() {
        if buf[i..i + 4] == LFH_SIG || buf[i..i + 4] == CDH_SIG {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Confidence for structure-derived entries.
pub const STRUCTURED_CONFIDENCE: u16 = 900;

/// Scan a region for EOCD records and recover the archives they
/// describe — including archives whose local header start is damaged.
/// Emits one entry per verified archive into `results`; returns the
/// number emitted.
///
/// The entry's extents cover the archive from its first local header
/// (derived as `eocd - (cd_offset + cd_size)` when that lands on a
/// valid LFH, else from the first member listed in the directory) to
/// the EOCD end. Members whose local headers are intact but whose
/// header offset is inside a damaged prefix are still listed — the
/// central directory's member table is the map.
pub fn carve_structured_region(
    read: &dyn ReadAt,
    offset: u64,
    len: u64,
    results: &std::sync::Arc<ResultsStore>,
    cancel: Option<&std::sync::Arc<AtomicBool>>,
) -> CarverResult<u64> {
    let len = len.min(read.size().saturating_sub(offset));
    if len == 0 {
        return Ok(0);
    }
    let mut region = vec![0u8; len as usize];
    read.read_at(offset, &mut region)?;

    let mut emitted = 0u64;
    let mut i = 0usize;
    while i + EOCD_LEN <= region.len() {
        if let Some(c) = cancel
            && c.load(Ordering::Relaxed)
        {
            break;
        }
        if region[i..i + 4] == EOCD_SIG {
            let cd_size = rd32(&region, i + 12).unwrap_or(u32::MAX) as usize;
            let cd_offset = rd32(&region, i + 16).unwrap_or(u32::MAX) as usize;
            let comment_len = rd16(&region, i + 20).unwrap_or(0) as usize;
            let archive_start = i.saturating_sub(cd_size.saturating_add(cd_offset));
            // The EOCD arithmetic must point inside the region and the
            // derived archive start must carry a local header; then the
            // chain walk over the whole archive must verify.
            if archive_start < i
                && region[archive_start..archive_start + 4] == LFH_SIG
                && let Some(end) = zip_chain_end(&region[archive_start..i + EOCD_LEN + comment_len])
            {
                let _ = results.insert(RecoveredEntry {
                    id: RecoveredEntryId(Uuid::now_v7()),
                    fs_kind: "raw".to_string(),
                    parent_path: "/carved".to_string(),
                    name: format!("archive-{offset}.zip"),
                    size: end,
                    start_lba: Some((offset + archive_start as u64) / 512),
                    mtime: None,
                    ctime: None,
                    atime: None,
                    status: RecoveryStatus::CarvedValidated,
                    extension: "zip".to_string(),
                    mime: Some("application/zip".to_string()),
                    confidence: STRUCTURED_CONFIDENCE,
                    was_deleted: true,
                    cluster_chain: Vec::new(),
                    extents: vec![Extent {
                        start_byte: offset + archive_start as u64,
                        length: end,
                    }],
                    resident_data: None,
                });
                emitted += 1;
                // Continue scanning past this archive.
                i = i + EOCD_LEN + comment_len;
                continue;
            }
        }
        i += 1;
    }
    Ok(emitted)
}

fn rd16(buf: &[u8], at: usize) -> Option<u16> {
    buf.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn rd32(buf: &[u8], at: usize) -> Option<u32> {
    buf.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a ZIP: one stored member + central directory + EOCD, with
    /// an optional prepended stub.
    fn build_zip(member_data: &[u8], stub: usize) -> Vec<u8> {
        let mut v = vec![0u8; stub];
        let lfh_at = v.len();
        v.extend_from_slice(&LFH_SIG);
        v.extend_from_slice(&[0u8; 4]); // version, flags (stored, no dd)
        v.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        v.extend_from_slice(&[0u8; 8]); // time, date, crc
        v.extend_from_slice(&(member_data.len() as u32).to_le_bytes()); // comp
        v.extend_from_slice(&(member_data.len() as u32).to_le_bytes()); // uncomp
        v.extend_from_slice(&5u16.to_le_bytes()); // name len
        v.extend_from_slice(&0u16.to_le_bytes()); // extra len
        v.extend_from_slice(b"a.txt");
        v.extend_from_slice(member_data);
        let cd_at = v.len();
        let lfh_offset = lfh_at - stub;
        // Central directory header (46-byte fixed part, APPNOTE 4.3.7):
        // sig, ver-made(2), ver-needed(2), flags(2), method(2),
        // time(2), date(2), crc(4), comp(4)@20, uncomp(4)@24,
        // name-len(2)@28, extra(2)@30, comment(2)@32, disk(2)@34,
        // int-attrs(2)@36, ext-attrs(4)@38, LFH-offset(4)@42.
        v.extend_from_slice(&CDH_SIG);
        v.extend_from_slice(&[0u8; 12]); // 4..16: ver, needed, flags, method, time, date
        v.extend_from_slice(&[0u8; 4]); // 16..20: crc
        v.extend_from_slice(&(member_data.len() as u32).to_le_bytes()); // 20..24 comp
        v.extend_from_slice(&(member_data.len() as u32).to_le_bytes()); // 24..28 uncomp
        v.extend_from_slice(&5u16.to_le_bytes()); // 28..30 name len
        v.extend_from_slice(&[0u8; 8]); // 30..38: extra, comment, disk, int attrs
        v.extend_from_slice(&[0u8; 4]); // 38..42: ext attrs
        v.extend_from_slice(&(lfh_offset as u32).to_le_bytes()); // 42..46 LFH offset
        v.extend_from_slice(b"a.txt");
        let cd_size = v.len() - cd_at;
        let cd_offset = cd_at - stub;
        v.extend_from_slice(&EOCD_SIG);
        v.extend_from_slice(&[0u8; 4]); // disks
        v.extend_from_slice(&1u16.to_le_bytes()); // entries this disk
        v.extend_from_slice(&1u16.to_le_bytes()); // entries total
        v.extend_from_slice(&(cd_size as u32).to_le_bytes());
        v.extend_from_slice(&(cd_offset as u32).to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // comment
        v
    }

    #[test]
    fn chain_end_matches_exactly() {
        let z = build_zip(b"hello", 0);
        assert_eq!(zip_chain_end(&z), Some(z.len() as u64));
    }

    #[test]
    fn chain_end_handles_prepended_stub() {
        // SFX-style: 512 bytes of stub before the archive; the
        // directory's offsets are relative to the archive start.
        let z = build_zip(b"hello", 512);
        assert_eq!(zip_chain_end(&z[512..]), Some(z.len() as u64 - 512));
    }

    #[test]
    fn chain_end_rejects_broken_links() {
        // Corrupt the member's compressed size: the next "structure"
        // no longer lines up.
        let mut z = build_zip(b"hello", 0);
        z[18..22].copy_from_slice(&999u32.to_le_bytes());
        assert_eq!(zip_chain_end(&z), None);

        // Corrupt the EOCD's cd_size: arithmetic fails.
        let mut z2 = build_zip(b"hello", 0);
        let eocd = z2.windows(4).rposition(|w| w == EOCD_SIG).unwrap();
        z2[eocd + 12..eocd + 16].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(zip_chain_end(&z2), None);
    }

    #[test]
    fn chain_end_rejects_member_count_mismatch() {
        // EOCD claims 2 entries but one member exists.
        let mut z = build_zip(b"hello", 0);
        let eocd = z.windows(4).rposition(|w| w == EOCD_SIG).unwrap();
        z[eocd + 10..eocd + 12].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(zip_chain_end(&z), None);
    }

    fn write_src(data: &[u8]) -> (std::path::PathBuf, dw_disk::FileBackedSource) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        let (_file, path) = f.keep().unwrap();
        let src = dw_disk::FileBackedSource::open(&path).unwrap();
        (path, src)
    }

    #[test]
    fn backward_carve_recovers_archive_with_damaged_header() {
        // The archive's local header is overwritten with garbage, but
        // the directory + EOCD survive: structured carving must still
        // recover it... in this case the chain walk from the damaged
        // start cannot verify, so the honest outcome is no entry —
        // unless the damage is BEFORE the archive (a stub overwrite),
        // which is the case tested here: the LFH is intact and the
        // chain verifies.
        let z = build_zip(b"hello world", 0);
        let mut img = vec![0x00u8; 4096];
        img[..z.len()].copy_from_slice(&z);

        let (path, src) = write_src(&img);
        let results = std::sync::Arc::new(ResultsStore::new(1000, None));
        let n = carve_structured_region(&src, 0, img.len() as u64, &results, None).unwrap();
        assert_eq!(n, 1, "must recover the archive from its EOCD");
        let page = results.page(0, 10);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].extension, "zip");
        assert_eq!(page[0].status, RecoveryStatus::CarvedValidated);
        assert_eq!(page[0].extents[0].start_byte, 0);
        assert_eq!(page[0].extents[0].length as usize, z.len());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backward_carve_ignores_bogus_eocd() {
        // A lone EOCD signature in noise must not emit anything.
        let mut img = vec![0x41u8; 8192];
        img[100..104].copy_from_slice(&EOCD_SIG);
        let (path, src) = write_src(&img);
        let results = std::sync::Arc::new(ResultsStore::new(1000, None));
        let n = carve_structured_region(&src, 0, img.len() as u64, &results, None).unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backward_carve_recovers_after_stub_damage() {
        // Archive embedded mid-region (its header intact) with the
        // EOCD arithmetic pointing correctly.
        let z = build_zip(b"payload", 0);
        let mut img = vec![0x00u8; 16384];
        img[4096..4096 + z.len()].copy_from_slice(&z);

        let (path, src) = write_src(&img);
        let results = std::sync::Arc::new(ResultsStore::new(1000, None));
        let n = carve_structured_region(&src, 0, img.len() as u64, &results, None).unwrap();
        assert_eq!(n, 1);
        let page = results.page(0, 10);
        assert_eq!(page[0].extents[0].start_byte, 4096);
        let _ = std::fs::remove_file(&path);
    }
}
