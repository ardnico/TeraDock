use anyhow::{anyhow, Context, Result};
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tdcore::oplog;
use tdcore::profile::{Profile, ProfileStore};
use tdcore::transfer::{
    build_scp_args, build_sftp_args, build_sftp_batch, TransferDirection, TransferTempDir,
    TransferVia,
};
use tracing::warn;

pub struct TransferOutcome {
    pub ok: bool,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub client_used: PathBuf,
    pub insecure: bool,
}

pub fn run_transfer_with_log(
    store: &ProfileStore,
    profile: &Profile,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
    via: TransferVia,
    client: PathBuf,
    auth_args: &[OsString],
    allow_insecure_transfers: bool,
    insecure_flag: bool,
    op: &str,
) -> Result<()> {
    let outcome = execute_transfer(
        profile,
        direction,
        local_path,
        remote_path,
        via,
        client,
        auth_args,
        allow_insecure_transfers,
        insecure_flag,
    )?;
    store.touch_last_used(&profile.profile_id)?;
    let meta_json = serde_json::json!({
        "via": via.as_str(),
        "direction": match direction {
            TransferDirection::Push => "push",
            TransferDirection::Pull => "pull",
        },
        "local_path": local_path.display().to_string(),
        "remote_path": remote_path,
        "insecure": outcome.insecure,
    });
    let entry = oplog::OpLogEntry {
        op: op.into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(outcome.client_used.to_string_lossy().into_owned()),
        ok: outcome.ok,
        exit_code: Some(outcome.exit_code),
        duration_ms: Some(outcome.duration_ms),
        meta_json: Some(meta_json),
    };
    oplog::log_operation(store.conn(), entry)?;
    if outcome.ok {
        Ok(())
    } else {
        Err(anyhow!("{op} failed with exit code {}", outcome.exit_code))
    }
}

pub fn execute_transfer(
    profile: &Profile,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
    via: TransferVia,
    client: PathBuf,
    auth_args: &[OsString],
    allow_insecure_transfers: bool,
    insecure_flag: bool,
) -> Result<TransferOutcome> {
    ensure_insecure_allowed(via, allow_insecure_transfers, insecure_flag)?;

    let mut cmd = Command::new(&client);
    let mut stdin_file: Option<File> = None;
    let _batch_guard: Option<TransferTempDir>;
    let insecure = via.is_insecure();
    if insecure {
        warn!(
            "insecure ftp transfer approved for {} -> {}",
            profile.profile_id, remote_path
        );
    }
    let args = match via {
        TransferVia::Scp => {
            _batch_guard = None;
            build_scp_args(profile, direction, local_path, remote_path)
        }
        TransferVia::Sftp => {
            let batch_dir = TransferTempDir::new("sftp-batch")?;
            let batch_path = batch_dir.path().join("batch.txt");
            let batch_contents = build_sftp_batch(direction, local_path, remote_path);
            std::fs::write(&batch_path, batch_contents)?;
            _batch_guard = Some(batch_dir);
            build_sftp_args(profile, &batch_path)
        }
        TransferVia::Ftp => {
            let batch_dir = TransferTempDir::new("ftp-batch")?;
            let batch_path = batch_dir.path().join("batch.txt");
            let password = env::var("TD_FTP_PASSWORD").unwrap_or_default();
            if password.is_empty() {
                warn!("TD_FTP_PASSWORD is not set; FTP login may fail unless anonymous access is enabled.");
            }
            let batch_contents =
                build_ftp_batch(profile, &password, direction, local_path, remote_path);
            std::fs::write(&batch_path, batch_contents)?;
            stdin_file = Some(File::open(&batch_path)?);
            _batch_guard = Some(batch_dir);
            build_ftp_args(profile)
        }
    };

    if matches!(via, TransferVia::Scp | TransferVia::Sftp) {
        cmd.args(auth_args);
    }
    if let Some(file) = stdin_file {
        cmd.stdin(file);
    }
    cmd.args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let started = Instant::now();
    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {}", via.as_str()))?;
    let duration_ms = started.elapsed().as_millis() as i64;
    let exit_code = status.code().unwrap_or_default();
    Ok(TransferOutcome {
        ok: status.success(),
        exit_code,
        duration_ms,
        client_used: client,
        insecure,
    })
}

pub fn ensure_insecure_allowed(
    via: TransferVia,
    allow_insecure_transfers: bool,
    insecure_flag: bool,
) -> Result<()> {
    if via.is_insecure() && (!allow_insecure_transfers || !insecure_flag) {
        return Err(anyhow!(
            "ftp transfers are disabled; set allow_insecure_transfers=true and pass --i-know-its-insecure"
        ));
    }
    Ok(())
}

fn build_ftp_args(profile: &Profile) -> Vec<OsString> {
    vec![
        OsString::from("-i"),
        OsString::from("-n"),
        OsString::from("-v"),
        OsString::from(profile.host.clone()),
        OsString::from(profile.port.to_string()),
    ]
}

fn build_ftp_batch(
    profile: &Profile,
    password: &str,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
) -> String {
    let local = quote_ftp_arg(&local_path.to_string_lossy());
    let remote = quote_ftp_arg(remote_path);
    let user = quote_ftp_arg(&profile.user);
    let pass = quote_ftp_arg(password);
    let transfer = match direction {
        TransferDirection::Push => format!("put {local} {remote}"),
        TransferDirection::Pull => format!("get {remote} {local}"),
    };
    format!("user {user} {pass}\nbinary\n{transfer}\nquit\n",)
}

fn quote_ftp_arg(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
