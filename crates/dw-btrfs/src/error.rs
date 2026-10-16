use thiserror::Error;

pub type BtrfsResult<T> = Result<T, BtrfsError>;

#[derive(Debug, Error)]
pub enum BtrfsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("disk error: {0}")]
    Disk(#[from] dw_disk::DiskError),
    /// Volume is not Btrfs (probe miss).
    #[error("not a btrfs volume")]
    NotBtrfs,
    #[error("corrupt on-disk structure: {0}")]
    Corrupt(String),
    #[error("unsupported on-disk profile: {0}")]
    Unsupported(String),
    #[error("out of range: offset={offset}, len={len}")]
    OutOfRange { offset: u64, len: u64 },
}
