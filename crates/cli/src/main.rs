use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tdcore::db;
use tdcore::doctor::{self, ClientKind, ClientOverrides};
use tdcore::oplog;
use tdcore::paths;
use tdcore::profile::{
    DangerLevel, NewProfile, Profile, ProfileFilters, ProfileStore, ProfileType, UpdateProfile,
};
use tdcore::secret::{NewSecret, SecretStore};
use tdcore::settings;
use wait_timeout::ChildExt;
use tracing::{info, warn};
use tracing_subscriber::prelude::*;
use zeroize::Zeroizing;

#[derive(Debug, Parser)]
#[command(author, version, about = "TeraDock CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },
    /// Manage global configuration (client overrides)
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Check environment and required clients
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Execute a non-interactive command over SSH
    Exec {
        /// Profile ID to use
        profile_id: String,
        /// Timeout in milliseconds
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Command to execute (pass after --)
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Connect to a profile (SSH/Telnet/Serial)
    Connect {
        /// Profile ID to connect to
        profile_id: String,
    },
    /// Manage secrets (master password required for reveal)
    Secret {
        #[command(subcommand)]
        command: SecretCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ProfileCommands {
    /// Add a profile
    Add(ProfileAddArgs),
    /// Edit an existing profile
    Edit(ProfileEditArgs),
    /// List profiles
    List(ProfileListArgs),
    /// Show a profile in JSON
    Show { profile_id: String },
    /// Remove a profile
    Rm { profile_id: String },
}

#[derive(Debug, Args)]
struct ProfileAddArgs {
    /// Explicit profile ID (auto-generated if omitted)
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    name: String,
    #[arg(long)]
    host: String,
    #[arg(long)]
    user: String,
    #[arg(long, default_value_t = 22)]
    port: u16,
    #[arg(long, default_value = "ssh")]
    r#type: String,
    #[arg(long, default_value = "normal")]
    danger: String,
    #[arg(long)]
    group: Option<String>,
    #[arg(long, action = ArgAction::Append, value_delimiter = ',')]
    tag: Vec<String>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    client_overrides_json: Option<String>,
}

#[derive(Debug, Args)]
struct ProfileEditArgs {
    /// Profile ID to edit
    profile_id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    user: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    r#type: Option<String>,
    #[arg(long)]
    danger: Option<String>,
    #[arg(long)]
    group: Option<String>,
    #[arg(long)]
    clear_group: bool,
    #[arg(long, value_delimiter = ',')]
    tags: Option<Vec<String>>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    clear_note: bool,
    #[arg(long)]
    client_overrides_json: Option<String>,
    #[arg(long)]
    clear_client_overrides: bool,
}

#[derive(Debug, Args)]
struct ProfileListArgs {
    /// Filter by group
    #[arg(long)]
    group: Option<String>,
    /// Filter by tag (comma-delimited, AND match)
    #[arg(long, action = ArgAction::Append, value_delimiter = ',')]
    tag: Vec<String>,
    /// Filter by profile type (ssh/telnet/serial)
    #[arg(long)]
    r#type: Option<String>,
    /// Filter by danger level (normal/high/critical)
    #[arg(long)]
    danger: Option<String>,
    /// Free-text query over id/name/host/user
    #[arg(long)]
    query: Option<String>,
}

#[derive(Debug, Subcommand)]
enum SecretCommands {
    /// Set the master password (one-time)
    SetMaster,
    /// Add a secret (requires master password)
    Add(SecretAddArgs),
    /// List secrets (metadata only)
    List,
    /// Reveal a secret value (requires master password)
    Reveal { secret_id: String },
    /// Remove a secret
    Rm { secret_id: String },
}

#[derive(Debug, Args)]
struct SecretAddArgs {
    /// Explicit secret ID (auto-generated if omitted)
    #[arg(long)]
    secret_id: Option<String>,
    #[arg(long)]
    kind: String,
    #[arg(long)]
    label: String,
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    /// Set or clear global client overrides (ssh/scp/sftp/telnet)
    SetClient(ClientOverrideArgs),
    /// Show current global client overrides
    ShowClient,
    /// Clear all global client overrides
    ClearClient,
}

#[derive(Debug, Args, Default)]
struct ClientOverrideArgs {
    /// Override ssh client path
    #[arg(long)]
    ssh: Option<String>,
    /// Override scp client path
    #[arg(long)]
    scp: Option<String>,
    /// Override sftp client path
    #[arg(long)]
    sftp: Option<String>,
    /// Override telnet client path
    #[arg(long)]
    telnet: Option<String>,
    /// Clear all overrides before applying provided values
    #[arg(long)]
    clear_all: bool,
}

fn main() -> Result<()> {
    let _guard = init_logging()?;
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Profile { command }) => handle_profile(command),
        Some(Commands::Config { command }) => handle_config(command),
        Some(Commands::Doctor { json }) => handle_doctor(json),
        Some(Commands::Exec {
            profile_id,
            timeout_ms,
            json,
            cmd,
        }) => handle_exec(profile_id, timeout_ms, json, cmd),
        Some(Commands::Connect { profile_id }) => handle_connect(profile_id),
        Some(Commands::Secret { command }) => handle_secret(command),
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn handle_profile(cmd: ProfileCommands) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    match cmd {
        ProfileCommands::Add(args) => {
            let profile_type = parse_profile_type(&args.r#type)?;
            let danger = parse_danger(&args.danger)?;
            let overrides = parse_client_overrides(args.client_overrides_json)?;
            let created = store.insert(NewProfile {
                profile_id: args.profile_id,
                name: args.name,
                profile_type,
                host: args.host,
                port: args.port,
                user: args.user,
                danger_level: danger,
                group: args.group,
                tags: args.tag,
                note: args.note,
                client_overrides: overrides,
            })?;
            info!("profile created: {}", created.profile_id);
            println!("{}", created.profile_id);
            Ok(())
        }
        ProfileCommands::Edit(args) => {
            let profile_type = match args.r#type {
                Some(ref t) => Some(parse_profile_type(t)?),
                None => None,
            };
            let danger = match args.danger {
                Some(ref d) => Some(parse_danger(d)?),
                None => None,
            };
            let overrides = if args.clear_client_overrides {
                Some(None)
            } else {
                parse_client_overrides(args.client_overrides_json)?.map(Some)
            };
            let group = if args.clear_group {
                Some(None)
            } else {
                args.group.map(Some)
            };
            let note = if args.clear_note {
                Some(None)
            } else {
                args.note.map(Some)
            };
            let updated = store.update(
                &args.profile_id,
                UpdateProfile {
                    name: args.name,
                    profile_type,
                    host: args.host,
                    port: args.port,
                    user: args.user,
                    danger_level: danger,
                    group,
                    tags: args.tags,
                    note,
                    client_overrides: overrides,
                },
            )?;
            info!("profile updated: {}", updated.profile_id);
            println!("{}", updated.profile_id);
            Ok(())
        }
        ProfileCommands::List(args) => {
            let profile_type = match args.r#type {
                Some(ref t) => Some(parse_profile_type(t)?),
                None => None,
            };
            let danger = match args.danger {
                Some(ref d) => Some(parse_danger(d)?),
                None => None,
            };
            let filters = ProfileFilters {
                group: args.group,
                tags: args.tag,
                profile_type,
                danger,
                query: args.query,
            };
            let profiles = store.list_filtered(&filters)?;
            if profiles.is_empty() {
                println!("(no profiles)");
                return Ok(());
            }
            for p in profiles {
                println!(
                    "{:<16} {:<10} {:<5} {:<15} {:<12} {:<8} {}",
                    p.profile_id, p.name, p.profile_type, p.host, p.user, p.port, p.danger_level
                );
            }
            Ok(())
        }
        ProfileCommands::Show { profile_id } => {
            match store.get(&profile_id)? {
                Some(profile) => {
                    let serialized = serde_json::to_string_pretty(&profile)?;
                    println!("{serialized}");
                }
                None => return Err(anyhow!("profile not found: {profile_id}")),
            }
            Ok(())
        }
        ProfileCommands::Rm { profile_id } => {
            if store.delete(&profile_id)? {
                info!("removed profile {}", profile_id);
            } else {
                warn!("profile not found: {}", profile_id);
            }
            Ok(())
        }
    }
}

