use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;

use directories::BaseDirs;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::doctor::{self, ClientKind, ClientOverrides};
use crate::profile::{DangerLevel, Profile, ProfileStore, ProfileType};
use crate::settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshAuthMethod {
    Agent,
    Keys,
    Password,
}

impl SshAuthMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Keys => "keys",
            Self::Password => "password",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshInvocationMode {
    Interactive,
    Exec,
    CommandSet,
}

impl SshInvocationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Exec => "exec",
            Self::CommandSet => "commandset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub profile_id: String,
    pub name: String,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub danger_level: DangerLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshInvocation {
    pub client_path: PathBuf,
    pub args: Vec<OsString>,
    pub target: SshTarget,
    pub auth_context: SshAuthContext,
    pub safe_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy)]
pub struct SshInvocationRequest<'a> {
    pub profile_id: &'a str,
    pub source: &'a str,
    pub mode: SshInvocationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAuthAvailability {
    pub agent: bool,
    pub keys: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAuthContext {
    pub order: Vec<SshAuthMethod>,
    pub args: Vec<OsString>,
    pub hint: Option<String>,
    pub warn_password_fallback: bool,
}

#[derive(Debug, Error)]
pub enum SshBuildError {
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("profile '{profile_id}' is {profile_type}; SSH requires an SSH profile")]
    UnsupportedProfileType {
        profile_id: String,
        profile_type: ProfileType,
    },
    #[error("{kind} client not found via overrides or PATH")]
    ClientNotFound { kind: &'static str },
    #[error("invalid ssh auth order: {0}")]
    InvalidAuthOrder(String),
    #[error("settings error: {0}")]
    SettingsError(String),
}

pub type SshBuildResult<T> = std::result::Result<T, SshBuildError>;

pub fn build_ssh_invocation(
    store: &ProfileStore,
    request: SshInvocationRequest<'_>,
) -> SshBuildResult<SshInvocation> {
    let profile = store
        .get(request.profile_id)
        .map_err(|err| SshBuildError::SettingsError(err.to_string()))?
        .ok_or_else(|| SshBuildError::ProfileNotFound(request.profile_id.to_string()))?;
    let target = ssh_target_from_profile(&profile)?;
    let client_path = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        store.conn(),
    )?;
    let auth = ssh_auth_context(store.conn())?;
    let args = build_ssh_args(&target, &auth.args);
    let safe_metadata = safe_ssh_metadata(&target, request.source, request.mode, None);

    Ok(SshInvocation {
        client_path,
        args,
        target,
        auth_context: auth,
        safe_metadata,
    })
}

pub fn ssh_target_from_profile(profile: &Profile) -> SshBuildResult<SshTarget> {
    if profile.profile_type != ProfileType::Ssh {
        return Err(SshBuildError::UnsupportedProfileType {
            profile_id: profile.profile_id.clone(),
            profile_type: profile.profile_type,
        });
    }
    Ok(SshTarget {
        profile_id: profile.profile_id.clone(),
        name: profile.name.clone(),
        user: profile.user.clone(),
        host: profile.host.clone(),
        port: profile.port,
        danger_level: profile.danger_level,
    })
}

pub fn resolve_client_for(
    kind: ClientKind,
    profile_overrides: Option<&ClientOverrides>,
    conn: &Connection,
) -> SshBuildResult<PathBuf> {
    let global_overrides = settings::get_client_overrides(conn)
        .map_err(|err| SshBuildError::SettingsError(err.to_string()))?;
    doctor::resolve_client_with_overrides(kind, profile_overrides, global_overrides.as_ref())
        .ok_or_else(|| SshBuildError::ClientNotFound {
            kind: kind.as_str(),
        })
}

pub fn build_ssh_args(target: &SshTarget, auth_args: &[OsString]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-p"),
        OsString::from(target.port.to_string()),
    ];
    args.extend(auth_args.iter().cloned());
    args.push(OsString::from(format!("{}@{}", target.user, target.host)));
    args
}

pub fn safe_ssh_metadata(
    target: &SshTarget,
    source: &str,
    mode: SshInvocationMode,
    launch_error: Option<&str>,
) -> serde_json::Value {
    let mut meta = serde_json::json!({
        "mode": mode.as_str(),
        "source": source,
        "host": target.host.as_str(),
        "port": target.port,
        "user": target.user.as_str(),
        "profile_type": ProfileType::Ssh.to_string(),
    });
    if let Some(error) = launch_error {
        meta["launch_error"] = serde_json::Value::String(error.to_string());
    }
    meta
}

pub fn format_ssh_invocation(
    client_path: &std::path::Path,
    port: u16,
    auth_args: &[OsString],
) -> String {
    let mut parts = vec![
        client_path.to_string_lossy().to_string(),
        "-p".to_string(),
        port.to_string(),
    ];
    parts.extend(
        auth_args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string()),
    );
    parts.join(" ")
}

