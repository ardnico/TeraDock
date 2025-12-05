use std::ffi::OsString;
use std::path::PathBuf;

use tracing::debug;

use crate::{
    config::AppConfig,
    profile::{DangerLevel, Profile, Protocol},
};

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub window_title: Option<String>,
}

pub fn build_command(profile: &Profile, config: &AppConfig, password: Option<&str>) -> CommandSpec {
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

    let mut window_title: Option<String> = None;
    if profile.is_dangerous() {
        window_title = Some(format!("[PROD] {}", profile.display_title()));
    }

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

    debug!("built command" = ?args, "program" = ?config.tera_term_path);

    CommandSpec {
        program: config.tera_term_path.clone(),
        args,
        window_title,
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
