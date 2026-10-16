//! Error surface for the ZFS engine.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZfsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("source too small for vdev labels: {size} bytes (need at least {min} bytes)")]
    SourceTooSmall { size: u64, min: u64 },

    #[error("no valid vdev label (nvlist checksum or geometry) on any of the 4 labels")]
    NoValidLabel,

    #[error("label {label}: nvlist: {reason}")]
    BadNvlist { label: usize, reason: String },

    #[error("label {label}: checksum mismatch (nvlist fletcher4)")]
    LabelChecksumMismatch { label: usize },

    #[error("no uberblock in any label ring")]
    NoUberblock,

    #[error("pool geometry out of scope: {reason} (raw carving only)")]
    OutOfScope { reason: String },

    #[error("corrupt metadata: {0}")]
    Corrupt(String),

    #[error("block read of {size} bytes at {off:#x} exceeds source ({src} bytes)")]
    ReadOutOfBounds { off: u64, size: usize, src: u64 },

    #[error("checksum mismatch for block at {off:#x} (algo {algo})")]
    BlockChecksumMismatch { off: u64, algo: &'static str },

    #[error("decompression failed for block at {off:#x} (algo {algo})")]
    DecompressFailed { off: u64, algo: &'static str },

    #[error("embedded block pointer at {off:#x}: inline payloads are not data extents")]
    EmbeddedBp { off: u64 },

    #[error("indirect tree too deep ({levels} levels) at object {obj}")]
    IndirectTooDeep { levels: u8, obj: u64 },

    #[error("object {obj}: {reason}")]
    Object { obj: u64, reason: String },

    #[error("zap object {obj}: {reason}")]
    Zap { obj: u64, reason: String },
}

pub type ZfsResult<T> = Result<T, ZfsError>;