fn handle_config(cmd: ConfigCommands) -> Result<()> {
    let conn = db::init_connection()?;
    match cmd {
        ConfigCommands::SetClient(args) => {
            let mut overrides = if args.clear_all {
                ClientOverrides::default()
            } else {
                settings::get_client_overrides(&conn)?.unwrap_or_default()
            };
            if let Some(path) = args.ssh {
                overrides.ssh = Some(path);
            }
            if let Some(path) = args.scp {
                overrides.scp = Some(path);
            }
            if let Some(path) = args.sftp {
                overrides.sftp = Some(path);
            }
            if let Some(path) = args.telnet {
                overrides.telnet = Some(path);
            }
            settings::set_client_overrides(&conn, &overrides)?;
            info!("updated client overrides");
            println!(
                "{}",
                serde_json::to_string_pretty(&overrides).unwrap_or_else(|_| "{}".into())
            );
            Ok(())
        }
        ConfigCommands::ShowClient => {
            let overrides = settings::get_client_overrides(&conn)?.unwrap_or_default();
            println!(
                "{}",
                serde_json::to_string_pretty(&overrides).unwrap_or_else(|_| "{}".into())
            );
            Ok(())
        }
        ConfigCommands::ClearClient => {
            settings::clear_client_overrides(&conn)?;
            info!("cleared client overrides");
            println!("client overrides cleared");
            Ok(())
        }
    }
}

