//! Signature database types.

use serde::{Deserialize, Serialize};

/// Stable ID into the loaded database.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureId(pub u32);

/// One entry in the signature DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub id: SignatureId,
    /// Lowercase extension without leading dot, e.g. `"pdf"`.
    pub extension: String,
    /// MIME type if known, e.g. `"application/pdf"`.
    pub mime: Option<String>,
    /// Offset where the header magic is expected.
    pub header_offset: u32,
    /// Header magic bytes.
    pub header_magic: Vec<u8>,
    /// Offset from END of file where tail magic is expected (0 = no tail).
    pub tail_offset_from_end: u32,
    /// Tail magic bytes (empty if no tail).
    pub tail_magic: Vec<u8>,
    /// Minimum file size to accept a match.
    pub min_size: u64,
    /// Maximum file size (0 = unlimited).
    pub max_size: u64,
    /// Confidence weight 0..=1000.
    pub weight: u16,
}

/// In-memory signature DB.
pub struct SignatureDb {
    pub signatures: Vec<Signature>,
}

impl SignatureDb {
    #[must_use]
    pub fn new() -> Self {
        Self {
            signatures: Vec::new(),
        }
    }

    pub fn push(&mut self, sig: Signature) -> SignatureId {
        let id = SignatureId(self.signatures.len() as u32);
        let mut sig = sig;
        sig.id = id;
        self.signatures.push(sig);
        id
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }
}

impl Default for SignatureDb {
    fn default() -> Self {
        Self::new()
    }
}
