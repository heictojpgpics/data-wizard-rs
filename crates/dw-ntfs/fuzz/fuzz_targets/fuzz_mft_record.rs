//! Fuzz target: NTFS MFT record parser.
//!
//! Feeds arbitrary byte sequences into `FileRecord::parse`. The parser
//! should never panic — it should return `Err` for invalid input.

#![no_main]

use dw_ntfs::mft::FileRecord;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 1024 {
        let _ = FileRecord::parse(data, 1024);
    }
});
