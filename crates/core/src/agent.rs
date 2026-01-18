use serde::Serialize;
use std::env;
use std::path::Path;
use std::process::{Command, Output};

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub auth_sock: Option<String>,
    pub key_count: Option<usize>,
    pub keys: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentList {
    pub keys: Vec<String>,
    pub raw: String,
    pub error: Option<String>,
}

pub fn status() -> AgentStatus {
    let auth_sock = env::var_os("SSH_AUTH_SOCK").and_then(|value| {
        let trimmed = value.to_string_lossy();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    if auth_sock.is_none() {
        return AgentStatus {
            auth_sock: None,
            key_count: None,
            keys: Vec::new(),
            error: Some("SSH_AUTH_SOCK is not set; ssh-agent may be unavailable.".to_string()),
        };
    }
    let list = list();
    let key_count = if list.error.is_none() {
        Some(list.keys.len())
    } else {
        None
    };
    AgentStatus {
        auth_sock,
        key_count,
        keys: list.keys,
        error: list.error,
    }
}

pub fn list() -> AgentList {
    let auth_sock = env::var_os("SSH_AUTH_SOCK").and_then(|value| {
        let trimmed = value.to_string_lossy();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    if auth_sock.is_none() {
        return AgentList {
            keys: Vec::new(),
            raw: String::new(),
            error: Some("SSH_AUTH_SOCK is not set; ssh-agent may be unavailable.".to_string()),
        };
    }
    match run_ssh_add(&["-l"]) {
        Ok(output) => parse_list_output(&output),
        Err(err) => AgentList {
            keys: Vec::new(),
            raw: String::new(),
            error: Some(format!("failed to run ssh-add -l: {err}")),
        },
    }
}

pub fn run_add(key_path: &Path) -> std::io::Result<Output> {
    Command::new("ssh-add").arg(key_path).output()
}

pub fn run_clear() -> std::io::Result<Output> {
    Command::new("ssh-add").arg("-D").output()
}

fn parse_list_output(output: &Output) -> AgentList {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut keys = extract_keys(&stdout);
    let no_identities = contains_no_identities(&stdout) || contains_no_identities(&stderr);
    if no_identities {
        keys.clear();
    }
    let raw = if !stdout.is_empty() {
        stdout.clone()
    } else {
        stderr.clone()
    };
    let error = if output.status.success() || no_identities {
        None
    } else if !stderr.is_empty() {
        Some(stderr.clone())
    } else if !stdout.is_empty() {
        Some(stdout.clone())
    } else {
        Some(format!(
            "ssh-add -l failed with status {status}",
            status = output.status
        ))
    };
    AgentList { keys, raw, error }
}

fn extract_keys(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

fn contains_no_identities(text: &str) -> bool {
    text.to_ascii_lowercase()
        .contains("the agent has no identities")
}

fn run_ssh_add(args: &[&str]) -> std::io::Result<Output> {
    Command::new("ssh-add").args(args).output()
}
