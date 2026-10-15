//! Per-format structure analysers for the carver.
//!
//! A signature match alone proves only that a plausible header exists at
//! an offset. These analysers walk the file's internal structure to find
//! the true end of the file and to reject false positives (headers that
//! appear in slack space, in other files' data, or in random noise).
//!
//! This mirrors the reference product's observed two-level design
//! (generic signature table + dedicated per-format analysers, per the
//! `EUGetAnalyserList` export family documented in
//! `docs/architecture/fs-parser-pattern.md`). Our analysers are written
//! from public format specifications:
//!
//! - **JPEG**: ISO/IEC 10918-1 / T.81 marker structure. Walk markers from
//!   SOI; every standalone/segment marker must be structurally sane; on
//!   SOS (start of scan) the entropy-coded data cannot contain a raw
//!   `FF D9` (any `FF` in entropy data is escaped as `FF 00` or is a
//!   valid marker), so the *first* `FF D9` after the SOS header is the
//!   true EOI. End of file = EOI + 2.
//! - **PNG**: ISO/IEC 15948 chunk structure. Signature (8 bytes), then
//!   chunks of `length(BE u32) + type(4) + data + CRC(4)`. The first
//!   chunk must be IHDR; the file ends after the IEND chunk's CRC.
//!   IEND must have length 0 (spec).
//! - **ZIP**: PKWARE APPNOTE. The End of Central Directory record
//!   (`50 4B 05 06`) terminates the archive; the record's own
//!   comment-length field extends the end. First EOCD from the header
//!   wins (sequential files; zip-in-zip nesting is documented as a
//!   limitation).
//!
//! Tiering used by the carver (see `header_footer.rs`):
//!  1. analyser-validated end -> `CarvedValidated`, exact extents;
//!  2. structure plausible but end beyond the analysis cap ->
//!     `Carved` (truncated at cap, honest partial);
//!  3. structurally invalid -> rejected, nothing emitted.
//!
//! Limits (v1): analysis buffers are capped at 64 MiB per candidate.
//! Larger files fall to tier 2 at the cap. Streaming/incremental
//! analysers and additional formats (MP4 `moov` walk, PDF xref, PE
//! `SizeOfImage`, RAR, 7z, GIF, BMP) are planned follow-ups.

/// Outcome of running an analyser over a buffer that starts at the
/// candidate's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyserOutcome {
    /// Structure valid; the file ends at `end` (relative to the header
    /// start, exclusive).
    Validated(u64),
    /// Structure valid so far, but the end was not reached within the
    /// provided buffer. The caller may retry with a larger buffer.
    NeedMore,
    /// Structurally invalid: reject this candidate entirely.
    Invalid,
}

/// Formats whose structure we can validate today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyserKind {
    Jpeg,
    Png,
    Zip,
    Mp4,
    Pe,
}

/// Map a file extension to its analyser. All ZIP-based container formats
/// (jar, apk, docx, xlsx, pptp family, odf, epub, ...) validate through
/// the ZIP EOCD walk. MP4 covers the ISO-BMFF family (m4v, mov uses the
/// same box grammar but different brand - kept separate to stay honest).
pub fn analyser_for(extension: &str) -> Option<AnalyserKind> {
    match extension {
        "jpg" | "jpeg" => Some(AnalyserKind::Jpeg),
        "png" => Some(AnalyserKind::Png),
        "zip" | "jar" | "apk" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub"
        | "ipa" | "wal" => Some(AnalyserKind::Zip),
        "mp4" | "m4v" | "m4a" => Some(AnalyserKind::Mp4),
        "exe" | "dll" | "sys" | "scr" => Some(AnalyserKind::Pe),
        _ => None,
    }
}

/// Run the analyser for `kind` over `buf` (which starts exactly at the
/// candidate header).
pub fn analyse(kind: AnalyserKind, buf: &[u8]) -> AnalyserOutcome {
    match kind {
        AnalyserKind::Jpeg => analyse_jpeg(buf),
        AnalyserKind::Png => analyse_png(buf),
        AnalyserKind::Zip => analyse_zip(buf),
        AnalyserKind::Mp4 => analyse_mp4(buf),
        AnalyserKind::Pe => analyse_pe(buf),
    }
}

