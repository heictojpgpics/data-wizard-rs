//! Shared physical-extent construction helpers.
//!
//! Filesystem walkers translate FS-native structures (FAT cluster chains,
//! exFAT chains, NTFS run lists) into canonical `Vec<Extent>` lists that
//! the recovery writer consumes. All walkers share the same
//! merge-and-trim rule so extent semantics are identical across engines:
//!
//!  - input: ascending absolute byte offsets, each `step` bytes long
//!    (one per FS cluster/run unit);
//!  - consecutive offsets merge into one extent;
//!  - the total is trimmed to `file_size` (the last unit is cut short);
//!  - a chain shorter than `file_size` yields proportionally shorter
//!    extents; the caller marks the entry `PartiallyRecovered`.

use crate::results::Extent;

/// Merge ascending byte offsets (each `step` bytes long) into contiguous
/// extents and trim the total to `file_size`.
///
/// See the module docs for the exact rule. `step` must be non-zero when
/// `offsets` is non-empty; empty input yields an empty vector.
pub fn merge_and_trim_offsets(offsets: &[u64], step: u64, file_size: u64) -> Vec<Extent> {
    if step == 0 || offsets.is_empty() {
        return Vec::new();
    }
    let mut extents: Vec<Extent> = Vec::new();
    let mut remaining = file_size;

    for &off in offsets {
        if remaining == 0 {
            break;
        }
        let want = step.min(remaining);
        // Extend the previous extent if contiguous, else push a new one.
        if let Some(last) = extents.last_mut()
            && last.start_byte + last.length == off
        {
            last.length += want;
        } else {
            extents.push(Extent {
                start_byte: off,
                length: want,
            });
        }
        remaining -= want;
    }

    extents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_offsets_merge_into_one_extent() {
        let extents = merge_and_trim_offsets(&[100, 200, 300], 100, 250);
        assert_eq!(extents.len(), 1);
        assert_eq!(extents[0].start_byte, 100);
        // Trimmed to file_size 250, not 300.
        assert_eq!(extents[0].length, 250);
    }

    #[test]
    fn fragmented_offsets_yield_multiple_extents() {
        // clusters at 100, 200, then a gap, then 500, 600
        let extents = merge_and_trim_offsets(&[100, 200, 500, 600], 100, 400);
        assert_eq!(extents.len(), 2);
        assert_eq!((extents[0].start_byte, extents[0].length), (100, 200));
        assert_eq!((extents[1].start_byte, extents[1].length), (500, 200));
    }

    #[test]
    fn short_chain_is_trimmed_honestly() {
        // File says 1000 bytes but the chain only covers 2 clusters (200).
        let extents = merge_and_trim_offsets(&[100, 200], 100, 1000);
        assert_eq!(extents.len(), 1);
        assert_eq!(extents[0].length, 200);
    }

    #[test]
    fn zero_step_or_empty_input_is_noop() {
        assert!(merge_and_trim_offsets(&[1, 2, 3], 0, 100).is_empty());
        assert!(merge_and_trim_offsets(&[], 512, 100).is_empty());
    }

    #[test]
    fn zero_file_size_yields_nothing() {
        assert!(merge_and_trim_offsets(&[100, 200], 100, 0).is_empty());
    }

    #[test]
    fn exact_size_chain_is_not_over_read() {
        let extents = merge_and_trim_offsets(&[100, 200], 100, 200);
        assert_eq!(extents.len(), 1);
        assert_eq!(extents[0].length, 200);
    }
}
