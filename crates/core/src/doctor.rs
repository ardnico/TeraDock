use serde::Serialize;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub name: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub clients: Vec<ClientStatus>,
}

/// Check for required external clients (ssh/scp/sftp/telnet) in PATH.
pub fn check_clients() -> DoctorReport {
    let mut clients = Vec::new();
    for (name, aliases) in [
        ("ssh", &["ssh", "ssh.exe"][..]),
        ("scp", &["scp", "scp.exe"][..]),
        ("sftp", &["sftp", "sftp.exe"][..]),
        ("telnet", &["telnet", "telnet.exe"][..]),
    ] {
        clients.push(ClientStatus {
            name: name.to_string(),
            path: resolve_client(aliases),
        });
    }
    DoctorReport { clients }
}

/// Resolve the first matching client executable from PATH using common extensions.
pub fn resolve_client(candidates: &[&str]) -> Option<PathBuf> {
    let path_env = env::var_os("PATH")?;
    find_in_path(&path_env, candidates)
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
}