// ---------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------

/// Walk JPEG markers to EOI.
fn analyse_jpeg(buf: &[u8]) -> AnalyserOutcome {
    // SOI: FF D8. The signature already matched FF D8 FF; re-verify SOI
    // and that a marker follows.
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 || buf[2] != 0xFF {
        return AnalyserOutcome::Invalid;
    }

    let mut pos: usize = 2;
    loop {
        // Find the next marker: skip fill bytes (FF FF ... allowed before
        // a marker per T.IA). We must land on FF <marker>.
        if pos >= buf.len() {
            return AnalyserOutcome::NeedMore;
        }
        if buf[pos] != 0xFF {
            // Entropy data reached without a preceding SOS header — that
            // means the marker sequence was malformed (we only expect
            // entropy data after SOS, which we scan internally).
            return AnalyserOutcome::Invalid;
        }
        // Skip consecutive FF fill bytes.
        while pos < buf.len() && buf[pos] == 0xFF {
            pos += 1;
        }
        if pos >= buf.len() {
            return AnalyserOutcome::NeedMore;
        }
        let marker = buf[pos];

        match marker {
            // TEM and RST markers: standalone, no payload.
            0x01 | 0xD0..=0xD7 => {
                pos += 1;
            }
            // EOI reached: end = after this byte.
            0xD9 => return AnalyserOutcome::Validated(pos as u64 + 1),
            // SOI inside the stream: invalid for a top-level file walk.
            0xD8 => return AnalyserOutcome::Invalid,
            // FF 00 stuffing is only legal inside entropy data (post-SOS);
            // before SOS it means the stream is malformed.
            0x00 => return AnalyserOutcome::Invalid,
            // SOS: header with length, then entropy data to EOI.
            0xDA => {
                // Layout: [FF] [DA] [L1 L2] [header (sos_len-2)] [entropy]
                // `pos` points at DA; entropy starts at pos + 1 + sos_len.
                // Both length bytes readable iff pos + 3 <= len.
                if pos + 3 > buf.len() {
                    return AnalyserOutcome::NeedMore;
                }
                let sos_len = u16::from_be_bytes([buf[pos + 1], buf[pos + 2]]) as usize;
                if sos_len < 2 {
                    return AnalyserOutcome::Invalid;
                }
                let entropy_start = pos + 1 + sos_len;
                if entropy_start > buf.len() {
                    return AnalyserOutcome::NeedMore;
                }
                // Scan for EOI (FF D9) in entropy data. Entropy data can
                // contain FF only as FF 00 (stuffing), RSTn, or EOI; a raw
                // FF D9 is the true EOI.
                let mut i = entropy_start;
                while i + 1 < buf.len() {
                    if buf[i] == 0xFF && buf[i + 1] == 0xD9 {
                        return AnalyserOutcome::Validated(i as u64 + 2);
                    }
                    i += 1;
                }
                return AnalyserOutcome::NeedMore;
            }
            // Segments with a 2-byte length (APPn, COM, DQT, DHT, SOFn,
            // DRI, and other accepted markers).
            _ => {
                if pos + 3 > buf.len() {
                    return AnalyserOutcome::NeedMore;
                }
                let seg_len = u16::from_be_bytes([buf[pos + 1], buf[pos + 2]]) as usize;
                if seg_len < 2 {
                    return AnalyserOutcome::Invalid;
                }
                // [FF] [marker] [L1 L2] [payload (seg_len-2)]; the next
                // marker's FF sits at pos + seg_len + 1.
                let next_ff = pos + seg_len + 1;
                if next_ff > buf.len() {
                    return AnalyserOutcome::NeedMore;
                }
                pos = next_ff;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------

const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Walk PNG chunks to IEND.
fn analyse_png(buf: &[u8]) -> AnalyserOutcome {
    if buf.len() < 8 || buf[..8] != PNG_SIG {
        return AnalyserOutcome::Invalid;
    }

    let mut pos: usize = 8;
    let mut first_chunk = true;
    loop {
        // Chunk header: length (4 BE) + type (4) — 8 bytes.
        if pos + 8 > buf.len() {
            return AnalyserOutcome::NeedMore;
        }
        let length = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        // Sanity: chunk length must stay within 2^31-1 (PNG spec) and
        // cannot point past the buffer.
        if length >= 0x8000_0000 {
            return AnalyserOutcome::Invalid;
        }
        let chunk_type = &buf[pos + 4..pos + 8];
        let is_iend = chunk_type == b"IEND";
        let is_ihdr = chunk_type == b"IHDR";

        if first_chunk && !is_ihdr {
            // The first chunk must be IHDR (spec 5.3).
            return AnalyserOutcome::Invalid;
        }
        first_chunk = false;
        if is_iend && length != 0 {
            // IEND has no data by definition.
            return AnalyserOutcome::Invalid;
        }

        let data_end = pos as u64 + 8 + u64::from(length);
        let next = data_end + 4; // + CRC
        if next > buf.len() as u64 {
            return AnalyserOutcome::NeedMore;
        }
        if is_iend {
            return AnalyserOutcome::Validated(next);
        }
        pos = next as usize;
    }
}

// ---------------------------------------------------------------------------
// ZIP
// ---------------------------------------------------------------------------

const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const EOCD_LEN: usize = 22; // fixed part before the comment

/// Walk to the ZIP End of Central Directory record.
fn analyse_zip(buf: &[u8]) -> AnalyserOutcome {
    // Find the first EOCD signature from the start of the archive.
    let mut i = 0usize;
    while i + 4 <= buf.len() {
        if buf[i..i + 4] == EOCD_SIG {
            // Found; validate the comment length.
            if i + EOCD_LEN > buf.len() {
                return AnalyserOutcome::NeedMore;
            }
            let comment_len = u16::from_le_bytes([buf[i + 20], buf[i + 21]]) as usize;
            let end = i + EOCD_LEN + comment_len;
            if end > buf.len() {
                return AnalyserOutcome::NeedMore;
            }
            return AnalyserOutcome::Validated(end as u64);
        }
        i += 1;
    }
    // EOCD not in buffer yet.
    AnalyserOutcome::NeedMore
}

// ---------------------------------------------------------------------------
// MP4 / ISO-BMFF
// ---------------------------------------------------------------------------

const MP4_FTYP: [u8; 4] = *b"ftyp";
/// Box types that terminate the walk with "extends to EOF" semantics.
/// (size == 0 box: everything to the end of the file belongs to it.)
const MP4_HINT_BOXES: [&[u8; 4]; 4] = [b"mdat", b"free", b"skip", b"sidx"];
/// Hard cap on the number of boxes (guards against corrupt size chains).
const MP4_MAX_BOXES: usize = 4096;

/// Walk ISO-BMFF boxes.
///
/// Grammar (ISO/IEC 14496-12): each box = `size u32 BE` + `type 4CC`;
/// size 1 -> 64-bit largesize follows; size 0 -> box extends to EOF.
/// A candidate is Validated when the box chain ends exactly at a
/// boundary AND the first box is a plausible type (ftyp/mdat/moov/free).
/// A size-0 box means "to EOF": for carving that is the honest
/// TruncatedAtCap tier (we cannot know the true file end), handled by
/// the caller via NeedMore -> cap.
fn analyse_mp4(buf: &[u8]) -> AnalyserOutcome {
    if buf.len() < 8 {
        return AnalyserOutcome::NeedMore;
    }
    // Sanity: the first box type should be a known top-level type.
    let first_type = &buf[4..8];
    let plausible_first = first_type == MP4_FTYP
        || first_type == b"moov"
        || first_type == b"mdat"
        || first_type == b"free"
        || first_type == b"skip"
        || first_type == b"wide";
    if !plausible_first {
        return AnalyserOutcome::Invalid;
    }

    let mut pos: usize = 0;
    let mut boxes = 0usize;
    loop {
        if pos + 8 > buf.len() {
            return AnalyserOutcome::NeedMore;
        }
        let size = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as u64;

        if size == 0 {
            // Extends to EOF: honest truncated tier.
            return AnalyserOutcome::NeedMore;
        }
        if size == 1 {
            // 64-bit largesize.
            if pos + 16 > buf.len() {
                return AnalyserOutcome::NeedMore;
            }
            let largesize = u64::from_be_bytes(buf[pos + 8..pos + 16].try_into().expect("8 bytes"));
            if largesize < 16 {
                return AnalyserOutcome::Invalid;
            }
            let end = pos as u64 + largesize;
            if end > buf.len() as u64 {
                return AnalyserOutcome::NeedMore;
            }
            pos = end as usize;
        } else {
            if size < 8 {
                return AnalyserOutcome::Invalid;
            }
            let end = pos as u64 + size;
            if end > buf.len() as u64 {
                return AnalyserOutcome::NeedMore;
            }
            // Well-known size-0-style "payload to EOF" boxes are fine
            // when their declared size fits; unknown types only invalidate
            // when the payload size chain breaks (already covered above).
            let _ = MP4_HINT_BOXES;
            pos = end as usize;
        }
        boxes += 1;
        if boxes > MP4_MAX_BOXES {
            return AnalyserOutcome::Invalid;
        }
        if pos == buf.len() {
            // Box chain consumed the buffer exactly.
            return AnalyserOutcome::Validated(pos as u64);
        }
        if pos > buf.len() {
            return AnalyserOutcome::Invalid; // unreachable; belt+braces
        }
    }
}

// ---------------------------------------------------------------------------
// PE (COFF) - the role the reference product's PE_Tools fills
// ---------------------------------------------------------------------------

const PE_SIG: [u8; 4] = *b"PE\0\0";

/// Validate a PE image and compute its file end.
///
/// Layout (MS PE/COFF spec): MZ @0; e_lfanew u32 @0x3C -> PE\0\0;
/// FileHeader (20 bytes) at e_lfanew+4 (NumberOfSections @+2 of header,
/// SizeOfOptionalHeader @+16); OptionalHeader at e_lfanew+24;
/// section table follows; the file's data ends at the last section's
/// PointerToRawData + SizeOfRawData (aligned up to FileAlignment).
/// Validated end = that maximum, rounded up to 512 (file alignment is
/// usually 512; the on-disk file can carry alignment padding).
fn analyse_pe(buf: &[u8]) -> AnalyserOutcome {
    if buf.len() < 0x40 {
        return AnalyserOutcome::NeedMore;
    }
    if &buf[..2] != b"MZ" {
        return AnalyserOutcome::Invalid;
    }
    let e_lfanew = u32::from_le_bytes([buf[0x3C], buf[0x3D], buf[0x3E], buf[0x3F]]) as usize;
    // e_lfanew must land inside the DOS stub and be aligned to 4/8; cap
    // at 64 KiB (DOS stub with padding can be large but never huge).
    if !(0x40..=0x10000).contains(&e_lfanew) || e_lfanew + 24 + 20 > buf.len() {
        if e_lfanew > 0x10000 {
            return AnalyserOutcome::Invalid;
        }
        return AnalyserOutcome::NeedMore;
    }
    if buf[e_lfanew..e_lfanew + 4] != PE_SIG {
        return AnalyserOutcome::Invalid;
    }
    // FileHeader @ e_lfanew+4:
    let fh = e_lfanew + 4;
    let machine = u16::from_le_bytes([buf[fh], buf[fh + 1]]);
    let num_sections = u16::from_le_bytes([buf[fh + 2], buf[fh + 3]]) as usize;
    let size_opt = u16::from_le_bytes([buf[fh + 16], buf[fh + 17]]) as usize;
    if machine == 0 || num_sections == 0 || num_sections > 96 {
        return AnalyserOutcome::Invalid;
    }
    if size_opt < 64 {
        return AnalyserOutcome::Invalid;
    }
    // OptionalHeader @ e_lfanew+24: sanity on magic (0x10b PE32, 0x20b PE32+).
    let oh = e_lfanew + 24;
    if oh + 2 > buf.len() {
        return AnalyserOutcome::NeedMore;
    }
    let magic = u16::from_le_bytes([buf[oh], buf[oh + 1]]);
    if magic != 0x10B && magic != 0x20B {
        return AnalyserOutcome::Invalid;
    }
    // Section table @ oh + size_opt; each entry 40 bytes.
    let sec_table = oh + size_opt;
    if sec_table + num_sections * 40 > buf.len() {
        return AnalyserOutcome::NeedMore;
    }
    let mut end: u64 = 0;
    for i in 0..num_sections {
        let e = sec_table + i * 40;
        // PointerToRawData @+20, SizeOfRawData @+16.
        let ptr = u32::from_le_bytes([buf[e + 20], buf[e + 21], buf[e + 22], buf[e + 23]]) as u64;
        let sz = u32::from_le_bytes([buf[e + 16], buf[e + 17], buf[e + 18], buf[e + 19]]) as u64;
        if sz == 0 {
            continue;
        }
        end = end.max(ptr + sz);
    }
    if end == 0 {
        return AnalyserOutcome::Invalid; // no raw data at all
    }
    // Round up to 512 (typical file alignment padding).
    let end_aligned = end.div_ceil(512) * 512;
    let end_aligned = end_aligned.min(buf.len() as u64);
    if end_aligned < 0x200 {
        return AnalyserOutcome::Invalid;
    }
    AnalyserOutcome::Validated(end_aligned)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    // ---- JPEG ----

    fn minimal_jpeg(payload: usize) -> Vec<u8> {
        // SOI + APP0(len 16) + DQT(len 69) + SOF0(len 17) + SOS(len 12)
        // + payload bytes + EOI
        let mut v = vec![0xFF, 0xD8];
        // APP0: FF E0 00 10 + 14 bytes
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        v.extend_from_slice(&[0x00; 14]);
        // DQT: FF DB 00 45 + 67 bytes
        v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x45]);
        v.extend_from_slice(&[0x11; 67]);
        // SOF0: FF C0 00 11 + 15 bytes
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
        v.extend_from_slice(&[0x22; 15]);
        // SOS: FF DA 00 0C + 10 bytes
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]);
        v.extend_from_slice(&[0x33; 10]);
        // entropy
        v.extend_from_slice(&vec![0x55u8; payload]);
        // EOI
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn jpeg_validated_to_eoi() {
        let jpg = minimal_jpeg(100);
        let expected_end = jpg.len();
        match analyse(AnalyserKind::Jpeg, &jpg) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, expected_end),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn jpeg_entropy_scan_finds_first_ffd9() {
        // Entropy data containing FF 00 stuffing must not terminate early;
        // the FF D9 at the end is the true EOI.
        let jpg = minimal_jpeg(0);
        // Insert stuffing pairs + a fake RST into the entropy area before EOI:
        // rebuild: SOI..SOS + [FF 00] x3 + [FF D0 (RST)] + [FF D9]
        let mut v = jpg[..jpg.len() - 2].to_vec(); // strip EOI
        v.extend_from_slice(&[0xFF, 0x00]);
        v.extend_from_slice(&[0xAB]);
        v.extend_from_slice(&[0xFF, 0x00]);
        v.extend_from_slice(&[0xFF, 0xD0]); // RST0 marker
        v.extend_from_slice(&[0xCD]);
        v.extend_from_slice(&[0xFF, 0xD9]); // true EOI
        match analyse(AnalyserKind::Jpeg, &v) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, v.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn jpeg_truncated_entropy_is_need_more() {
        let mut jpg = minimal_jpeg(50);
        jpg.truncate(jpg.len() - 1); // cut the EOI's last byte
        jpg.truncate(jpg.len() - 1); // cut EOI entirely
        assert_eq!(analyse(AnalyserKind::Jpeg, &jpg), AnalyserOutcome::NeedMore);
    }

    #[test]
    fn jpeg_bad_segment_length_is_invalid() {
        // DQT with length 0 (segment lengths include their own 2 bytes).
        let v = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x00];
        assert_eq!(analyse(AnalyserKind::Jpeg, &v), AnalyserOutcome::Invalid);
    }

    #[test]
    fn jpeg_first_chunk_not_marker_is_invalid() {
        // After SOI, a non-FF byte means garbage.
        let v = vec![0xFF, 0xD8, 0x12, 0x34];
        assert_eq!(analyse(AnalyserKind::Jpeg, &v), AnalyserOutcome::Invalid);
    }

    #[test]
    fn jpeg_fill_bytes_before_marker_are_tolerated() {
        // FF FF FF E0 is legal (fill bytes then APP0).
        let mut v = vec![0xFF, 0xD8];
        v.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xE0, 0x00, 0x10]);
        v.extend_from_slice(&[0x00; 14]);
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]);
        v.extend_from_slice(&[0x33; 10]);
        v.extend_from_slice(&[0xFF, 0xD9]);
        match analyse(AnalyserKind::Jpeg, &v) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, v.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    // ---- PNG ----

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(data.len() as u32).to_be_bytes());
        v.extend_from_slice(kind);
        v.extend_from_slice(data);
        v.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // CRC (not validated)
        v
    }

    fn minimal_png(payload: usize) -> Vec<u8> {
        let mut v = PNG_SIG.to_vec();
        v.extend(png_chunk(b"IHDR", &[0u8; 13]));
        v.extend(png_chunk(b"IDAT", &vec![0x42u8; payload]));
        v.extend(png_chunk(b"IEND", &[]));
        v
    }

    #[test]
    fn png_validated_to_iend() {
        let img = minimal_png(64);
        match analyse(AnalyserKind::Png, &img) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, img.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn png_first_chunk_must_be_ihdr() {
        let mut v = PNG_SIG.to_vec();
        v.extend(png_chunk(b"IDAT", &[0u8; 8])); // first chunk not IHDR
        assert_eq!(analyse(AnalyserKind::Png, &v), AnalyserOutcome::Invalid);
    }

    #[test]
    fn png_iend_with_data_is_invalid() {
        let mut v = PNG_SIG.to_vec();
        v.extend(png_chunk(b"IHDR", &[0u8; 13]));
        v.extend(png_chunk(b"IEND", &[1u8; 4])); // IEND with data
        assert_eq!(analyse(AnalyserKind::Png, &v), AnalyserOutcome::Invalid);
    }

    #[test]
    fn png_cut_mid_chunk_is_need_more() {
        let img = minimal_png(64);
        let cut = &img[..img.len() - 6]; // mid last chunk
        assert_eq!(analyse(AnalyserKind::Png, cut), AnalyserOutcome::NeedMore);
    }

    #[test]
    fn png_oversized_chunk_length_is_invalid() {
        let mut v = PNG_SIG.to_vec();
        // IHDR with length 0x8000_0000 (>= 2^31 -> Invalid). 0x7FFF_FFFF
        // is the largest VALID length and yields NeedMore instead.
        v.extend_from_slice(&0x8000_0000u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&[0u8; 13]);
        v.extend_from_slice(&[0; 4]);
        assert_eq!(analyse(AnalyserKind::Png, &v), AnalyserOutcome::Invalid);
    }

    // ---- ZIP ----

    fn minimal_zip(comment: &[u8]) -> Vec<u8> {
        // Local file header (30 bytes fixed) + filename + EOCD.
        let mut v = Vec::new();
        v.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // LFH sig
        v.extend_from_slice(&[0u8; 26]); // rest of fixed LFH
        v.extend_from_slice(b"a.txt"); // filename
        // EOCD: sig + 16 fixed bytes + disk numbers... layout:
        // 0..4 sig, 4..6 disk, 6..8 cd disk, 8..10 entries disk,
        // 10..12 entries total, 12..16 cd size, 16..20 cd offset,
        // 20..22 comment len
        v.extend_from_slice(&EOCD_SIG);
        v.extend_from_slice(&[0u8; 16]);
        v.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        v.extend_from_slice(comment);
        v
    }

    #[test]
    fn zip_validated_with_zero_comment() {
        let z = minimal_zip(b"");
        match analyse(AnalyserKind::Zip, &z) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, z.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn zip_comment_extends_the_end() {
        let z = minimal_zip(b"hello comment");
        match analyse(AnalyserKind::Zip, &z) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, z.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn zip_missing_eocd_is_need_more() {
        let v = vec![0x50, 0x4B, 0x03, 0x04, 0x00, 0x00];
        assert_eq!(analyse(AnalyserKind::Zip, &v), AnalyserOutcome::NeedMore);
    }

    #[test]
    fn zip_eocd_cut_mid_comment_is_need_more() {
        // EOCD present, comment length claims 10 bytes but only 2 exist.
        let mut v = Vec::new();
        v.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        v.extend_from_slice(&[0u8; 26]);
        v.extend_from_slice(b"a.txt");
        v.extend_from_slice(&EOCD_SIG);
        v.extend_from_slice(&[0u8; 16]);
        v.extend_from_slice(&10u16.to_le_bytes());
        v.extend_from_slice(&[0xAA, 0xBB]); // only 2 of 10 comment bytes
        assert_eq!(analyse(AnalyserKind::Zip, &v), AnalyserOutcome::NeedMore);
    }

    // ---- MP4 ----

    fn minimal_mp4(ftyp_size: u32, payload: usize) -> Vec<u8> {
        // ftyp box then an mdat box.
        let mut v = Vec::new();
        v.extend_from_slice(&ftyp_size.to_be_bytes());
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(b"isom"); // major brand
        v.extend_from_slice(&0u32.to_be_bytes()); // minor version
        v.extend_from_slice(b"isomiso2avc1mp41"); // compatible brands (16 bytes)
        let ftyp_len = 8 + 4 + 4 + 16;
        assert_eq!(ftyp_len as u32, ftyp_size);
        let mdat_size = (8 + payload) as u32;
        v.extend_from_slice(&mdat_size.to_be_bytes());
        v.extend_from_slice(b"mdat");
        v.extend_from_slice(&vec![0x42u8; payload]);
        v
    }

    #[test]
    fn mp4_validated_when_box_chain_ends_exactly() {
        let mp4 = minimal_mp4(32, 100);
        match analyse(AnalyserKind::Mp4, &mp4) {
            AnalyserOutcome::Validated(end) => assert_eq!(end as usize, mp4.len()),
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn mp4_size0_box_is_need_more() {
        let mut v = minimal_mp4(32, 0);
        // Replace the mdat box with a size-0 box (extends to EOF).
        let mdat_at = 32;
        v[mdat_at..mdat_at + 4].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(analyse(AnalyserKind::Mp4, &v), AnalyserOutcome::NeedMore);
    }

    #[test]
    fn mp4_bad_first_box_is_invalid() {
        let mut v = minimal_mp4(32, 10);
        v[4..8].copy_from_slice(b"XXXX"); // unknown first type
        assert_eq!(analyse(AnalyserKind::Mp4, &v), AnalyserOutcome::Invalid);
    }

    #[test]
    fn mp4_box_size_below_8_is_invalid() {
        let mut v = minimal_mp4(32, 10);
        // Second box size = 4 (below the 8-byte box header minimum).
        let mdat_at = 32;
        v[mdat_at..mdat_at + 4].copy_from_slice(&4u32.to_be_bytes());
        assert_eq!(analyse(AnalyserKind::Mp4, &v), AnalyserOutcome::Invalid);
    }

    // ---- PE ----

    fn minimal_pe(num_sections: usize, raw_size: u32) -> Vec<u8> {
        let mut v = vec![0u8; 0x1000];
        v[0..2].copy_from_slice(b"MZ");
        let e_lfanew = 0x80usize;
        v[0x3C..0x40].copy_from_slice(&(e_lfanew as u32).to_le_bytes());
        v[e_lfanew..e_lfanew + 4].copy_from_slice(b"PE\0\0");
        let fh = e_lfanew + 4;
        v[fh..fh + 2].copy_from_slice(&0x8664u16.to_le_bytes()); // machine amd64
        v[fh + 2..fh + 4].copy_from_slice(&(num_sections as u16).to_le_bytes());
        v[fh + 16..fh + 18].copy_from_slice(&240u16.to_le_bytes()); // sizeof opt hdr
        let oh = e_lfanew + 24;
        v[oh..oh + 2].copy_from_slice(&0x20Bu16.to_le_bytes()); // PE32+ magic
        // sections at oh+240
        let sec = oh + 240;
        for i in 0..num_sections {
            let e = sec + i * 40;
            v[e..e + 8].copy_from_slice(b".text\0\0\0");
            // VirtualSize @+8, VirtualAddress @+12 (zeros fine)
            v[e + 16..e + 20].copy_from_slice(&raw_size.to_le_bytes()); // SizeOfRawData
            v[e + 20..e + 24].copy_from_slice(&0x400u32.to_le_bytes()); // PointerToRawData
        }
        // Raw data at 0x400..0x400+raw_size
        let total = 0x400 + raw_size as usize;
        v.resize(total.max(0x1000), 0);
        v
    }

    #[test]
    fn pe_validated_to_section_end() {
        let pe = minimal_pe(1, 0x600);
        match analyse(AnalyserKind::Pe, &pe) {
            AnalyserOutcome::Validated(end) => {
                // end = 0x400+0x600 = 0xA00 aligned to 512 = 0xA00.
                assert_eq!(end, 0xA00);
            }
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[test]
    fn pe_rejects_bad_dos_or_pe_sig() {
        let mut pe = minimal_pe(1, 0x200);
        pe[1] = b'X'; // not MZ
        assert_eq!(analyse(AnalyserKind::Pe, &pe), AnalyserOutcome::Invalid);
        let mut pe = minimal_pe(1, 0x200);
        pe[0x80 + 1] = b'Q'; // not PE\0\0
        assert_eq!(analyse(AnalyserKind::Pe, &pe), AnalyserOutcome::Invalid);
    }

    #[test]
    fn pe_rejects_zero_sections() {
        let pe = minimal_pe(0, 0x200);
        assert_eq!(analyse(AnalyserKind::Pe, &pe), AnalyserOutcome::Invalid);
    }

    #[test]
    fn pe_short_buffer_is_need_more() {
        let v = vec![0u8; 0x30];
        assert_eq!(analyse(AnalyserKind::Pe, &v), AnalyserOutcome::NeedMore);
    }

    // ---- dispatch ----

    #[test]
    fn analyser_for_maps_extensions() {
        assert_eq!(analyser_for("jpg"), Some(AnalyserKind::Jpeg));
        assert_eq!(analyser_for("jpeg"), Some(AnalyserKind::Jpeg));
        assert_eq!(analyser_for("png"), Some(AnalyserKind::Png));
        assert_eq!(analyser_for("zip"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("docx"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("apk"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("exe"), Some(AnalyserKind::Pe));
        assert_eq!(analyser_for("mp4"), Some(AnalyserKind::Mp4));
        assert_eq!(analyser_for("m4v"), Some(AnalyserKind::Mp4));
        assert_eq!(analyser_for("gif"), None); // not yet — GIF analyser is planned
        assert_eq!(analyser_for("elf"), None); // no ELF structure walk yet
    }
}
