use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Configuration missing at {0}")]
    MissingConfig(PathBuf),
}
