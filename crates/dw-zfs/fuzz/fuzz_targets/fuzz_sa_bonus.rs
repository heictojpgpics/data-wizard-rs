//! Fuzz target: SA bonus parsing (SA magic path + legacy znode path).
//! Field access after parse must be panic-free.

#![no_main]

use dw_zfs::sa::SaState;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let st = SaState::empty();
    let attrs = st.parse_bonus(data);
    let _ = attrs.size;
    let _ = attrs.mtime;
    let _ = attrs.atime;
});
