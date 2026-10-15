//! Error types for `dw-signatures`.

use thiserror::Error;

pub type SignaturesResult<T> = Result<T, SignaturesError>;

#[derive(Debug, Error)]
pub enum SignaturesError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt signature DB: {0}")]
    Corrupt(String),
    #[error("build error: {0}")]
    Build(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
