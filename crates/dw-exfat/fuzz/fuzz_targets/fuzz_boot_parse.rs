//! Fuzz target: exFAT boot sector parser.

#![no_main]

use dw_exfat::boot::ExfatBoot;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 512 {
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&data[..512]);
        let _ = ExfatBoot::parse(&buf);
    }
});
