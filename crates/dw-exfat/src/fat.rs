//! exFAT FAT (file allocation table) chain following.
//!
//! The exFAT FAT (spec v1.00 §6.2) is an array of 32-bit entries, one per
//! cluster (numbered from 2), possibly preceded by two reserved entries
//! (0 and 1, media descriptor values — never part of any chain):
//!
//! ```text
//! entry value            meaning
//! 0x00000000              cluster is free
//! 0x00000002..0x0FFFFFFF  next cluster in the chain
//! 0xFFFFFFF7              bad cluster
//! 0xFFFFFFF8..0xFFFFFFFF  end of chain (EOC)
//! ```
//!
//! The FAT is a *secondary* allocation source in exFAT: the allocation
//! bitmap decides allocation state, and streams flagged `NoFatChain` are
//! contiguous without consulting the FAT at all. Chain following is only
//! meaningful for streams whose Stream Extension does not set the
//! NoFatChain flag (and for the root directory, which has no stream
//! entry and is always FAT-chained).

use crate::error::{ExfatError, ExfatResult};

/// Parsed exFAT FAT table.
#[derive(Debug, Clone)]
pub struct ExfatFat {
    /// Raw table bytes (validated multiple of 4).
    data: Vec<u8>,
    /// Number of 32-bit entries stored in `data`.
    entries: usize,
}

/// Free-cluster marker.
pub const ENTRY_FREE: u32 = 0x0000_0000;
/// Bad-cluster marker (spec value).
pub const ENTRY_BAD: u32 = 0xFFFF_FFF7;
/// First end-of-chain value (any entry >= this is EOC).
pub const ENTRY_EOC_FIRST: u32 = 0xFFFF_FFF8;
/// Last valid data-cluster pointer (guards against corrupt tables that
/// point past the valid cluster range; the practical limit comes from
/// `cluster_count` at the call site).
pub const MAX_DATA_ENTRY: u32 = 0x0FFF_FFFF;

impl ExfatFat {
    /// Parse a FAT region. `data.len()` must be a multiple of 4.
    pub fn parse(data: Vec<u8>) -> ExfatResult<Self> {
        if !data.len().is_multiple_of(4) {
            return Err(ExfatError::Corrupt(format!(
                "FAT region length {} is not a multiple of 4",
                data.len()
            )));
        }
        let entries = data.len() / 4;
        Ok(Self { data, entries })
    }

    /// Raw 32-bit entry at cluster index `c` (0-based over the table,
    /// i.e. index 2 is the first data cluster).
    pub fn entry(&self, cluster: u32) -> u32 {
        let c = cluster as usize;
        if c >= self.entries {
            return ENTRY_FREE;
        }
        u32::from_le_bytes([
            self.data[c * 4],
            self.data[c * 4 + 1],
            self.data[c * 4 + 2],
            self.data[c * 4 + 3],
        ])
    }

    /// Is this cluster free per the FAT? (Entry == 0.)
    pub fn is_free(&self, cluster: u32) -> bool {
        self.entry(cluster) == ENTRY_FREE
    }

    /// Is this cluster the last of its chain (EOC or bad)?
    pub fn is_end_of_chain(&self, cluster: u32) -> bool {
        let e = self.entry(cluster);
        e >= ENTRY_EOC_FIRST || e == ENTRY_BAD || e == ENTRY_FREE || e == ENTRY_BAD
    }

