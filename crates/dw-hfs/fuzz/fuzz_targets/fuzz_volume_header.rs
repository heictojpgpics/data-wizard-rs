//! Fuzz target: HFS+ volume header parser.
//!
//! Feeds arbitrary 512-byte buffers into `VolumeHeader::parse`; the
//! signature/geometry checks must reject bad headers with `Err`, never
//! panic.

#![no_main]

use dw_hfs::volumeheader::VolumeHeader;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 512 {
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&data[..512]);
        let _ = VolumeHeader::parse(&buf);
    }
});
