//! Signature database types.

use serde::{Deserialize, Serialize};

use crate::spec::{Signature, SignatureSpec};

/// Stable ID into the loaded database.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureId(pub u32);

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

    pub fn push(&mut self, mut sig: Signature) -> SignatureId {
        let id = SignatureId(self.signatures.len() as u32);
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

    /// Build the database from the compile-time-embedded signatures.
    ///
    /// This is the primary constructor. The signatures are compiled from
    /// the TOML files under `assets/public-signatures/` at build time
    /// by `build.rs`.
    #[must_use]
    pub fn from_embedded() -> Self {
        let mut db = Self::new();
        for spec in SIGNATURES {
            db.push(Signature::from(spec));
        }
        db
    }
}

impl Default for SignatureDb {
    fn default() -> Self {
        Self::new()
    }
}

// Include the auto-generated signatures from build.rs.
// This provides `SIGNATURES: &[SignatureSpec]`.
include!(concat!(env!("OUT_DIR"), "/signatures_gen.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_signatures_load() {
        let db = SignatureDb::from_embedded();
        assert!(
            db.len() > 10,
            "expected at least 10 embedded signatures, got {}",
            db.len()
        );
    }

    #[test]
    fn embedded_signatures_have_extensions() {
        let db = SignatureDb::from_embedded();
        for sig in &db.signatures {
            assert!(
                !sig.extension.is_empty(),
                "signature with source '{}' has empty extension",
                sig.source
            );
        }
    }

    #[test]
    fn embedded_signatures_have_header_magic() {
        let db = SignatureDb::from_embedded();
        for sig in &db.signatures {
            assert!(
                !sig.header_magic.is_empty(),
                "signature '{}' has empty header_magic",
                sig.extension
            );
        }
    }

    #[test]
    fn embedded_signatures_have_sources() {
        let db = SignatureDb::from_embedded();
        for sig in &db.signatures {
            assert!(
                !sig.source.is_empty(),
                "signature '{}' has empty source citation",
                sig.extension
            );
        }
    }

    #[test]
    fn embedded_signatures_include_known_formats() {
        let db = SignatureDb::from_embedded();
        let extensions: std::collections::HashSet<&str> =
            db.signatures.iter().map(|s| s.extension.as_str()).collect();

        // Verify a representative set of formats are present.
        for expected in ["jpg", "png", "pdf", "zip", "exe", "sqlite"] {
            assert!(
                extensions.contains(expected),
                "expected '.{expected}' in embedded signatures"
            );
        }
    }
}