fn handle_exec(
    profile_id: String,
    timeout_ms: Option<u64>,
    json_output: bool,
    cmd: Vec<String>,
) -> Result<()> {
    if cmd.is_empty() {
        return Err(anyhow!("no command provided; pass after --"));
    }
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if profile.profile_type != ProfileType::Ssh {
        return Err(anyhow!("exec only supports SSH profiles for now"));
    }
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }

    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &store,
    )?;
    let mut command = Command::new(&ssh);
    command
        .arg("-p")
        .arg(profile.port.to_string())
        .arg(format!("{}@{}", profile.user, profile.host))
        .args(&cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let started = Instant::now();
    let output = match timeout_ms {
        Some(ms) => run_with_timeout(command, Duration::from_millis(ms))
            .map_err(|e| anyhow!("exec timed out after {ms}ms: {e}"))?,
        None => command.output().context("failed to execute ssh")?,
    };
    let duration_ms = started.elapsed().as_millis() as i64;
    let exit_code = output.status.code().unwrap_or_default();
    let ok = output.status.success();

    store.touch_last_used(&profile.profile_id)?;
    let entry = oplog::OpLogEntry {
        op: "exec".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(ssh.to_string_lossy().into_owned()),
        ok,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        meta_json: None,
    };
    oplog::log_operation(store.conn(), entry)?;

    if json_output {
        let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .unwrap_or_else(|_| serde_json::json!({}));
        let json = serde_json::json!({
            "ok": ok,
            "exit_code": exit_code,
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "duration_ms": duration_ms,
            "parsed": parsed,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        if !ok {
            return Err(anyhow!("ssh exited with code {exit_code}"));
        }
    }
    Ok(())
}

fn handle_doctor(json: bool) -> Result<()> {
    let conn = db::init_connection()?;
    let global_overrides = settings::get_client_overrides(&conn)?;
    let report = doctor::check_clients_with_overrides(None, global_overrides.as_ref());
    let meta_json = serde_json::to_value(&report)?;
    let entry = oplog::OpLogEntry {
        op: "doctor".into(),
        profile_id: None,
        client_used: None,
        ok: true,
        exit_code: None,
        duration_ms: None,
        meta_json: Some(meta_json),
    };
    oplog::log_operation(&conn, entry)?;
    if json {
        let serialized = serde_json::to_string_pretty(&report)?;
        println!("{serialized}");
        return Ok(());
    }
    let note = if global_overrides.is_some() {
        " (global overrides applied)"
    } else {
        ""
    };
    println!("Client discovery{note}:");
    for client in report.clients {
        match client.path {
            Some(path) => println!(
                "{:<8}: {} [{}]",
                client.name,
                path.display(),
                client.source
            ),
            None => println!(
                "{:<8}: MISSING [{}]",
                client.name,
                client.source
            ),
        }
    }
    Ok(())
}

fn handle_connect(profile_id: String) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }

    let overrides = profile.client_overrides.clone();
    match profile.profile_type {
        ProfileType::Ssh => {
            let ssh = resolve_client_for(ClientKind::Ssh, overrides.as_ref(), &store)?;
            connect_ssh(&store, profile, ssh)
        }
        ProfileType::Telnet => {
            let telnet = resolve_client_for(ClientKind::Telnet, overrides.as_ref(), &store)?;
            connect_telnet(&store, profile, telnet)
        }
        ProfileType::Serial => connect_serial(&store, profile),
    }
}