pub fn normalize_auth_order(order: Vec<SshAuthMethod>) -> SshBuildResult<Vec<SshAuthMethod>> {
    if order.is_empty() {
        return Err(SshBuildError::InvalidAuthOrder(
            "auth order cannot be empty".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for method in order {
        if !seen.insert(method) {
            return Err(SshBuildError::InvalidAuthOrder(format!(
                "auth order contains duplicate '{}'",
                method.as_str()
            )));
        }
        normalized.push(method);
    }
    Ok(normalized)
}

pub fn parse_auth_order_setting(raw: &str) -> SshBuildResult<Vec<SshAuthMethod>> {
    if raw.trim().is_empty() {
        return Err(SshBuildError::InvalidAuthOrder(
            "auth order setting is empty".to_string(),
        ));
    }
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for item in raw.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let method = match trimmed {
            "agent" => SshAuthMethod::Agent,
            "keys" => SshAuthMethod::Keys,
            "password" => SshAuthMethod::Password,
            _ => {
                return Err(SshBuildError::InvalidAuthOrder(format!(
                    "unknown auth method '{trimmed}'"
                )))
            }
        };
        if !seen.insert(method) {
            return Err(SshBuildError::InvalidAuthOrder(format!(
                "auth order contains duplicate '{trimmed}'"
            )));
        }
        order.push(method);
    }
    normalize_auth_order(order)
}

pub fn format_auth_order(order: &[SshAuthMethod]) -> String {
    order
        .iter()
        .map(SshAuthMethod::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn default_auth_order() -> Vec<SshAuthMethod> {
    vec![
        SshAuthMethod::Agent,
        SshAuthMethod::Keys,
        SshAuthMethod::Password,
    ]
}

pub fn load_ssh_auth_order(conn: &Connection) -> SshBuildResult<Vec<SshAuthMethod>> {
    match settings::get_ssh_auth_order(conn)
        .map_err(|err| SshBuildError::SettingsError(err.to_string()))?
    {
        Some(raw) => parse_auth_order_setting(&raw),
        None => Ok(default_auth_order()),
    }
}

pub fn detect_ssh_auth_availability() -> SshAuthAvailability {
    let agent = std::env::var_os("SSH_AUTH_SOCK")
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    let keys = if let Some(dirs) = BaseDirs::new() {
        let ssh_dir = dirs.home_dir().join(".ssh");
        [
            "id_ed25519",
            "id_rsa",
            "id_ecdsa",
            "id_ed25519_sk",
            "id_ecdsa_sk",
            "id_dsa",
            "identity",
        ]
        .iter()
        .any(|name| ssh_dir.join(name).exists())
    } else {
        false
    };
    SshAuthAvailability { agent, keys }
}

pub fn build_ssh_auth_args(
    order: &[SshAuthMethod],
    availability: &SshAuthAvailability,
) -> Vec<OsString> {
    let mut preferred = Vec::new();
    let mut publickey_added = false;
    for method in order {
        match method {
            SshAuthMethod::Agent | SshAuthMethod::Keys => {
                if !publickey_added {
                    preferred.push("publickey");
                    publickey_added = true;
                }
            }
            SshAuthMethod::Password => {
                preferred.push("keyboard-interactive");
                preferred.push("password");
            }
        }
    }
    let mut args = Vec::new();
    if !preferred.is_empty() {
        args.push(OsString::from("-o"));
        args.push(OsString::from(format!(
            "PreferredAuthentications={}",
            preferred.join(",")
        )));
    }
    if !availability.agent || !order.contains(&SshAuthMethod::Agent) {
        args.push(OsString::from("-o"));
        args.push(OsString::from("IdentityAgent=none"));
    }
    args
}

pub fn ssh_auth_context(conn: &Connection) -> SshBuildResult<SshAuthContext> {
    let order = load_ssh_auth_order(conn)?;
    let availability = detect_ssh_auth_availability();
    let args = build_ssh_auth_args(&order, &availability);
    let hint = match order.first().copied() {
        Some(SshAuthMethod::Agent) if !availability.agent => Some(
            "Hint: SSH auth order prefers agent; start ssh-agent or set SSH_AUTH_SOCK to avoid password prompts.".to_string(),
        ),
        Some(SshAuthMethod::Keys) if !availability.keys => Some(
            "Hint: SSH auth order prefers keys; add a key under ~/.ssh or update the auth order.".to_string(),
        ),
        _ => None,
    };
    let first_available = order
        .iter()
        .copied()
        .find(|method| is_auth_method_available(*method, &availability));
    let warn_password_fallback = matches!(first_available, Some(SshAuthMethod::Password))
        && order.first().copied() != Some(SshAuthMethod::Password);
    Ok(SshAuthContext {
        order,
        args,
        hint,
        warn_password_fallback,
    })
}

fn is_auth_method_available(method: SshAuthMethod, availability: &SshAuthAvailability) -> bool {
    match method {
        SshAuthMethod::Agent => availability.agent,
        SshAuthMethod::Keys => availability.keys,
        SshAuthMethod::Password => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    use crate::db;
    use crate::profile::NewProfile;

    fn fake_ssh_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "teradock-core-fake-ssh-{name}-{}{}",
            std::process::id(),
            if cfg!(windows) { ".cmd" } else { "" }
        ));
        fs::write(&path, "fake ssh").unwrap();
        path
    }

    fn insert_profile(
        store: &ProfileStore,
        profile_id: &str,
        profile_type: ProfileType,
        ssh_path: Option<&std::path::Path>,
    ) {
        store
            .insert(NewProfile {
                profile_id: Some(profile_id.to_string()),
                name: "Test Profile".to_string(),
                profile_type,
                host: "example.com".to_string(),
                port: 2222,
                user: "alice".to_string(),
                danger_level: DangerLevel::Normal,
                group: None,
                tags: Vec::new(),
                note: None,
                initial_send: None,
                client_overrides: ssh_path.map(|path| ClientOverrides {
                    ssh: Some(path.to_string_lossy().into_owned()),
                    ..Default::default()
                }),
            })
            .unwrap();
    }

    #[test]
    fn builds_ssh_invocation_from_profile() {
        let fake_ssh = fake_ssh_path("invocation");
        let store = ProfileStore::new(db::init_in_memory().unwrap());
        insert_profile(&store, "p_test", ProfileType::Ssh, Some(&fake_ssh));

        let invocation = build_ssh_invocation(
            &store,
            SshInvocationRequest {
                profile_id: "p_test",
                source: "tui",
                mode: SshInvocationMode::Interactive,
            },
        )
        .unwrap();

        assert_eq!(invocation.client_path, fake_ssh);
        assert_eq!(invocation.target.profile_id, "p_test");
        assert_eq!(invocation.args[0], OsStr::new("-p"));
        assert_eq!(invocation.args[1], OsStr::new("2222"));
        assert_eq!(
            invocation.args.last().unwrap(),
            OsStr::new("alice@example.com")
        );
        assert_eq!(invocation.safe_metadata["source"], "tui");
        assert_eq!(invocation.safe_metadata["mode"], "interactive");

        let _ = fs::remove_file(invocation.client_path);
    }

    #[test]
    fn rejects_non_ssh_profile() {
        let fake_ssh = fake_ssh_path("non-ssh");
        let store = ProfileStore::new(db::init_in_memory().unwrap());
        insert_profile(&store, "p_telnet", ProfileType::Telnet, Some(&fake_ssh));

        let err = build_ssh_invocation(
            &store,
            SshInvocationRequest {
                profile_id: "p_telnet",
                source: "cli",
                mode: SshInvocationMode::Interactive,
            },
        )
        .unwrap_err();

        assert!(matches!(err, SshBuildError::UnsupportedProfileType { .. }));
        let _ = fs::remove_file(fake_ssh);
    }

    #[test]
    fn rejects_missing_profile() {
        let store = ProfileStore::new(db::init_in_memory().unwrap());

        let err = build_ssh_invocation(
            &store,
            SshInvocationRequest {
                profile_id: "missing",
                source: "cli",
                mode: SshInvocationMode::Interactive,
            },
        )
        .unwrap_err();

        assert!(matches!(err, SshBuildError::ProfileNotFound(_)));
    }

    #[test]
    fn safe_metadata_excludes_secret_material() {
        let target = SshTarget {
            profile_id: "p_test".to_string(),
            name: "Test Profile".to_string(),
            user: "alice".to_string(),
            host: "example.com".to_string(),
            port: 22,
            danger_level: DangerLevel::Normal,
        };

        let cli = safe_ssh_metadata(&target, "cli", SshInvocationMode::Interactive, None);
        let tui = safe_ssh_metadata(
            &target,
            "tui",
            SshInvocationMode::Interactive,
            Some("launch failed"),
        );

        assert_eq!(cli["source"], "cli");
        assert_eq!(tui["source"], "tui");
        assert_eq!(tui["launch_error"], "launch failed");
        for meta in [cli, tui] {
            assert!(meta.get("password").is_none());
            assert!(meta.get("secret").is_none());
            assert!(meta.get("token").is_none());
            assert!(meta.get("auth_args").is_none());
            assert!(meta.get("command").is_none());
            assert!(meta.get("private_key_path").is_none());
        }
    }

    #[test]
    fn auth_args_are_built_from_order_without_exposing_paths() {
        let args = build_ssh_auth_args(
            &[SshAuthMethod::Agent, SshAuthMethod::Password],
            &SshAuthAvailability {
                agent: false,
                keys: false,
            },
        );

        assert_eq!(args[0], OsStr::new("-o"));
        assert_eq!(
            args[1],
            OsStr::new("PreferredAuthentications=publickey,keyboard-interactive,password")
        );
        assert_eq!(args[2], OsStr::new("-o"));
        assert_eq!(args[3], OsStr::new("IdentityAgent=none"));
    }

    #[test]
    fn invalid_auth_order_is_rejected() {
        let err = parse_auth_order_setting("agent,agent").unwrap_err();
        assert!(matches!(err, SshBuildError::InvalidAuthOrder(_)));
    }
}
