use thiserror::Error;

pub type ExtResult<T> = Result<T, ExtError>;

#[derive(Debug, Error)]
pub enum ExtError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("disk error: {0}")]
    Disk(#[from] dw_disk::DiskError),
    /// Volume is not ext2/3/4 (probe miss).
    #[error("not an ext filesystem")]
    NotExt,
    #[error("corrupt on-disk structure: {0}")]
    Corrupt(String),
    #[error("out of range: offset={offset}, len={len}")]
    OutOfRange { offset: u64, len: u64 },
}
