//! FAT cluster chain reader.
//!
//! Reads the File Allocation Table and provides utilities for following
//! cluster chains.
//!
//! # FAT entry sizes
//!
//! - FAT12: each entry is 12 bits (1.5 bytes). Entries are packed two per
//!   3 bytes.
//! - FAT16: each entry is 16 bits (2 bytes).
//! - FAT32: each entry is 28 bits (the high 4 bits of the 32-bit word are
//!   reserved).
//!
//! # Special entry values
//!
//! - 0x000: free cluster
//! - 0x002..=0xFFEF (FAT16) / 0x0FFFFFF7 (FAT32): next cluster in chain
//! - 0xFF8..=0xFFF (FAT12) / 0xFFF8..=0xFFFF (FAT16) / 0x0FFFFFF8..=0x0FFFFFFF
//!   (FAT32): end of chain (EOC)
//! - 0xFF7 (FAT12/16) / 0x0FFFFFF7 (FAT32): bad cluster

use crate::bpb::FatKind;
use crate::error::{FatError, FatResult};

/// End-of-chain marker threshold.
const EOC_FAT12: u16 = 0xFF8;
const EOC_FAT16: u16 = 0xFFF8;
const EOC_FAT32: u32 = 0x0FFFFFF8;

/// Bad cluster marker.
const BAD_FAT12: u16 = 0xFF7;
const BAD_FAT16: u16 = 0xFFF7;
const BAD_FAT32: u32 = 0x0FFFFFF7;

/// A reader for the File Allocation Table.
pub struct FatTable<'a> {
    /// The raw FAT bytes (one copy; we don't need all copies).
    fat_data: &'a [u8],
    kind: FatKind,
}

impl<'a> FatTable<'a> {
    /// Create a `FatTable` from a slice of FAT bytes.
    ///
    /// `fat_data` should be the contents of one FAT copy (starting at
    /// `fat_offset_bytes()` in the volume, length `fat_size_sectors *
    /// bytes_per_sector`).
    pub fn new(fat_data: &'a [u8], kind: FatKind) -> FatResult<Self> {
        // Basic sanity: FAT must be at least a few bytes.
        if fat_data.len() < 6 {
            return Err(FatError::Corrupt(format!(
                "FAT data too short: {} bytes",
                fat_data.len()
            )));
        }
        Ok(Self { fat_data, kind })
    }

    /// Get the next cluster in a chain.
    ///
    /// Returns:
    /// - `Ok(Some(next_cluster))` if `cluster` points to another cluster
    /// - `Ok(None)` if `cluster` is an end-of-chain marker
    /// - `Err` if `cluster` is out of range or points to a bad cluster
    pub fn next_cluster(&self, cluster: u32) -> FatResult<Option<u32>> {
        match self.kind {
            FatKind::Fat12 => {
                let entry = self.get_fat12_entry(cluster)?;
                if entry >= EOC_FAT12 {
                    Ok(None)
                } else if entry == BAD_FAT12 {
                    Err(FatError::Corrupt(format!(
                        "bad cluster {cluster} in FAT12 chain"
                    )))
                } else if entry == 0 {
                    // Free cluster in the middle of a chain — corrupt
                    Err(FatError::Corrupt(format!(
                        "free cluster {cluster} in FAT12 chain"
                    )))
                } else {
                    Ok(Some(u32::from(entry)))
                }
            }
            FatKind::Fat16 => {
                let entry = self.get_fat16_entry(cluster)?;
                if entry >= EOC_FAT16 {
                    Ok(None)
                } else if entry == BAD_FAT16 {
                    Err(FatError::Corrupt(format!(
                        "bad cluster {cluster} in FAT16 chain"
                    )))
                } else if entry == 0 {
                    Err(FatError::Corrupt(format!(
                        "free cluster {cluster} in FAT16 chain"
                    )))
                } else {
                    Ok(Some(u32::from(entry)))
                }
            }
            FatKind::Fat32 => {
                let entry = self.get_fat32_entry(cluster)?;
                if entry >= EOC_FAT32 {
                    Ok(None)
                } else if entry == BAD_FAT32 {
                    Err(FatError::Corrupt(format!(
                        "bad cluster {cluster} in FAT32 chain"
                    )))
                } else if entry == 0 {
                    Err(FatError::Corrupt(format!(
                        "free cluster {cluster} in FAT32 chain"
                    )))
                } else {
                    Ok(Some(entry))
                }
            }
        }
    }

