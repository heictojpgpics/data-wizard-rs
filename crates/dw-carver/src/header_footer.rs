//! Header/footer carving engine with per-format validation.
//!
//! Scans a byte region for file signatures and emits *validated* candidate
//! matches. Tiering:
//!
//!  1. **Analyser-validated** (JPEG EOI walk, PNG IEND walk, ZIP EOCD
//!     walk — see `analysers.rs`): the candidate's internal structure is
//!     walked to find the true end of the file. Emitted as
//!     `CarvedValidated` with exact `extents`; directly recoverable.
//!  2. **Truncated at cap**: structure plausible but the end was not
//!     reached within the analysis cap (default 64 MiB, or the
//!     signature's `max_size`). Emitted as `Carved` with capped extents —
//!     honest partial, still recoverable up to the cap.
//!  3. **Footer scan** (signatures with a tail magic, e.g. GIF `3B`,
//!     PDF `%%EOF`): scan forward from the header for the tail. Strict:
//!     no footer within the cap -> no entry.
//!  4. **Unvalidated header** (no analyser, no footer): emitted as a
//!     size-unknown marker (`size = 0`, no extents, halved confidence).
//!     These are detections, not recoverable files — recovery refuses
//!     them with a typed error rather than writing guessed bytes.
//!
//! Candidates at the same offset are deduplicated: the highest-weight
//! signature wins (a ZIP archive matches the `zip`, `jar` and `apk`
//! signatures simultaneously).
//!
//! Rejected candidates (structurally invalid, or footer missing within
//! the cap) emit nothing — that is the false-positive filter.

use std::sync::Arc;

use tracing::{debug, trace};
use uuid::Uuid;

use dw_core::{
    Extent, ProgressEvent, ProgressPhase, ProgressSink, RecoveredEntry, RecoveredEntryId,
    RecoveryStatus, ResultsStore,
};
use dw_disk::ReadAt;
use dw_signatures::{Matcher, SignatureDb};

use crate::analysers::{AnalyserOutcome, analyse, analyser_for};

/// Default per-candidate analysis cap when the signature has no
/// `max_size` (64 MiB).
const DEFAULT_ANALYSIS_CAP: u64 = 64 * 1024 * 1024;

/// Initial analysis window (doubles up to the cap).
const INITIAL_WINDOW: usize = 256 * 1024;

/// Footer-scan read granularity.
const FOOTER_WINDOW: u64 = 1024 * 1024;

/// How a candidate's end was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateEnd {
    /// Tier 1: structure-validated end (absolute).
    Validated { end: u64 },
    /// Tier 2: structure plausible, capped at `cap` (absolute end = header + cap).
    TruncatedAtCap { cap: u64 },
    /// Tier 3: footer found (absolute end).
    Footer { end: u64 },
    /// Tier 4: header only, size unknown.
    Unvalidated,
}

/// Carve files from a disk region by scanning for signatures and
/// validating candidates (see the module docs for the tiering).
///
/// Returns the number of entries inserted into `results`.
#[allow(clippy::too_many_arguments)]
pub fn carve_region(
    read_at: &dyn ReadAt,
    start_offset: u64,
    length: u64,
    db: &SignatureDb,
    results: &Arc<ResultsStore>,
    progress: &Arc<dyn ProgressSink>,
    scan_id: Uuid,
    chunk_size: usize,
) -> Result<u64, String> {
    let matcher = Matcher::new(db);
    let chunk_size = chunk_size.max(4096);
    let mut carved_count: u64 = 0;
    let mut offset = start_offset;
    let end = start_offset + length;
    let total_chunks = length.div_ceil(chunk_size as u64);
    let mut chunk_index: u64 = 0;
    let source_size = read_at.size();

    while offset < end {
        let read_len = std::cmp::min(chunk_size as u64, end - offset)
            .min(source_size.saturating_sub(offset)) as usize;
        if read_len == 0 {
            break; // past the end of the source
        }
        let mut buf = vec![0u8; read_len];
        if let Err(e) = read_at.read_at(offset, &mut buf) {
            trace!(offset, error = %e, "Carver read error, skipping chunk");
            offset += read_len as u64;
            chunk_index += 1;
            continue;
        }

        // Scan this chunk for header matches and deduplicate by offset
        // (highest weight wins).
        let matches = matcher.find_headers(&buf, offset);
        let deduped = dedup_by_offset(matches);

        for m in deduped {
            if let Some(entry) = resolve_candidate_and_build(read_at, db, &m, source_size) {
                results.insert(entry);
                carved_count += 1;
            }
        }

        chunk_index += 1;
        offset += read_len as u64;
        if chunk_index.is_multiple_of(64) {
            debug!(chunk_index, total_chunks, carved_count, "Carver progress");
        }
    }

    let _ = progress;
    let _ = scan_id;
    Ok(carved_count)
}

