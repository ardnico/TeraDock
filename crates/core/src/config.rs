use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
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
    pub secret_key_path: PathBuf,
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
        let secret_key_path = base_dir.join("secret.key");
        Ok(Self {
            base_dir,
            settings_path,
            secret_key_path,
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
    #[serde(default)]
    pub ui: UiPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default = "ThemePreference::default_mode")]
    pub theme: ThemePreference,
    #[serde(default = "UiPreferences::default_font_family")]
    pub font_family: String,
    #[serde(default = "UiPreferences::default_text_size")]
    pub text_size: f32,
}

impl UiPreferences {
    fn default_text_size() -> f32 {
        16.0
    }

    fn default_font_family() -> String {
        "proportional".to_string()
    }
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            font_family: Self::default_font_family(),
            text_size: Self::default_text_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn default_mode() -> Self {
        ThemePreference::System
    }
}

impl AppConfig {
    pub fn load_or_default(paths: &AppPaths) -> Result<Self> {
        paths.ensure_base()?;
        if paths.settings_path.exists() {
            let mut reader = BufReader::new(File::open(&paths.settings_path)?);
            let mut content = String::new();
            reader.read_to_string(&mut content)?;
            let mut config: AppConfig = toml::from_str(&content)?;
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
        if (self.ui.text_size - 0.0).abs() < f32::EPSILON {
            self.ui.text_size = UiPreferences::default_text_size();
        }
        if self.ui.font_family.trim().is_empty() {
            self.ui.font_family = UiPreferences::default_font_family();
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
            ui: UiPreferences::default(),
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
