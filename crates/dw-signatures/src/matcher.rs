//! Streaming matcher: given a window of bytes, find all signature hits.

use crate::{Signature, SignatureDb};

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub signature_id: crate::SignatureId,
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

    /// Find every signature whose header magic appears in `window`. The
    /// returned offset is the absolute offset of the *magic* (not the
    /// start of the file).
    pub fn find_headers(&self, window: &[u8], window_base_offset: u64) -> Vec<MatchResult> {
        let mut out = Vec::new();
        for sig in &self.db.signatures {
            if sig.header_magic.is_empty() {
                continue;
            }
            let mut search_start = 0usize;
            let needle = &sig.header_magic[..];
            while let Some(rel) = window[search_start..].windows(needle.len()).position(|w| w == needle) {
                let abs = window_base_offset + (search_start + rel) as u64;
                if abs >= u64::from(sig.header_offset) {
                    let header_pos = abs - u64::from(sig.header_offset);
                    out.push(MatchResult {
                        signature_id: sig.id,
                        offset: header_pos,
                        extension: sig.extension.clone(),
                        mime: sig.mime.clone(),
                        weight: sig.weight,
                    });
                }
                search_start += rel + 1;
            }
        }
        out
    }
}
