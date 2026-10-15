//! exFAT allocation bitmap.
//!
//! The allocation bitmap records, one bit per cluster (numbered from 2),
//! whether a cluster is allocated (bit set) or free (bit clear). It lives
//! in its own cluster chain in the cluster heap; the *directory entry*
//! that describes it sits in the root directory.
//!
//! Layout of the Allocation Bitmap directory entry (exFAT spec v1.00
//! §7.1; absolute offsets in the 32-byte entry):
//! ```text
//!  0      EntryType (0x81 active / 0x01 deleted)
//!  1      BitmapFlags (bit0: bitmap 1 or 2 when the volume has 2 FATs)
//!  2..5   Reserved
//!  5..9   FirstClusterOfBitmap (u32 LE)
//!  9..28  DataLengthOfBitmap (19 bytes, value fits u64 comfortably)
//! 28..32  CustomCode
//! ```
//!
//! The bitmap data covers `cluster_count` clusters starting at cluster 2:
//! bit (c-2) of the byte stream is the allocation state of cluster c.
//! Trailing bits beyond `cluster_count` are undefined padding.

use crate::error::{ExfatError, ExfatResult};

/// Parsed Allocation Bitmap directory entry (the on-disk descriptor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitmapEntry {
    /// Which FAT/bitmap this is (bit0 of BitmapFlags).
    pub flags: u8,
    /// First cluster of the bitmap's cluster chain.
    pub first_cluster: u32,
    /// Data length of the bitmap in bytes.
    pub length: u64,
}

impl BitmapEntry {
    /// Parse the 32-byte raw entry at its spec offsets.
    pub fn parse(buf: &[u8; 32]) -> ExfatResult<Self> {
        let flags = buf[1];
        let first_cluster = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
        // 19-byte length field: the value fits u64; the high 11 bytes
        // must be zero for a spec-conforming image. Non-zero high bytes
        // mean a corrupt entry — reject rather than truncate.
        let low = u64::from_le_bytes([
            buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16],
        ]);
        let high_nonzero = buf[17..28].iter().any(|&b| b != 0);
        if high_nonzero {
            return Err(ExfatError::Corrupt(
                "bitmap entry: DataLength high bytes not zero".into(),
            ));
        }
        Ok(Self {
            flags,
            first_cluster,
            length: low,
        })
    }
}

/// In-memory allocation bitmap with fast membership and free-run queries.
#[derive(Debug, Clone)]
pub struct ExfatBitmap {
    data: Vec<u8>,
    /// Number of clusters the bitmap covers (clusters 2..cluster_count+2).
    cluster_count: u32,
}

impl ExfatBitmap {
    /// Build from raw bitmap bytes. `data` must cover at least
    /// `cluster_count` bits (spec: length is rounded up to 4 bytes).
    pub fn new(data: Vec<u8>, cluster_count: u32) -> ExfatResult<Self> {
        let needed = (cluster_count as u64).div_ceil(8);
        if (data.len() as u64) < needed {
            return Err(ExfatError::Corrupt(format!(
                "bitmap covers {} bytes but needs >= {needed} for {cluster_count} clusters",
                data.len()
            )));
        }
        Ok(Self {
            data,
            cluster_count,
        })
    }

    /// Allocation state of cluster `c` (2-based). Out-of-range clusters
    /// read as free.
    pub fn is_allocated(&self, cluster: u32) -> bool {
        if cluster < 2 {
            return false;
        }
        let idx = cluster - 2;
        if idx >= self.cluster_count {
            return false;
        }
        self.data[(idx / 8) as usize] & (1 << (idx % 8)) != 0
    }

    /// Iterate free runs as `(first_cluster, run_len_clusters)` over the
    /// managed range 2..cluster_count+2. Used by the deep scan to carve
    /// exactly the unallocated regions.
    ///
    /// Word-at-a-time: fully-allocated words (0xFF) and fully-free words
    /// (0x00) are skipped with leading_zeros/trailing_ones tricks, so the
    /// cost is O(bytes/8) for uniform regions instead of O(bits).
    pub fn free_runs(&self) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let total = self.cluster_count;
        let mut cluster: u32 = 0; // 0-based index over 2..cluster_count+2

        while cluster < total {
            // Fast path: check one u64 word (8 bytes = 64 clusters).
            let byte_idx = (cluster / 8) as usize;
            if byte_idx + 8 <= self.data.len() && cluster.is_multiple_of(64) {
                let word = u64::from_le_bytes(
                    self.data[byte_idx..byte_idx + 8]
                        .try_into()
                        .expect("8-byte slice"),
                );
                let remaining_clusters = total - cluster;
                if remaining_clusters >= 64 {
                    if word == u64::MAX {
                        cluster += 64; // fully allocated
                        continue;
                    }
                    if word == 0 {
                        // fully free word: extend/append a 64-cluster run
                        push_run(&mut runs, cluster + 2, 64);
                        cluster += 64;
                        continue;
                    }
                }
            }

            // Bit-by-bit path (tail of the volume or mixed word).
            if self.is_allocated_idx(cluster) {
                cluster += 1;
            } else {
                let start = cluster;
                while cluster < total && !self.is_allocated_idx(cluster) {
                    cluster += 1;
                }
                push_run(&mut runs, start + 2, cluster - start);
            }
        }
        runs
    }

    /// 0-based cluster index lookup (internal).
    fn is_allocated_idx(&self, idx: u32) -> bool {
        self.data[(idx / 8) as usize] & (1 << (idx % 8)) != 0
    }
}

