//! Bifragment gap carving (Richard & Roussev, "Forensic File Carving",
//! 2006, §Gap Carving).
//!
//! A file split into exactly two fragments by a small overwriting gap:
//!
//! ```text
//!   [ fragment 1 ][   gap (foreign)   ][ fragment 2 ]
//!   ^H            ^X                    ^Y
//! ```
//!
//! For every cluster-aligned `(X, Y)` split inside the search window we
//! reassemble `[H..X) ++ [Y..Y + rest)` and hand it to the per-format
//! structure analyser. The first split that validates end-to-end wins
//! — the analyser is the arbiter, not a checksum guess.
//!
//! Scope, stated honestly: only structure-chained formats participate
//! (PNG, ZIP, MP4, PE). JPEG is excluded by design: its entropy-coded
//! scan data accepts arbitrary continuation bytes, so a wrong split
//! still "validates" (the first `FF D9` in the reassembled stream
//! terminates it) and the search would fabricate files. Chained
//! grammars break immediately at a wrong junction — a false positive
//! requires a structurally valid accidental continuation, which the
//! chain + CRC/EOCD checks make negligible.
//!
//! Cost model: the search is `(fragment-1 candidates) x (gap
//! candidates)` analyser probes. Each probe starts with a small
//! reassembled prefix (`probe` bytes of fragment 2) — wrong splits fail
//! the chain walk inside the probe and are rejected at probe cost;
//! promising splits grow geometrically to `max_file`. Defaults keep a
//! full search under ~100 MiB of memcpy per header candidate.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dw_core::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use dw_disk::ReadAt;
use dw_signatures::SignatureDb;
use uuid::Uuid;

use crate::analysers::{AnalyserKind, AnalyserOutcome, analyse, analyser_for};
use crate::error::CarverResult;

/// Search parameters; defaults are tuned for cluster-granular
/// fragmentation with small overwrite gaps.
#[derive(Debug, Clone)]
pub struct BifragmentParams {
    /// Fragment alignment (volume cluster size).
    pub cluster: u64,
    /// Smallest fragment 1 we consider (a header alone proves nothing).
    pub min_fragment1: u64,
    /// Largest fragment 1 we consider.
    pub max_fragment1: u64,
    /// Largest overwriting gap between the fragments.
    pub max_gap: u64,
    /// Total reassembled-size ceiling.
    pub max_file: u64,
    /// Initial fragment-2 probe length (grows geometrically).
    pub probe: usize,
}

impl Default for BifragmentParams {
    fn default() -> Self {
        Self {
            cluster: 4096,
            min_fragment1: 8 * 1024,
            max_fragment1: 256 * 1024,
            max_gap: 64 * 1024,
            max_file: 4 * 1024 * 1024,
            probe: 64 * 1024,
        }
    }
}

/// A successful two-fragment reassembly.
#[derive(Debug, Clone)]
pub struct BifragmentHit {
    /// The two data runs (ordered).
    pub extents: Vec<Extent>,
    /// Total reassembled length.
    pub total_len: u64,
    /// Analyser that validated the reassembly.
    pub kind: AnalyserKind,
}

/// Try to reassemble a two-fragment file whose header sits at
/// `header_off`.
///
/// Returns `Ok(None)` when the contiguous analysis already validates
/// (the file is not fragmented — `header_footer`'s territory), when no
/// split in the search window validates, or when the format does not
/// participate. Returns an error only on read failures.
pub fn try_bifragment(
    read: &dyn ReadAt,
    header_off: u64,
    extension: &str,
    params: &BifragmentParams,
    cancel: Option<&AtomicBool>,
) -> CarverResult<Option<BifragmentHit>> {
    // JPEG excluded: entropy data accepts arbitrary continuations (see
    // module docs).
    let Some(kind) = analyser_for(extension) else {
        return Ok(None);
    };
    if kind == AnalyserKind::Jpeg {
        return Ok(None);
    }

    let win_len = params.max_file.min(read.size().saturating_sub(header_off)) as usize;
    if win_len < params.min_fragment1 as usize + params.cluster as usize {
        return Ok(None);
    }
    let mut win = vec![0u8; win_len];
    read.read_at(header_off, &mut win)?;

    // Not fragmented? Let header_footer handle it.
    if let AnalyserOutcome::Validated(_) = analyse(kind, &win) {
        return Ok(None);
    }

    let cluster = params.cluster as usize;
    let f1_start = (params.min_fragment1 as usize).max(cluster);
    let f1_end = (params.max_fragment1 as usize).min(win_len.saturating_sub(cluster));
    let mut frag1 = f1_start;
    while frag1 <= f1_end {
        if let Some(c) = cancel
            && c.load(Ordering::Relaxed)
        {
            return Ok(None);
        }
        // Y starts one cluster past X, gap <= max_gap, and fragment 2
        // must have at least one cluster of room.
        let y_min = frag1 + cluster;
        let y_max = (frag1 + params.max_gap as usize).min(win_len.saturating_sub(cluster));
        let mut y = y_min;
        while y <= y_max {
            if let Some(hit) = probe_split(kind, &win, frag1, y, params) {
                // Convert to absolute extents.
                let e1 = Extent {
                    start_byte: header_off,
                    length: frag1 as u64,
                };
                let e2 = Extent {
                    start_byte: header_off + y as u64,
                    length: hit,
                };
                return Ok(Some(BifragmentHit {
                    extents: vec![e1, e2],
                    total_len: frag1 as u64 + hit,
                    kind,
                }));
            }
            y += cluster;
        }
        frag1 += cluster;
    }
    Ok(None)
}

