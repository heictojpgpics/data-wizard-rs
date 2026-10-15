//! Fuzz target: exFAT directory entry parser.

#![no_main]

use dw_exfat::direntry::DirEntry;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 32 {
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&data[..32]);
        let _ = DirEntry::parse(&buf);
    }
});
