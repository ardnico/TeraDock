use std::path::PathBuf;

use directories::BaseDirs;

use crate::error::{CoreError, Result};

pub fn config_dir() -> Result<PathBuf> {
    let dirs = BaseDirs::new().ok_or(CoreError::DirectoryResolution)?;
    let base = if cfg!(windows) {
        dirs.config_dir().join("TeraDock")
    } else {
        dirs.config_dir().join("teradock")
    };
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

pub fn logs_dir() -> Result<PathBuf> {
    let mut dir = config_dir()?;
    dir.push("logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn database_path() -> Result<PathBuf> {
    let mut dir = config_dir()?;
    dir.push("teradock.db");
    Ok(dir)
}