/// Analyse the reassembly `[0..x) ++ [y..)` with a geometrically
/// growing fragment-2 probe. Returns the validated fragment-2 length.
fn probe_split(
    kind: AnalyserKind,
    win: &[u8],
    x: usize,
    y: usize,
    params: &BifragmentParams,
) -> Option<u64> {
    let mut probe = params.probe.min(win.len() - y);
    loop {
        let mut buf = Vec::with_capacity(x + probe);
        buf.extend_from_slice(&win[..x]);
        buf.extend_from_slice(&win[y..y + probe]);
        match analyse(kind, &buf) {
            AnalyserOutcome::Validated(end) => {
                // The validated end must land inside fragment 2
                // (beyond the junction) — an end inside fragment 1
                // means the split is spurious.
                if end as usize > x {
                    return Some((end as usize - x) as u64);
                }
                return None;
            }
            AnalyserOutcome::Invalid => return None,
            AnalyserOutcome::NeedMore => {
                if probe >= win.len() - y {
                    // Fragment 2 exhausted without validation.
                    return None;
                }
                probe = (probe * 2).min(win.len() - y);
            }
        }
    }
}

/// Confidence for bifragment-reassembled entries (structure-validated,
/// but the reassembly itself is inferred).
pub const BIFRAGMENT_CONFIDENCE: u16 = 850;

