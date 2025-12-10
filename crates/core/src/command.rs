use std::ffi::OsString;
use std::path::PathBuf;

use tracing::debug;

use crate::{
    config::AppConfig,
    profile::{ClientKind, DangerLevel, Profile, Protocol},
};

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub window_title: Option<String>,
}

pub fn build_command(profile: &Profile, config: &AppConfig, password: Option<&str>) -> CommandSpec {
    match effective_client_kind(profile, config) {
        ClientKind::WindowsTerminalSsh => build_windows_terminal_command(profile, config),
        ClientKind::PlainSsh => build_plain_ssh_command(profile, config),
        ClientKind::TeraTerm => build_teraterm_command(profile, config, password),
    }
}

pub fn confirmation_message(profile: &Profile) -> String {
    format!(
        "Connect to {} ({}) as {}{}?",
        profile.name,
        profile.host,
        profile.user.as_deref().unwrap_or("<default>"),
        match profile.danger_level {
            DangerLevel::Critical => " [CRITICAL]",
            DangerLevel::Warn => " [WARN]",
            DangerLevel::Normal => "",
        }
    )
}

fn effective_client_kind(profile: &Profile, config: &AppConfig) -> ClientKind {
    match &profile.client_kind {
        ClientKind::WindowsTerminalSsh if !config.windows_terminal_available() => {
            ClientKind::PlainSsh
        }
        other => other.clone(),
    }
}

fn build_windows_terminal_command(profile: &Profile, config: &AppConfig) -> CommandSpec {
    let mut args: Vec<OsString> = Vec::new();
    args.push(OsString::from("new-tab"));

    let window_title = danger_window_title(profile);
    if let Some(title) = window_title.as_ref() {
        args.push(OsString::from("--title"));
        args.push(OsString::from(title));
    }

    args.push(config.ssh_path.as_os_str().to_os_string());
    args.extend(build_ssh_args(profile));

    let program = config
        .windows_terminal_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("wt.exe"));

    debug!("client" = "windows_terminal", "program" = ?program, ?args);

    CommandSpec {
        program,
        args,
        window_title,
    }
}

fn build_plain_ssh_command(profile: &Profile, config: &AppConfig) -> CommandSpec {
    let mut args: Vec<OsString> = vec![OsString::from("/c"), OsString::from("start")];

    let window_title = danger_window_title(profile);
    args.push(OsString::from(window_title.as_deref().unwrap_or("")));

    args.push(config.ssh_path.as_os_str().to_os_string());
    args.extend(build_ssh_args(profile));

    let program = PathBuf::from("cmd.exe");
    debug!("client" = "plain_ssh", "program" = ?program, ?args);

    CommandSpec {
        program,
        args,
        window_title,
    }
}

fn build_teraterm_command(
    profile: &Profile,
    config: &AppConfig,
    password: Option<&str>,
) -> CommandSpec {
    const DEFAULT_TERA_TERM_PATH: &str = "C:/Program Files (x86)/teraterm/ttermpro.exe";

    let mut args: Vec<OsString> = Vec::new();

    if matches!(profile.protocol, Protocol::Telnet) {
        args.push(OsString::from("/telnet"));
    } else {
        args.push(OsString::from("/ssh"));
    }

    let host = if let Some(port) = profile.port {
        format!("{}:{}", profile.host, port)
    } else {
        profile.host.clone()
    };
    args.push(OsString::from(host));

    if let Some(user) = &profile.user {
        args.push(OsString::from(format!("/user=\"{}\"", user)));
    }

    if let Some(pass) = password {
        args.push(OsString::from(format!("/passwd=\"{}\"", pass)));
    }

    let window_title = danger_window_title(profile);
    if let Some(title) = window_title.as_ref() {
        args.push(OsString::from(format!("/W=\"{}\"", title)));
    }

    if let Some(macro_path) = &profile.macro_path {
        args.push(OsString::from(format!(
            "/MACRO=\"{}\"",
            macro_path.display()
        )));
    }

    if let Some(extra) = &profile.extra_args {
        for item in extra {
            args.push(OsString::from(item));
        }
    }

    for forwarding in profile.forwarding_args() {
        args.push(OsString::from(forwarding));
    }

    let program = config
        .tera_term_path
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_TERA_TERM_PATH));

    debug!("client" = "teraterm", "program" = ?program, ?args);

    CommandSpec {
        program,
        args,
        window_title,
    }
}

fn build_ssh_args(profile: &Profile) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    args.push(OsString::from(ssh_target(profile)));

    if let Some(port) = profile.port {
        args.push(OsString::from("-p"));
        args.push(OsString::from(port.to_string()));
    }

    if let Some(extra) = &profile.extra_args {
        for item in extra {
            args.push(OsString::from(item));
        }
    }

    for forwarding in profile.forwarding_args() {
        args.push(OsString::from(forwarding));
    }

    args
}

fn ssh_target(profile: &Profile) -> String {
    if let Some(user) = &profile.user {
        format!("{}@{}", user, profile.host)
    } else {
        profile.host.clone()
    }
}

fn danger_window_title(profile: &Profile) -> Option<String> {
    if profile.is_dangerous() {
        Some(format!("[PROD] {}", profile.display_title()))
    } else {
        None
    }
}
