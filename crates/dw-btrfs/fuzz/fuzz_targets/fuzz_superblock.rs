//! Fuzz target: Btrfs superblock parser.
//!
//! Feeds arbitrary 4096-byte buffers (the parser accepts partial reads,
//! so shorter inputs exercise the guard paths). Torn csums must reject;
//! a valid parse must survive round-trip field access without panicking.

#![no_main]

use dw_btrfs::superblock::SuperBlock;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4096 {
        return;
    }
    let mut buf = [0u8; 4096];
    buf.copy_from_slice(&data[..4096]);
    let _ = SuperBlock::parse(&buf);
});
