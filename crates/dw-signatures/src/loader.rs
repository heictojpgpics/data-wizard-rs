//! Signature DB loader.
//!
//! The actual signature data is compiled at build time by `build.rs`
//! from the TOML files under `assets/public-signatures/`. This module
//! provides the runtime API for loading the compiled data.

use std::path::Path;

use crate::{SignatureDb, SignaturesError, SignaturesResult};

/// Load the compile-time-embedded signature database.
///
/// This is the primary constructor. The signatures are compiled from
/// public-source TOML files at build time.
#[must_use]
pub fn load_embedded() -> SignatureDb {
    SignatureDb::from_embedded()
}

/// Load a signature database from a compiled `signatures.bin` file.
///
/// This is for the planned future binary format. For now, use
/// [`load_embedded`] instead.
pub fn load_compiled(_path: &Path) -> SignaturesResult<SignatureDb> {
    Err(SignaturesError::Build(
        "compiled binary format not yet implemented — use load_embedded()".into(),
    ))
}

/// Build a DB from the embedded public sources. Alias for [`load_embedded`].
pub fn build_db_from_sources() -> SignaturesResult<SignatureDb> {
    Ok(load_embedded())
}
