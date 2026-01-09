use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Copy)]
pub enum ClientKind {
    Ssh,
    Scp,
    Sftp,
    Telnet,
}

impl ClientKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClientKind::Ssh => "ssh",
            ClientKind::Scp => "scp",
            ClientKind::Sftp => "sftp",
            ClientKind::Telnet => "telnet",
        }
    }

    fn candidates(&self) -> &'static [&'static str] {
        match self {
            ClientKind::Ssh => &["ssh", "ssh.exe"],
            ClientKind::Scp => &["scp", "scp.exe"],
            ClientKind::Sftp => &["sftp", "sftp.exe"],
            ClientKind::Telnet => &["telnet", "telnet.exe"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientOverrides {
    pub ssh: Option<String>,
    pub scp: Option<String>,
    pub sftp: Option<String>,
    pub telnet: Option<String>,
}

impl ClientOverrides {
    fn path_for(&self, kind: ClientKind) -> Option<&str> {
        match kind {
            ClientKind::Ssh => self.ssh.as_deref(),
            ClientKind::Scp => self.scp.as_deref(),
            ClientKind::Sftp => self.sftp.as_deref(),
            ClientKind::Telnet => self.telnet.as_deref(),
        }
    }
}

/// Check for required external clients (ssh/scp/sftp/telnet) in PATH.
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
    DoctorReport { clients }
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
