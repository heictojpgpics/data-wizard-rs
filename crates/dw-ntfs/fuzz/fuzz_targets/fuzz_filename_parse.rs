//! Fuzz target: NTFS $FILE_NAME attribute parser.

#![no_main]

use dw_ntfs::filename::FileName;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = FileName::parse(data);
});
