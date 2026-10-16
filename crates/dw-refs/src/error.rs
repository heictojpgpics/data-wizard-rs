//! Error surface for the ReFS engine.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RefsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("source too small for a ReFS boot sector: {size} bytes (need 512)")]
    TooSmall { size: u64 },

    #[error("not a ReFS volume (boot magic absent)")]
    NotRefs,

    #[error("unsupported geometry: {reason}")]
    BadGeometry { reason: String },

    #[error("page scan of {size} bytes at {off:#x} exceeds source ({src} bytes)")]
    ReadOutOfBounds { off: u64, size: usize, src: u64 },

    #[error("corrupt structure: {0}")]
    Corrupt(String),
}

pub type RefsResult<T> = Result<T, RefsError>;
