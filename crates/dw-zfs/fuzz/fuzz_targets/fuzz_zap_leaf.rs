//! Fuzz target: zap leaf parser (fatzap leaf blocks).
//!
//! Arbitrary 16 KiB blocks; entries must never panic and names must
//! always be valid UTF-8 by construction.

#![no_main]

use dw_zfs::zap::{parse_leaf, ZapValue};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut buf = vec![0u8; 16384];
    let n = data.len().min(16384);
    buf[..n].copy_from_slice(&data[..n]);
    let mut out = Vec::new();
    let _ = parse_leaf(&buf, &mut out, 0);
    for (name, v) in &out {
        let _ = name.len();
        if let ZapValue::U64(x) = v {
            let _ = x.wrapping_add(1);
        }
    }
});
