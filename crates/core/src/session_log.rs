use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use common::id::{generate_id, validate_id};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::doctor;
use crate::error::{CoreError, Result};
use crate::paths;
use crate::settings::{self, SettingScope};
use crate::ssh::SshTarget;
use crate::util::now_ms;

pub const SESSION_LOG_ENABLED_KEY: &str = "session.log.enabled";
pub const SESSION_LOG_DIR_KEY: &str = "session.log.dir";
pub const SESSION_LOG_BACKEND_KEY: &str = "session.log.backend";

pub const SESSION_LOG_BACKEND_AUTO: &str = "auto";
pub const SESSION_LOG_BACKEND_SCRIPT: &str = "script";
pub const SESSION_LOG_BACKEND_NO_LOG: &str = "no-log";

pub const SESSION_LOG_REASON_DISABLED: &str = "disabled";
pub const SESSION_LOG_REASON_BACKEND_NO_LOG: &str = "backend_no_log";
pub const SESSION_LOG_REASON_SCRIPT_UNAVAILABLE: &str = "script_unavailable";
pub const SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS: &str = "unsupported_on_windows";
pub const SESSION_LOG_REASON_SETUP_FAILED: &str = "setup_failed";
pub const SESSION_LOG_REASON_SCRIPT_LAUNCH_FAILED: &str = "script_launch_failed";
pub const SESSION_LOG_REASON_METADATA_WRITE_FAILED: &str = "metadata_write_failed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLogBackendSetting {
    Auto,
    Script,
    NoLog,
}

