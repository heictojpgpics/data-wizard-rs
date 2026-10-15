//! Signature DB loader.
//!
//! This module reads the build-time-compiled `signatures.bin` file and
//! returns a [`SignatureDb`]. The build script that produces
//! `signatures.bin` is in `crates/dw-signatures/build/` and consumes only
//! public sources (Gary Kessler's GCF file signatures, NIST DFXML,
//! PRONOM, libmagic). No proprietary signature data is ever read or
//! redistributed.

use std::path::Path;

use crate::{Signature, SignatureDb, SignaturesError, SignaturesResult};

const MAGIC: &[u8; 4] = b"DWSG";
const VERSION: u16 = 1;

pub fn load_compiled(path: &Path) -> SignaturesResult<SignatureDb> {
    let bytes = std::fs::read(path)?;
    parse_compiled(&bytes)
}

fn parse_compiled(bytes: &[u8]) -> SignaturesResult<SignatureDb> {
    if bytes.len() < 32 {
        return Err(SignaturesError::Corrupt("file too short".into()));
    }
    if &bytes[0..4] != MAGIC {
        return Err(SignaturesError::Corrupt("bad magic".into()));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(SignaturesError::Corrupt(format!(
            "unsupported version {version}"
        )));
    }
    let count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let mut db = SignatureDb::new();
    let mut p = 32usize;
    for _ in 0..count {
        if p >= bytes.len() {
            return Err(SignaturesError::Corrupt("truncated".into()));
        }
        let ext_len = bytes[p] as usize;
        p += 1;
        let ext = std::str::from_utf8(&bytes[p..p + ext_len])
            .map_err(|e| SignaturesError::Corrupt(format!("ext utf8: {e}")))?
            .to_owned();
        p += ext_len;
        let mime_len = bytes[p] as usize;
        p += 1;
        let mime = if mime_len == 0 {
            None
        } else {
            Some(
                std::str::from_utf8(&bytes[p..p + mime_len])
                    .map_err(|e| SignaturesError::Corrupt(format!("mime utf8: {e}")))?
                    .to_owned(),
            )
        };
        p += mime_len;
        if p + 6 > bytes.len() {
            return Err(SignaturesError::Corrupt("hdr offset/len".into()));
        }
        let header_offset =
            u32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]);
        p += 4;
        let header_len = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + header_len > bytes.len() {
            return Err(SignaturesError::Corrupt("hdr magic".into()));
        }
        let header_magic = bytes[p..p + header_len].to_vec();
        p += header_len;
        if p + 6 > bytes.len() {
            return Err(SignaturesError::Corrupt("tail offset/len".into()));
        }
        let tail_offset_from_end =
            u32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]);
        p += 4;
        let tail_len = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + tail_len > bytes.len() {
            return Err(SignaturesError::Corrupt("tail magic".into()));
        }
        let tail_magic = bytes[p..p + tail_len].to_vec();
        p += tail_len;
        if p + 16 > bytes.len() {
            return Err(SignaturesError::Corrupt("size range".into()));
        }
        let min_size = u64::from_le_bytes(bytes[p..p + 8].try_into().unwrap());
        p += 8;
        let max_size = u64::from_le_bytes(bytes[p..p + 8].try_into().unwrap());
        p += 8;
        if p + 2 > bytes.len() {
            return Err(SignaturesError::Corrupt("weight".into()));
        }
        let weight = u16::from_le_bytes([bytes[p], bytes[p + 1]]);
        p += 2;
        db.push(Signature {
            id: crate::SignatureId(0), // reassigned by push
            extension: ext,
            mime,
            header_offset,
            header_magic,
            tail_offset_from_end,
            tail_magic,
            min_size,
            max_size,
            weight,
        });
    }
    Ok(db)
}

/// Build a DB from public sources (placeholder -- the actual build script
/// will pull from `assets/public-signatures/` and compile to
/// `signatures.bin`).
pub fn build_db_from_sources() -> SignaturesResult<SignatureDb> {
    // Placeholder: real implementation will read TOML/JSON files under
    // crates/dw-signatures/assets/public-signatures/ and compile them.
    Ok(SignatureDb::new())
}
