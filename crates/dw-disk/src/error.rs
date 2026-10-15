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

    /// A positional read returned fewer bytes than requested before
    /// hitting end-of-file. Short reads must NEVER be silently accepted:
    /// the unwritten tail of the buffer would be zero-filled garbage that
    /// filesystem parsers would then interpret as on-disk data.
    #[error(
        "short read at offset {offset}: got {got} of {requested} bytes (source_size={source_size})"
    )]
    ShortRead {
        offset: u64,
        got: u64,
        requested: u64,
        source_size: u64,
    },

    /// A single read request exceeded the configured cap. Guards against
    /// unbounded allocations driven by attacker-controlled or corrupt
    /// length fields.
    #[error("read request too large: {len} bytes exceeds the {cap} byte cap")]
    RequestTooLarge { len: u64, cap: u64 },

    #[error("geometry error: {0}")]
    Geometry(String),

    #[error("corrupt structure: {0}")]
    Corrupt(String),

    #[error("unsupported source URI: {0}")]
    UnsupportedUri(String),

    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(String),
}