fn connect_ssh(store: &ProfileStore, profile: Profile, ssh: PathBuf) -> Result<()> {
    let mut cmd = Command::new(&ssh);
    cmd.arg("-p")
        .arg(profile.port.to_string())
        .arg(format!("{}@{}", profile.user, profile.host));
    let started = Instant::now();
    let status = cmd.status().context("failed to launch ssh")?;
    let duration_ms = started.elapsed().as_millis() as i64;
    let ok = status.success();
    let exit_code = status.code().unwrap_or_default();
    store.touch_last_used(&profile.profile_id)?;
    let entry = oplog::OpLogEntry {
        op: "connect".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(ssh.to_string_lossy().into_owned()),
        ok,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        meta_json: None,
    };
    oplog::log_operation(store.conn(), entry)?;
    if ok {
        Ok(())
    } else {
        Err(anyhow!("ssh exited with code {}", exit_code))
    }
}

fn connect_telnet(store: &ProfileStore, profile: Profile, telnet: PathBuf) -> Result<()> {
    let mut cmd = Command::new(&telnet);
    cmd.arg(&profile.host).arg(profile.port.to_string());
    let started = Instant::now();
    let status = cmd.status().context("failed to launch telnet")?;
    let duration_ms = started.elapsed().as_millis() as i64;
    let ok = status.success();
    let exit_code = status.code().unwrap_or_default();
    store.touch_last_used(&profile.profile_id)?;
    let entry = oplog::OpLogEntry {
        op: "connect".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(telnet.to_string_lossy().into_owned()),
        ok,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        meta_json: None,
    };
    oplog::log_operation(store.conn(), entry)?;
    if ok {
        Ok(())
    } else {
        Err(anyhow!("telnet exited with code {}", exit_code))
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn connect_serial(store: &ProfileStore, profile: Profile) -> Result<()> {
    let port_name = profile.host.clone();
    let baud_rate = profile.port as u32;
    let mut port = serialport::new(&port_name, baud_rate)
        .timeout(Duration::from_millis(20))
        .open()
        .with_context(|| format!("failed to open serial port {port_name} at {baud_rate}"))?;
    let started = Instant::now();
    let result = run_serial_session(&mut port);
    let duration_ms = started.elapsed().as_millis() as i64;
    store.touch_last_used(&profile.profile_id)?;
    let entry = oplog::OpLogEntry {
        op: "connect".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(format!("serialport:{port_name}")),
        ok: result.is_ok(),
        exit_code: None,
        duration_ms: Some(duration_ms),
        meta_json: None,
    };
    oplog::log_operation(store.conn(), entry)?;
    result
}

fn run_serial_session(port: &mut Box<dyn serialport::SerialPort>) -> Result<()> {
    let _raw = RawModeGuard::enter()?;
    let running = Arc::new(AtomicBool::new(true));
    let mut port_reader = port.try_clone().context("failed to clone serial port")?;
    let running_reader = Arc::clone(&running);
    let reader = thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut buffer = [0u8; 1024];
        while running_reader.load(Ordering::Relaxed) {
            match port_reader.read(&mut buffer) {
                Ok(0) => {}
                Ok(n) => {
                    if stdout.write_all(&buffer[..n]).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
                Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
    });
    let mut stdin = io::stdin();
    let mut buffer = [0u8; 1024];
    let mut result = Ok(());
    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(err) = port.write_all(&buffer[..n]).and_then(|_| port.flush()) {
                    result = Err(err.into());
                    break;
                }
            }
            Err(err) => {
                result = Err(err.into());
                break;
            }
        }
    }
    running.store(false, Ordering::Relaxed);
    let _ = reader.join();
    result
}

