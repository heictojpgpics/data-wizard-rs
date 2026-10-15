//! Error types for `dw-core`.

use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("disk I/O error: {0}")]
    Disk(#[from] dw_disk::DiskError),

    #[error("plugin error: {0}")]
    Plugin(String),

    #[error("scan not found: {id}")]
    ScanNotFound { id: uuid::Uuid },

    #[error("scan state invalid: expected {expected:?}, got {actual:?}")]
    InvalidState {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("results store is full (capacity {capacity}, current {current})")]
    ResultsFull { capacity: usize, current: usize },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("results spill error: {0}")]
    Spill(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("cancelled by user")]
    Cancelled,
}
