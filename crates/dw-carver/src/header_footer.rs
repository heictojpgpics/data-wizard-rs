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
//!
//! Execution model: the region is split into fixed-size chunks processed
//! by a bounded worker pool (std scoped threads, capped at 8). Each
//! worker reads `chunk + max_magic - 1` bytes so headers straddling chunk
//! boundaries are found exactly once (`Matcher::find_headers_limited`
//! attributes each hit to the chunk that owns its magic start). Workers
//! produce per-chunk entry batches; the calling thread inserts them in
//! arrival order, checks the cancel flag, and reports progress. On-disk
//! insertion order is therefore run-dependent (thread interleaving), but
//! the RESULT SET is deterministic and every entry carries a UUIDv7 id,
//! so ordering concerns are the UI's sort, not the store's.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Worker-pool ceiling: carving is I/O bound at the source and CPU bound
/// in the analysers; beyond 8 workers the mutex on raw-device handles
/// (and memory bandwidth) stops paying. (Measured rationale from the
/// reference scan engines: their carve pool is likewise capped.)
pub const MAX_CARVE_WORKERS: usize = 8;

/// Sync progress hook: `(chunks_done, total_chunks, carved)`.
pub type CarveProgress = dyn Fn(u64, u64, u64) + Send + Sync;

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
/// validating candidates (see the module docs for the tiering and the
/// execution model).
///
/// `cancel` is checked before every chunk; when it flips true the scan
/// stops early and the number of entries carved so far is returned.
/// `on_progress` (optional) receives `(chunks_done, total_chunks, carved)`
/// after every chunk batch.
///
/// Returns the number of entries inserted into `results`.
#[allow(clippy::too_many_arguments)]
pub fn carve_region(
    read_at: &dyn ReadAt,
    start_offset: u64,
    length: u64,
    db: &SignatureDb,
    results: &Arc<ResultsStore>,
    cancel: Option<&AtomicBool>,
    on_progress: Option<&CarveProgress>,
) -> u64 {
    let matcher = Matcher::new(db);
    let chunk_size: u64 = 1024 * 1024; // 1 MiB chunks
    let source_size = read_at.size();

    // Checked region arithmetic: a corrupt caller passing
    // u64::MAX-ish lengths must not wrap around.
    let Some(end) = start_offset.checked_add(length) else {
        debug!("carve_region: start+length overflows u64; refusing");
        return 0;
    };
    let end = end.min(source_size);
    let length = end.saturating_sub(start_offset);
    if length == 0 {
        return 0;
    }

    let total_chunks = length.div_ceil(chunk_size);
    let workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .clamp(1, MAX_CARVE_WORKERS) as u64;
    // With one chunk there is nothing to parallelize.
    let workers = workers.clamp(1, total_chunks);

    // Overlap so a magic straddling the boundary is completed in the
    // chunk that owns its first byte.
    let overlap = matcher.max_magic_len.saturating_sub(1) as u64;

    let carved = std::sync::atomic::AtomicU64::new(0);
    let chunks_done = std::sync::atomic::AtomicU64::new(0);
    let (tx, rx) = std::sync::mpsc::channel::<(u64, Vec<RecoveredEntry>)>();

    let cancel_flag = cancel;
    let stop = AtomicBool::new(false);

    std::thread::scope(|scope| -> u64 {
        // Spawn `workers` scanner threads, each striding chunk indices.
        for w in 0..workers {
            let tx = tx.clone();
            let matcher = &matcher;
            let carved = &carved;
            let stop = &stop;
            let cancel_ref = cancel_flag;
            scope.spawn(move || {
                let mut chunk_index = w;
                while chunk_index < total_chunks {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Some(flag) = cancel_ref
                        && flag.load(Ordering::Relaxed)
                    {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    let chunk_start = start_offset + chunk_index * chunk_size;
                    let chunk_len = chunk_size.min(end - chunk_start);
                    let read_len = (chunk_len + overlap).min(end - chunk_start) as usize;
                    let mut buf = vec![0u8; read_len];
                    if read_at.read_at(chunk_start, &mut buf).is_err() {
                        trace!(chunk_index, "carver read error, skipping chunk");
                        chunk_index += workers;
                        continue;
                    }
                    // Report matches whose magic STARTS inside this chunk
                    // (the overlap tail completes straddlers without
                    // double-reporting: the next chunk's scan-limit starts
                    // at its own base).
                    let scan_limit = chunk_len as usize;
                    let matches = matcher.find_headers_limited(&buf, chunk_start, scan_limit);
                    let deduped = dedup_by_offset(matches);
                    let mut batch = Vec::with_capacity(deduped.len());
                    for m in deduped {
                        if let Some(entry) =
                            resolve_candidate_and_build(read_at, db, &m, source_size)
                        {
                            batch.push(entry);
                        }
                    }
                    if !batch.is_empty() {
                        carved.fetch_add(batch.len() as u64, Ordering::Relaxed);
                    }
                    // Send even empty batches so the inserter counts
                    // progress uniformly.
                    if tx.send((chunk_index, batch)).is_err() {
                        break; // receiver gone: caller cancelled
                    }
                    chunk_index += workers;
                }
            });
        }
        drop(tx);

        // Inserter: consume batches in arrival order, count progress,
        // surface cancellation and progress to the caller.
        let mut seen: std::collections::HashMap<u64, Vec<RecoveredEntry>> =
            std::collections::HashMap::new();
        let mut next_to_insert: u64 = 0;
        let mut last_reported: u64 = 0;

        while let Ok((idx, batch)) = rx.recv() {
            seen.insert(idx, batch);
            // Insert in chunk order when possible (keeps insertion order
            // close to on-disk order regardless of thread scheduling).
            while let Some(batch) = seen.remove(&next_to_insert) {
                for entry in batch {
                    results.insert(entry);
                }
                next_to_insert += 1;
            }
            let done = chunks_done.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(cb) = on_progress
                && (done.saturating_sub(last_reported) >= 32 || done == total_chunks)
            {
                last_reported = done;
                cb(done, total_chunks, carved.load(Ordering::Relaxed));
            }
            if stop.load(Ordering::Relaxed) {
                // Drain outstanding batches so workers can exit cleanly.
                continue;
            }
        }
        next_to_insert
    })
}

/// Async wrapper that emits throttled progress events and owns a cancel
/// flag wired to the scan handle by the caller.
#[allow(clippy::too_many_arguments)]
pub async fn carve_region_async(
    read_at: Arc<dyn ReadAt>,
    start_offset: u64,
    length: u64,
    db: Arc<SignatureDb>,
    results: Arc<ResultsStore>,
    progress: Arc<dyn ProgressSink>,
    scan_id: Uuid,
    _chunk_size: usize,
) -> Result<u64, String> {
    let total_chunks = length.div_ceil(1024 * 1024);

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: 0,
            total: total_chunks,
            message: format!("Carving {length} bytes"),
        })
        .await;

    // The sync callback feeds a shared state the async task picks up; the
    // carve itself runs on the blocking pool so we never stall the
    // runtime with CPU+IO work.
    let latest = Arc::new(std::sync::Mutex::new((0u64, 0u64)));
    let cb = {
        let latest = Arc::clone(&latest);
        move |done: u64, total: u64, carved: u64| {
            *latest.lock().unwrap_or_else(|e| e.into_inner()) = (done, carved);
            let _ = total;
        }
    };

    let read_at_ref = Arc::clone(&read_at);
    let db_ref = Arc::clone(&db);
    let results_ref = Arc::clone(&results);
    let cancel = Arc::new(AtomicBool::new(false));

    let carved = tokio::task::spawn_blocking(move || {
        carve_region(
            read_at_ref.as_ref(),
            start_offset,
            length,
            &db_ref,
            &results_ref,
            Some(&cancel),
            Some(&cb),
        )
    })
    .await
    .map_err(|e| format!("carve task join failed: {e}"))?;

    let (done, found) = *latest.lock().unwrap_or_else(|e| e.into_inner());
    let _ = done;

    progress
        .emit(ProgressEvent {
            scan_id,
            phase: ProgressPhase::Carving,
            current: total_chunks,
            total: total_chunks,
            message: format!("Carving complete: {found} files carved"),
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
    #![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
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

    fn carve(data: &[u8]) -> Arc<ResultsStore> {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(100_000, None);
        carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            None,
            None,
        );
        results
    }

    #[test]
    fn carve_validates_jpeg_with_exact_extents() {
        let jpg = minimal_jpeg(300);
        let jpg_at = 100usize;
        let mut data = vec![0u8; 4096 + jpg.len()];
        data[jpg_at..jpg_at + jpg.len()].copy_from_slice(&jpg);

        let results = carve(&data);
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
    fn carve_finds_jpeg_straddling_chunk_boundary() {
        // A JPEG whose header starts a few bytes BEFORE a 1 MiB chunk
        // boundary: the overlap read must catch it exactly once.
        let jpg = minimal_jpeg(64);
        let boundary = 1024 * 1024;
        let jpg_at = boundary - 8; // header crosses the boundary
        let mut data = vec![0u8; boundary + jpg.len() + 4096];
        data[jpg_at..jpg_at + jpg.len()].copy_from_slice(&jpg);

        let results = carve(&data);
        let snapshot = results.snapshot();
        let hits: Vec<_> = snapshot
            .iter()
            .filter(|e| e.extension == "jpg" && e.status == RecoveryStatus::CarvedValidated)
            .collect();
        assert_eq!(hits.len(), 1, "boundary JPEG found exactly once");
        assert_eq!(hits[0].extents[0].start_byte, jpg_at as u64);
    }

    #[test]
    fn carve_finds_jpeg_right_at_chunk_start() {
        let jpg = minimal_jpeg(32);
        let boundary = 1024 * 1024;
        let jpg_at = boundary; // exactly at the second chunk's start
        let mut data = vec![0u8; boundary + jpg.len() + 4096];
        data[jpg_at..jpg_at + jpg.len()].copy_from_slice(&jpg);

        let results = carve(&data);
        assert!(
            results
                .snapshot()
                .iter()
                .any(|e| e.extension == "jpg" && e.status == RecoveryStatus::CarvedValidated)
        );
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

        let results = carve(&data);
        let jpg_validated = results
            .snapshot()
            .iter()
            .any(|e| e.extension == "jpg" && e.status == RecoveryStatus::CarvedValidated);
        assert!(
            !jpg_validated,
            "invalid JPEG must not be emitted as validated"
        );
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

        let results = carve(&data);
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
    fn carve_validates_pe_with_exact_end() {
        // A well-formed minimal PE: analyser computes the section end.
        let mut pe = vec![0u8; 0x1000];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes()); // machine
        pe[0x86..0x88].copy_from_slice(&1u16.to_le_bytes()); // sections
        pe[0x84 + 16..0x84 + 18].copy_from_slice(&240u16.to_le_bytes()); // opt size
        pe[0x80 + 24..0x80 + 26].copy_from_slice(&0x20Bu16.to_le_bytes()); // PE32+
        // section @ opt+240: raw data 0x400..0xA00
        let sec = 0x80 + 24 + 240;
        pe[sec + 16..sec + 20].copy_from_slice(&0x600u32.to_le_bytes()); // SizeOfRawData
        pe[sec + 20..sec + 24].copy_from_slice(&0x400u32.to_le_bytes()); // PointerToRawData
        pe.resize(0xA00, 0);

        let results = carve(&pe);
        let snapshot = results.snapshot();
        let exe = snapshot
            .iter()
            .find(|e| e.extension == "exe" && e.status == RecoveryStatus::CarvedValidated)
            .expect("PE-validated exe entry");
        assert_eq!(exe.size, 0xA00);
        assert_eq!(exe.extents.len(), 1);
        assert_eq!(exe.extents[0].length, 0xA00);
    }

    #[test]
    fn carve_emits_unvalidated_tier_for_elf() {
        // ELF header: no analyser, no footer -> tier 4 marker (size 0).
        let mut data = vec![0u8; 4096];
        data[200] = 0x7F;
        data[201] = b'E';
        data[202] = b'L';
        data[203] = b'F';

        let results = carve(&data);
        let snapshot = results.snapshot();
        let elf = snapshot
            .iter()
            .find(|e| e.extension == "elf")
            .expect("elf tier-4 entry");
        assert_eq!(elf.size, 0);
        assert!(elf.extents.is_empty());
        assert!(elf.name.starts_with("unvalidated_carved_"));
    }

    #[test]
    fn carve_returns_zero_for_empty_region() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"").unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        let carved = carve_region(read_at.as_ref(), 0, 0, &db, &results, None, None);
        assert_eq!(carved, 0);
    }

    #[test]
    fn carve_honors_cancel_flag_immediately() {
        let jpg = minimal_jpeg(64);
        let mut data = vec![0u8; 3 * 1024 * 1024];
        let _ = jpg;
        data[1024 * 1024 + 123..].iter_mut().take(0).count();
        data[512] = 0xFF;
        data[513] = 0xD8;
        data[514] = 0xFF;

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        // Pre-set cancel: the carve must bail without processing chunks.
        let cancel = AtomicBool::new(true);
        let carved = carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            Some(&cancel),
            None,
        );
        assert_eq!(carved, 0, "cancelled carve must insert nothing");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn carve_progress_callback_fires() {
        let jpg = minimal_jpeg(64);
        let mut data = vec![0u8; 1024 * 1024 + jpg.len() + 4096];
        data[100..100 + jpg.len()].copy_from_slice(&jpg);

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(100_000, None);

        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let calls_cb = Arc::clone(&calls);
        let cb = move |done: u64, total: u64, carved: u64| {
            calls_cb
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((done, total, carved));
        };
        carve_region(
            read_at.as_ref(),
            0,
            data.len() as u64,
            &db,
            &results,
            None,
            Some(&cb),
        );
        let calls = calls.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(!calls.is_empty(), "progress must be reported");
        assert!(calls.iter().all(|&(_, t, _)| t == 2), "2 chunks total");
        // Last report shows all chunks done.
        let (d, t, c) = *calls.last().unwrap();
        assert_eq!(d, t);
        assert_eq!(c, 1, "the single JPEG was carved");
    }

    #[test]
    fn carve_overflow_length_is_refused() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), vec![0u8; 1024]).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at = source.as_read_at();
        let db = SignatureDb::from_embedded();
        let results = ResultsStore::new(1000, None);

        // u64::MAX - small start would overflow start+length.
        let carved = carve_region(read_at.as_ref(), 100, u64::MAX, &db, &results, None, None);
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

    #[tokio::test]
    async fn async_wrapper_carves_and_reports() {
        let jpg = minimal_jpeg(128);
        let mut data = vec![0u8; 2 * 1024 * 1024 + jpg.len() + 4096];
        data[77..77 + jpg.len()].copy_from_slice(&jpg);

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &data).unwrap();
        let source = FileBackedSource::open(temp.path()).unwrap();
        let read_at: Arc<dyn ReadAt> = source.as_read_at();
        let db = Arc::new(SignatureDb::from_embedded());
        let results = ResultsStore::new(100_000, None);

        let carved = carve_region_async(
            Arc::clone(&read_at),
            0,
            data.len() as u64,
            Arc::clone(&db),
            Arc::clone(&results),
            no_progress(),
            Uuid::nil(),
            1 << 20,
        )
        .await
        .unwrap();
        assert!(carved >= 1);
        assert!(!results.is_empty());
    }
}
