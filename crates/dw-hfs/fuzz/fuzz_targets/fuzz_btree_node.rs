//! Fuzz target: HFS+ B-tree node parser.
//!
//! Feeds arbitrary bytes into `BTreeNode::parse`. The parser must never
//! panic: invalid nodes come back as `Err`.

#![no_main]

use dw_hfs::btree::BTreeNode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 14 {
        // Node number 0 is the header node; exercise a spread of numbers.
        let _ = BTreeNode::parse(0, data.to_vec());
        let _ = BTreeNode::parse(u32::from(data[0]) | 1, data.to_vec());
    }
});
