use std::fs::File;
use std::io::{BufReader, Read};
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
    #[serde(default = "ClientKind::default_kind")]
    pub client_kind: ClientKind,
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
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub ssh_forwardings: Vec<SshForwarding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    WindowsTerminalSsh,
    PlainSsh,
    TeraTerm,
}

impl Default for ClientKind {
    fn default() -> Self {
        ClientKind::WindowsTerminalSsh
    }
}

impl ClientKind {
    pub fn default_kind() -> Self {
        ClientKind::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ForwardDirection {
    Local,
    Remote,
    Dynamic,
}

impl Default for ForwardDirection {
    fn default() -> Self {
        ForwardDirection::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SshForwarding {
    #[serde(default)]
    pub direction: ForwardDirection,
    #[serde(default)]
    pub local_host: Option<String>,
    #[serde(default)]
    pub local_port: u16,
    #[serde(default)]
    pub remote_host: String,
    #[serde(default)]
    pub remote_port: u16,
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

    pub fn forwarding_args(&self) -> Vec<String> {
        self.ssh_forwardings
            .iter()
            .filter_map(|f| f.to_arg())
            .collect()
    }
}

impl SshForwarding {
    pub fn to_arg(&self) -> Option<String> {
        let lh = self
            .local_host
            .clone()
            .unwrap_or_else(|| "127.0.0.1".into());
        match self.direction {
            ForwardDirection::Local => Some(format!(
                "-L{lh}:{lp}:{rh}:{rp}",
                lp = self.local_port,
                rh = self.remote_host,
                rp = self.remote_port
            )),
            ForwardDirection::Remote => Some(format!(
                "-R{lh}:{lp}:{rh}:{rp}",
                lp = self.local_port,
                rh = self.remote_host,
                rp = self.remote_port
            )),
            ForwardDirection::Dynamic => {
                if self.local_port == 0 {
                    None
                } else {
                    Some(format!("-D{lh}:{lp}", lp = self.local_port))
                }
            }
        }
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
        let mut reader = BufReader::new(File::open(path)?);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        let set: ProfileSet = toml::from_str(&content)?;
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