    /// Read a FAT12 entry (12 bits, packed two per 3 bytes).
    fn get_fat12_entry(&self, cluster: u32) -> FatResult<u16> {
        let cluster = cluster as usize;
        let offset = cluster * 3 / 2;
        if offset + 1 >= self.fat_data.len() {
            return Err(FatError::Corrupt(format!(
                "FAT12 entry {cluster} out of bounds (offset {offset}, len {})",
                self.fat_data.len()
            )));
        }
        // FAT12 packing:
        // - Even cluster: entry = fat[offset] | ((fat[offset+1] & 0x0F) << 8)
        // - Odd cluster:  entry = (fat[offset] >> 4) | (fat[offset+1] << 4)
        if cluster.is_multiple_of(2) {
            Ok(u16::from(self.fat_data[offset])
                | (u16::from(self.fat_data[offset + 1] & 0x0F) << 8))
        } else {
            Ok((u16::from(self.fat_data[offset]) >> 4)
                | (u16::from(self.fat_data[offset + 1]) << 4))
        }
    }

    /// Read a FAT16 entry (16 bits, 2 bytes).
    fn get_fat16_entry(&self, cluster: u32) -> FatResult<u16> {
        let offset = cluster as usize * 2;
        if offset + 1 >= self.fat_data.len() {
            return Err(FatError::Corrupt(format!(
                "FAT16 entry {cluster} out of bounds (offset {offset}, len {})",
                self.fat_data.len()
            )));
        }
        Ok(u16::from_le_bytes([
            self.fat_data[offset],
            self.fat_data[offset + 1],
        ]))
    }

    /// Read a FAT32 entry (28 bits, 4 bytes; high 4 bits masked off).
    fn get_fat32_entry(&self, cluster: u32) -> FatResult<u32> {
        let offset = cluster as usize * 4;
        if offset + 3 >= self.fat_data.len() {
            return Err(FatError::Corrupt(format!(
                "FAT32 entry {cluster} out of bounds (offset {offset}, len {})",
                self.fat_data.len()
            )));
        }
        let raw = u32::from_le_bytes([
            self.fat_data[offset],
            self.fat_data[offset + 1],
            self.fat_data[offset + 2],
            self.fat_data[offset + 3],
        ]);
        Ok(raw & 0x0FFFFFFF)
    }

    /// Follow a cluster chain starting at `start_cluster`, collecting all
    /// clusters in order.
    ///
    /// Stops at the end-of-chain marker. Limits to `max_clusters` to
    /// prevent infinite loops on corrupt FATs.
    pub fn follow_chain(&self, start_cluster: u32, max_clusters: u32) -> FatResult<Vec<u32>> {
        let mut chain = Vec::new();
        let mut current = start_cluster;
        loop {
            if chain.len() as u32 >= max_clusters {
                return Err(FatError::Corrupt(format!(
                    "FAT chain exceeded {max_clusters} clusters (possible cycle)"
                )));
            }
            chain.push(current);
            match self.next_cluster(current)? {
                Some(next) => {
                    if next == current {
                        return Err(FatError::Corrupt(format!(
                            "FAT chain self-loop at cluster {current}"
                        )));
                    }
                    current = next;
                }
                None => break, // End of chain
            }
        }
        Ok(chain)
    }
}

impl<'a> FatTable<'a> {
    /// Raw entry value for `cluster`, 0 = free. Out-of-bounds reads are
    /// reported as 0xFFFF_FFFF (never free): a cluster beyond the table
    /// cannot be carved as unallocated space.
    pub fn raw_entry(&self, cluster: u32) -> u32 {
        let v: u32 = match self.kind {
            FatKind::Fat12 => match self.get_fat12_entry(cluster) {
                Ok(e) => u32::from(e),
                Err(_) => 0xFFFF_FFFF,
            },
            FatKind::Fat16 => match self.get_fat16_entry(cluster) {
                Ok(e) => u32::from(e),
                Err(_) => 0xFFFF_FFFF,
            },
            FatKind::Fat32 => self.get_fat32_entry(cluster).unwrap_or(0xFFFF_FFFF),
        };
        v
    }

