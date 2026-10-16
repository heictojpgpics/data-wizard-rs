//! zio decompression for the algorithms a scanner can encounter on
//! real-world pools: OFF (identity), LZJB, GZIP-1..9 (raw DEFLATE),
//! ZLE, LZ4 (OpenZFS layout: 4-byte big-endian payload length, then a
//! standard LZ4 block - `lz4_zfs.c`), and ZSTD (decode-only, `ruzstd`).
//!
//! Every algorithm must expand to exactly `lsize` bytes; anything else is
//! corruption.

use crate::blkptr::compress as c;
use crate::error::{ZfsError, ZfsResult};
use std::io::Read;

/// Decompress `raw` (the on-disk `psize` bytes) into exactly `lsize` bytes.
/// `off` is the physical offset, used for error reporting only.
pub fn decompress(algo: u8, raw: &[u8], lsize: u64, off: u64) -> ZfsResult<Vec<u8>> {
    match algo {
        c::OFF => {
            if raw.len() as u64 != lsize {
                return Err(ZfsError::Corrupt(format!(
                    "uncompressed block psize {} != lsize {lsize}",
                    raw.len()
                )));
            }
            Ok(raw.to_vec())
        }
        c::EMPTY => {
            // "Empty" blocks are fully-zero: the on-disk payload is
            // omitted (psize 0) and the block expands to zeros.
            if !raw.is_empty() {
                return Err(ZfsError::Corrupt("EMPTY block carries payload".into()));
            }
            Ok(vec![0u8; lsize as usize])
        }
        c::LZJB => {
            let mut out = vec![0u8; lsize as usize];
            if !lzjb_decompress(raw, &mut out) {
                return Err(ZfsError::DecompressFailed { off, algo: "lzjb" });
            }
            Ok(out)
        }
        x if (c::GZIP_1..=c::GZIP_9).contains(&x) => {
            let mut out = Vec::with_capacity(lsize as usize);
            flate2::read::DeflateDecoder::new(raw)
                .read_to_end(&mut out)
                .map_err(|_| ZfsError::DecompressFailed { off, algo: "gzip" })?;
            if out.len() as u64 != lsize {
                return Err(ZfsError::DecompressFailed { off, algo: "gzip" });
            }
            Ok(out)
        }
        c::ZLE => {
            let mut out = vec![0u8; lsize as usize];
            if !zle_decompress(raw, &mut out, 64) {
                return Err(ZfsError::DecompressFailed { off, algo: "zle" });
            }
            Ok(out)
        }
        c::LZ4 => {
            // 4-byte BE payload length, then the LZ4 block (`lz4_zfs.c`).
            if raw.len() < 4 {
                return Err(ZfsError::DecompressFailed { off, algo: "lz4" });
            }
            let payload_len = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
            if 4 + payload_len > raw.len() {
                return Err(ZfsError::DecompressFailed { off, algo: "lz4" });
            }
            let payload = &raw[4..4 + payload_len];
            let out = lz4_flex::block::decompress(payload, lsize as usize)
                .map_err(|_| ZfsError::DecompressFailed { off, algo: "lz4" })?;
            if out.len() as u64 != lsize {
                return Err(ZfsError::DecompressFailed { off, algo: "lz4" });
            }
            Ok(out)
        }
        c::ZSTD => {
            let mut out = Vec::with_capacity(lsize as usize);
            ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(raw))
                .map_err(|_| ZfsError::DecompressFailed { off, algo: "zstd" })?
                .read_to_end(&mut out)
                .map_err(|_| ZfsError::DecompressFailed { off, algo: "zstd" })?;
            if out.len() as u64 != lsize {
                return Err(ZfsError::DecompressFailed { off, algo: "zstd" });
            }
            Ok(out)
        }
        _ => Err(ZfsError::DecompressFailed {
            off,
            algo: "unsupported",
        }),
    }
}

/// LZJB decompression, transcribed from `module/zfs/lzjb.c`:
/// MATCH_BITS=6, MATCH_MIN=3, OFFSET_MASK=(1<<10)-1, NBBY=8.
pub fn lzjb_decompress(src: &[u8], dst: &mut [u8]) -> bool {
    const MATCH_BITS: i32 = 6;
    const MATCH_MIN: i32 = 3;
    const OFFSET_MASK: i32 = (1 << (16 - MATCH_BITS)) - 1;
    const NBBY: i32 = 8;

    let mut s = 0usize;
    let mut d = 0usize;
    let d_len = dst.len();
    let s_len = src.len();
    let mut copymap: u8 = 0;
    let mut copymask: i32 = 1 << (NBBY - 1);

    while d < d_len {
        copymask <<= 1;
        if copymask == 1 << NBBY {
            copymask = 1;
            if s >= s_len {
                return false;
            }
            copymap = src[s];
            s += 1;
        }
        if copymap & copymask as u8 != 0 {
            if s + 1 >= s_len {
                return false;
            }
            let b0 = src[s];
            let b1 = src[s + 1];
            s += 2;
            let mlen = ((b0 as i32) >> (NBBY - MATCH_BITS)) + MATCH_MIN;
            let offset = (((b0 as i32) << NBBY) | b1 as i32) & OFFSET_MASK;
            let mut cpy = d as i64 - offset as i64;
            if cpy < 0 {
                return false;
            }
            let mut m = mlen;
            while m > 0 && d < d_len {
                dst[d] = dst[cpy as usize];
                d += 1;
                cpy += 1;
                m -= 1;
            }
        } else {
            if s >= s_len {
                return false;
            }
            dst[d] = src[s];
            s += 1;
            d += 1;
        }
    }
    true
}