/// Async wrapper that emits start/end progress events.
#[allow(clippy::too_many_arguments)]
pub async fn carve_region_async(
    read_at: Arc<dyn ReadAt>,
    start_offset: u64,
    length: u64,
    db: Arc<SignatureDb>,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    scan_id: Uuid,
    chunk_size: usize,
) -> Result<u64, String> {
    let total_chunks = length.div_ceil(chunk_size.max(4096) as u64);

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: 0,
            total: total_chunks,
            message: format!("Starting carving pass ({length} bytes)"),
        })
        .await;

    let carved = carve_region(
        read_at.as_ref(),
        start_offset,
        length,
        &db,
        &results,
        &progress,
        scan_id,
        chunk_size,
    )?;

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: total_chunks,
            total: total_chunks,
            message: format!("Carving complete: {carved} files carved"),
        })
        .await;

    Ok(carved)
}

/// Deduplicate matches at the same file-start offset: highest weight
/// wins; ties break by extension alphabetically (deterministic).
fn dedup_by_offset(
    mut matches: Vec<dw_signatures::MatchResult>,
) -> Vec<dw_signatures::MatchResult> {
    // Stable sort by weight descending, then extension ascending.
    matches.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    let mut out: Vec<dw_signatures::MatchResult> = Vec::new();
    let mut last_offset: Option<u64> = None;
    for m in matches {
        if last_offset == Some(m.offset) {
            continue; // lower-weight (or later alphabetical) duplicate at same offset
        }
        last_offset = Some(m.offset);
        out.push(m);
    }
    out
}

/// Resolve one candidate's end via the tiering rules and build the entry.
/// Returns `None` when the candidate is rejected (invalid / no footer).
fn resolve_candidate_and_build(
    read_at: &dyn ReadAt,
    db: &SignatureDb,
    m: &dw_signatures::MatchResult,
    source_size: u64,
) -> Option<RecoveredEntry> {
    let sig = db
        .signatures
        .get(m.signature_id.0 as usize)
        .expect("signature id in range");
    let cap: u64 = if sig.max_size > 0 {
        sig.max_size
    } else {
        DEFAULT_ANALYSIS_CAP
    };
    let cap = cap.min(source_size.saturating_sub(m.offset));

    let resolved: CandidateEnd = if let Some(kind) = analyser_for(&m.extension) {
        // Tier 1/2: windowed structural analysis.
        let mut window = INITIAL_WINDOW;
        loop {
            let w = window.min(cap as usize).max(1);
            let mut buf = vec![0u8; w];
            if read_at.read_at(m.offset, &mut buf).is_err() {
                // Read failure at this offset: treat as truncated at the
                // window we could read.
                break CandidateEnd::TruncatedAtCap { cap: w as u64 };
            }
            match analyse(kind, &buf) {
                AnalyserOutcome::Validated(end) => {
                    break CandidateEnd::Validated {
                        end: m.offset + end,
                    };
                }
                AnalyserOutcome::Invalid => return None, // rejected
                AnalyserOutcome::NeedMore => {
                    if w >= cap as usize {
                        break CandidateEnd::TruncatedAtCap { cap };
                    }
                    window = (w * 2).min(cap as usize);
                }
            }
        }
    } else if !sig.tail_magic.is_empty() {
        // Tier 3: footer scan.
        let found = scan_forward_for_tail(read_at, m.offset, &sig.tail_magic, cap)?;
        let tail_len = sig.tail_magic.len() as u64;
        let end = found + tail_len + u64::from(sig.tail_offset_from_end);
        CandidateEnd::Footer { end }
    } else {
        CandidateEnd::Unvalidated
    };

    let (size, extents, status, confidence, name): (u64, Vec<Extent>, RecoveryStatus, u16, String) =
        match resolved {
            CandidateEnd::Validated { end } => {
                let size = end - m.offset;
                (
                    size,
                    vec![Extent {
                        start_byte: m.offset,
                        length: size,
                    }],
                    RecoveryStatus::CarvedValidated,
                    m.weight,
                    format!("carved_{}.{}", m.offset, m.extension),
                )
            }
            CandidateEnd::TruncatedAtCap { cap: c } => (
                c,
                vec![Extent {
                    start_byte: m.offset,
                    length: c,
                }],
                RecoveryStatus::Carved,
                m.weight,
                format!("carved_{}.{}", m.offset, m.extension),
            ),
            CandidateEnd::Footer { end } => {
                let size = end - m.offset;
                (
                    size,
                    vec![Extent {
                        start_byte: m.offset,
                        length: size,
                    }],
                    RecoveryStatus::Carved,
                    m.weight,
                    format!("carved_{}.{}", m.offset, m.extension),
                )
            }
            CandidateEnd::Unvalidated => (
                0,
                Vec::new(),
                RecoveryStatus::Carved,
                m.weight / 2,
                format!("unvalidated_carved_{}.{}", m.offset, m.extension),
            ),
        };

    let mut entry = RecoveredEntry {
        id: RecoveredEntryId(Uuid::now_v7()),
        fs_kind: "carved".to_string(),
        parent_path: "[carved]".to_string(),
        name,
        size,
        start_lba: None,
        mtime: None,
        ctime: None,
        atime: None,
        status,
        extension: m.extension.clone(),
        mime: m.mime.clone(),
        confidence,
        was_deleted: true,
        cluster_chain: Vec::new(),
        extents,
        resident_data: None,
    };
    entry.recompute_start_lba();
    Some(entry)
}

