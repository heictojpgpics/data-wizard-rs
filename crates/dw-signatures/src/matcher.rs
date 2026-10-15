//! Streaming matcher: given a window of bytes, find all signature hits.

use crate::{SignatureDb, SignatureId};

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub signature_id: SignatureId,
    pub offset: u64,
    pub extension: String,
    pub mime: Option<String>,
    pub weight: u16,
}

pub struct Matcher<'a> {
    db: &'a SignatureDb,
}

impl<'a> Matcher<'a> {
    #[must_use]
    pub fn new(db: &'a SignatureDb) -> Self {
        Self { db }
    }

    /// Find every signature whose header magic appears in `window`.
    ///
    /// The returned offset is the absolute offset of the *start of the
    /// file* (i.e. where the header magic's expected position begins),
    /// not the offset of the magic itself.
    pub fn find_headers(&self, window: &[u8], window_base_offset: u64) -> Vec<MatchResult> {
        let mut out = Vec::new();
        for sig in &self.db.signatures {
            if sig.header_magic.is_empty() {
                continue;
            }
            // Scan the window for the header magic at any position.
            // The magic may contain wildcards (None = match any byte).
            let magic = &sig.header_magic;
            let magic_len = magic.len();
            if magic_len > window.len() {
                continue;
            }
            for i in 0..=window.len() - magic_len {
                if matches_at(window, i, magic) {
                    // The file starts at: window_base_offset + i - header_offset
                    // (because the magic is at header_offset within the file)
                    let file_start = window_base_offset + i as u64 - u64::from(sig.header_offset);
                    out.push(MatchResult {
                        signature_id: sig.id,
                        offset: file_start,
                        extension: sig.extension.clone(),
                        mime: sig.mime.clone(),
                        weight: sig.weight,
                    });
                }
            }
        }
        out
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

        let matcher = Matcher::new(&db);
        let window = [0x00, 0x00, 0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00];
        let results = matcher.find_headers(&window, 1000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].offset, 1002); // 1000 + 2 (position of FF D8 FF in window)
        assert_eq!(results[0].extension, "jpg");
    }
}
