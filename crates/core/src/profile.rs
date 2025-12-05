use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Ssh,
    Telnet,
}

impl Default for Protocol {
    fn default() -> Self {
        Protocol::Ssh
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DangerLevel {
    Normal,
    Warn,
    Critical,
}

impl Default for DangerLevel {
    fn default() -> Self {
        DangerLevel::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub danger_level: DangerLevel,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub macro_path: Option<PathBuf>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

impl Profile {
    pub fn is_dangerous(&self) -> bool {
        self.danger_level == DangerLevel::Critical
            || self
                .group
                .as_deref()
                .map(|g| g.eq_ignore_ascii_case("prod"))
                .unwrap_or(false)
    }

    pub fn display_title(&self) -> String {
        self.name.clone()
    }

    pub fn matches_filter(&self, text: &str) -> bool {
        let t = text.to_lowercase();
        self.id.to_lowercase().contains(&t)
            || self.name.to_lowercase().contains(&t)
            || self
                .group
                .as_deref()
                .map(|g| g.to_lowercase().contains(&t))
                .unwrap_or(false)
            || self.tags.iter().any(|tag| tag.to_lowercase().contains(&t))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSet {
    pub profiles: Vec<Profile>,
}

impl ProfileSet {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::MissingConfig(path.to_path_buf()));
        }
        let reader = BufReader::new(File::open(path)?);
        let set: ProfileSet = toml::de::from_reader(reader)?;
        Ok(set)
    }

    pub fn find(&self, id: &str) -> Option<Profile> {
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .or_else(|| self.profiles.iter().find(|p| p.name == id).cloned())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(path, toml)?;
        Ok(())
    }
}
