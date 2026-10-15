//! Fuzz target: FAT table entry reader.
//!
//! Feeds arbitrary FAT data into the FatTable reader with each FAT kind.

#![no_main]

use dw_fat::bpb::FatKind;
use dw_fat::fat::FatTable;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for kind in [FatKind::Fat12, FatKind::Fat16, FatKind::Fat32] {
        if let Ok(table) = FatTable::new(data, kind) {
            // Try reading entries 0..100
            for cluster in 0u32..100 {
                let _ = table.next_cluster(cluster);
            }
            // Try following a chain from cluster 2
            let _ = table.follow_chain(2, 1000);
        }
    }
});