/// Scan a region for headers whose contiguous analysis fails and try
/// bifragment reassembly for each. Emits validated two-extent entries
/// into `results`. Returns the number of entries emitted.
///
/// This is the deep-mode companion to `header_footer::carve_region`:
/// run it over the same region after the contiguous pass — every
/// header it already validated is skipped here (`try_bifragment`
/// declines contiguous files).
pub fn carve_bifragment_region(
    read: &dyn ReadAt,
    offset: u64,
    len: u64,
    db: &SignatureDb,
    results: &Arc<ResultsStore>,
    cancel: Option<&Arc<AtomicBool>>,
    params: &BifragmentParams,
) -> CarverResult<u64> {
    let matcher = dw_signatures::Matcher::new(db);

    let len = len.min(read.size().saturating_sub(offset));
    if len == 0 {
        return Ok(0);
    }
    // Read the region once (bounded so the scan itself is cheap;
    // per-candidate windows are read separately by try_bifragment).
    let scan_len = len.min(64 * 1024 * 1024) as usize;
    let mut region = vec![0u8; scan_len];
    read.read_at(offset, &mut region)?;

    let hits = matcher.find_headers(&region, offset);
    let mut emitted = 0u64;
    for hit in hits {
        if let Some(c) = cancel
            && c.load(Ordering::Relaxed)
        {
            break;
        }
        let abs = hit.offset;
        let Ok(Some(reassembly)) = try_bifragment(read, abs, &hit.extension, params, None) else {
            continue;
        };
        let ext = hit.extension.clone();
        let _ = results.insert(RecoveredEntry {
            id: RecoveredEntryId(Uuid::now_v7()),
            fs_kind: "raw".to_string(),
            parent_path: "/carved".to_string(),
            name: format!("fragmented.{ext}"),
            size: reassembly.total_len,
            start_lba: Some(abs / 512),
            mtime: None,
            ctime: None,
            atime: None,
            status: RecoveryStatus::CarvedValidated,
            extension: ext,
            mime: None,
            confidence: BIFRAGMENT_CONFIDENCE,
            was_deleted: true,
            cluster_chain: Vec::new(),
            extents: reassembly.extents,
            resident_data: None,
        });
        emitted += 1;
    }
    Ok(emitted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dw_disk::FileBackedSource;
    use std::io::Write;

    fn no_cancel() -> Option<&'static AtomicBool> {
        None
    }

    /// Build a valid PNG of roughly `payload` bytes: IHDR + one big
    /// IDAT + IEND.
    fn build_png(payload: usize) -> Vec<u8> {
        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR
        let ihdr_data = [0u8; 13];
        write_chunk(&mut png, b"IHDR", &ihdr_data);
        // IDAT
        let idat = vec![0xAB; payload];
        write_chunk(&mut png, b"IDAT", &idat);
        // IEND
        write_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(data);
        let crc = crc32(&crc_input);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    /// CRC-32 (IEEE), table-free bitwise implementation (test only).
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn file_source(data: &[u8]) -> std::path::PathBuf {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        let (_file, path) = f.keep().unwrap();
        path
    }

    #[test]
    fn reassembles_two_fragment_png() {
        // PNG split at a cluster boundary with a 2-cluster gap.
        let png = build_png(96 * 1024);
        let split = 8 * 4096; // inside the IDAT payload
        let gap = 2 * 4096;
        let mut img = vec![0u8; 1024 * 1024];
        img[0..split].copy_from_slice(&png[..split]);
        let second_at = split + gap;
        img[second_at..second_at + (png.len() - split)].copy_from_slice(&png[split..]);

        let path = file_source(&img);
        let src = FileBackedSource::open(&path).unwrap();
        let hit = try_bifragment(&src, 0, "png", &BifragmentParams::default(), no_cancel())
            .unwrap()
            .expect("must reassemble");
        assert_eq!(hit.extents.len(), 2);
        assert_eq!(hit.extents[0].length as usize, split);
        assert_eq!(hit.extents[1].start_byte as usize, second_at);
        assert_eq!(hit.total_len as usize, png.len());
        // The reassembled content matches the original byte for byte.
        let mut recovered = vec![0u8; png.len()];
        let mut w = 0usize;
        for e in &hit.extents {
            let mut tmp = vec![0u8; e.length as usize];
            src.read_at(e.start_byte, &mut tmp).unwrap();
            recovered[w..w + tmp.len()].copy_from_slice(&tmp);
            w += tmp.len();
        }
        assert_eq!(&recovered, &png);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn declines_contiguous_file() {
        // Not fragmented: contiguous analysis validates, so
        // bifragment must decline (header_footer's territory).
        let png = build_png(96 * 1024);
        let mut img = vec![0u8; 1024 * 1024];
        img[..png.len()].copy_from_slice(&png);
        let path = file_source(&img);
        let src = FileBackedSource::open(&path).unwrap();
        assert!(
            try_bifragment(&src, 0, "png", &BifragmentParams::default(), no_cancel())
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn declines_jpeg_by_design() {
        // Entropy ambiguity: JPEG never participates.
        let mut img = vec![0u8; 64 * 1024];
        img[..2].copy_from_slice(&[0xFF, 0xD8]);
        img[2..5].copy_from_slice(&[0xFF, 0xE0, 0x00]);
        let path = file_source(&img);
        let src = FileBackedSource::open(&path).unwrap();
        assert!(
            try_bifragment(&src, 0, "jpg", &BifragmentParams::default(), no_cancel())
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_split_when_gap_exceeds_window() {
        // A gap beyond max_gap must not be found.
        let png = build_png(96 * 1024);
        let split = 8 * 4096;
        let gap = 512 * 1024; // >> max_gap (64 KiB)
        let mut img = vec![0u8; 2 * 1024 * 1024];
        img[0..split].copy_from_slice(&png[..split]);
        let second_at = split + gap;
        img[second_at..second_at + (png.len() - split)].copy_from_slice(&png[split..]);
        let path = file_source(&img);
        let src = FileBackedSource::open(&path).unwrap();
        assert!(
            try_bifragment(&src, 0, "png", &BifragmentParams::default(), no_cancel())
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }
}
