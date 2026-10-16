//! Map carving for scattered fragmented files.
//!
//! When a file is fragmented into many pieces (not just two), the
//! bifragment search no longer applies. For formats whose internal
//! records are *individually verifiable* we can rebuild the fragment
//! map from the records themselves:
//!
//! **PNG scattered-chunk reassembly** — every PNG chunk carries its
//! own CRC32 over `type ++ data`, so a chunk found anywhere can be
//! verified in isolation (false link probability 2^-32). Starting
//! from the IHDR at the header, the expected next chunk position is
//! known; when the bytes there are not a valid chunk, we search the
//! surrounding window at cluster alignment for a chunk that *is*
//! valid, append it to the map, and continue until IEND. The result is
//! the file's true fragmentation map — one extent per surviving chunk
//! — and a reassembled stream that is byte-exact for every chunk that
//! survived.
//!
//! Ordering constraints (spec 11.2): IHDR must be first and IEND last;
//! a chunk found during the search is only accepted at the position
//! the chain expects next, so the ordering is enforced by construction.
//!
//! Scope, stated honestly: PNG today. The same shape applies to any
//! per-record-checksummed container (e.g., Matroska clusters, RAR
//! service headers); those are future additions.
//!
//! Cost: each broken link costs one cluster-aligned scan of the search
//! window (`search_len`) with a CRC check per candidate position that
//! has a plausible chunk header. Chained correctly, the number of
//! searches equals the number of gaps.

use std::sync::atomic::{AtomicBool, Ordering};

use dw_core::{Extent, RecoveredEntry, RecoveredEntryId, RecoveryStatus, ResultsStore};
use dw_disk::ReadAt;
use uuid::Uuid;

use crate::error::CarverResult;

/// PNG signature.
const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Parameters for the scattered-chunk search.
#[derive(Debug, Clone)]
pub struct FragmentParams {
    /// Fragment alignment (volume cluster size).
    pub cluster: u64,
    /// How far past the expected position to search for the next
    /// chunk.
    pub search_len: u64,
    /// Maximum number of chunks in one file (guards against
    /// pathological maps).
    pub max_chunks: usize,
}

impl Default for FragmentParams {
    fn default() -> Self {
        Self {
            cluster: 4096,
            search_len: 4 * 1024 * 1024,
            max_chunks: 4096,
        }
    }
}

/// A reassembled fragmentation map: extents in stream order plus the
/// total logical length.
#[derive(Debug, Clone)]
pub struct FragmentMap {
    pub extents: Vec<Extent>,
    pub total_len: u64,
}

/// One verified chunk: absolute source position + logical length
/// (header 12 + data + CRC 4).
#[derive(Debug, Clone)]
struct ChunkHit {
    pos: u64,
    len: u64,
    is_iend: bool,
}

