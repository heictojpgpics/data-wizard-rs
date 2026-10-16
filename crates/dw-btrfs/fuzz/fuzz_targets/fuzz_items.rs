//! Fuzz target: Btrfs item parsers (inode, dir entry, file extent, root).
//!
//! All four are pure byte parsers over untrusted data; invalid input
//! must come back None/Err, never panic.

#![no_main]

use dw_btrfs::items::{DirEntry, FileExtent, InodeItem, RootItem};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = InodeItem::parse(data);
    let _ = DirEntry::parse(data);
    let _ = FileExtent::parse(data);
    let _ = RootItem::parse(data);
});
