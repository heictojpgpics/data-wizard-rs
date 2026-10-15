use thiserror::Error;

pub type FatResult<T> = Result<T, FatError>;

#[derive(Debug, Error)]
pub enum FatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("disk error: {0}")]
    Disk(#[from] dw_disk::DiskError),
    #[error("corrupt on-disk structure: {0}")]
    Corrupt(String),
    #[error("out of range: offset={offset}, len={len}")]
    OutOfRange { offset: u64, len: u64 },
    #[error("not yet implemented: {0}")]
    Todo(&'static str),
}