    /// Is `cluster` free (raw entry == 0)?
    pub fn is_free(&self, cluster: u32) -> bool {
        self.raw_entry(cluster) == 0
    }

    /// Iterate free runs over clusters 2..cluster_count+2 as
    /// `(first_cluster, run_len_clusters)`.
    ///
    /// Used by the deep scan to carve exactly the unallocated regions:
    /// deleted files have their FAT entries zeroed, so their remnants
    /// live precisely in these runs.
    pub fn free_runs(&self, cluster_count: u32) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        let end = cluster_count.saturating_add(2);
        let mut cluster: u32 = 2;
        while cluster < end {
            if self.is_free(cluster) {
                let first = cluster;
                while cluster < end && self.is_free(cluster) {
                    cluster += 1;
                }
                let len = cluster - first;
                if let Some(last) = runs.last_mut()
                    && last.0 + last.1 == first
                {
                    last.1 += len;
                } else {
                    runs.push((first, len));
                }
            } else {
                cluster += 1;
            }
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_runs_over_fat16_table() {
        // FAT16 table: entries 2..10: 2=3(used),3=4(used),4=EOC(used chain),
        // 5..7 free, 8=EOC?? -> use: 5,6,7 free (0), 8=9, 9=EOC, 10..12 free
        let mut fat = vec![0u8; 32]; // 16 entries
        fat[0..2].copy_from_slice(&0xFFF8u16.to_le_bytes());
        fat[2..4].copy_from_slice(&0xFFF8u16.to_le_bytes());
        // entry 2: 3 (allocated)
        fat[4..6].copy_from_slice(&3u16.to_le_bytes());
        // entry 3: 4
        fat[6..8].copy_from_slice(&4u16.to_le_bytes());
        // entry 4: EOC
        fat[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        // entries 5,6,7: free (0) - already zero
        // entry 8: 9
        fat[16..18].copy_from_slice(&9u16.to_le_bytes());
        // entry 9: EOC
        fat[18..20].copy_from_slice(&0xFFFFu16.to_le_bytes());
        // entries 10,11,12: free
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        let runs = table.free_runs(11); // clusters 2..13
        // free: 5..8 (3), 10..13 (3)
        assert_eq!(runs, vec![(5u32, 3u32), (10u32, 3u32)]);
    }

    #[test]
    fn is_free_and_raw_entry() {
        let mut fat = vec![0u8; 32];
        fat[4..6].copy_from_slice(&3u16.to_le_bytes()); // entry 2 = 3
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        assert!(!table.is_free(2));
        assert!(table.is_free(3));
        // out-of-bounds is never free
        assert!(!table.is_free(999));
    }

    fn make_fat16_table() -> Vec<u8> {
        // 10-entry FAT16 table
        let mut fat = vec![0u8; 20];
        // Entry 0,1: media descriptor + end-of-chain marker
        fat[0..2].copy_from_slice(&0xFFF8u16.to_le_bytes());
        fat[2..4].copy_from_slice(&0xFFFFu16.to_le_bytes());
        // Entry 2: 3 (chain: 2 -> 3 -> 4 -> EOC)
        fat[4..6].copy_from_slice(&3u16.to_le_bytes());
        // Entry 3: 4
        fat[6..8].copy_from_slice(&4u16.to_le_bytes());
        // Entry 4: EOC
        fat[8..10].copy_from_slice(&0xFFF8u16.to_le_bytes());
        // Entry 5: 6 (chain: 5 -> 6 -> EOC)
        fat[10..12].copy_from_slice(&6u16.to_le_bytes());
        // Entry 6: EOC
        fat[12..14].copy_from_slice(&0xFFF8u16.to_le_bytes());
        // Entry 7: bad
        fat[14..16].copy_from_slice(&0xFFF7u16.to_le_bytes());
        // Entry 8: free
        fat[16..18].copy_from_slice(&0x0000u16.to_le_bytes());
        // Entry 9: EOC
        fat[18..20].copy_from_slice(&0xFFF8u16.to_le_bytes());
        fat
    }

    #[test]
    fn fat16_next_cluster_works() {
        let fat = make_fat16_table();
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        // Chain 2 -> 3 -> 4 -> EOC
        assert_eq!(table.next_cluster(2).unwrap(), Some(3));
        assert_eq!(table.next_cluster(3).unwrap(), Some(4));
        assert_eq!(table.next_cluster(4).unwrap(), None); // EOC
        // Chain 5 -> 6 -> EOC
        assert_eq!(table.next_cluster(5).unwrap(), Some(6));
        assert_eq!(table.next_cluster(6).unwrap(), None);
    }

    #[test]
    fn fat16_bad_cluster_errors() {
        let fat = make_fat16_table();
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        assert!(table.next_cluster(7).is_err()); // bad cluster
    }

    #[test]
    fn fat16_free_cluster_in_chain_errors() {
        let fat = make_fat16_table();
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        assert!(table.next_cluster(8).is_err()); // free cluster
    }

    #[test]
    fn fat16_follow_chain_works() {
        let fat = make_fat16_table();
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        let chain = table.follow_chain(2, 100).unwrap();
        assert_eq!(chain, vec![2, 3, 4]);

        let chain = table.follow_chain(5, 100).unwrap();
        assert_eq!(chain, vec![5, 6]);

        let chain = table.follow_chain(9, 100).unwrap();
        assert_eq!(chain, vec![9]); // single-cluster chain
    }

    #[test]
    fn fat16_follow_chain_detects_cycle() {
        // Create a FAT with a cycle: 2 -> 3 -> 2
        let mut fat = vec![0u8; 20];
        fat[0..2].copy_from_slice(&0xFFF8u16.to_le_bytes());
        fat[2..4].copy_from_slice(&0xFFFFu16.to_le_bytes());
        fat[4..6].copy_from_slice(&3u16.to_le_bytes()); // 2 -> 3
        fat[6..8].copy_from_slice(&2u16.to_le_bytes()); // 3 -> 2 (cycle!)
        let table = FatTable::new(&fat, FatKind::Fat16).unwrap();
        // With max_clusters = 100, the chain would be 2, 3, 2, 3, 2, 3...
        // But our self-loop check only catches next == current, not general cycles.
        // The follow_chain will detect it when chain.len() reaches max_clusters.
        let result = table.follow_chain(2, 5);
        assert!(result.is_err()); // Should error after 5 clusters
    }

    #[test]
    fn fat12_packing_works() {
        // FAT12: entries packed two per 3 bytes
        // Entry 0 (even): bytes 0,1 -> low 12 bits
        // Entry 1 (odd):  bytes 1,2 -> high 12 bits
        let mut fat = vec![0u8; 6];
        // Entry 0 = 0xFF8 (EOC): bytes 0=0xF8, 1 low nibble=0xF
        fat[0] = 0xF8;
        fat[1] = 0xFF; // high nibble = F (for entry 0), low nibble = F (for entry 1)
        fat[2] = 0xFF; // entry 1 high byte = FF
        // Entry 0 should be 0xFF8 (EOC)
        let table = FatTable::new(&fat, FatKind::Fat12).unwrap();
        assert_eq!(table.get_fat12_entry(0).unwrap(), 0xFF8);
        // Entry 1 should be 0xFFF (EOC)
        assert_eq!(table.get_fat12_entry(1).unwrap(), 0xFFF);
    }

    #[test]
    fn fat32_masks_high_bits() {
        // FAT32: 28 bits used, high 4 bits masked
        let mut fat = vec![0u8; 8];
        // Entry 0: 0x0FFFFFF8 (EOC) with high nibble = 0xF (reserved)
        fat[0..4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        let table = FatTable::new(&fat, FatKind::Fat32).unwrap();
        // Should be masked to 0x0FFFFFFF (EOC)
        let entry = table.get_fat32_entry(0).unwrap();
        assert!(entry >= EOC_FAT32); // EOC
        assert_eq!(entry, 0x0FFFFFFF);
    }
}
