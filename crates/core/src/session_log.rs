use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
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
pub const SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT: &str = "powershell-transcript";
pub const SESSION_LOG_BACKEND_CONPTY: &str = "conpty";
pub const SESSION_LOG_BACKEND_NO_LOG: &str = "no-log";

pub const SESSION_LOG_REASON_DISABLED: &str = "disabled";
pub const SESSION_LOG_REASON_BACKEND_NO_LOG: &str = "backend_no_log";
pub const SESSION_LOG_REASON_SCRIPT_UNAVAILABLE: &str = "script_unavailable";
pub const SESSION_LOG_REASON_POWERSHELL_NOT_FOUND: &str = "powershell_not_found";
pub const SESSION_LOG_REASON_SSH_NOT_FOUND: &str = "ssh_not_found";
pub const SESSION_LOG_REASON_LOG_DIR_NOT_WRITABLE: &str = "log_dir_not_writable";
pub const SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS: &str = "unsupported_on_windows";
pub const SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM: &str = "unsupported_on_platform";
pub const SESSION_LOG_REASON_SETUP_FAILED: &str = "setup_failed";
pub const SESSION_LOG_REASON_SCRIPT_LAUNCH_FAILED: &str = "script_launch_failed";
pub const SESSION_LOG_REASON_POWERSHELL_LAUNCH_FAILED: &str = "powershell_launch_failed";
pub const SESSION_LOG_REASON_CONPTY_LAUNCH_FAILED: &str = "conpty_launch_failed";
pub const SESSION_LOG_REASON_METADATA_WRITE_FAILED: &str = "metadata_write_failed";
pub const SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY: &str =
    "windows_terminal_content_logging_requires_explicit_conpty";
pub const SESSION_LOG_STATUS_FAILED: &str = "failed";
pub const SESSION_LOG_STATUS_ABORTED: &str = "aborted";
pub const SESSION_LOG_FAILURE_PHASE_CREATE_LOG: &str = "create_log";
pub const SESSION_LOG_FAILURE_PHASE_OPEN_PTY: &str = "open_pty";
pub const SESSION_LOG_FAILURE_PHASE_ENTER_RAW_MODE: &str = "enter_raw_mode";
pub const SESSION_LOG_FAILURE_PHASE_SPAWN_CHILD: &str = "spawn_child";
pub const SESSION_LOG_FAILURE_PHASE_INPUT_BRIDGE: &str = "input_bridge";
pub const SESSION_LOG_FAILURE_PHASE_OUTPUT_BRIDGE: &str = "output_bridge";
pub const SESSION_LOG_FAILURE_PHASE_CHILD_WAIT: &str = "child_wait";
pub const SESSION_LOG_FAILURE_PHASE_WAITING_INITIAL_OUTPUT: &str = "waiting_initial_output";
pub const SESSION_LOG_FAILURE_PHASE_USER_ABORT: &str = "user_abort";
pub const SESSION_LOG_FAILURE_REASON_CREATE_LOG_FAILED: &str = "create_log_failed";
pub const SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED: &str = "open_pty_failed";
pub const SESSION_LOG_FAILURE_REASON_RAW_MODE_FAILED: &str = "raw_mode_failed";
pub const SESSION_LOG_FAILURE_REASON_SPAWN_CHILD_FAILED: &str = "spawn_child_failed";
pub const SESSION_LOG_FAILURE_REASON_INPUT_BRIDGE_FAILED: &str = "input_bridge_failed";
pub const SESSION_LOG_FAILURE_REASON_OUTPUT_BRIDGE_FAILED: &str = "output_bridge_failed";
pub const SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED: &str = "child_wait_failed";
pub const SESSION_LOG_FAILURE_REASON_INITIAL_OUTPUT_TIMEOUT: &str = "initial_output_timeout";
pub const SESSION_LOG_FAILURE_REASON_CTRL_C: &str = "ctrl_c";
pub const SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT: &str = "best_effort";
pub const SESSION_LOG_CONTENT_CAPTURE_TERMINAL_IO: &str = "terminal_io";
pub const SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY: &str = "explicit_ready";
pub const SESSION_LOG_CONPTY_AUTO_SELECTION_DEFERRED: &str = "deferred";
pub const SESSION_LOG_CONPTY_TUI_SUCCESS_REASON: &str =
    "normal TUI logging and Japanese output succeeded; failure cases still require evidence.";
pub const SESSION_LOG_WINDOWS_AUTO_DEFERRED_REASON: &str =
    "explicit ConPTY is available, but auto selection is deferred until failure-case evidence is complete.";
pub const SESSION_LOG_BACKEND_WARNING_POWERSHELL_TRANSCRIPT: &str =
    "powershell_transcript_may_not_capture_interactive_ssh_io";
pub const SESSION_LOG_BACKEND_WARNING_CONPTY_EXPLICIT_NOT_AUTO: &str =
    "conpty_backend_is_explicit_and_not_selected_by_auto";
pub const SESSION_LOG_CAPTURE_STATUS_HOST_ONLY_OR_EMPTY: &str = "host_only_or_empty";
pub const SESSION_LOG_CAPTURE_WARNING_NO_SSH_CONTENT: &str =
    "No SSH terminal content appears to have been captured.";
pub const SESSION_LOG_DIAGNOSTIC_WARNING_POWERSHELL_TRANSCRIPT: &str =
    "may not capture interactive SSH input/output";
pub const SESSION_LOG_DIAGNOSTIC_WARNING_CONPTY_EXPLICIT_READY: &str =
    "ConPTY logging is explicit; failure cases still require evidence before auto selection.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLogBackendSetting {
    Auto,
    Script,
    PowerShellTranscript,
    Conpty,
    NoLog,
}

