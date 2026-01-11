use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::doctor::ClientKind;
use crate::error::{CoreError, Result};
use crate::profile::Profile;
use crate::util::now_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Push,
    Pull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferVia {
    Scp,
    Sftp,
    Ftp,
}

impl TransferVia {
    pub fn from_str(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "scp" => Ok(Self::Scp),
            "sftp" => Ok(Self::Sftp),
            "ftp" => Ok(Self::Ftp),
            _ => Err(CoreError::InvalidCommandSpec(format!(
                "invalid transfer client: {value}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scp => "scp",
            Self::Sftp => "sftp",
            Self::Ftp => "ftp",
        }
    }

    pub fn client_kind(&self) -> ClientKind {
        match self {
            Self::Scp => ClientKind::Scp,
            Self::Sftp => ClientKind::Sftp,
            Self::Ftp => ClientKind::Ftp,
        }
    }

    pub fn is_insecure(&self) -> bool {
        matches!(self, Self::Ftp)
    }
}

pub fn build_scp_args(
    profile: &Profile,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
) -> Vec<OsString> {
    let mut args = Vec::new();
    args.push(OsString::from("-P"));
    args.push(OsString::from(profile.port.to_string()));
    let remote = OsString::from(format!(
        "{}@{}:{}",
        profile.user, profile.host, remote_path
    ));
    match direction {
        TransferDirection::Push => {
            args.push(local_path.as_os_str().to_owned());
            args.push(remote);
        }
        TransferDirection::Pull => {
            args.push(remote);
            args.push(local_path.as_os_str().to_owned());
        }
    }
    args
}

pub fn build_sftp_args(profile: &Profile, batch_path: &Path) -> Vec<OsString> {
    vec![
        OsString::from("-P"),
        OsString::from(profile.port.to_string()),
        OsString::from("-b"),
        batch_path.as_os_str().to_owned(),
        OsString::from(format!("{}@{}", profile.user, profile.host)),
    ]
}

pub fn build_sftp_batch(
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
) -> String {
    let local = quote_sftp_arg(&local_path.to_string_lossy());
    let remote = quote_sftp_arg(remote_path);
    match direction {
        TransferDirection::Push => format!("put {local} {remote}\nquit\n"),
        TransferDirection::Pull => format!("get {remote} {local}\nquit\n"),
    }
}

fn quote_sftp_arg(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

pub struct TransferTempDir {
    path: PathBuf,
}

impl TransferTempDir {
    pub fn new(prefix: &str) -> Result<Self> {
        let mut base = env::temp_dir();
        base.push(format!("teradock-{prefix}-{}", now_ms()));
        std::fs::create_dir_all(&base)?;
        Ok(Self { path: base })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TransferTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
