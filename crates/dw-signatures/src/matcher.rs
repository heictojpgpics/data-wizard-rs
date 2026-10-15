//! Streaming matcher: given a window of bytes, find all signature hits.
//!
//! Scan strategy: signatures are bucketed by their first concrete
//! (non-wildcard) byte. A single pass over the window looks up one bucket
//! per position and verifies only the handful of signatures that can
//! start there, instead of testing every signature at every byte. With
//! the ~258-signature database this collapses the per-position work from
//! 258 candidate checks to (typically) 0-2.
//!
//! The matcher is also boundary-aware: `scan_limit` bounds which matches
//! are *reported* while the window may extend further, so the carver can
//! read `chunk + max_magic` bytes and still attribute each hit to the
//! correct chunk (headers that straddle chunk edges are found, never
//! missed, and never double-reported).

use crate::{SignatureDb, SignatureId};

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub signature_id: SignatureId,
    pub offset: u64,
    pub extension: String,
    pub mime: Option<String>,
    pub weight: u16,
}

/// One bucketed signature: verified when `window[pos] == first_byte`.
struct Bucketed {
    /// Index into `db.signatures`.
    sig_index: usize,
    /// Position of the first concrete byte within the header magic.
    first_concrete: usize,
}

pub struct Matcher<'a> {
    db: &'a SignatureDb,
    /// 256 buckets keyed by the first concrete magic byte.
    buckets: Vec<(u8, Vec<Bucketed>)>,
    /// Signatures with no concrete byte in their magic (all wildcards):
    /// rare; checked at every position.
    wildcard_only: Vec<usize>,
    /// Longest magic in the database (windows must be at least this long
    /// for any match to be possible).
    pub max_magic_len: usize,
}

impl<'a> Matcher<'a> {
    #[must_use]
    pub fn new(db: &'a SignatureDb) -> Self {
        let mut buckets: std::collections::BTreeMap<u8, Vec<Bucketed>> =
            std::collections::BTreeMap::new();
        let mut wildcard_only = Vec::new();
        let mut max_magic_len = 0usize;

        for (idx, sig) in db.signatures.iter().enumerate() {
            if sig.header_magic.is_empty() {
                continue;
            }
            max_magic_len = max_magic_len.max(sig.header_magic.len());
            match sig.header_magic.iter().position(|b| b.is_some()) {
                Some(pos) => {
                    let byte = sig.header_magic[pos].expect("position() found a Some");
                    buckets.entry(byte).or_default().push(Bucketed {
                        sig_index: idx,
                        first_concrete: pos,
                    });
                }
                None => wildcard_only.push(idx),
            }
        }

        let buckets: Vec<(u8, Vec<Bucketed>)> = buckets.into_iter().collect();
        Self {
            db,
            buckets,
            wildcard_only,
            max_magic_len,
        }
    }

    /// Find every signature whose header magic appears in `window`.
    ///
    /// The returned offset is the absolute offset of the *start of the
    /// file* (i.e. where the header magic's expected position begins),
    /// not the offset of the magic itself.
    ///
    /// `scan_limit` (a window-relative index) bounds which matches are
    /// reported: a match whose magic *starts* at `window_base_offset +
    /// magic_pos` with `magic_pos < scan_limit` is reported. Pass
    /// `window.len()` for the classic whole-window behavior, or a chunk
    /// boundary to implement overlapped reads without double-reporting.
    pub fn find_headers_limited(
        &self,
        window: &[u8],
        window_base_offset: u64,
        scan_limit: usize,
    ) -> Vec<MatchResult> {
        let mut out = Vec::new();
        if self.buckets.is_empty() && self.wildcard_only.is_empty() {
            return out;
        }
        let limit = scan_limit.min(window.len());
        if limit == 0 {
            return out;
        }

        // Binary-search the bucket for each position's byte. The bucket
        // list is small (distinct first bytes), so the search is cheap and
        // most positions skip all verification.
        for pos in 0..limit {
            let byte = window[pos];
            let bucket = self
                .buckets
                .binary_search_by(|(b, _)| b.cmp(&byte))
                .ok()
                .map(|i| &self.buckets[i].1);
            if let Some(bucket) = bucket {
                for entry in bucket {
                    // The magic's first concrete byte must sit at `pos`;
                    // the magic therefore starts at pos - first_concrete.
                    let magic_start = pos - entry.first_concrete;
                    // (pos >= first_concrete is guaranteed: the bucket was
                    // keyed by the byte AT first_concrete, which must occur
                    // at or before pos... except first_concrete > pos is
                    // impossible because magic_start would underflow. Guard
                    // for safety; skip when it would.)
                    if entry.first_concrete > pos {
                        continue;
                    }
                    self.verify_and_emit(
                        window,
                        window_base_offset,
                        magic_start,
                        entry.sig_index,
                        &mut out,
                    );
                }
            }
            for &sig_index in &self.wildcard_only {
                self.verify_and_emit(window, window_base_offset, pos, sig_index, &mut out);
            }
        }
        out
    }