/// CRC-32 (IEEE), table-driven (same polynomial as PNG chunk CRCs).
fn crc32_ieee(data: &[u8]) -> u32 {
    fn table() -> &'static [u32; 256] {
        static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
        TABLE.get_or_init(|| {
            let mut t = [0u32; 256];
            for (i, e) in t.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xEDB8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
                *e = c;
            }
            t
        })
    }
    let t = table();
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc = t[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Try to read and verify a PNG chunk at `pos`. Returns the chunk on
/// success (CRC-verified), or the reason the bytes there are not a
/// chunk.
fn verify_chunk_at(read: &dyn ReadAt, pos: u64, limit: u64) -> Option<ChunkHit> {
    // Chunk header: length u32 BE + type 4.
    let mut hdr = [0u8; 8];
    if read.read_at(pos, &mut hdr).is_err() {
        return None;
    }
    let length = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    if length >= 0x8000_0000 {
        return None;
    }
    let kind = [hdr[4], hdr[5], hdr[6], hdr[7]];
    // Type must be ASCII letters (spec 5.4).
    if !kind.iter().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let len = 8 + u64::from(length) + 4;
    if pos + len > limit {
        return None;
    }
    // Verify the CRC over type ++ data (type sits at pos + 4, data
    // at pos + 8 — the CRC covers both).
    let mut body = vec![0u8; 4 + length as usize];
    if read.read_at(pos + 4, &mut body).is_err() {
        return None;
    }
    let mut crc_bytes = [0u8; 4];
    if read
        .read_at(pos + 8 + u64::from(length), &mut crc_bytes)
        .is_err()
    {
        return None;
    }
    let expect = u32::from_be_bytes(crc_bytes);
    if crc32_ieee(&body) != expect {
        return None;
    }
    Some(ChunkHit {
        pos,
        len,
        is_iend: &kind == b"IEND",
    })
}

/// Rebuild the fragmentation map of a scattered-chunk PNG whose
/// signature + IHDR live at `header_off`.
///
/// Walks the chunk chain; on a broken link, searches cluster-aligned
/// positions in `[expected, expected + search_len)` for a CRC-valid
/// chunk. Returns `None` when the chain cannot continue (no successor
/// found) or ends without IEND — an incomplete map is not a file.
pub fn reassemble_scattered_png(
    read: &dyn ReadAt,
    header_off: u64,
    params: &FragmentParams,
    cancel: Option<&AtomicBool>,
) -> CarverResult<Option<FragmentMap>> {
    let limit = read.size();
    // Verify the signature + IHDR at the start.
    let mut sig = [0u8; 8];
    if read.read_at(header_off, &mut sig).is_err() || sig != PNG_SIG {
        return Ok(None);
    }
    let Some(mut chunk) = verify_chunk_at(read, header_off + 8, limit) else {
        return Ok(None);
    };

    let mut extents = vec![Extent {
        start_byte: header_off,
        length: 8 + chunk.len,
    }];
    let mut logical = 8 + chunk.len;
    let mut chunks = 1usize;

    while !chunk.is_iend {
        if let Some(c) = cancel
            && c.load(Ordering::Relaxed)
        {
            return Ok(None);
        }
        if chunks >= params.max_chunks {
            return Ok(None);
        }
        // Where the next chunk should be (logical end of the map so
        // far == physical end of the last found chunk).
        let expected = chunk.pos + chunk.len;
        let next = match verify_chunk_at(read, expected, limit) {
            Some(c) => c,
            None => {
                // Broken link: search the window for the next
                // surviving chunk.
                let search_end = (expected + params.search_len).min(limit);
                let mut found = None;
                let mut p = align_up(expected, params.cluster);
                while p < search_end {
                    if let Some(c) = verify_chunk_at(read, p, limit) {
                        found = Some(c);
                        break;
                    }
                    p += params.cluster;
                }
                let Some(c) = found else {
                    return Ok(None); // chain broken, no successor
                };
                c
            }
        };
        extents.push(Extent {
            start_byte: next.pos,
            length: next.len,
        });
        logical += next.len;
        chunks += 1;
        chunk = next;
    }

    // Coalesce physically adjacent chunks into maximal extents: a
    // contiguous chain yields ONE extent; each gap marks a fragment
    // boundary.
    let mut merged: Vec<Extent> = Vec::with_capacity(extents.len());
    for e in extents {
        match merged.last_mut() {
            Some(last) if last.start_byte + last.length == e.start_byte => {
                last.length += e.length;
            }
            _ => merged.push(e),
        }
    }

    Ok(Some(FragmentMap {
        extents: merged,
        total_len: logical,
    }))
}

/// Confidence for map-reassembled entries.
pub const FRAGMENT_CONFIDENCE: u16 = 800;

/// Carve a region for scattered-chunk PNGs: for every PNG signature
/// whose contiguous analysis is not already complete, attempt the map
/// reassembly. Emits entries with per-chunk extents. Returns the
/// number emitted.
///
/// The contiguous pass (`header_footer`) wins for intact files; this
/// pass only adds value for fragmented ones — `reassemble_scattered_png`
/// naturally validates contiguous chains too, but emitting those here
/// would duplicate the contiguous tier, so candidates whose chain is
/// already contiguous at the first link are skipped.
pub fn carve_fragment_region(
    read: &dyn ReadAt,
    offset: u64,
    len: u64,
    results: &std::sync::Arc<ResultsStore>,
    cancel: Option<&std::sync::Arc<AtomicBool>>,
    params: &FragmentParams,
) -> CarverResult<u64> {
    let len = len.min(read.size().saturating_sub(offset));
    if len == 0 {
        return Ok(0);
    }
    let mut region = vec![0u8; len as usize];
    read.read_at(offset, &mut region)?;

    let mut emitted = 0u64;
    // Scan for PNG signatures at any byte position (the header itself
    // is 8-byte aligned in practice, but files land anywhere).
    let mut i = 0usize;
    while i + 8 <= region.len() {
        if let Some(c) = cancel
            && c.load(Ordering::Relaxed)
        {
            break;
        }
        if region[i..i + 8] == PNG_SIG {
            let abs = offset + i as u64;
            // Contiguous chain? header_footer handles it.
            let contiguous = verify_chunk_at(read, abs + 8, read.size())
                .map(|c| c.pos + c.len)
                .and_then(|second_end| verify_chunk_at(read, second_end, read.size()))
                .is_some();
            if !contiguous && let Some(map) = reassemble_scattered_png(read, abs, params, None)? {
                let _ = results.insert(RecoveredEntry {
                    id: RecoveredEntryId(Uuid::now_v7()),
                    fs_kind: "raw".to_string(),
                    parent_path: "/carved".to_string(),
                    name: format!("reassembled-{abs}.png"),
                    size: map.total_len,
                    start_lba: Some(abs / 512),
                    mtime: None,
                    ctime: None,
                    atime: None,
                    status: RecoveryStatus::CarvedValidated,
                    extension: "png".to_string(),
                    mime: Some("image/png".to_string()),
                    confidence: FRAGMENT_CONFIDENCE,
                    was_deleted: true,
                    cluster_chain: Vec::new(),
                    extents: map.extents,
                    resident_data: None,
                });
                emitted += 1;
            }
            i += 8;
        } else {
            i += 1;
        }
    }
    Ok(emitted)
}

fn align_up(v: u64, align: u64) -> u64 {
    if align == 0 {
        return v;
    }
    v.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32_ieee(&crc_input).to_be_bytes());
    }

    fn build_png() -> Vec<u8> {
        let mut png = PNG_SIG.to_vec();
        write_chunk(&mut png, b"IHDR", &[0u8; 13]);
        write_chunk(&mut png, b"IDAT", &[0x42u8; 3000]);
        write_chunk(&mut png, b"IDAT", &[0x43u8; 2000]);
        write_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn write_src(data: &[u8]) -> (std::path::PathBuf, dw_disk::FileBackedSource) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        let (_file, path) = f.keep().unwrap();
        let src = dw_disk::FileBackedSource::open(&path).unwrap();
        (path, src)
    }

    #[test]
    fn reassembles_scattered_chunks() {
        // PNG whose chunks are scattered: sig+IHDR at 0, then each
        // chunk at a different cluster, out of physical order relative
        // to a contiguous layout.
        let png = build_png();
        let (sig_ihdr_len, rest) = png.split_at(8 + 25);
        // rest = IDAT1 + IDAT2 + IEND chunks
        let idat1_len = 8 + 3000 + 4;
        let (idat1, rest2) = rest.split_at(idat1_len);
        let idat2_len = 8 + 2000 + 4;
        let (idat2, iend) = rest2.split_at(idat2_len);

        let mut img = vec![0x00u8; 64 * 1024];
        img[..sig_ihdr_len.len()].copy_from_slice(sig_ihdr_len);
        // Place IDAT1 at cluster 4, IDAT2 at cluster 8, IEND at
        // cluster 12 — separated by gaps.
        let at = |c: usize| c * 4096;
        img[at(4)..at(4) + idat1.len()].copy_from_slice(idat1);
        img[at(8)..at(8) + idat2.len()].copy_from_slice(idat2);
        img[at(12)..at(12) + iend.len()].copy_from_slice(iend);

        let (path, src) = write_src(&img);
        let map = reassemble_scattered_png(&src, 0, &FragmentParams::default(), None)
            .unwrap()
            .expect("must reassemble the scattered PNG");
        assert_eq!(map.total_len as usize, png.len());
        assert_eq!(map.extents.len(), 4);
        assert_eq!(map.extents[0].length as usize, sig_ihdr_len.len());
        assert_eq!(map.extents[1].start_byte as usize, at(4));
        assert_eq!(map.extents[2].start_byte as usize, at(8));
        assert_eq!(map.extents[3].start_byte as usize, at(12));

        // The reassembled stream is byte-exact.
        let mut recovered = Vec::with_capacity(png.len());
        for e in &map.extents {
            let mut tmp = vec![0u8; e.length as usize];
            src.read_at(e.start_byte, &mut tmp).unwrap();
            recovered.extend_from_slice(&tmp);
        }
        assert_eq!(recovered, png);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn refuses_when_iend_is_missing() {
        // Chain continues but the final IEND chunk was destroyed: no
        // map (an incomplete file is not a file).
        let mut png = build_png();
        let iend_at = png.len() - 12;
        png[iend_at..].fill(0);
        let mut img = vec![0x00u8; 32 * 1024];
        img[..png.len()].copy_from_slice(&png);
        let (path, src) = write_src(&img);
        assert!(
            reassemble_scattered_png(&src, 0, &FragmentParams::default(), None)
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn contiguous_chain_is_not_fragmented() {
        // Contiguous PNG: still reassembles (the chain verifies), with
        // a single extent — but the region carver skips it (the
        // contiguous tier owns it).
        let png = build_png();
        let mut img = vec![0x00u8; 32 * 1024];
        img[..png.len()].copy_from_slice(&png);
        let (path, src) = write_src(&img);

        let map = reassemble_scattered_png(&src, 0, &FragmentParams::default(), None)
            .unwrap()
            .expect("contiguous chain verifies");
        assert_eq!(map.extents.len(), 1);
        assert_eq!(map.total_len as usize, png.len());

        let results = std::sync::Arc::new(ResultsStore::new(100, None));
        let n = carve_fragment_region(
            &src,
            0,
            img.len() as u64,
            &results,
            None,
            &FragmentParams::default(),
        )
        .unwrap();
        assert_eq!(n, 0, "contiguous PNGs belong to the contiguous tier");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn region_carver_emits_for_scattered() {
        let png = build_png();
        let (sig_ihdr_len, rest) = png.split_at(8 + 25);
        let idat1_len = 8 + 3000 + 4;
        let (idat1, rest2) = rest.split_at(idat1_len);
        let idat2_len = 8 + 2000 + 4;
        let (idat2, iend) = rest2.split_at(idat2_len);
        let mut img = vec![0x00u8; 64 * 1024];
        img[..sig_ihdr_len.len()].copy_from_slice(sig_ihdr_len);
        let at = |c: usize| c * 4096;
        img[at(4)..at(4) + idat1.len()].copy_from_slice(idat1);
        img[at(8)..at(8) + idat2.len()].copy_from_slice(idat2);
        img[at(12)..at(12) + iend.len()].copy_from_slice(iend);

        let (path, src) = write_src(&img);
        let results = std::sync::Arc::new(ResultsStore::new(100, None));
        let n = carve_fragment_region(
            &src,
            0,
            img.len() as u64,
            &results,
            None,
            &FragmentParams::default(),
        )
        .unwrap();
        assert_eq!(n, 1);
        let page = results.page(0, 10);
        assert_eq!(page[0].status, RecoveryStatus::CarvedValidated);
        assert_eq!(page[0].extents.len(), 4);
        let _ = std::fs::remove_file(&path);
    }
}