/// ZLE decompression, transcribed from `module/zfs/zle.c` (n = 64 from the
/// zio_compress table row).
pub fn zle_decompress(src: &[u8], dst: &mut [u8], n: usize) -> bool {
    let mut s = 0usize;
    let mut d = 0usize;
    let s_len = src.len();
    let d_len = dst.len();

    while s < s_len && d < d_len {
        let len = 1 + src[s] as usize;
        s += 1;
        if len <= n {
            if s + len > s_len || d + len > d_len {
                return false;
            }
            for _ in 0..len {
                dst[d] = src[s];
                s += 1;
                d += 1;
            }
        } else {
            let zeros = len - n;
            if d + zeros > d_len {
                return false;
            }
            for _ in 0..zeros {
                dst[d] = 0;
                d += 1;
            }
        }
    }
    d == d_len
}

/// LZJB "compressor" used by the fixture builder: all literals, which is a
/// valid LZJB stream (control bytes 0x00 every 8 literals).
pub fn lzjb_compress_literals(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + src.len() / 8 + 16);
    let mut lit_in_group = 0;
    for &b in src {
        if lit_in_group == 0 {
            out.push(0); // control byte: all literals
            lit_in_group = 8;
        }
        out.push(b);
        lit_in_group -= 1;
    }
    out
}

/// ZLE "compressor" for the fixture builder: emits literal chunks only
/// (a valid ZLE stream: len = chunk-1, chunk <= 64).
pub fn zle_compress_literals(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let take = (src.len() - i).min(64);
        out.push((take - 1) as u8); // decode: len = 1 + stored <= 64
        out.extend_from_slice(&src[i..i + take]);
        i += take;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrip() {
        let data = vec![1u8, 2, 3, 4];
        let out = decompress(c::OFF, &data, 4, 0).unwrap();
        assert_eq!(out, data);
        assert!(decompress(c::OFF, &data, 5, 0).is_err());
    }

    #[test]
    fn empty_expands_to_zeros() {
        let out = decompress(c::EMPTY, &[], 16, 0).unwrap();
        assert_eq!(out, vec![0u8; 16]);
    }

    #[test]
    fn lzjb_literals_decode() {
        let payload = lzjb_compress_literals(b"abcabcabc");
        let mut out = vec![0u8; 9];
        assert!(lzjb_decompress(&payload, &mut out));
        assert_eq!(&out, b"abcabcabc");
    }

    #[test]
    fn lzjb_repeating_match_decodes() {
        // Control bits are consumed LSB-first within a group (bit0 flags
        // the 1st symbol, bit1 the 2nd): control 0b10 => symbol 1 literal,
        // symbol 2 a match. Match: mlen 4, offset 1 (b0 = ((mlen-3)<<6)
        // | offset>>8, b1 = offset & 0xff) copies 'x' three more times.
        let b0: u8 = (4 - 3) << 6;
        let b1: u8 = 1;
        let stream = vec![0b0000_0010u8, b'x', b0, b1];
        let mut out = vec![0u8; 4];
        assert!(lzjb_decompress(&stream, &mut out));
        assert_eq!(out, vec![b'x', b'x', b'x', b'x']);
    }

    #[test]
    fn zle_roundtrip() {
        // Chunk 1: stored 68 -> len 69 > 64 => 69-64 = 5 zeros.
        // Chunk 2: stored 1 -> len 2 <= 64 => literals 'a','b'.
        let stream = vec![68u8, 0x01, b'a', b'b'];
        let mut out = vec![0u8; 7];
        assert!(zle_decompress(&stream, &mut out, 64));
        assert_eq!(out, vec![0, 0, 0, 0, 0, b'a', b'b']);
    }

    #[test]
    fn zle_literal_stream_roundtrip() {
        let raw: Vec<u8> = (0..200u8).collect();
        let packed = zle_compress_literals(&raw);
        let mut out = vec![0u8; raw.len()];
        assert!(zle_decompress(&packed, &mut out, 64));
        assert_eq!(out, raw);
    }

    #[test]
    fn zle_short_output_is_error() {
        // 5 zeros + 2 literals = 7 bytes; dst wants 8: d != d_len => Err.
        let stream = vec![68u8, 0x01, b'a', b'b'];
        let mut out = vec![0u8; 8];
        assert!(!zle_decompress(&stream, &mut out, 64));
    }

    #[test]
    fn gzip_roundtrip() {
        let raw: Vec<u8> = [104u8, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]
            .iter()
            .copied()
            .cycle()
            .take(33)
            .collect();
        let mut enc = flate2::read::DeflateEncoder::new(&raw[..], flate2::Compression::new(6));
        let mut packed = Vec::new();
        enc.read_to_end(&mut packed).unwrap();
        let out = decompress(c::GZIP_1, &packed, raw.len() as u64, 0).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn lz4_roundtrip_with_zfs_header() {
        let raw: Vec<u8> = b"the quick brown fox jumps over the lazy dog. "
            .iter()
            .copied()
            .cycle()
            .take(200)
            .collect();
        let block = lz4_flex::block::compress(&raw);
        let mut packed = Vec::with_capacity(4 + block.len());
        packed.extend_from_slice(&(block.len() as u32).to_be_bytes());
        packed.extend_from_slice(&block);
        let out = decompress(c::LZ4, &packed, raw.len() as u64, 0).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn lz4_rejects_wrong_lsize() {
        let raw = vec![0x41u8; 64];
        let block = lz4_flex::block::compress(&raw);
        let mut packed = Vec::new();
        packed.extend_from_slice(&(block.len() as u32).to_be_bytes());
        packed.extend_from_slice(&block);
        assert!(decompress(c::LZ4, &packed, 63, 0).is_err());
    }
}
