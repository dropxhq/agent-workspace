use thiserror::Error;

#[derive(Debug, Error)]
pub enum WsError {
    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("path escapes workspace: {0}")]
    PathEscape(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("lock conflict: {0}")]
    LockConflict(String),

    #[error("invalid ranges: {0}")]
    InvalidRanges(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl WsError {
    pub fn exit_code(&self) -> u8 {
        match self {
            WsError::InvalidPath(_) | WsError::PathEscape(_) => 2,
            WsError::NotFound(_) => 3,
            WsError::LockConflict(_) => 4,
            _ => 1,
        }
    }
}

pub type WsResult<T> = Result<T, WsError>;
