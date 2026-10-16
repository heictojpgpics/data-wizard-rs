//! Fuzz target: the zio decompression dispatch. Every algorithm must
//! either return exactly `lsize` bytes or a typed error - never panic,
//! never return a wrong length.

#![no_main]

use dw_zfs::blkptr::compress as c;
use dw_zfs::compress::decompress;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Deterministic-ish sweep: use the first byte as the algorithm, the
    // second+third as lsize (bounded), the rest as the payload.
    if data.len() < 4 {
        return;
    }
    let algo = data[0];
    let lsize = u64::from(u16::from_le_bytes([data[1], data[2]])) * 64;
    let lsize = lsize.clamp(0, 1 << 20);
    for a in [algo, c::OFF, c::LZJB, c::GZIP_1, c::ZLE, c::LZ4, c::ZSTD, c::EMPTY] {
        if let Ok(out) = decompress(a, &data[3..], lsize, 0) {
            assert_eq!(out.len() as u64, lsize, "wrong length for algo {a}");
        }
    }
});