/// Scan forward from `from` for a (possibly wildcarded) tail pattern,
/// reading in `FOOTER_WINDOW` chunks up to `cap`. Overlap by
/// `pattern_len - 1` so a pattern straddling a window boundary is found.
/// Returns the absolute offset of the pattern start, or None when not
/// found within the cap (caller rejects the candidate).
fn scan_forward_for_tail(
    read_at: &dyn ReadAt,
    from: u64,
    pattern: &[Option<u8>],
    cap: u64,
) -> Option<u64> {
    let plen = pattern.len();
    if plen == 0 {
        return None;
    }
    let mut pos = from;
    let limit = from + cap;
    while pos < limit {
        let want = FOOTER_WINDOW.min(limit - pos) as usize;
        let want = want.max(plen);
        let mut buf = vec![0u8; want];
        if read_at.read_at(pos, &mut buf).is_err() {
            return None; // read failure: cannot confirm footer -> reject
        }
        if let Some(rel) = find_pattern(&buf, pattern) {
            return Some(pos + rel as u64);
        }
        // Advance, overlapping by plen - 1.
        pos += (want as u64).saturating_sub(plen as u64 - 1);
    }
    None
}

/// Find `pattern` (with wildcards) anywhere in `buf`. Returns the index
/// of the first match.
fn find_pattern(buf: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
    let plen = pattern.len();
    if plen == 0 || buf.len() < plen {
        return None;
    }
    'outer: for i in 0..=buf.len() - plen {
        for (j, p) in pattern.iter().enumerate() {
            if let Some(b) = p
                && buf[i + j] != *b
            {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_disk::{DiskSource, FileBackedSource};

    fn no_progress() -> Arc<dyn ProgressSink> {
        struct NoSink;
        #[async_trait::async_trait]
        impl ProgressSink for NoSink {
            async fn emit(&self, _: ProgressEvent) {}
            async fn fail(&self, _: String) {}
            async fn done(&self) {}
        }
        Arc::new(NoSink)
    }

    /// Build a minimal valid JPEG (same shape as the analysers tests).
    fn minimal_jpeg(payload: usize) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        v.extend_from_slice(&[0x00; 14]);
        v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x45]);
        v.extend_from_slice(&[0x11; 67]);
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11]);
        v.extend_from_slice(&[0x22; 15]);
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]);
        v.extend_from_slice(&[0x33; 10]);
        v.extend_from_slice(&vec![0x55u8; payload]);
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn carve_validates_jpeg_with_exact_extents() {
        let jpg = minimal_jpeg(300);
        let jpg_at = 100usize;
        let mut data = vec![0u8; 4096 + jpg.len()];
        data[jpg_at..jpg_at + jpg.len()].copy_from_slice(&jpg);

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);
        let progress = no_progress();

        let carved = carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            &progress,
            Uuid::nil(),
            4096,
        )
        .unwrap();
        assert!(carved >= 1, "expected at least the JPEG, got {carved}");

        let snapshot = results.snapshot();
        let jpg_entry = snapshot
            .iter()
            .find(|e| e.extension == "jpg" && e.status == RecoveryStatus::CarvedValidated)
            .expect("validated jpg entry");
        assert_eq!(jpg_entry.size, jpg.len() as u64);
        assert_eq!(
            jpg_entry.extents,
            vec![Extent {
                start_byte: jpg_at as u64,
                length: jpg.len() as u64
            }]
        );
        assert_eq!(jpg_entry.start_lba, Some((jpg_at / 512) as u64));
    }

    #[test]
    fn carve_rejects_invalid_jpeg_structure() {
        // JPEG header + garbage that fails the marker walk.
        let mut data = vec![0u8; 4096];
        data[100] = 0xFF;
        data[101] = 0xD8;
        data[102] = 0xFF;
        data[103] = 0xE0;
        // APP0 length 0 -> structurally invalid.
        data[104] = 0x00;
        data[105] = 0x00;

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        let carved = carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            &no_progress(),
            Uuid::nil(),
            4096,
        )
        .unwrap();

        let snapshot = results.snapshot();
        let jpg_validated = snapshot
            .iter()
            .any(|e| e.extension == "jpg" && e.status == RecoveryStatus::CarvedValidated);
        assert!(
            !jpg_validated,
            "invalid JPEG must not be emitted as validated"
        );
        let _ = carved;
    }

    #[test]
    fn carve_emits_footer_tier_for_gif() {
        // GIF89a header ... 0x3B trailer (the gif signature has a tail).
        let mut data = vec![0u8; 2048];
        let start = 64usize;
        let gif: Vec<u8> = b"GIF89a".to_vec();
        data[start..start + 6].copy_from_slice(&gif);
        data[start + 6..start + 6 + 100].copy_from_slice(&[0x42u8; 100]);
        data[start + 106] = 0x3B; // trailer

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            &no_progress(),
            Uuid::nil(),
            4096,
        )
        .unwrap();

        let snapshot = results.snapshot();
        let gif_entry = snapshot
            .iter()
            .find(|e| e.extension == "gif" && e.size > 0)
            .expect("gif footer-tier entry");
        // End = trailer position + 1; size = end - start = 107 bytes.
        assert_eq!(gif_entry.size, 107);
        assert_eq!(gif_entry.status, RecoveryStatus::Carved);
        assert_eq!(
            gif_entry.extents,
            vec![Extent {
                start_byte: start as u64,
                length: 107
            }]
        );
    }

    #[test]
    fn carve_emits_unvalidated_tier_for_exe() {
        // MZ header with no validation path: tier 4, size 0.
        let mut data = vec![0u8; 4096];
        data[200] = b'M';
        data[201] = b'Z';

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            &no_progress(),
            Uuid::nil(),
            4096,
        )
        .unwrap();

        let snapshot = results.snapshot();
        let exe = snapshot
            .iter()
            .find(|e| e.extension == "exe")
            .expect("exe tier-4 entry");
        assert_eq!(exe.size, 0);
        assert!(exe.extents.is_empty());
        assert!(exe.name.starts_with("unvalidated_carved_"));
    }

    #[test]
    fn carve_returns_zero_for_empty_region() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"").unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        let carved = carve_region(
            read_at.as_ref(),
            0,
            0,
            &db,
            &results,
            &no_progress(),
            Uuid::nil(),
            4096,
        )
        .unwrap();
        assert_eq!(carved, 0);
    }

    #[test]
    fn find_pattern_matches_with_wildcards() {
        // The pattern AA ?? 03 occurs at index 1 (AA 42 03); the bytes at
        // index 2 (02 BB 03) do NOT match (first byte must be AA).
        let buf = [0x01, 0xAA, 0x42, 0x03, 0xBB];
        let pat: Vec<Option<u8>> = vec![Some(0xAA), None, Some(0x03)];
        assert_eq!(find_pattern(&buf, &pat), Some(1));

        let none: Vec<Option<u8>> = vec![Some(0xFF)];
        assert_eq!(find_pattern(&buf, &none), None);

        // Wildcard-only pattern matches at 0.
        let allwild: Vec<Option<u8>> = vec![None, None];
        assert_eq!(find_pattern(&buf, &allwild), Some(0));

        // Pattern longer than buffer: no match.
        let long: Vec<Option<u8>> = vec![Some(0xAA), Some(0x42), Some(0x03), Some(0x9)];
        assert_eq!(find_pattern(&buf, &long), None);
    }

    #[test]
    fn dedup_keeps_highest_weight_per_offset() {
        let mk = |offset: u64, weight: u16, ext: &str| dw_signatures::MatchResult {
            signature_id: dw_signatures::SignatureId(0),
            offset,
            extension: ext.to_string(),
            mime: None,
            weight,
        };
        let matches = vec![
            mk(100, 500, "zip"),
            mk(100, 700, "jar"),
            mk(100, 700, "apk"),
            mk(200, 400, "zip"),
        ];
        let deduped = dedup_by_offset(matches);
        assert_eq!(deduped.len(), 2);
        // offset 100: weight 700, tie broken alphabetically -> apk
        assert_eq!(deduped[0].extension, "apk");
        assert_eq!(deduped[0].weight, 700);
        assert_eq!(deduped[1].offset, 200);
    }
}