impl SessionLogBackendSetting {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            SESSION_LOG_BACKEND_AUTO => Ok(Self::Auto),
            SESSION_LOG_BACKEND_SCRIPT => Ok(Self::Script),
            SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT => Ok(Self::PowerShellTranscript),
            SESSION_LOG_BACKEND_CONPTY => Ok(Self::Conpty),
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
            Self::PowerShellTranscript => SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT,
            Self::Conpty => SESSION_LOG_BACKEND_CONPTY,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionLogDiagnostics {
    pub enabled: bool,
    pub backend_setting: String,
    pub resolved_backend: String,
    pub tui_integration: String,
    pub script_command: Option<PathBuf>,
    pub script_command_note: Option<String>,
    pub powershell_command: Option<PathBuf>,
    pub powershell_command_note: Option<String>,
    pub ssh_command: Option<PathBuf>,
    pub ssh_command_note: Option<String>,
    pub log_directory: PathBuf,
    pub log_directory_exists: bool,
    pub log_directory_writable: Option<bool>,
    pub last_session_log: Option<String>,
    pub platform: String,
    pub platform_supported: bool,
    pub fallback_reason: Option<String>,
    pub content_capture_reliability: Option<String>,
    pub warning: Option<String>,
    pub status: String,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLogFiles {
    pub session_id: String,
    pub log_path: PathBuf,
    pub metadata_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLogFailureMetadata<'a> {
    pub status: &'a str,
    pub failure_phase: &'a str,
    pub failure_reason: &'a str,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLogLaunchFailurePolicy {
    FallbackToPlain,
    FailSession,
}

impl SessionLogLaunchFailurePolicy {
    pub fn fallback_to_plain(self) -> bool {
        matches!(self, Self::FallbackToPlain)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLogPlan {
    Disabled,
    NoLog {
        reason: String,
    },
    Error {
        reason: String,
    },
    Script {
        script_path: PathBuf,
        files: SessionLogFiles,
        launch_failure_policy: SessionLogLaunchFailurePolicy,
    },
    PowerShellTranscript {
        powershell_path: PathBuf,
        files: SessionLogFiles,
        launch_failure_policy: SessionLogLaunchFailurePolicy,
    },
    Conpty {
        files: SessionLogFiles,
        launch_failure_policy: SessionLogLaunchFailurePolicy,
    },
}

impl SessionLogPlan {
    pub fn not_saved_reference(&self) -> SessionLogReference {
        match self {
            Self::Disabled => SessionLogReference::not_saved(SESSION_LOG_REASON_DISABLED),
            Self::NoLog { reason } => SessionLogReference::not_saved(reason),
            Self::Error { reason } => SessionLogReference::not_saved(reason),
            Self::Script { .. } => SessionLogReference::not_saved("not_completed"),
            Self::PowerShellTranscript { .. } => SessionLogReference::not_saved("not_completed"),
            Self::Conpty { .. } => SessionLogReference::not_saved("not_completed"),
        }
    }

    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Disabled => None,
            Self::NoLog { reason } => Some(format!(
                "TeraDock session logging requested but no terminal log will be saved ({reason}); continuing with normal SSH."
            )),
            Self::Error { reason } => Some(format!(
                "TeraDock session logging requested, but the selected backend is not ready ({reason})."
            )),
            Self::Script { files, .. } => Some(format!(
                "TeraDock session logging is enabled; terminal output may include secrets and will be saved to {}.",
                files.log_path.display()
            )),
            Self::PowerShellTranscript { files, .. } => Some(format!(
                "TeraDock PowerShell Transcript logging is experimental and best-effort; it may miss interactive SSH input/output. Log path: {}.",
                files.log_path.display()
            )),
            Self::Conpty { files, .. } => Some(format!(
                "TeraDock ConPTY logging is explicit and auto selection is deferred; terminal output may include secrets and will be saved to {}.",
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_capture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_capture_reliable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_capture_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_capture_warning: Option<String>,
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

pub fn resolve_global_config(conn: &Connection) -> Result<SessionLogConfig> {
    let enabled = settings::get_setting(conn, SESSION_LOG_ENABLED_KEY)?
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let backend_raw = settings::get_setting(conn, SESSION_LOG_BACKEND_KEY)?
        .unwrap_or_else(|| SESSION_LOG_BACKEND_AUTO.to_string());
    let backend = SessionLogBackendSetting::parse(&backend_raw)?;
    let dir = configured_session_log_dir(conn)?;

    Ok(SessionLogConfig {
        enabled,
        dir,
        backend,
    })
}

pub fn default_value_for_key(conn: &Connection, key: &str) -> Result<Option<String>> {
    match key {
        SESSION_LOG_ENABLED_KEY => Ok(Some("false".to_string())),
        SESSION_LOG_BACKEND_KEY => Ok(Some(SESSION_LOG_BACKEND_AUTO.to_string())),
        SESSION_LOG_DIR_KEY => Ok(Some(
            configured_session_log_dir(conn)?.display().to_string(),
        )),
        _ => Ok(None),
    }
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

pub fn diagnose(conn: &Connection, profile_id: Option<&str>) -> Result<SessionLogDiagnostics> {
    let config = match profile_id {
        Some(profile_id) => resolve_config(conn, profile_id)?,
        None => resolve_global_config(conn)?,
    };
    diagnose_config(&config)
}

pub fn diagnose_config(config: &SessionLogConfig) -> Result<SessionLogDiagnostics> {
    diagnose_config_with_environment(
        config,
        SessionLogPlatform::current(),
        detect_session_log_clients(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionLogPlatform {
    Windows,
    Macos,
    Unix,
    Unknown,
}

impl SessionLogPlatform {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(unix) {
            Self::Unix
        } else {
            Self::Unknown
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Unix => "unix",
            Self::Unknown => "unknown",
        }
    }

    fn supports_script(self) -> bool {
        matches!(self, Self::Macos | Self::Unix)
    }

    fn supports_powershell_transcript(self) -> bool {
        matches!(self, Self::Windows)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SessionLogClientCommands {
    script: Option<PathBuf>,
    powershell: Option<PathBuf>,
    ssh: Option<PathBuf>,
}

fn detect_session_log_clients() -> SessionLogClientCommands {
    SessionLogClientCommands {
        script: doctor::resolve_client(&["script"]),
        powershell: resolve_powershell_command(),
        ssh: doctor::resolve_client(&["ssh", "ssh.exe"]),
    }
}

fn resolve_powershell_command() -> Option<PathBuf> {
    doctor::resolve_client(&["powershell.exe", "powershell", "pwsh.exe", "pwsh"])
}

fn diagnose_config_with_environment(
    config: &SessionLogConfig,
    platform: SessionLogPlatform,
    clients: SessionLogClientCommands,
) -> Result<SessionLogDiagnostics> {
    let (log_directory_exists, log_directory_writable, log_directory_reason) =
        log_directory_readiness(&config.dir, should_check_log_directory(config, platform));
    let last_session_log = list_session_logs_in_dir(&config.dir)?
        .first()
        .map(|metadata| metadata.session_id.clone());
    let platform_name = platform.name().to_string();
    let backend_setting = config.backend.as_str().to_string();
    let mut script_command = clients.script;
    let mut powershell_command = clients.powershell;
    let mut ssh_command = clients.ssh;
    let mut script_command_note = command_note(script_command.as_ref());
    let mut powershell_command_note = command_note(powershell_command.as_ref());
    let mut ssh_command_note = command_note(ssh_command.as_ref());

    let resolution = resolve_backend(
        config,
        platform,
        script_command.as_ref(),
        powershell_command.as_ref(),
        ssh_command.as_ref(),
        log_directory_reason.as_deref(),
    );

    if !config.enabled {
        script_command = None;
        powershell_command = None;
        ssh_command = None;
        script_command_note = Some("not checked because logging is disabled".to_string());
        powershell_command_note = Some("not checked because logging is disabled".to_string());
        ssh_command_note = Some("not checked because logging is disabled".to_string());
    } else if config.backend == SessionLogBackendSetting::NoLog {
        script_command = None;
        powershell_command = None;
        ssh_command = None;
        script_command_note = Some("not checked because backend is no-log".to_string());
        powershell_command_note = Some("not checked because backend is no-log".to_string());
        ssh_command_note = Some("not checked because backend is no-log".to_string());
    } else if config.backend == SessionLogBackendSetting::Auto
        && platform == SessionLogPlatform::Windows
    {
        script_command = None;
        powershell_command = None;
        ssh_command = None;
        script_command_note =
            Some("not checked because script is unsupported on this platform".to_string());
        powershell_command_note =
            Some("not checked because Windows auto does not use PowerShell Transcript".to_string());
        ssh_command_note = Some("not checked because Windows auto resolves to no-log".to_string());
    } else if config.backend == SessionLogBackendSetting::Conpty {
        script_command = None;
        powershell_command = None;
        script_command_note = Some("not checked for this backend".to_string());
        powershell_command_note = Some("not checked for this backend".to_string());
    } else if !matches!(
        config.backend,
        SessionLogBackendSetting::Auto | SessionLogBackendSetting::Script
    ) {
        script_command = None;
        script_command_note = Some("not checked for this backend".to_string());
    } else if !platform.supports_script() {
        script_command = None;
        script_command_note =
            Some("not checked because script is unsupported on this platform".to_string());
    }

    let status = diagnostics_status(
        config.enabled,
        &resolution.resolved_backend,
        resolution.fallback_reason.as_deref(),
    );
    let tui_integration = diagnostics_tui_integration(
        config.enabled,
        config.backend,
        &resolution.resolved_backend,
        resolution.fallback_reason.as_deref(),
    );
    let (content_capture_reliability, warning) =
        diagnostics_capture_fields(&resolution.resolved_backend);
    let hints = diagnostics_hints(
        config.enabled,
        config.backend,
        platform,
        resolution.platform_supported,
        &resolution.resolved_backend,
        resolution.fallback_reason.as_deref(),
        last_session_log.is_some(),
    );

    Ok(SessionLogDiagnostics {
        enabled: config.enabled,
        backend_setting,
        resolved_backend: resolution.resolved_backend,
        tui_integration,
        script_command,
        script_command_note,
        powershell_command,
        powershell_command_note,
        ssh_command,
        ssh_command_note,
        log_directory: config.dir.clone(),
        log_directory_exists,
        log_directory_writable,
        last_session_log,
        platform: platform_name,
        platform_supported: resolution.platform_supported,
        fallback_reason: resolution.fallback_reason,
        content_capture_reliability,
        warning,
        status,
        hints,
    })
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

pub fn plan_for_target_with_ssh(
    conn: &Connection,
    target: &SshTarget,
    ssh_executable: &Path,
) -> SessionLogPlan {
    let config = match resolve_config(conn, &target.profile_id) {
        Ok(config) => config,
        Err(err) => {
            return SessionLogPlan::NoLog {
                reason: format!("{SESSION_LOG_REASON_SETUP_FAILED}: {err}"),
            }
        }
    };
    let mut clients = detect_session_log_clients();
    clients.ssh = Some(ssh_executable.to_path_buf());
    plan_from_config_with_environment(&config, SessionLogPlatform::current(), clients)
}

pub fn plan_for_explicit_backend_with_ssh(
    conn: &Connection,
    ssh_executable: &Path,
    backend: SessionLogBackendSetting,
) -> Result<SessionLogPlan> {
    let config = SessionLogConfig {
        enabled: true,
        dir: configured_session_log_dir(conn)?,
        backend,
    };
    let mut clients = detect_session_log_clients();
    clients.ssh = Some(ssh_executable.to_path_buf());
    Ok(plan_from_config_with_environment(
        &config,
        SessionLogPlatform::current(),
        clients,
    ))
}

pub fn plan_from_config(config: &SessionLogConfig) -> SessionLogPlan {
    plan_from_config_with_environment(
        config,
        SessionLogPlatform::current(),
        detect_session_log_clients(),
    )
}

fn plan_from_config_with_environment(
    config: &SessionLogConfig,
    platform: SessionLogPlatform,
    clients: SessionLogClientCommands,
) -> SessionLogPlan {
    if !config.enabled {
        return SessionLogPlan::Disabled;
    }
    if config.backend == SessionLogBackendSetting::NoLog {
        return SessionLogPlan::NoLog {
            reason: SESSION_LOG_REASON_BACKEND_NO_LOG.to_string(),
        };
    }

    match config.backend {
        SessionLogBackendSetting::Auto => {
            if platform.supports_script() {
                plan_script_backend(config, clients.script, true)
            } else if platform == SessionLogPlatform::Windows {
                SessionLogPlan::NoLog {
                    reason: SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY.to_string(),
                }
            } else {
                SessionLogPlan::NoLog {
                    reason: SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM.to_string(),
                }
            }
        }
        SessionLogBackendSetting::Script => {
            if !platform.supports_script() {
                return SessionLogPlan::Error {
                    reason: SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS.to_string(),
                };
            }
            plan_script_backend(config, clients.script, false)
        }
        SessionLogBackendSetting::PowerShellTranscript => {
            if !platform.supports_powershell_transcript() {
                return SessionLogPlan::Error {
                    reason: SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM.to_string(),
                };
            }
            plan_powershell_backend(config, clients.powershell, clients.ssh, false)
        }
        SessionLogBackendSetting::Conpty => {
            if platform != SessionLogPlatform::Windows {
                return SessionLogPlan::Error {
                    reason: SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM.to_string(),
                };
            }
            plan_conpty_backend(config, clients.ssh, false)
        }
        SessionLogBackendSetting::NoLog => unreachable!("handled above"),
    }
}

fn plan_script_backend(
    config: &SessionLogConfig,
    script_path: Option<PathBuf>,
    auto_fallback: bool,
) -> SessionLogPlan {
    let Some(script_path) = script_path else {
        return fallback_or_error(auto_fallback, SESSION_LOG_REASON_SCRIPT_UNAVAILABLE);
    };
    let files = match prepare_session_files(&config.dir, auto_fallback) {
        Ok(files) => files,
        Err(plan) => return plan,
    };
    SessionLogPlan::Script {
        script_path,
        files,
        launch_failure_policy: launch_policy(auto_fallback),
    }
}

fn plan_powershell_backend(
    config: &SessionLogConfig,
    powershell_path: Option<PathBuf>,
    ssh_path: Option<PathBuf>,
    auto_fallback: bool,
) -> SessionLogPlan {
    if powershell_path.is_none() {
        return fallback_or_error(auto_fallback, SESSION_LOG_REASON_POWERSHELL_NOT_FOUND);
    }
    if ssh_path.is_none() {
        return fallback_or_error(auto_fallback, SESSION_LOG_REASON_SSH_NOT_FOUND);
    }
    let files = match prepare_session_files(&config.dir, auto_fallback) {
        Ok(files) => files,
        Err(plan) => return plan,
    };
    SessionLogPlan::PowerShellTranscript {
        powershell_path: powershell_path.expect("checked above"),
        files,
        launch_failure_policy: launch_policy(auto_fallback),
    }
}

fn plan_conpty_backend(
    config: &SessionLogConfig,
    ssh_path: Option<PathBuf>,
    auto_fallback: bool,
) -> SessionLogPlan {
    if ssh_path.is_none() {
        return fallback_or_error(auto_fallback, SESSION_LOG_REASON_SSH_NOT_FOUND);
    }
    let files = match prepare_session_files(&config.dir, auto_fallback) {
        Ok(files) => files,
        Err(plan) => return plan,
    };
    SessionLogPlan::Conpty {
        files,
        launch_failure_policy: launch_policy(auto_fallback),
    }
}

fn prepare_session_files(
    dir: &Path,
    auto_fallback: bool,
) -> std::result::Result<SessionLogFiles, SessionLogPlan> {
    if let Err(err) = ensure_session_log_dir(dir) {
        return Err(fallback_or_error(
            auto_fallback,
            format!("{SESSION_LOG_REASON_LOG_DIR_NOT_WRITABLE}: {err}"),
        ));
    }
    allocate_session_files(dir).map_err(|err| {
        fallback_or_error(
            auto_fallback,
            format!("{SESSION_LOG_REASON_SETUP_FAILED}: {err}"),
        )
    })
}

fn fallback_or_error(auto_fallback: bool, reason: impl Into<String>) -> SessionLogPlan {
    let reason = reason.into();
    if auto_fallback {
        SessionLogPlan::NoLog { reason }
    } else {
        SessionLogPlan::Error { reason }
    }
}

fn launch_policy(auto_fallback: bool) -> SessionLogLaunchFailurePolicy {
    if auto_fallback {
        SessionLogLaunchFailurePolicy::FallbackToPlain
    } else {
        SessionLogLaunchFailurePolicy::FailSession
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

pub fn build_powershell_transcript_invocation(
    powershell_path: &Path,
    files: &SessionLogFiles,
    ssh_executable: &Path,
    ssh_args: &[OsString],
    launch_failure_policy: SessionLogLaunchFailurePolicy,
) -> ExternalCommandInvocation {
    ExternalCommandInvocation {
        executable: powershell_path.to_path_buf(),
        args: vec![
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-Command"),
            OsString::from(powershell_transcript_command(
                &files.log_path,
                ssh_executable,
                ssh_args,
                launch_failure_policy.fallback_to_plain(),
            )),
        ],
    }
}

pub fn complete_script_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
) -> Result<SessionLogMetadata> {
    complete_logged_session(
        files,
        target,
        started_at,
        duration_ms,
        exit_code,
        SESSION_LOG_BACKEND_SCRIPT,
    )
}

pub fn complete_powershell_transcript_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
) -> Result<SessionLogMetadata> {
    complete_logged_session(
        files,
        target,
        started_at,
        duration_ms,
        exit_code,
        SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT,
    )
}

pub fn prepare_conpty_session_files(conn: &Connection) -> Result<SessionLogFiles> {
    let dir = configured_session_log_dir(conn)?;
    ensure_session_log_dir(&dir)?;
    allocate_session_files(&dir)
}

pub fn complete_conpty_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
) -> Result<SessionLogMetadata> {
    complete_logged_session(
        files,
        target,
        started_at,
        duration_ms,
        exit_code,
        SESSION_LOG_BACKEND_CONPTY,
    )
}

pub fn complete_conpty_failure_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    failure: SessionLogFailureMetadata<'_>,
) -> Result<SessionLogMetadata> {
    complete_logged_session_with_status(
        files,
        target,
        started_at,
        duration_ms,
        SESSION_LOG_BACKEND_CONPTY,
        SessionLogCompletion {
            exit_code: failure.exit_code,
            status: failure.status.to_string(),
            reason: Some(failure.failure_reason.to_string()),
            failure_phase: Some(failure.failure_phase.to_string()),
            failure_reason: Some(failure.failure_reason.to_string()),
            require_log_file: false,
        },
    )
}

struct SessionLogCompletion {
    exit_code: Option<i32>,
    status: String,
    reason: Option<String>,
    failure_phase: Option<String>,
    failure_reason: Option<String>,
    require_log_file: bool,
}

fn complete_logged_session(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
    backend: &str,
) -> Result<SessionLogMetadata> {
    complete_logged_session_with_status(
        files,
        target,
        started_at,
        duration_ms,
        backend,
        SessionLogCompletion {
            exit_code,
            status: status_for_exit(exit_code),
            reason: None,
            failure_phase: None,
            failure_reason: None,
            require_log_file: true,
        },
    )
}

fn complete_logged_session_with_status(
    files: &SessionLogFiles,
    target: &SshTarget,
    started_at: i64,
    duration_ms: i64,
    backend: &str,
    completion: SessionLogCompletion,
) -> Result<SessionLogMetadata> {
    if !files.log_path.is_file() {
        if completion.require_log_file {
            return Err(CoreError::NotFound(format!(
                "session log not found: {}",
                files.log_path.display()
            )));
        }
    } else {
        set_user_only_file_permissions(&files.log_path)?;
    }
    let mut metadata = SessionLogMetadata {
        session_id: files.session_id.clone(),
        profile_id: target.profile_id.clone(),
        user: target.user.clone(),
        host: target.host.clone(),
        port: target.port,
        started_at,
        ended_at: now_ms(),
        duration_ms,
        exit_code: completion.exit_code,
        backend: backend.to_string(),
        log_path: files.log_path.is_file().then(|| files.log_path.clone()),
        metadata_path: files.metadata_path.clone(),
        status: completion.status,
        reason: completion.reason,
        failure_phase: completion.failure_phase,
        failure_reason: completion.failure_reason,
        content_capture: None,
        content_capture_reliable: None,
        backend_status: None,
        backend_warning: None,
        content_capture_status: None,
        content_capture_warning: None,
    };
    if backend == SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT {
        annotate_powershell_transcript_metadata(&mut metadata, &files.log_path);
    } else if backend == SESSION_LOG_BACKEND_CONPTY {
        annotate_conpty_metadata(&mut metadata);
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendResolution {
    resolved_backend: String,
    platform_supported: bool,
    fallback_reason: Option<String>,
}

fn resolve_backend(
    config: &SessionLogConfig,
    platform: SessionLogPlatform,
    script_command: Option<&PathBuf>,
    powershell_command: Option<&PathBuf>,
    ssh_command: Option<&PathBuf>,
    log_directory_reason: Option<&str>,
) -> BackendResolution {
    if !config.enabled {
        return BackendResolution {
            resolved_backend: "disabled".to_string(),
            platform_supported: false,
            fallback_reason: None,
        };
    }
    if config.backend == SessionLogBackendSetting::NoLog {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(SESSION_LOG_REASON_BACKEND_NO_LOG.to_string()),
        };
    }

    let fallback = |reason: &'static str| BackendResolution {
        resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
        platform_supported: false,
        fallback_reason: Some(reason.to_string()),
    };

    match config.backend {
        SessionLogBackendSetting::Auto => {
            if platform.supports_script() {
                resolve_script_backend(script_command, log_directory_reason)
            } else if platform == SessionLogPlatform::Windows {
                fallback(SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY)
            } else {
                fallback(SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM)
            }
        }
        SessionLogBackendSetting::Script => {
            if !platform.supports_script() {
                return fallback(SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS);
            }
            resolve_script_backend(script_command, log_directory_reason)
        }
        SessionLogBackendSetting::PowerShellTranscript => {
            if !platform.supports_powershell_transcript() {
                return fallback(SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM);
            }
            resolve_powershell_backend(powershell_command, ssh_command, log_directory_reason)
        }
        SessionLogBackendSetting::Conpty => {
            if platform != SessionLogPlatform::Windows {
                return fallback(SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM);
            }
            resolve_conpty_backend(ssh_command, log_directory_reason)
        }
        SessionLogBackendSetting::NoLog => unreachable!("handled above"),
    }
}

fn resolve_script_backend(
    script_command: Option<&PathBuf>,
    log_directory_reason: Option<&str>,
) -> BackendResolution {
    if script_command.is_none() {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(SESSION_LOG_REASON_SCRIPT_UNAVAILABLE.to_string()),
        };
    }
    if let Some(reason) = log_directory_reason {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(reason.to_string()),
        };
    }
    BackendResolution {
        resolved_backend: SESSION_LOG_BACKEND_SCRIPT.to_string(),
        platform_supported: true,
        fallback_reason: None,
    }
}

fn resolve_powershell_backend(
    powershell_command: Option<&PathBuf>,
    ssh_command: Option<&PathBuf>,
    log_directory_reason: Option<&str>,
) -> BackendResolution {
    if powershell_command.is_none() {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(SESSION_LOG_REASON_POWERSHELL_NOT_FOUND.to_string()),
        };
    }
    if ssh_command.is_none() {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(SESSION_LOG_REASON_SSH_NOT_FOUND.to_string()),
        };
    }
    if let Some(reason) = log_directory_reason {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(reason.to_string()),
        };
    }
    BackendResolution {
        resolved_backend: SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT.to_string(),
        platform_supported: true,
        fallback_reason: None,
    }
}

fn resolve_conpty_backend(
    ssh_command: Option<&PathBuf>,
    log_directory_reason: Option<&str>,
) -> BackendResolution {
    if ssh_command.is_none() {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(SESSION_LOG_REASON_SSH_NOT_FOUND.to_string()),
        };
    }
    if let Some(reason) = log_directory_reason {
        return BackendResolution {
            resolved_backend: SESSION_LOG_BACKEND_NO_LOG.to_string(),
            platform_supported: false,
            fallback_reason: Some(reason.to_string()),
        };
    }
    BackendResolution {
        resolved_backend: SESSION_LOG_BACKEND_CONPTY.to_string(),
        platform_supported: true,
        fallback_reason: None,
    }
}

fn command_note(path: Option<&PathBuf>) -> Option<String> {
    if path.is_some() {
        None
    } else {
        Some("missing".to_string())
    }
}

fn should_check_log_directory(config: &SessionLogConfig, platform: SessionLogPlatform) -> bool {
    if !config.enabled || config.backend == SessionLogBackendSetting::NoLog {
        return false;
    }
    match config.backend {
        SessionLogBackendSetting::Auto => platform.supports_script(),
        SessionLogBackendSetting::Script => platform.supports_script(),
        SessionLogBackendSetting::PowerShellTranscript => platform.supports_powershell_transcript(),
        SessionLogBackendSetting::Conpty => platform == SessionLogPlatform::Windows,
        SessionLogBackendSetting::NoLog => false,
    }
}

fn log_directory_readiness(dir: &Path, enabled: bool) -> (bool, Option<bool>, Option<String>) {
    if !enabled {
        return (dir.is_dir(), None, None);
    }
    match ensure_session_log_dir(dir) {
        Ok(()) => {
            let writable = can_create_probe_file(dir);
            let reason = if writable {
                None
            } else {
                Some(SESSION_LOG_REASON_LOG_DIR_NOT_WRITABLE.to_string())
            };
            (dir.is_dir(), Some(writable), reason)
        }
        Err(_) => (
            dir.is_dir(),
            Some(false),
            Some(SESSION_LOG_REASON_LOG_DIR_NOT_WRITABLE.to_string()),
        ),
    }
}

fn can_create_probe_file(dir: &Path) -> bool {
    let probe = dir.join(format!(
        ".teradock-session-log-write-test-{}",
        std::process::id()
    ));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(err) if err.kind() == ErrorKind::AlreadyExists => true,
        Err(_) => false,
    }
}

fn diagnostics_status(
    enabled: bool,
    resolved_backend: &str,
    fallback_reason: Option<&str>,
) -> String {
    if !enabled {
        return "disabled".to_string();
    }
    if fallback_reason.is_some() {
        return "not_ready".to_string();
    }
    if resolved_backend == SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT {
        return "degraded".to_string();
    }
    if resolved_backend == SESSION_LOG_BACKEND_CONPTY {
        return "degraded".to_string();
    }
    if resolved_backend == SESSION_LOG_BACKEND_SCRIPT {
        "ready".to_string()
    } else {
        "not_ready".to_string()
    }
}

fn diagnostics_tui_integration(
    enabled: bool,
    backend: SessionLogBackendSetting,
    resolved_backend: &str,
    fallback_reason: Option<&str>,
) -> String {
    if !enabled {
        return "disabled".to_string();
    }
    if backend == SessionLogBackendSetting::NoLog
        || (resolved_backend == SESSION_LOG_BACKEND_NO_LOG
            && fallback_reason == Some(SESSION_LOG_REASON_BACKEND_NO_LOG))
    {
        return "not logging s-key SSH sessions".to_string();
    }
    if fallback_reason.is_some() {
        return "not ready for s-key SSH sessions".to_string();
    }
    if matches!(
        resolved_backend,
        SESSION_LOG_BACKEND_SCRIPT
            | SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT
            | SESSION_LOG_BACKEND_CONPTY
    ) {
        return "enabled for s-key SSH sessions".to_string();
    }
    "not logging s-key SSH sessions".to_string()
}

fn diagnostics_capture_fields(resolved_backend: &str) -> (Option<String>, Option<String>) {
    if resolved_backend == SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT {
        (
            Some(SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT.to_string()),
            Some(SESSION_LOG_DIAGNOSTIC_WARNING_POWERSHELL_TRANSCRIPT.to_string()),
        )
    } else if resolved_backend == SESSION_LOG_BACKEND_CONPTY {
        (
            Some(SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY.to_string()),
            Some(SESSION_LOG_DIAGNOSTIC_WARNING_CONPTY_EXPLICIT_READY.to_string()),
        )
    } else {
        (None, None)
    }
}

fn diagnostics_hints(
    enabled: bool,
    backend: SessionLogBackendSetting,
    platform: SessionLogPlatform,
    platform_supported: bool,
    resolved_backend: &str,
    fallback_reason: Option<&str>,
    has_last_session_log: bool,
) -> Vec<String> {
    let mut hints = Vec::new();
    if !enabled {
        hints.push("Enable logging with: td config set session.log.enabled true".to_string());
        hints.push("Or open settings UI with: td config ui".to_string());
        return hints;
    }
    if backend == SessionLogBackendSetting::NoLog {
        hints.push("Set backend to auto with: td config set session.log.backend auto".to_string());
    }
    if !platform_supported {
        hints.push("Current backend is not ready on this platform.".to_string());
    }
    match fallback_reason {
        Some(SESSION_LOG_REASON_SCRIPT_UNAVAILABLE) => {
            hints
                .push("Install a script command or set session.log.backend to no-log.".to_string());
        }
        Some(SESSION_LOG_REASON_POWERSHELL_NOT_FOUND) => {
            hints.push("Install PowerShell or set session.log.backend to no-log.".to_string());
        }
        Some(SESSION_LOG_REASON_SSH_NOT_FOUND) => {
            hints.push("Install OpenSSH client or configure an ssh client override.".to_string());
        }
        Some(SESSION_LOG_REASON_LOG_DIR_NOT_WRITABLE) => {
            hints.push("Update session.log.dir or fix directory permissions.".to_string());
        }
        Some(SESSION_LOG_REASON_UNSUPPORTED_ON_WINDOWS) => {
            hints.push(
                "Windows full SSH terminal logging requires a future ConPTY backend.".to_string(),
            );
        }
        Some(SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM) => {
            hints.push("Use script on Linux/macOS or no-log on unsupported platforms.".to_string());
        }
        Some(SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY) => {
            hints.push(
                "Windows auto remains no-log until failure-case evidence is complete.".to_string(),
            );
            hints.push(
                "Use explicit ConPTY with: td config set session.log.backend conpty.".to_string(),
            );
            hints.push(
                "PowerShell Transcript is available only as an explicit best-effort backend."
                    .to_string(),
            );
            hints.push(
                "Set session.log.backend powershell-transcript only if you accept that SSH I/O may not be captured."
                    .to_string(),
            );
        }
        _ => {}
    }
    if platform == SessionLogPlatform::Windows && resolved_backend == SESSION_LOG_BACKEND_CONPTY {
        hints.push("ConPTY backend is explicit and remains unselected by auto.".to_string());
        hints.push("Set it with: td config set session.log.backend conpty".to_string());
        hints.push("Open settings UI with: td config ui".to_string());
        hints.push("ConPTY remains explicit until failure-case evidence is complete.".to_string());
    }
    if platform == SessionLogPlatform::Windows
        && resolved_backend == SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT
    {
        hints.push(
            "PowerShell Transcript may capture only the PowerShell host transcript.".to_string(),
        );
        hints.push("It may not capture interactive SSH terminal input/output.".to_string());
        hints.push(
            "For full SSH terminal logging on Windows, a ConPTY backend is required.".to_string(),
        );
    }
    if !has_last_session_log {
        hints.push("Saved logs appear after an interactive SSH session exits.".to_string());
    }
    hints
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

fn annotate_powershell_transcript_metadata(metadata: &mut SessionLogMetadata, log_path: &Path) {
    metadata.content_capture = Some(SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT.to_string());
    metadata.content_capture_reliable = Some(false);
    metadata.backend_warning = Some(SESSION_LOG_BACKEND_WARNING_POWERSHELL_TRANSCRIPT.to_string());

    let Ok(raw) = fs::read_to_string(log_path) else {
        return;
    };
    if powershell_transcript_is_host_only_or_empty(&raw) {
        metadata.content_capture_status =
            Some(SESSION_LOG_CAPTURE_STATUS_HOST_ONLY_OR_EMPTY.to_string());
        metadata.content_capture_warning =
            Some(SESSION_LOG_CAPTURE_WARNING_NO_SSH_CONTENT.to_string());
    }
}

fn annotate_conpty_metadata(metadata: &mut SessionLogMetadata) {
    metadata.content_capture = Some(SESSION_LOG_CONTENT_CAPTURE_TERMINAL_IO.to_string());
    metadata.content_capture_reliable = Some(true);
    metadata.backend_status = Some(SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY.to_string());
    metadata.backend_warning =
        Some(SESSION_LOG_BACKEND_WARNING_CONPTY_EXPLICIT_NOT_AUTO.to_string());
}

fn powershell_transcript_is_host_only_or_empty(raw: &str) -> bool {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return true;
    }
    let has_transcript_marker = lines.iter().any(|line| {
        line.eq_ignore_ascii_case("PowerShell transcript start")
            || line.eq_ignore_ascii_case("PowerShell transcript end")
    });
    if !has_transcript_marker {
        return false;
    }
    lines
        .iter()
        .all(|line| is_known_powershell_transcript_metadata_line(line))
}

fn is_known_powershell_transcript_metadata_line(line: &str) -> bool {
    if line.chars().all(|ch| ch == '*') || line.chars().all(|ch| ch == '-') {
        return true;
    }
    let lower = line.to_ascii_lowercase();
    lower == "powershell transcript start"
        || lower == "powershell transcript end"
        || lower.starts_with("start time:")
        || lower.starts_with("end time:")
        || lower.starts_with("username:")
        || lower.starts_with("runas user:")
        || lower.starts_with("configuration name:")
        || lower.starts_with("machine:")
        || lower.starts_with("host application:")
        || lower.starts_with("process id:")
        || lower.starts_with("psversion:")
        || lower.starts_with("psedition:")
        || lower.starts_with("gitcommitid:")
        || lower.starts_with("os:")
        || lower.starts_with("platform:")
        || lower.starts_with("pscompatibleversions:")
        || lower.starts_with("psremotingprotocolversion:")
        || lower.starts_with("serializationversion:")
        || lower.starts_with("wsmanstackversion:")
        || lower.starts_with("transcript started, output file is")
        || lower.starts_with("transcript stopped, output file is")
}

fn powershell_transcript_command(
    log_path: &Path,
    ssh_executable: &Path,
    ssh_args: &[OsString],
    fallback_to_plain: bool,
) -> String {
    let ssh_command = powershell_call_command(ssh_executable, ssh_args);
    let transcript_start_catch = if fallback_to_plain {
        "catch { Write-Warning \"TeraDock session logging failed to start; continuing without logging.\" }"
    } else {
        "catch { throw }"
    };
    format!(
        "$ErrorActionPreference = 'Stop'; \
         $teradockTranscriptStarted = $false; \
         try {{ \
             Start-Transcript -Path {} -Force | Out-Null; \
             $teradockTranscriptStarted = $true; \
         }} {} \
         try {{ \
             {}; \
             $teradockExitCode = if ($null -eq $LASTEXITCODE) {{ 0 }} else {{ $LASTEXITCODE }}; \
         }} finally {{ \
             if ($teradockTranscriptStarted) {{ \
                 try {{ Stop-Transcript | Out-Null }} catch {{ Write-Warning $_ }} \
             }} \
         }}; \
         exit $teradockExitCode",
        powershell_single_quote(log_path.as_os_str()),
        transcript_start_catch,
        ssh_command
    )
}

fn powershell_call_command(executable: &Path, args: &[OsString]) -> String {
    let mut parts = vec![
        "&".to_string(),
        powershell_single_quote(executable.as_os_str()),
    ];
    parts.extend(
        args.iter()
            .map(|arg| powershell_single_quote(arg.as_os_str())),
    );
    parts.join(" ")
}

fn powershell_single_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    format!("'{}'", text.replace('\'', "''"))
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
    fn diagnostics_report_disabled_defaults() {
        let conn = db::init_in_memory().unwrap();
        let diagnostics = diagnose(&conn, None).unwrap();

        assert!(!diagnostics.enabled);
        assert_eq!(diagnostics.backend_setting, SESSION_LOG_BACKEND_AUTO);
        assert_eq!(diagnostics.resolved_backend, "disabled");
        assert_eq!(diagnostics.tui_integration, "disabled");
        assert_eq!(
            diagnostics.script_command_note.as_deref(),
            Some("not checked because logging is disabled")
        );
        assert_eq!(diagnostics.log_directory_writable, None);
        assert!(diagnostics
            .hints
            .iter()
            .any(|hint| hint.contains("td config ui")));
    }

    #[test]
    fn diagnostics_report_backend_no_log() {
        let dir = temp_dir("diagnostics-no-log");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::NoLog,
        };

        let diagnostics = diagnose_config(&config).unwrap();

        assert_eq!(diagnostics.backend_setting, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(
            diagnostics.tui_integration,
            "not logging s-key SSH sessions"
        );
        assert_eq!(
            diagnostics.fallback_reason.as_deref(),
            Some(SESSION_LOG_REASON_BACKEND_NO_LOG)
        );
        assert_eq!(
            diagnostics.script_command_note.as_deref(),
            Some("not checked because backend is no-log")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn windows_auto_resolves_to_no_log_until_conpty_backend_exists() {
        let dir = temp_dir("windows-auto-ready");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::Auto,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from(
                    r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                )),
                ssh: Some(PathBuf::from(r"C:\Windows\System32\OpenSSH\ssh.exe")),
                script: None,
            },
        )
        .unwrap();

        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(diagnostics.status, "not_ready");
        assert_eq!(
            diagnostics.tui_integration,
            "not ready for s-key SSH sessions"
        );
        assert!(!diagnostics.platform_supported);
        assert_eq!(
            diagnostics.fallback_reason.as_deref(),
            Some(SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY)
        );
        assert_eq!(diagnostics.content_capture_reliability, None);
        assert!(diagnostics.hints.iter().any(|hint| hint.contains("ConPTY")));

        let plan = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from("powershell.exe")),
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        );
        assert_eq!(
            plan,
            SessionLogPlan::NoLog {
                reason: SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY.to_string()
            }
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn windows_auto_does_not_require_powershell() {
        let dir = temp_dir("windows-auto-no-powershell");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::Auto,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        )
        .unwrap();

        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(
            diagnostics.tui_integration,
            "not ready for s-key SSH sessions"
        );
        assert_eq!(
            diagnostics.fallback_reason.as_deref(),
            Some(SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY)
        );
        assert_eq!(diagnostics.status, "not_ready");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn windows_auto_does_not_require_ssh() {
        let dir = temp_dir("windows-auto-no-ssh");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::Auto,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from("powershell.exe")),
                ssh: None,
                script: None,
            },
        )
        .unwrap();

        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(
            diagnostics.tui_integration,
            "not ready for s-key SSH sessions"
        );
        assert_eq!(
            diagnostics.fallback_reason.as_deref(),
            Some(SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY)
        );
        assert_eq!(diagnostics.status, "not_ready");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_powershell_transcript_reports_degraded_best_effort() {
        let dir = temp_dir("explicit-powershell-degraded");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::PowerShellTranscript,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from("powershell.exe")),
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        )
        .unwrap();

        assert_eq!(
            diagnostics.resolved_backend,
            SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT
        );
        assert_eq!(
            diagnostics.tui_integration,
            "enabled for s-key SSH sessions"
        );
        assert_eq!(diagnostics.status, "degraded");
        assert_eq!(diagnostics.fallback_reason, None);
        assert_eq!(
            diagnostics.content_capture_reliability.as_deref(),
            Some(SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT)
        );
        assert_eq!(
            diagnostics.warning.as_deref(),
            Some(SESSION_LOG_DIAGNOSTIC_WARNING_POWERSHELL_TRANSCRIPT)
        );
        assert!(diagnostics
            .hints
            .iter()
            .any(|hint| hint.contains("PowerShell Transcript")));

        let plan = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from("powershell.exe")),
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        );
        match plan {
            SessionLogPlan::PowerShellTranscript {
                launch_failure_policy,
                ..
            } => assert_eq!(
                launch_failure_policy,
                SessionLogLaunchFailurePolicy::FailSession
            ),
            other => panic!("expected PowerShell transcript plan, got {other:?}"),
        }

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_powershell_transcript_errors_when_not_ready_or_wrong_platform() {
        let dir = temp_dir("explicit-powershell");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::PowerShellTranscript,
        };

        let missing_powershell = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        );
        assert_eq!(
            missing_powershell,
            SessionLogPlan::Error {
                reason: SESSION_LOG_REASON_POWERSHELL_NOT_FOUND.to_string()
            }
        );

        let unsupported_platform = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Unix,
            SessionLogClientCommands {
                powershell: Some(PathBuf::from("pwsh")),
                ssh: Some(PathBuf::from("ssh")),
                script: Some(PathBuf::from("script")),
            },
        );
        assert_eq!(
            unsupported_platform,
            SessionLogPlan::Error {
                reason: SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM.to_string()
            }
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_conpty_is_explicit_ready_and_plans_on_windows() {
        let dir = temp_dir("explicit-conpty");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::Conpty,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        )
        .unwrap();

        assert_eq!(diagnostics.backend_setting, SESSION_LOG_BACKEND_CONPTY);
        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_CONPTY);
        assert_eq!(
            diagnostics.tui_integration,
            "enabled for s-key SSH sessions"
        );
        assert_eq!(diagnostics.status, "degraded");
        assert_eq!(
            diagnostics.content_capture_reliability.as_deref(),
            Some(SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY)
        );
        assert_eq!(
            diagnostics.warning.as_deref(),
            Some(SESSION_LOG_DIAGNOSTIC_WARNING_CONPTY_EXPLICIT_READY)
        );
        assert!(diagnostics
            .hints
            .iter()
            .any(|hint| hint.contains("session.log.backend conpty")));

        let plan = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Windows,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh.exe")),
                script: None,
            },
        );
        match plan {
            SessionLogPlan::Conpty {
                launch_failure_policy,
                ..
            } => assert_eq!(
                launch_failure_policy,
                SessionLogLaunchFailurePolicy::FailSession
            ),
            other => panic!("expected ConPTY plan, got {other:?}"),
        }

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_conpty_is_unsupported_outside_windows() {
        let dir = temp_dir("explicit-conpty-non-windows");
        let config = SessionLogConfig {
            enabled: true,
            dir: dir.clone(),
            backend: SessionLogBackendSetting::Conpty,
        };

        let diagnostics = diagnose_config_with_environment(
            &config,
            SessionLogPlatform::Unix,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh")),
                script: Some(PathBuf::from("script")),
            },
        )
        .unwrap();

        assert_eq!(diagnostics.resolved_backend, SESSION_LOG_BACKEND_NO_LOG);
        assert_eq!(
            diagnostics.tui_integration,
            "not ready for s-key SSH sessions"
        );
        assert_eq!(diagnostics.status, "not_ready");
        assert_eq!(
            diagnostics.fallback_reason.as_deref(),
            Some(SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM)
        );

        let plan = plan_from_config_with_environment(
            &config,
            SessionLogPlatform::Unix,
            SessionLogClientCommands {
                powershell: None,
                ssh: Some(PathBuf::from("ssh")),
                script: Some(PathBuf::from("script")),
            },
        );
        assert_eq!(
            plan,
            SessionLogPlan::Error {
                reason: SESSION_LOG_REASON_UNSUPPORTED_ON_PLATFORM.to_string()
            }
        );

        let _ = fs::remove_dir_all(dir);
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
    fn powershell_invocation_quotes_paths_and_arguments() {
        let files = SessionLogFiles {
            session_id: "sl_test".to_string(),
            log_path: PathBuf::from(r"C:\Users\Alice Logs\sl_test.log"),
            metadata_path: PathBuf::from(r"C:\Users\Alice Logs\sl_test.json"),
        };

        let invocation = build_powershell_transcript_invocation(
            Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            &files,
            Path::new(r"C:\Program Files\OpenSSH\ssh.exe"),
            &[
                OsString::from("-p"),
                OsString::from("2222"),
                OsString::from("alice.o'hara@example.com"),
            ],
            SessionLogLaunchFailurePolicy::FailSession,
        );
        let command = invocation
            .args
            .last()
            .expect("PowerShell command should be last")
            .to_string_lossy();

        assert_eq!(
            invocation.executable,
            PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        );
        assert!(command.contains(r"Start-Transcript -Path 'C:\Users\Alice Logs\sl_test.log'"));
        assert!(command.contains(r"& 'C:\Program Files\OpenSSH\ssh.exe'"));
        assert!(command.contains("'alice.o''hara@example.com'"));
        assert!(command.contains("catch { throw }"));
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
        assert_eq!(metadata.failure_phase, None);
        assert_eq!(metadata.failure_reason, None);
        assert!(metadata.log_path.is_some());
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("private_key_path"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_powershell_metadata_without_secret_fields() {
        let dir = temp_dir("powershell-metadata");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(&files.log_path, "PowerShell transcript\n").unwrap();
        let target = sample_target();

        let metadata =
            complete_powershell_transcript_session(&files, &target, 1000, 42, Some(0)).unwrap();
        let loaded = get_session_log_in_dir(&dir, &files.session_id).unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.backend, SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT);
        assert_eq!(loaded.backend, SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT);
        assert_eq!(
            metadata.content_capture.as_deref(),
            Some(SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT)
        );
        assert_eq!(metadata.content_capture_reliable, Some(false));
        assert_eq!(
            metadata.backend_warning.as_deref(),
            Some(SESSION_LOG_BACKEND_WARNING_POWERSHELL_TRANSCRIPT)
        );
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("command"));
        assert!(!raw.contains("private_key_path"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_conpty_metadata_without_secret_fields() {
        let dir = temp_dir("conpty-metadata");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(&files.log_path, "remote output\n").unwrap();
        let target = sample_target();

        let metadata = complete_conpty_session(&files, &target, 1000, 42, Some(0)).unwrap();
        let loaded = get_session_log_in_dir(&dir, &files.session_id).unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.backend, SESSION_LOG_BACKEND_CONPTY);
        assert_eq!(loaded.backend, SESSION_LOG_BACKEND_CONPTY);
        assert_eq!(
            metadata.content_capture.as_deref(),
            Some(SESSION_LOG_CONTENT_CAPTURE_TERMINAL_IO)
        );
        assert_eq!(metadata.content_capture_reliable, Some(true));
        assert_eq!(
            metadata.backend_status.as_deref(),
            Some(SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY)
        );
        assert_eq!(
            metadata.backend_warning.as_deref(),
            Some(SESSION_LOG_BACKEND_WARNING_CONPTY_EXPLICIT_NOT_AUTO)
        );
        assert_eq!(metadata.failure_phase, None);
        assert_eq!(metadata.failure_reason, None);
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("command"));
        assert!(!raw.contains("private_key_path"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_conpty_initial_timeout_metadata_without_secret_fields() {
        let dir = temp_dir("conpty-timeout-metadata");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(&files.log_path, "").unwrap();
        let target = sample_target();

        let metadata = complete_conpty_failure_session(
            &files,
            &target,
            1000,
            10_000,
            SessionLogFailureMetadata {
                status: SESSION_LOG_STATUS_FAILED,
                failure_phase: SESSION_LOG_FAILURE_PHASE_WAITING_INITIAL_OUTPUT,
                failure_reason: SESSION_LOG_FAILURE_REASON_INITIAL_OUTPUT_TIMEOUT,
                exit_code: None,
            },
        )
        .unwrap();
        let loaded = get_session_log_in_dir(&dir, &files.session_id).unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.status, SESSION_LOG_STATUS_FAILED);
        assert_eq!(loaded.status, SESSION_LOG_STATUS_FAILED);
        assert_eq!(
            metadata.failure_phase.as_deref(),
            Some(SESSION_LOG_FAILURE_PHASE_WAITING_INITIAL_OUTPUT)
        );
        assert_eq!(
            metadata.failure_reason.as_deref(),
            Some(SESSION_LOG_FAILURE_REASON_INITIAL_OUTPUT_TIMEOUT)
        );
        assert_eq!(
            metadata.content_capture.as_deref(),
            Some(SESSION_LOG_CONTENT_CAPTURE_TERMINAL_IO)
        );
        assert_eq!(metadata.content_capture_reliable, Some(true));
        assert_eq!(
            metadata.backend_status.as_deref(),
            Some(SESSION_LOG_BACKEND_STATUS_EXPLICIT_READY)
        );
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("command"));
        assert!(!raw.contains("private_key_path"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_conpty_abort_metadata_without_log_file() {
        let dir = temp_dir("conpty-abort-metadata");
        let files = allocate_session_files(&dir).unwrap();
        let target = sample_target();

        let metadata = complete_conpty_failure_session(
            &files,
            &target,
            1000,
            100,
            SessionLogFailureMetadata {
                status: SESSION_LOG_STATUS_ABORTED,
                failure_phase: SESSION_LOG_FAILURE_PHASE_USER_ABORT,
                failure_reason: SESSION_LOG_FAILURE_REASON_CTRL_C,
                exit_code: None,
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.status, SESSION_LOG_STATUS_ABORTED);
        assert_eq!(
            metadata.failure_phase.as_deref(),
            Some(SESSION_LOG_FAILURE_PHASE_USER_ABORT)
        );
        assert_eq!(
            metadata.failure_reason.as_deref(),
            Some(SESSION_LOG_FAILURE_REASON_CTRL_C)
        );
        assert_eq!(metadata.log_path, None);
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("private_key_path"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn writes_conpty_spawn_failure_metadata_without_secret_fields() {
        let dir = temp_dir("conpty-spawn-failure-metadata");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(&files.log_path, "").unwrap();
        let target = sample_target();

        let metadata = complete_conpty_failure_session(
            &files,
            &target,
            1000,
            50,
            SessionLogFailureMetadata {
                status: SESSION_LOG_STATUS_FAILED,
                failure_phase: SESSION_LOG_FAILURE_PHASE_SPAWN_CHILD,
                failure_reason: SESSION_LOG_FAILURE_REASON_SPAWN_CHILD_FAILED,
                exit_code: None,
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&files.metadata_path).unwrap();

        assert_eq!(metadata.status, SESSION_LOG_STATUS_FAILED);
        assert_eq!(
            metadata.failure_phase.as_deref(),
            Some(SESSION_LOG_FAILURE_PHASE_SPAWN_CHILD)
        );
        assert_eq!(
            metadata.failure_reason.as_deref(),
            Some(SESSION_LOG_FAILURE_REASON_SPAWN_CHILD_FAILED)
        );
        assert!(metadata.log_path.is_some());
        assert!(!raw.contains("auth_args"));
        assert!(!raw.contains("command"));
        assert!(!raw.contains("private_key_path"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("secret"));
        assert!(!raw.contains("token"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn detects_host_only_powershell_transcript_metadata() {
        let dir = temp_dir("powershell-host-only");
        let files = allocate_session_files(&dir).unwrap();
        fs::write(
            &files.log_path,
            "\
**********************
PowerShell transcript start
Start time: 20260613000000
Username: alice
Host Application: powershell.exe -NoLogo -NoProfile -Command Start-Transcript ssh.EXE
Process ID: 100
PSVersion: 5.1.0
**********************
PowerShell transcript end
End time: 20260613000100
**********************
",
        )
        .unwrap();
        let target = sample_target();

        let metadata =
            complete_powershell_transcript_session(&files, &target, 1000, 42, Some(0)).unwrap();
        let loaded = get_session_log_in_dir(&dir, &files.session_id).unwrap();

        assert_eq!(
            metadata.content_capture_status.as_deref(),
            Some(SESSION_LOG_CAPTURE_STATUS_HOST_ONLY_OR_EMPTY)
        );
        assert_eq!(
            metadata.content_capture_warning.as_deref(),
            Some(SESSION_LOG_CAPTURE_WARNING_NO_SSH_CONTENT)
        );
        assert_eq!(
            loaded.content_capture_status.as_deref(),
            Some(SESSION_LOG_CAPTURE_STATUS_HOST_ONLY_OR_EMPTY)
        );

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
