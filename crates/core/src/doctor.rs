use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::agent::{self, AgentStatus};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum ClientSource {
    ProfileOverride,
    GlobalOverride,
    Path,
    Missing,
}

impl std::fmt::Display for ClientSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientSource::ProfileOverride => write!(f, "profile override"),
            ClientSource::GlobalOverride => write!(f, "global override"),
            ClientSource::Path => write!(f, "path"),
            ClientSource::Missing => write!(f, "missing"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub name: String,
    pub path: Option<PathBuf>,
    pub source: ClientSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub clients: Vec<ClientStatus>,
    pub agent: AgentStatus,
    pub warnings: Vec<DoctorMessage>,
    pub errors: Vec<DoctorMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorMessage {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ClientKind {
    Ssh,
    Scp,
    Sftp,
    Ftp,
    Telnet,
}

impl ClientKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClientKind::Ssh => "ssh",
            ClientKind::Scp => "scp",
            ClientKind::Sftp => "sftp",
            ClientKind::Ftp => "ftp",
            ClientKind::Telnet => "telnet",
        }
    }

    fn candidates(&self) -> &'static [&'static str] {
        match self {
            ClientKind::Ssh => &["ssh", "ssh.exe"],
            ClientKind::Scp => &["scp", "scp.exe"],
            ClientKind::Sftp => &["sftp", "sftp.exe"],
            ClientKind::Ftp => &["ftp", "ftp.exe"],
            ClientKind::Telnet => &["telnet", "telnet.exe"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientOverrides {
    pub ssh: Option<String>,
    pub scp: Option<String>,
    pub sftp: Option<String>,
    pub ftp: Option<String>,
    pub telnet: Option<String>,
}

impl ClientOverrides {
    fn path_for(&self, kind: ClientKind) -> Option<&str> {
        match kind {
            ClientKind::Ssh => self.ssh.as_deref(),
            ClientKind::Scp => self.scp.as_deref(),
            ClientKind::Sftp => self.sftp.as_deref(),
            ClientKind::Ftp => self.ftp.as_deref(),
            ClientKind::Telnet => self.telnet.as_deref(),
        }
    }
}

/// Check for required external clients (ssh/scp/sftp/ftp/telnet) in PATH.
pub fn check_clients() -> DoctorReport {
    check_clients_with_overrides(None, None)
}

/// Check for required external clients, honoring profile/global overrides when provided.
pub fn check_clients_with_overrides(
    profile_overrides: Option<&ClientOverrides>,
    global_overrides: Option<&ClientOverrides>,
) -> DoctorReport {
    let mut clients = Vec::new();
    for kind in [
        ClientKind::Ssh,
        ClientKind::Scp,
        ClientKind::Sftp,
        ClientKind::Ftp,
        ClientKind::Telnet,
    ] {
        let resolved =
            resolve_client_with_source(kind, profile_overrides, global_overrides);
        clients.push(ClientStatus {
            name: kind.as_str().to_string(),
            path: resolved.path,
            source: resolved.source,
        });
    }
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let agent_status = agent::status();
    if agent_status.auth_sock.is_none() {
        warnings.push(DoctorMessage {
            code: "ssh_agent_missing".to_string(),
            message: "SSH_AUTH_SOCK is not set; ssh-agent may be unavailable.".to_string(),
        });
    }
    if agent_status.auth_sock.is_some() {
        if let Some(error) = &agent_status.error {
            warnings.push(DoctorMessage {
                code: "ssh_agent_list_failed".to_string(),
                message: format!("ssh-agent keys could not be listed: {error}"),
            });
        }
    }
    let base_dirs = BaseDirs::new();
    let home = base_dirs.as_ref().map(|dirs| dirs.home_dir());
    for path in ssh_config_paths(home) {
        scan_ssh_config(&path, home, &mut warnings, &mut errors);
    }
    DoctorReport {
        clients,
        agent: agent_status,
        warnings,
        errors,
    }
}

/// Resolve the first matching client executable from PATH using common extensions.
pub fn resolve_client(candidates: &[&str]) -> Option<PathBuf> {
    let path_env = env::var_os("PATH")?;
    find_in_path(&path_env, candidates)
}

/// Resolve a client honoring profile overrides, then global overrides, then PATH.
pub fn resolve_client_with_overrides(
    kind: ClientKind,
    profile_overrides: Option<&ClientOverrides>,
    global_overrides: Option<&ClientOverrides>,
) -> Option<PathBuf> {
    resolve_client_with_source(kind, profile_overrides, global_overrides).path
}

#[derive(Debug, Clone)]
pub struct ResolvedClient {
    pub path: Option<PathBuf>,
    pub source: ClientSource,
}

fn resolve_client_with_source(
    kind: ClientKind,
    profile_overrides: Option<&ClientOverrides>,
    global_overrides: Option<&ClientOverrides>,
) -> ResolvedClient {
    if let Some(path) = profile_overrides
        .and_then(|ovr| ovr.path_for(kind))
        .and_then(valid_override)
    {
        return ResolvedClient {
            path: Some(path),
            source: ClientSource::ProfileOverride,
        };
    }
    if let Some(path) = global_overrides
        .and_then(|ovr| ovr.path_for(kind))
        .and_then(valid_override)
    {
        return ResolvedClient {
            path: Some(path),
            source: ClientSource::GlobalOverride,
        };
    }
    let path = resolve_client(kind.candidates());
    if let Some(p) = path {
        ResolvedClient {
            path: Some(p),
            source: ClientSource::Path,
        }
    } else {
        ResolvedClient {
            path: None,
            source: ClientSource::Missing,
        }
    }
}

fn find_in_path(path_env: &OsStr, candidates: &[&str]) -> Option<PathBuf> {
    let exts = pathext();
    for dir in env::split_paths(path_env) {
        for candidate in candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
            if path.extension().is_none() {
                for ext in &exts {
                    let with_ext = dir.join(format!("{candidate}{ext}"));
                    if with_ext.is_file() {
                        return Some(with_ext);
                    }
                }
            }
        }
    }
    None
}

fn valid_override(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    if p.is_file() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

fn ssh_config_paths(home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home {
        paths.push(home.join(".ssh").join("config"));
    }
    if cfg!(unix) {
        paths.push(PathBuf::from("/etc/ssh/ssh_config"));
    } else if cfg!(windows) {
        if let Some(program_data) = env::var_os("PROGRAMDATA") {
            paths.push(PathBuf::from(program_data).join("ssh").join("ssh_config"));
        }
    }
    paths
}

fn scan_ssh_config(
    path: &Path,
    home: Option<&Path>,
    warnings: &mut Vec<DoctorMessage>,
    errors: &mut Vec<DoctorMessage>,
) {
    if !path.is_file() {
        return;
    }
    let Ok(contents) = std::fs::read_to_string(path) else {
        warnings.push(DoctorMessage {
            code: "ssh_config_unreadable".to_string(),
            message: format!(
                "SSH config at {} could not be read; skipping checks.",
                path.display()
            ),
        });
        return;
    };
    for raw in contents.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let key_lower = key.to_ascii_lowercase();
        let Some(value) = parts.next() else {
            continue;
        };
        if key_lower == "stricthostkeychecking" {
            let value_lower = value.to_ascii_lowercase();
            if matches!(value_lower.as_str(), "no" | "off" | "accept-new") {
                warnings.push(DoctorMessage {
                    code: "ssh_strict_host_key_unsafe".to_string(),
                    message: format!(
                        "SSH config {} sets StrictHostKeyChecking={} (unsafe).",
                        path.display(),
                        value
                    ),
                });
            }
        } else if key_lower == "userknownhostsfile" {
            let value_lower = value.to_ascii_lowercase();
            if matches!(value_lower.as_str(), "/dev/null" | "none" | "nul") {
                warnings.push(DoctorMessage {
                    code: "ssh_known_hosts_disabled".to_string(),
                    message: format!(
                        "SSH config {} disables known_hosts with UserKnownHostsFile={}.",
                        path.display(),
                        value
                    ),
                });
            }
        } else if key_lower == "identityfile" {
            if let Some(identity_path) = normalize_identity_path(value, home) {
                if !identity_path.is_file() {
                    errors.push(DoctorMessage {
                        code: "ssh_identity_missing".to_string(),
                        message: format!(
                            "SSH config {} references missing IdentityFile {}.",
                            path.display(),
                            identity_path.display()
                        ),
                    });
                }
            }
        }
    }
}

fn normalize_identity_path(raw: &str, home: Option<&Path>) -> Option<PathBuf> {
    let trimmed = raw.trim_matches(|c| c == '"' || c == '\'');
    if trimmed.contains('%') {
        return None;
    }
    if trimmed.starts_with("~/") {
        return home.map(|home| home.join(trimmed.trim_start_matches("~/")));
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        return Some(path);
    }
    home.map(|home| home.join(path))
}

fn pathext() -> Vec<String> {
    if cfg!(windows) {
        if let Some(raw) = env::var_os("PATHEXT") {
            let list = raw
                .to_string_lossy()
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let trimmed = s.trim();
                    if trimmed.starts_with('.') {
                        trimmed.to_string()
                    } else {
                        format!(".{trimmed}")
                    }
                })
                .collect::<Vec<_>>();
            if !list.is_empty() {
                return list;
            }
        }
        vec![
            ".exe".to_string(),
            ".bat".to_string(),
            ".cmd".to_string(),
            ".com".to_string(),
        ]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    #[test]
    fn finds_client_in_custom_path() {
        let temp = env::temp_dir().join("teradock-doctor-test");
        let _ = fs::create_dir_all(&temp);
        let binary = if cfg!(windows) {
            temp.join("ssh-test.exe")
        } else {
            temp.join("ssh-test")
        };
        File::create(&binary).expect("create fake binary");
        let path_env = env::join_paths([&temp]).expect("join path");
        let found = find_in_path(&path_env, &["ssh-test"]).expect("should find client");
        assert_eq!(found, binary);
        let _ = fs::remove_file(&binary);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn pathext_has_defaults_on_windows() {
        if cfg!(windows) {
            let extensions = pathext();
            assert!(!extensions.is_empty());
            assert!(extensions.iter().any(|e| e.eq_ignore_ascii_case(".exe")));
        } else {
            assert!(pathext().is_empty());
        }
    }

    #[test]
    fn override_precedence() {
        let temp = env::temp_dir().join("teradock-doctor-override");
        let _ = fs::create_dir_all(&temp);
        let override_path = temp.join(if cfg!(windows) { "ssh-custom.exe" } else { "ssh-custom" });
        File::create(&override_path).expect("create override binary");

        let profile_overrides = ClientOverrides {
            ssh: Some(override_path.to_string_lossy().to_string()),
            ..Default::default()
        };

        let resolved =
            resolve_client_with_source(ClientKind::Ssh, Some(&profile_overrides), None);

        assert_eq!(resolved.path, Some(override_path.clone()));
        assert_eq!(resolved.source, ClientSource::ProfileOverride);

        let _ = fs::remove_file(&override_path);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn global_override_is_used_and_reported() {
        let temp = env::temp_dir().join("teradock-doctor-global");
        let _ = fs::create_dir_all(&temp);
        let override_path = temp.join(if cfg!(windows) { "ssh-global.exe" } else { "ssh-global" });
        File::create(&override_path).expect("create override binary");

        let global = ClientOverrides {
            ssh: Some(override_path.to_string_lossy().to_string()),
            ..Default::default()
        };
        let report = check_clients_with_overrides(None, Some(&global));
        let ssh = report
            .clients
            .into_iter()
            .find(|c| c.name == "ssh")
            .expect("ssh present");
        assert_eq!(ssh.path, Some(override_path.clone()));
        assert_eq!(ssh.source, ClientSource::GlobalOverride);

        let _ = fs::remove_file(&override_path);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn empty_path_reports_missing() {
        let orig = env::var_os("PATH");
        env::set_var("PATH", "");
        let report = check_clients_with_overrides(None, None);
        assert!(report
            .clients
            .iter()
            .all(|c| c.source == ClientSource::Missing));
        if let Some(old) = orig {
            env::set_var("PATH", old);
        } else {
            env::remove_var("PATH");
        }
    }
}