    /// Classic whole-window scan (every match in the window reported).
    pub fn find_headers(&self, window: &[u8], window_base_offset: u64) -> Vec<MatchResult> {
        self.find_headers_limited(window, window_base_offset, window.len())
    }

    fn verify_and_emit(
        &self,
        window: &[u8],
        window_base_offset: u64,
        magic_start: usize,
        sig_index: usize,
        out: &mut Vec<MatchResult>,
    ) {
        let sig = &self.db.signatures[sig_index];
        let magic = &sig.header_magic;
        // Magic must fit inside the window.
        if magic_start >= window.len() || magic_start + magic.len() > window.len() {
            return;
        }
        if !matches_at(window, magic_start, magic) {
            return;
        }
        // The file starts at magic_start - header_offset. Signatures with
        // a nonzero header_offset whose magic sits too close to the window
        // start cannot be attributed: skip (checked arithmetic; the old
        // code underflowed here).
        let Some(rel_file_start) = magic_start.checked_sub(sig.header_offset as usize) else {
            return;
        };
        out.push(MatchResult {
            signature_id: sig.id,
            offset: window_base_offset + rel_file_start as u64,
            extension: sig.extension.clone(),
            mime: sig.mime.clone(),
            weight: sig.weight,
        });
    }
}

/// Check if `window[pos..pos+magic.len()]` matches the magic pattern.
/// `None` in the magic = wildcard (matches any byte).
fn matches_at(window: &[u8], pos: usize, magic: &[Option<u8>]) -> bool {
    for (i, mb) in magic.iter().enumerate() {
        match mb {
            None => continue, // wildcard
            Some(expected) => {
                if window[pos + i] != *expected {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signature;

    fn test_db() -> SignatureDb {
        let mut db = SignatureDb::new();
        db.push(Signature {
            id: SignatureId(0),
            extension: "jpg".to_string(),
            mime: Some("image/jpeg".to_string()),
            header_offset: 0,
            header_magic: vec![Some(0xFF), Some(0xD8), Some(0xFF)],
            tail_offset_from_end: 0,
            tail_magic: vec![Some(0xFF), Some(0xD9)],
            min_size: 4,
            max_size: 0,
            weight: 990,
            source: "test".to_string(),
        });
        db.push(Signature {
            id: SignatureId(1),
            extension: "webp".to_string(),
            mime: None,
            header_offset: 0,
            header_magic: vec![
                Some(0x52),
                Some(0x49),
                Some(0x46),
                Some(0x46), // RIFF
                None,
                None,
                None,
                None, // size
                Some(0x57),
                Some(0x45),
                Some(0x42),
                Some(0x50), // WEBP
            ],
            tail_offset_from_end: 0,
            tail_magic: vec![],
            min_size: 12,
            max_size: 0,
            weight: 900,
            source: "test".to_string(),
        });
        db.push(Signature {
            id: SignatureId(2),
            extension: "xyz".to_string(), // first concrete byte at index 2
            mime: None,
            header_offset: 0,
            header_magic: vec![None, None, Some(0xAB), Some(0xCD)],
            tail_offset_from_end: 0,
            tail_magic: vec![],
            min_size: 4,
            max_size: 0,
            weight: 500,
            source: "test".to_string(),
        });
        db
    }

    #[test]
    fn matches_at_handles_wildcards() {
        // "FF D8 FF" (no wildcards)
        let magic = vec![Some(0xFFu8), Some(0xD8), Some(0xFF)];
        assert!(matches_at(&[0xFF, 0xD8, 0xFF, 0x00], 0, &magic));
        assert!(!matches_at(&[0xFF, 0xD8, 0x00, 0x00], 0, &magic));

        // "RIFF ?? ?? ?? ?? WEBP" (with wildcards)
        let magic = vec![
            Some(0x52u8),
            Some(0x49),
            Some(0x46),
            Some(0x46),
            None,
            None,
            None,
            None,
            Some(0x57),
            Some(0x45),
            Some(0x42),
            Some(0x50),
        ];
        assert!(matches_at(
            &[
                0x52, 0x49, 0x46, 0x46, 0xAA, 0xBB, 0xCC, 0xDD, 0x57, 0x45, 0x42, 0x50
            ],
            0,
            &magic
        ));
        assert!(!matches_at(
            &[
                0x52, 0x49, 0x46, 0x46, 0xAA, 0xBB, 0xCC, 0xDD, 0x57, 0x45, 0x42, 0x00
            ],
            0,
            &magic
        ));
    }

    #[test]
    fn find_headers_finds_jpeg() {
        let db = test_db();
        let matcher = Matcher::new(&db);
        let window = [0x00, 0x00, 0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00];
        let results = matcher.find_headers(&window, 1000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].offset, 1002); // 1000 + 2 (position of FF D8 FF in window)
        assert_eq!(results[0].extension, "jpg");
    }

    #[test]
    fn first_concrete_byte_bucketing_finds_delayed_magic() {
        // The xyz signature's first concrete byte (0xAB) is at index 2 of
        // its magic. The bucket must anchor on it, not on index 0. The
        // two leading wildcard bytes of the magic absorb window bytes
        // 0x11/0x22, so the FILE starts at 0 (magic_start - offset).
        let db = test_db();
        let matcher = Matcher::new(&db);
        let window = [0x11, 0x22, 0xAB, 0xCD, 0x00];
        let results = matcher.find_headers(&window, 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].extension, "xyz");
        assert_eq!(results[0].offset, 0);

        // With a non-matching byte where a wildcard still absorbs it, the
        // hit only exists when the concrete pair is present. No AB CD:
        let window = [0x11, 0x22, 0x33, 0x44, 0x00];
        assert!(matcher.find_headers(&window, 0).is_empty());
    }

    #[test]
    fn scan_limit_splits_overlapped_windows() {
        // A match at window position 5 reported when limit > 5, hidden
        // when limit <= 5. This is the chunk-boundary contract.
        let db = test_db();
        let matcher = Matcher::new(&db);
        let window = [0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xD8, 0xFF, 0x00];
        assert_eq!(matcher.find_headers_limited(&window, 0, 9).len(), 1);
        assert_eq!(matcher.find_headers_limited(&window, 0, 5).len(), 0);
        assert_eq!(matcher.find_headers_limited(&window, 0, 6).len(), 1);
    }

    #[test]
    fn magic_straddling_window_end_is_not_matched() {
        let db = test_db();
        let matcher = Matcher::new(&db);
        // JPEG magic truncated at the window edge: no match (the carver
        // relies on the overlap read to complete it in the NEXT window).
        let window = [0x00, 0xFF, 0xD8];
        assert_eq!(matcher.find_headers(&window, 0).len(), 0);
    }

    #[test]
    fn nonzero_header_offset_never_underflows() {
        let mut db = SignatureDb::new();
        db.push(Signature {
            id: SignatureId(0),
            extension: "off".to_string(),
            mime: None,
            header_offset: 8, // magic occurs 8 bytes INTO the file
            header_magic: vec![Some(0x99), Some(0x88)],
            tail_offset_from_end: 0,
            tail_magic: vec![],
            min_size: 10,
            max_size: 0,
            weight: 100,
            source: "test".to_string(),
        });
        let matcher = Matcher::new(&db);
        // Magic at position 0 with header_offset 8: file start would be
        // -8. Must be skipped (old code panicked/underflowed).
        let window = [0x99, 0x88, 0x00];
        assert!(matcher.find_headers(&window, 100).is_empty());
        // Magic at position 8 is fine.
        let window = [0u8; 10];
        let mut w = window.to_vec();
        w[8] = 0x99;
        w[9] = 0x88;
        let results = matcher.find_headers(&w, 100);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].offset, 100);
    }

    #[test]
    fn full_db_bucketing_covers_every_signature() {
        // Every embedded signature with a magic must be findable by the
        // bucketed matcher (no signature silently lost to bucketing).
        let db = SignatureDb::from_embedded();
        let matcher = Matcher::new(&db);
        let mut found = 0;
        for sig in &db.signatures {
            if sig.header_magic.is_empty() {
                continue;
            }
            // Build a window with the magic at a safe position (past any
            // header_offset).
            let pad = sig.header_offset as usize + 4;
            let mut window = vec![0u8; pad + sig.header_magic.len() + 4];
            for (i, b) in sig.header_magic.iter().enumerate() {
                if let Some(v) = b {
                    window[pad + i] = *v;
                }
            }
            let hits = matcher.find_headers(&window, 0);
            if hits.iter().any(|h| h.signature_id == sig.id) {
                found += 1;
            }
        }
        let with_magic = db
            .signatures
            .iter()
            .filter(|s| !s.header_magic.is_empty())
            .count();
        assert_eq!(
            found, with_magic,
            "every magic-bearing signature must be findable ({found}/{with_magic})"
        );
    }
}