/// Append a free run, merging with the previous one when contiguous
/// (a run ending exactly where the new one starts).
fn push_run(runs: &mut Vec<(u32, u32)>, first_cluster: u32, len: u32) {
    if let Some(last) = runs.last_mut()
        && last.0 + last.1 == first_cluster
    {
        last.1 += len;
    } else {
        runs.push((first_cluster, len));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buf(first_cluster: u32, length: u64) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0] = 0x81; // active bitmap
        buf[1] = 0; // flags
        buf[5..9].copy_from_slice(&first_cluster.to_le_bytes());
        buf[9..17].copy_from_slice(&length.to_le_bytes());
        // bytes 17..28 stay zero; 28..32 custom
        buf
    }

    #[test]
    fn parses_bitmap_entry_spec_offsets() {
        let buf = make_buf(0x01020304, 4096);
        let e = BitmapEntry::parse(&buf).unwrap();
        assert_eq!(e.first_cluster, 0x01020304);
        assert_eq!(e.length, 4096);
        assert_eq!(e.flags, 0);
    }

    #[test]
    fn rejects_nonzero_high_length_bytes() {
        let mut buf = make_buf(2, 4096);
        buf[20] = 0xFF; // inside the 19-byte length field's high region
        assert!(BitmapEntry::parse(&buf).is_err());
    }

    #[test]
    fn allocation_roundtrip() {
        // 16 clusters, two bitmap bytes = 0xAA: LSB-first per spec, so
        // 0xAA = 10101010b means bit0=0, bit1=1, bit2=0... i.e. odd bits set.
        // Cluster 2 = bit 0 (free), cluster 3 = bit 1 (allocated), etc.
        let data = vec![0xAAu8; 2];
        let bm = ExfatBitmap::new(data, 16).unwrap();
        assert!(!bm.is_allocated(2)); // bit 0 clear
        assert!(bm.is_allocated(3)); // bit 1 set
        assert!(!bm.is_allocated(4)); // bit 2 clear
        assert!(bm.is_allocated(5)); // bit 3 set
        // out of range reads free
        assert!(!bm.is_allocated(100));
        // below 2 reads free
        assert!(!bm.is_allocated(0));
        assert!(!bm.is_allocated(1));
    }

    #[test]
    fn free_runs_are_merged_and_correct() {
        // 24 clusters. Intended allocation (LSB-first per spec):
        //   bits 0..9   allocated -> clusters 2..12
        //   bits 10..15 free      -> clusters 12..18
        //   bits 16..17 allocated -> clusters 18..20
        //   bits 18..23 free      -> clusters 20..26
        // Bytes (LSB-first): byte0 = 0b11111111; byte1 = 0b00000011
        // (bits 8,9 set = value 3); byte2 = 0b00000011 (bits 16,17 set).
        let data = vec![0xFFu8, 0x03, 0x03];
        let bm = ExfatBitmap::new(data, 24).unwrap();
        let runs = bm.free_runs();
        assert_eq!(runs, vec![(12u32, 6u32), (20u32, 6u32)]);
    }

    #[test]
    fn all_allocated_yields_no_runs() {
        let bm = ExfatBitmap::new(vec![0xFF; 4], 32).unwrap();
        assert!(bm.free_runs().is_empty());
    }

    #[test]
    fn all_free_yields_one_big_run() {
        let bm = ExfatBitmap::new(vec![0x00; 4], 32).unwrap();
        assert_eq!(bm.free_runs(), vec![(2u32, 32u32)]);
    }

    #[test]
    fn large_uniform_bitmap_uses_word_fast_path() {
        // 640 clusters (10 words): words 0-4 all free, word 5 all allocated,
        // words 6-9 mixed-lite. Tests the 64-cluster word skipping path.
        let mut data = vec![0x00u8; 80]; // 640 bits
        for b in &mut data[40..48] {
            *b = 0xFF; // word 5 allocated
        }
        let bm = ExfatBitmap::new(data, 640).unwrap();
        let runs = bm.free_runs();
        // one run 2..322 (320 clusters), then allocated 322..386, then free 386..642
        assert_eq!(runs, vec![(2u32, 320u32), (386u32, 256u32)]);
    }

    #[test]
    fn undersized_bitmap_is_rejected() {
        assert!(ExfatBitmap::new(vec![0x00; 1], 32).is_err());
    }
}
