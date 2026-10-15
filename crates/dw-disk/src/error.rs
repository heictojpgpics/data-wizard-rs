//! Error types for `dw-disk`.

use thiserror::Error;

pub type DiskResult<T> = Result<T, DiskError>;

#[derive(Debug, Error)]
pub enum DiskError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("source out of range: offset={offset}, len={len}, source_size={source_size}")]
    OutOfRange {
        offset: u64,
        len: u64,
        source_size: u64,
    },

    #[error("geometry error: {0}")]
    Geometry(String),

    #[error("unsupported source URI: {0}")]
    UnsupportedUri(String),

    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(String),
}
