use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::Result;
use crate::profile::SshForwarding;

const DEFAULT_SSH_PATH: &str = "ssh";
const DEFAULT_WINDOWS_TERMINAL_PATH: &str = "wt.exe";
const DEFAULT_TERA_TERM_PATH: &str = "C:/Program Files (x86)/teraterm/ttermpro.exe";
const DEFAULT_PROFILES: &str = include_str!("../../../config/default_profiles.toml");

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
    pub settings_path: PathBuf,
    pub secret_key_path: PathBuf,
    pub shared_profiles_path: PathBuf,
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
        let shared_profiles_path = base_dir.join("shared_profiles.toml");
        Ok(Self {
            base_dir,
            settings_path,
            secret_key_path,
            shared_profiles_path,
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
    #[serde(default = "AppConfig::default_ssh_path")]
    pub ssh_path: PathBuf,
    #[serde(default = "AppConfig::default_windows_terminal_path")]
    pub windows_terminal_path: Option<PathBuf>,
    #[serde(default = "AppConfig::default_tera_term_path")]
    pub tera_term_path: Option<PathBuf>,
    pub profiles_path: PathBuf,
    pub history_path: PathBuf,
    #[serde(default)]
    pub ui: UiPreferences,
    #[serde(default)]
    pub secrets: SecretsConfig,
    #[serde(default)]
    pub forwarding_presets: Vec<ForwardingPreset>,
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
    fn default_ssh_path() -> PathBuf {
        PathBuf::from(DEFAULT_SSH_PATH)
    }

    fn default_windows_terminal_path() -> Option<PathBuf> {
        Some(PathBuf::from(DEFAULT_WINDOWS_TERMINAL_PATH))
    }

    fn default_tera_term_path() -> Option<PathBuf> {
        Some(PathBuf::from(DEFAULT_TERA_TERM_PATH))
    }

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
        if self.ssh_path.as_os_str().is_empty() {
            self.ssh_path = Self::default_ssh_path();
        }
        if let Some(path) = self.windows_terminal_path.as_ref() {
            if path.as_os_str().is_empty() {
                self.windows_terminal_path = Self::default_windows_terminal_path();
            }
        }
        if let Some(path) = self.tera_term_path.as_ref() {
            if path.as_os_str().is_empty() {
                self.tera_term_path = None;
            }
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
        if self.secrets.credential_target.trim().is_empty() {
            self.secrets.credential_target = SecretsConfig::default_target();
        }
    }

    fn default_for(paths: &AppPaths) -> Self {
        let ssh_path = Self::default_ssh_path();
        let windows_terminal_path = Self::default_windows_terminal_path();
        let tera_term_path = Self::default_tera_term_path();
        let profiles_path = paths.base_dir.join("default_profiles.toml");
        let history_path = paths.base_dir.join("history.jsonl");
        Self {
            ssh_path,
            windows_terminal_path,
            tera_term_path,
            profiles_path,
            history_path,
            ui: UiPreferences::default(),
            secrets: SecretsConfig::default(),
            forwarding_presets: Vec::new(),
        }
    }

    pub fn windows_terminal_available(&self) -> bool {
        self.windows_terminal_path
            .as_ref()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    false
                } else if p.is_absolute() {
                    p.exists()
                } else {
                    true
                }
            })
            .unwrap_or(false)
    }

    pub fn ssh_available(&self) -> bool {
        !self.ssh_path.as_os_str().is_empty()
    }

    pub fn describe(&self) -> String {
        format!(
            "SSH: {}\nWindows Terminal: {}\nTera Term: {}\nProfiles: {}\nHistory: {}\nSecret backend: {:?}",
            self.ssh_path.display(),
            self
                .windows_terminal_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unset>".into()),
            self
                .tera_term_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unset>".into()),
            self.profiles_path.display(),
            self.history_path.display(),
            self.secrets.backend
        )
    }
}

pub fn log_paths(config: &AppConfig) {
    debug!("Using config: {}", config.describe());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default = "SecretsConfig::default_backend")]
    pub backend: SecretBackend,
    #[serde(default = "SecretsConfig::default_target")]
    pub credential_target: String,
}

impl SecretsConfig {
    fn default_target() -> String {
        "TeraDock/ttlaunch".to_string()
    }

    fn default_backend() -> SecretBackend {
        SecretBackend::FileKey
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            backend: Self::default_backend(),
            credential_target: Self::default_target(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretBackend {
    FileKey,
    WindowsCredentialManager,
    WindowsDpapi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardingPreset {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub rule: SshForwarding,
}

impl Default for ForwardingPreset {
    fn default() -> Self {
        Self {
            name: "default".into(),
            description: None,
            rule: SshForwarding::default(),
        }
    }
}
