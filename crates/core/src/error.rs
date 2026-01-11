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
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("invalid id: {0:?}")]
    InvalidId(IdError),
    #[error("invalid command spec: {0}")]
    InvalidCommandSpec(String),
    #[error("parser not found: {0}")]
    ParserNotFound(String),
    #[error("regex error: {0}")]
    Regex(String),
    #[error("unknown profile: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("import error: {0}")]
    Import(String),
    #[error("master password not set")]
    MasterNotSet,
    #[error("master password already set")]
    MasterAlreadySet,
    #[error("master password verification failed")]
    MasterVerificationFailed,
    #[error("decryption failed")]
    DecryptionFailed,
}
