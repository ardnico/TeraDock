use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::{Error, Result};

const DEFAULT_TERA_TERM_PATH: &str = "C:/Program Files (x86)/teraterm/ttermpro.exe";
const DEFAULT_PROFILES: &str = include_str!("../../../config/default_profiles.toml");

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
    pub settings_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let base_dir = if let Ok(home) = env::var("TTLAUNCH_HOME") {
            PathBuf::from(home)
        } else {
            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let repo_config = cwd.join("config");
            if repo_config.exists() {
                repo_config
            } else if let Some(dirs) = ProjectDirs::from("com", "teradock", "ttlaunch") {
                dirs.config_dir().to_path_buf()
            } else {
                cwd
            }
        };

        let settings_path = base_dir.join("settings.toml");
        Ok(Self {
            base_dir,
            settings_path,
        })
    }

    pub fn ensure_base(&self) -> Result<()> {
        if !self.base_dir.exists() {
            fs::create_dir_all(&self.base_dir)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub tera_term_path: PathBuf,
    pub profiles_path: PathBuf,
    pub history_path: PathBuf,
}

impl AppConfig {
    pub fn load_or_default(paths: &AppPaths) -> Result<Self> {
        paths.ensure_base()?;
        if paths.settings_path.exists() {
            let reader = BufReader::new(File::open(&paths.settings_path)?);
            let mut config: AppConfig = toml::de::from_reader(reader)?;
            config.normalize(paths);
            Ok(config)
        } else {
            let mut config = Self::default_for(paths);
            config.write_defaults_if_missing()?;
            config.save(&paths.settings_path)?;
            Ok(config)
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        let mut file = File::create(path)?;
        file.write_all(toml_str.as_bytes())?;
        Ok(())
    }

    pub fn write_defaults_if_missing(&mut self) -> Result<()> {
        if let Some(parent) = self.profiles_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !self.profiles_path.exists() {
            let mut file = File::create(&self.profiles_path)?;
            file.write_all(DEFAULT_PROFILES.as_bytes())?;
        }
        if let Some(parent) = self.history_path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn normalize(&mut self, paths: &AppPaths) {
        if self.tera_term_path.as_os_str().is_empty() {
            self.tera_term_path = PathBuf::from(DEFAULT_TERA_TERM_PATH);
        }
        if self.profiles_path.as_os_str().is_empty() {
            self.profiles_path = paths.base_dir.join("default_profiles.toml");
        }
        if self.history_path.as_os_str().is_empty() {
            self.history_path = paths.base_dir.join("history.jsonl");
        }
    }

    fn default_for(paths: &AppPaths) -> Self {
        let tera_term_path = PathBuf::from(DEFAULT_TERA_TERM_PATH);
        let profiles_path = paths.base_dir.join("default_profiles.toml");
        let history_path = paths.base_dir.join("history.jsonl");
        Self {
            tera_term_path,
            profiles_path,
            history_path,
        }
    }

    pub fn validate_tera_term_path(&self) -> Result<()> {
        if self.tera_term_path.exists() {
            Ok(())
        } else {
            Err(Error::MissingConfig(self.tera_term_path.clone()))
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "Tera Term: {}\nProfiles: {}\nHistory: {}",
            self.tera_term_path.display(),
            self.profiles_path.display(),
            self.history_path.display()
        )
    }
}

pub fn log_paths(config: &AppConfig) {
    debug!("Using config: {}", config.describe());
}