fn handle_secret(cmd: SecretCommands) -> Result<()> {
    let store = SecretStore::new(db::init_connection()?);
    match cmd {
        SecretCommands::SetMaster => {
            if store.is_master_set()? {
                return Err(anyhow!("master password already set"));
            }
            let first = prompt_password("Enter new master password: ")?;
            let second = prompt_password("Confirm master password: ")?;
            if first != second {
                return Err(anyhow!("passwords did not match"));
            }
            store.set_master(&first)?;
            info!("master password set");
            Ok(())
        }
        SecretCommands::Add(args) => {
            let master = load_master_prompt(&store)?;
            let value = prompt_password("Secret value (input hidden): ")?;
            let created = store.add(
                &master,
                NewSecret {
                    secret_id: args.secret_id,
                    kind: args.kind,
                    label: args.label,
                    value: Zeroizing::new(value),
                    meta: None,
                },
            )?;
            println!("{}", created.secret_id);
            Ok(())
        }
        SecretCommands::List => {
            let secrets = store.list()?;
            if secrets.is_empty() {
                println!("(no secrets)");
                return Ok(());
            }
            for s in secrets {
                println!(
                    "{:<16} {:<12} {:<20} created:{} updated:{}",
                    s.secret_id, s.kind, s.label, s.created_at, s.updated_at
                );
            }
            Ok(())
        }
        SecretCommands::Reveal { secret_id } => {
            let master = load_master_prompt(&store)?;
            let value = store.reveal(&master, &secret_id)?;
            println!("{value}");
            Ok(())
        }
        SecretCommands::Rm { secret_id } => {
            if store.delete(&secret_id)? {
                info!("removed secret {}", secret_id);
            } else {
                warn!("secret not found: {}", secret_id);
            }
            Ok(())
        }
    }
}

fn parse_profile_type(value: &str) -> Result<ProfileType> {
    match value.to_lowercase().as_str() {
        "ssh" => Ok(ProfileType::Ssh),
        "telnet" => Ok(ProfileType::Telnet),
        "serial" => Ok(ProfileType::Serial),
        _ => Err(anyhow!("invalid profile type: {value}")),
    }
}

fn parse_danger(value: &str) -> Result<DangerLevel> {
    match value.to_lowercase().as_str() {
        "normal" => Ok(DangerLevel::Normal),
        "high" => Ok(DangerLevel::High),
        "critical" => Ok(DangerLevel::Critical),
        _ => Err(anyhow!("invalid danger level: {value}")),
    }
}

