//! Fuzz target: HFS+ catalog thread/key/record parsers.
//!
//! `parse_record` must accept arbitrary bytes without panicking; unknown
//! record types are surfaced as `Ok(None)`/`Err`.

#![no_main]

use dw_hfs::catalog;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = catalog::parse_key(data);
    let _ = catalog::parse_record(data);
});
