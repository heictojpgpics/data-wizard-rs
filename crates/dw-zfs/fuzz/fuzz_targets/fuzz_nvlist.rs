//! Fuzz target: XDR nvlist parser.
//!
//! Arbitrary buffers must never panic; a successful parse must survive
//! full accessor traversal.

#![no_main]

use dw_zfs::nvlist::NvList;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(nvl) = NvList::parse(data) {
        let _ = nvl.get_u64("version");
        let _ = nvl.get_str("name");
        let _ = nvl.get_nvlist("vdev_tree");
        let _ = nvl.get_nvlist_array("children");
        for (k, v) in nvl.pairs() {
            let _ = (k.len(), v);
        }
    }
});