fn parse_client_overrides(raw: Option<String>) -> Result<Option<ClientOverrides>> {
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

fn resolve_client_for(
    kind: ClientKind,
    profile_overrides: Option<&ClientOverrides>,
    store: &ProfileStore,
) -> Result<PathBuf> {
    let global_overrides = settings::get_client_overrides(store.conn())?;
    doctor::resolve_client_with_overrides(kind, profile_overrides, global_overrides.as_ref())
        .ok_or_else(|| anyhow!("{} client not found via overrides or PATH", kind.as_str()))
}

fn confirm_danger(profile: &Profile) -> Result<bool> {
    println!(
        "Profile '{}' is marked critical. Proceed with connect to {}@{}:{} ?",
        profile.profile_id, profile.user, profile.host, profile.port
    );
    print!("Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("yes"))
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<std::process::Output> {
    let mut child = cmd.spawn().context("failed to spawn command")?;
    let status = child
        .wait_timeout(timeout)
        .context("failed waiting for command")?;
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(anyhow!("timeout after {}ms", timeout.as_millis()));
    }
    child
        .wait_with_output()
        .context("failed to collect command output")
}

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let logs_dir = paths::logs_dir()?;
    let file_appender = tracing_appender::rolling::never(logs_dir, "teradock.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(non_blocking);
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .context("failed to initialize logging")?;

    Ok(guard)
}

fn prompt_password(prompt: &str) -> Result<String> {
    let pw = rpassword::prompt_password(prompt)?;
    Ok(pw)
}

fn load_master_prompt(store: &SecretStore) -> Result<tdcore::crypto::MasterKey> {
    let password = prompt_password("Master password: ")?;
    let master = store.load_master(&password)?;
    Ok(master)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_profile_add_with_defaults_and_tags() {
        let cli = Cli::try_parse_from([
            "td",
            "profile",
            "add",
            "--name",
            "demo",
            "--host",
            "example.com",
            "--user",
            "alice",
            "--tag",
            "a,b",
            "--tag",
            "c",
        ])
        .expect("parses profile add");

        match cli.command {
            Some(Commands::Profile {
                command: ProfileCommands::Add(args),
            }) => {
                assert_eq!(args.name, "demo");
                assert_eq!(args.host, "example.com");
                assert_eq!(args.user, "alice");
                assert_eq!(args.port, 22);
                assert_eq!(args.r#type, "ssh");
                assert_eq!(args.danger, "normal");
                assert_eq!(
                    args.tag,
                    vec!["a".to_string(), "b".to_string(), "c".to_string()]
                );
            }
            _ => panic!("expected profile add command"),
        }
    }

    #[test]
    fn parses_secret_add_minimal() {
        let cli =
            Cli::try_parse_from(["td", "secret", "add", "--kind", "password", "--label", "db"])
                .expect("parses secret add");

        match cli.command {
            Some(Commands::Secret {
                command: SecretCommands::Add(args),
            }) => {
                assert_eq!(args.kind, "password");
                assert_eq!(args.label, "db");
                assert!(args.secret_id.is_none());
            }
            _ => panic!("expected secret add command"),
        }
    }

    #[test]
    fn parse_helpers_validate_known_values() {
        assert!(parse_profile_type("ssh").is_ok());
        assert!(parse_profile_type("bogus").is_err());
        assert!(parse_danger("critical").is_ok());
        assert!(parse_danger("unknown").is_err());
    }

    #[test]
    fn parses_profile_edit_with_clears() {
        let cli = Cli::try_parse_from([
            "td",
            "profile",
            "edit",
            "p1",
            "--name",
            "new",
            "--host",
            "example.net",
            "--clear-group",
            "--clear-note",
            "--clear-client-overrides",
            "--tags",
            "alpha,beta",
        ])
        .expect("parses profile edit");

        match cli.command {
            Some(Commands::Profile {
                command: ProfileCommands::Edit(args),
            }) => {
                assert_eq!(args.profile_id, "p1");
                assert_eq!(args.name.as_deref(), Some("new"));
                assert_eq!(args.host.as_deref(), Some("example.net"));
                assert!(args.clear_group);
                assert!(args.clear_note);
                assert!(args.clear_client_overrides);
                assert_eq!(
                    args.tags,
                    Some(vec!["alpha".to_string(), "beta".to_string()])
                );
            }
            _ => panic!("expected profile edit command"),
        }
    }

    #[test]
    fn parses_config_set_client() {
        let cli = Cli::try_parse_from([
            "td",
            "config",
            "set-client",
            "--ssh",
            "/usr/bin/ssh",
            "--clear-all",
        ])
        .expect("parses config set-client");

        match cli.command {
            Some(Commands::Config {
                command: ConfigCommands::SetClient(args),
            }) => {
                assert_eq!(args.ssh.as_deref(), Some("/usr/bin/ssh"));
                assert!(args.clear_all);
            }
            _ => panic!("expected config set-client command"),
        }
    }

    #[test]
    fn parses_exec_command() {
        let cli = Cli::try_parse_from([
            "td",
            "exec",
            "p1",
            "--timeout-ms",
            "5000",
            "--json",
            "--",
            "echo",
            "hello",
        ])
        .expect("parses exec");

        match cli.command {
            Some(Commands::Exec {
                profile_id,
                timeout_ms,
                json,
                cmd,
            }) => {
                assert_eq!(profile_id, "p1");
                assert_eq!(timeout_ms, Some(5000));
                assert!(json);
                assert_eq!(cmd, vec!["echo".to_string(), "hello".to_string()]);
            }
            _ => panic!("expected exec command"),
        }
    }
}