    /// Follow the chain starting at `start`, up to `max_clusters` entries.
    ///
    /// Semantics (mirrors the FAT-family contract used by the walkers):
    ///  - the start cluster is always included, even if its own entry is
    ///    already free (deleted-file case: the OS zeroes FAT entries on
    ///    delete, so a deleted stream yields a single-cluster chain and
    ///    the caller marks the entry PartiallyRecovered);
    ///  - a self-loop is reported as `Corrupt`;
    ///  - a `free` entry terminates the chain (broken/deleted chain);
    ///  - a `bad` entry terminates the chain;
    ///  - values >= 0xFFFFFFF8 terminate the chain (EOC);
    ///  - entries pointing below 2 or above `cluster_count` (checked by
    ///    the caller via the returned chain) are surfaced as-is so the
    ///    caller can reject them.
    pub fn follow_chain(&self, start_cluster: u32, max_clusters: u32) -> ExfatResult<Vec<u32>> {
        if start_cluster < 2 {
            return Err(ExfatError::Corrupt(format!(
                "chain start cluster {start_cluster} below the valid range 2.."
            )));
        }
        let mut chain = Vec::new();
        let mut current = start_cluster;
        loop {
            if chain.len() as u32 >= max_clusters {
                return Err(ExfatError::Corrupt(format!(
                    "FAT chain exceeded {max_clusters} clusters (possible cycle)"
                )));
            }
            chain.push(current);
            let next = self.entry(current);
            if next == ENTRY_FREE || next == ENTRY_BAD || next >= ENTRY_EOC_FIRST {
                break; // end of chain (free = deleted chain, bad, or EOC)
            }
            if next < 2 {
                return Err(ExfatError::Corrupt(format!(
                    "FAT entry for cluster {current} points to invalid cluster {next}"
                )));
            }
            if next == current {
                return Err(ExfatError::Corrupt(format!(
                    "FAT chain self-loop at cluster {current}"
                )));
            }
            current = next;
        }
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[u32]) -> ExfatFat {
        let mut data = Vec::new();
        for &e in entries {
            data.extend_from_slice(&e.to_le_bytes());
        }
        ExfatFat::parse(data).unwrap()
    }

    #[test]
    fn parses_simple_chain() {
        // cluster 2 -> 3 -> 4 -> EOC
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 3, 4, 0xFFFFFFFF, 0]);
        let chain = fat.follow_chain(2, 100).unwrap();
        assert_eq!(chain, vec![2, 3, 4]);
    }

    #[test]
    fn deleted_chain_yields_single_cluster() {
        // cluster 2's entry is free (0) — deleted file
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 0, 4, 0xFFFFFFFF]);
        let chain = fat.follow_chain(2, 100).unwrap();
        assert_eq!(chain, vec![2]);
    }

    #[test]
    fn bad_cluster_terminates_chain() {
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 3, 0xFFFFFFF7]);
        let chain = fat.follow_chain(2, 100).unwrap();
        assert_eq!(chain, vec![2, 3]);
    }

    #[test]
    fn self_loop_is_corrupt() {
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 2]);
        assert!(fat.follow_chain(2, 100).is_err());
    }

    #[test]
    fn long_chain_respects_max_bound() {
        let mut entries = vec![0xFFFFFFF8u32, 0xFFFFFFFF];
        for i in 0..100 {
            entries.push(if i == 99 {
                0xFFFFFFFF
            } else {
                (3 + i + 1) as u32
            });
        }
        // chain: 2 -> 3 -> ... -> 101 (EOC). entries[2]=3, entries[3]=4...
        let fat = table(&entries);
        let chain = fat.follow_chain(2, 10).unwrap_err();
        assert!(chain.to_string().contains("exceeded"));
    }

    #[test]
    fn start_below_two_is_rejected() {
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 0xFFFFFFFF]);
        assert!(fat.follow_chain(0, 10).is_err());
        assert!(fat.follow_chain(1, 10).is_err());
    }

    #[test]
    fn odd_length_is_rejected() {
        assert!(ExfatFat::parse(vec![0, 0, 0]).is_err());
    }

    #[test]
    fn is_free_and_entry_accessors() {
        let fat = table(&[0xFFFFFFF8, 0xFFFFFFFF, 0, 4]);
        assert_eq!(fat.entry(2), 0);
        assert_eq!(fat.entry(3), 4);
        assert!(fat.is_free(2));
        assert!(!fat.is_free(3));
        // out-of-range read is free, not a panic
        assert!(fat.is_free(9999));
    }
}
