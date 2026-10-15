//! Error types for `dw-carver`.

use thiserror::Error;

pub type CarverResult<T> = Result<T, CarverError>;

#[derive(Debug, Error)]
pub enum CarverError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("disk error: {0}")]
    Disk(#[from] dw_disk::DiskError),
    #[error("signatures error: {0}")]
    Signatures(#[from] dw_signatures::SignaturesError),
    #[error("carve plan invalid: {0}")]
    PlanInvalid(String),
    #[error("not yet implemented: {0}")]
    Todo(&'static str),
}
