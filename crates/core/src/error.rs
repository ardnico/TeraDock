use std::{io, result};

use common::id::IdError;
use thiserror::Error;

pub type Result<T> = result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("failed to resolve application directories")]
    DirectoryResolution,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid id: {0:?}")]
    InvalidId(#[from] IdError),
    #[error("unknown profile: {0}")]
    NotFound(String),
}