impl SessionLogBackendSetting {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            SESSION_LOG_BACKEND_AUTO => Ok(Self::Auto),
            SESSION_LOG_BACKEND_SCRIPT => Ok(Self::Script),
            SESSION_LOG_BACKEND_NO_LOG => Ok(Self::NoLog),
            other => Err(CoreError::InvalidSetting(format!(
                "unknown session log backend '{other}'"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => SESSION_LOG_BACKEND_AUTO,
            Self::Script => SESSION_LOG_BACKEND_SCRIPT,
            Self::NoLog => SESSION_LOG_BACKEND_NO_LOG,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLogConfig {
    pub enabled: bool,
    pub dir: PathBuf,
    pub backend: SessionLogBackendSetting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLogFiles {
    pub session_id: String,
    pub log_path: PathBuf,
    pub metadata_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLogPlan {
    Disabled,
    NoLog {
        reason: String,
    },
    Script {
        script_path: PathBuf,
        files: SessionLogFiles,
    },
}

impl SessionLogPlan {
    pub fn not_saved_reference(&self) -> SessionLogReference {
        match self {
            Self::Disabled => SessionLogReference::not_saved(SESSION_LOG_REASON_DISABLED),
            Self::NoLog { reason } => SessionLogReference::not_saved(reason),
            Self::Script { .. } => SessionLogReference::not_saved("not_completed"),
        }
    }

    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Disabled => None,
            Self::NoLog { reason } => Some(format!(
                "TeraDock session logging requested but no terminal log will be saved ({reason}); continuing with normal SSH."
            )),
            Self::Script { files, .. } => Some(format!(
                "TeraDock session logging is enabled; terminal output may include secrets and will be saved to {}.",
                files.log_path.display()
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionLogMetadata {
    pub session_id: String,
    pub profile_id: String,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub exit_code: Option<i32>,
    pub backend: String,
    pub log_path: Option<PathBuf>,
    pub metadata_path: PathBuf,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionLogReference {
    pub saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SessionLogReference {
    pub fn saved(session_id: impl Into<String>) -> Self {
        Self {
            saved: true,
            session_id: Some(session_id.into()),
            reason: None,
        }
    }

    pub fn not_saved(reason: impl Into<String>) -> Self {
        Self {
            saved: false,
            session_id: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCommandInvocation {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
}

pub fn resolve_config(conn: &Connection, profile_id: &str) -> Result<SessionLogConfig> {
    let profile_scope = SettingScope::profile(profile_id);
    let enabled = settings::get_setting_resolved(conn, &profile_scope, SESSION_LOG_ENABLED_KEY)?
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let backend_raw =
        settings::get_setting_resolved(conn, &profile_scope, SESSION_LOG_BACKEND_KEY)?
            .unwrap_or_else(|| SESSION_LOG_BACKEND_AUTO.to_string());
    let backend = SessionLogBackendSetting::parse(&backend_raw)?;
    let dir = configured_session_log_dir(conn)?;

    Ok(SessionLogConfig {
        enabled,
        dir,
        backend,
    })
}

pub fn configured_session_log_dir(conn: &Connection) -> Result<PathBuf> {
    match settings::get_setting(conn, SESSION_LOG_DIR_KEY)? {
        Some(raw) => Ok(PathBuf::from(raw)),
        None => {
            let mut dir = paths::config_dir()?;
            dir.push("session-logs");
            Ok(dir)
        }
    }
}

pub fn plan_for_target(conn: &Connection, target: &SshTarget) -> SessionLogPlan {
    let config = match resolve_config(conn, &target.profile_id) {
        Ok(config) => config,
        Err(err) => {
            return SessionLogPlan::NoLog {
                reason: format!("{SESSION_LOG_REASON_SETUP_FAILED}: {err}"),
            }
        }
    };
    plan_from_config(&config)
}

pub fn plan_from_config(config: &SessionLogConfig) -> SessionLogPlan {
    if !config.enabled {
        return SessionLogPlan::Disabled;
    }
    if config.backend == SessionLogBackendSetting::NoLog {
        return SessionLogPlan::NoLog {
            reason: SESSION_LOG_REASON_BACKEND_NO_LOG.to_string(),
        };
    }
    if cfg!(windows) {
        return SessionLogPlan::NoLog {
            reason: SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS.to_string(),
        };
    }
    let Some(script_path) = doctor::resolve_client(&["script"]) else {
        return SessionLogPlan::NoLog {
            reason: SESSION_LOG_REASON_SCRIPT_UNAVAILABLE.to_string(),
        };
    };
    if let Err(err) = ensure_session_log_dir(&config.dir) {
        return SessionLogPlan::NoLog {
            reason: format!("{SESSION_LOG_REASON_SETUP_FAILED}: {err}"),
        };
    }
    match allocate_session_files(&config.dir) {
        Ok(files) => SessionLogPlan::Script { script_path, files },
        Err(err) => SessionLogPlan::NoLog {
            reason: format!("{SESSION_LOG_REASON_SETUP_FAILED}: {err}"),
        },
    }
}

pub fn build_script_invocation(
    script_path: &Path,
    files: &SessionLogFiles,
    ssh_executable: &Path,
    ssh_args: &[OsString],
) -> ExternalCommandInvocation {
    let mut args = Vec::new();
    if cfg!(target_os = "macos") {
        args.push(OsString::from("-q"));
        args.push(files.log_path.as_os_str().to_os_string());
        args.push(ssh_executable.as_os_str().to_os_string());
        args.extend(ssh_args.iter().cloned());
    } else {
        args.push(OsString::from("-q"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("-e"));
        args.push(OsString::from("-c"));
        args.push(OsString::from(posix_shell_command(
            ssh_executable,
            ssh_args,
        )));
        args.push(files.log_path.as_os_str().to_os_string());
    }
    ExternalCommandInvocation {
        executable: script_path.to_path_buf(),
        args,
    }
}

pub fn complete_script_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
) -> Result<SessionLogMetadata> {
    if !files.log_path.is_file() {
        return Err(CoreError::NotFound(format!(
            "session log not found: {}",
            files.log_path.display()
        )));
    }
    set_user_only_file_permissions(&files.log_path)?;
    let metadata = SessionLogMetadata {
        session_id: files.session_id.clone(),
        profile_id: target.profile_id.clone(),
        user: target.user.clone(),
        host: target.host.clone(),
        port: target.port,
        started_at,
        ended_at: now_ms(),
        duration_ms,
        exit_code,
        backend: SESSION_LOG_BACKEND_SCRIPT.to_string(),
        log_path: Some(files.log_path.clone()),
        metadata_path: files.metadata_path.clone(),
        status: status_for_exit(exit_code),
        reason: None,
    };
    write_metadata(&metadata)?;
    Ok(metadata)
}

pub fn list_session_logs(conn: &Connection) -> Result<Vec<SessionLogMetadata>> {
    let dir = configured_session_log_dir(conn)?;
    list_session_logs_in_dir(&dir)
}

pub fn get_session_log(conn: &Connection, session_id: &str) -> Result<SessionLogMetadata> {
    let dir = configured_session_log_dir(conn)?;
    get_session_log_in_dir(&dir, session_id)
}

pub fn list_session_logs_in_dir(dir: &Path) -> Result<Vec<SessionLogMetadata>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(metadata) = serde_json::from_str::<SessionLogMetadata>(&raw) {
            items.push(metadata);
        }
    }
    items.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| right.session_id.cmp(&left.session_id))
    });
    Ok(items)
}

pub fn get_session_log_in_dir(dir: &Path, session_id: &str) -> Result<SessionLogMetadata> {
    validate_id(session_id).map_err(CoreError::InvalidId)?;
    let metadata_path = dir.join(format!("{session_id}.json"));
    if !metadata_path.is_file() {
        return Err(CoreError::NotFound(format!("session log: {session_id}")));
    }
    let raw = fs::read_to_string(metadata_path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn add_reference_to_meta(meta: &mut Value, reference: &SessionLogReference) {
    if !meta.is_object() {
        *meta = serde_json::json!({});
    }
    let map = meta.as_object_mut().expect("meta should be an object");
    map.insert(
        "session_log_saved".to_string(),
        Value::Bool(reference.saved),
    );
    if let Some(session_id) = &reference.session_id {
        map.insert(
            "session_log_id".to_string(),
            Value::String(session_id.clone()),
        );
    }
    if let Some(reason) = &reference.reason {
        map.insert(
            "session_log_reason".to_string(),
            Value::String(reason.clone()),
        );
    }
}

fn ensure_session_log_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    set_user_only_dir_permissions(dir)?;
    Ok(())
}

fn allocate_session_files(dir: &Path) -> Result<SessionLogFiles> {
    for _ in 0..10 {
        let session_id = generate_id("sl_");
        let log_path = dir.join(format!("{session_id}.log"));
        let metadata_path = dir.join(format!("{session_id}.json"));
        if !log_path.exists() && !metadata_path.exists() {
            return Ok(SessionLogFiles {
                session_id,
                log_path,
                metadata_path,
            });
        }
    }
    Err(CoreError::Conflict(
        "failed to allocate a unique session log id".to_string(),
    ))
}

fn write_metadata(metadata: &SessionLogMetadata) -> Result<()> {
    let raw = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata.metadata_path, raw)?;
    set_user_only_file_permissions(&metadata.metadata_path)?;
    Ok(())
}

fn status_for_exit(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => "completed".to_string(),
        Some(_) => "completed_nonzero".to_string(),
        None => "completed_without_exit_code".to_string(),
    }
}

fn posix_shell_command(executable: &Path, args: &[OsString]) -> String {
    let mut parts = vec![posix_shell_quote(executable.as_os_str())];
    parts.extend(args.iter().map(|arg| posix_shell_quote(arg.as_os_str())));
    parts.join(" ")
}

fn posix_shell_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.is_empty() {
        return "''".to_string();
    }
    if text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "_@%+=:,./-".contains(ch))
    {
        return text.into_owned();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn set_user_only_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_user_only_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_user_only_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_user_only_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn sample_target() -> SshTarget {
        SshTarget {
            profile_id: "p_test".to_string(),
            name: "Test".to_string(),
            user: "alice".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            danger_level: crate::profile::DangerLevel::Normal,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "teradock-session-log-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolves_disabled_config_by_default() {
        let conn = db::init_in_memory().unwrap();
        let config = resolve_config(&conn, "p_test").unwrap();

        assert!(!config.enabled);
        assert_eq!(config.backend, SessionLogBackendSetting::Auto);
    }

    #[test]
    fn no_log_backend_plans_without_external_script() {
        let conn = db::init_in_memory().unwrap();
        settings::set_setting_scoped(
            &conn,
            &SettingScope::profile("p_test"),
            SESSION_LOG_ENABLED_KEY,
            "true",
        )
        .unwrap();
        settings::set_setting_scoped(
            &conn,
            &SettingScope::profile("p_test"),
            SESSION_LOG_BACKEND_KEY,
            SESSION_LOG_BACKEND_NO_LOG,
        )
        .unwrap();

        let plan = plan_for_target(&conn, &sample_target());

        assert_eq!(
            plan,
            SessionLogPlan::NoLog {
                reason: SESSION_LOG_REASON_BACKEND_NO_LOG.to_string()
            }
        );
        assert_eq!(
            plan.not_saved_reference(),
            SessionLogReference::not_saved(SESSION_LOG_REASON_BACKEND_NO_LOG)
        );
    }

    #[test]
    fn script_invocation_uses_log_path_and_ssh_command() {
        let files = SessionLogFiles {
            session_id: "sl_test".to_string(),
            log_path: PathBuf::from("/tmp/sl_test.log"),
            metadata_path: PathBuf::from("/tmp/sl_test.json"),
        };
        let invocation = build_script_invocation(
            Path::new("/usr/bin/script"),
            &files,
            Path::new("/usr/bin/ssh"),
            &[
                OsString::from("-p"),
                OsString::from("2222"),
                OsString::from("alice@example.com"),
            ],
        );

        assert_eq!(invocation.executable, PathBuf::from("/usr/bin/script"));
        assert!(invocation.args.iter().any(|arg| arg == "-q"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == files.log_path.as_os_str()));
    }

    #[test]
    fn writes_and_lists_metadata_without_secret_fields() {
        let dir = temp_dir("metadata");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(&files.log_path, "terminal output\n").unwrap();
        let target = sample_target();

        let metadata = complete_script_session(&files, &target, 1000, 42, Some(0)).unwrap();
        let listed = list_session_logs_in_dir(&dir).unwrap();
        let loaded = get_session_log_in_dir(&dir, &files.session_id).unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.session_id, files.session_id);
        assert_eq!(listed.len(), 1);
        assert_eq!(loaded.session_id, files.session_id);
        assert_eq!(metadata.status, "completed");
        assert!(metadata.log_path.is_some());
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("private_key_path"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn adds_only_safe_oplog_reference_fields() {
        let reference = SessionLogReference::saved("sl_abc123");
        let mut meta = serde_json::json!({
            "mode": "interactive",
            "source": "tui",
        });

        add_reference_to_meta(&mut meta, &reference);

        assert_eq!(meta["session_log_saved"], true);
        assert_eq!(meta["session_log_id"], "sl_abc123");
        assert!(meta.get("log_path").is_none());
        assert!(meta.get("auth_args").is_none());
        assert!(meta.get("command").is_none());
    }
}
