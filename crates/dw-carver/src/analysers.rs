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
}

/// Map a file extension to its analyser. All ZIP-based container formats
/// (jar, apk, docx, xlsx, pptp family, odf, epub, ...) validate through
/// the ZIP EOCD walk.
pub fn analyser_for(extension: &str) -> Option<AnalyserKind> {
    match extension {
        "jpg" | "jpeg" => Some(AnalyserKind::Jpeg),
        "png" => Some(AnalyserKind::Png),
        "zip" | "jar" | "apk" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub"
        | "ipa" | "wal" => Some(AnalyserKind::Zip),
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

    // ---- dispatch ----

    #[test]
    fn analyser_for_maps_extensions() {
        assert_eq!(analyser_for("jpg"), Some(AnalyserKind::Jpeg));
        assert_eq!(analyser_for("jpeg"), Some(AnalyserKind::Jpeg));
        assert_eq!(analyser_for("png"), Some(AnalyserKind::Png));
        assert_eq!(analyser_for("zip"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("docx"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("apk"), Some(AnalyserKind::Zip));
        assert_eq!(analyser_for("exe"), None);
        assert_eq!(analyser_for("gif"), None); // not yet — GIF analyser is planned
    }
}
