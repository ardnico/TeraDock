use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tdcore::db;
use tdcore::doctor::{self, ClientKind, ClientOverrides};
use tdcore::oplog;
use tdcore::paths;
use tdcore::profile::{
    DangerLevel, NewProfile, Profile, ProfileFilters, ProfileStore, ProfileType,
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
    /// Connect to a profile (SSH/Telnet)
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

fn main() -> Result<()> {
    let _guard = init_logging()?;
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Profile { command }) => handle_profile(command),
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
        let json = serde_json::json!({
            "ok": ok,
            "exit_code": exit_code,
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "duration_ms": duration_ms,
            "parsed": serde_json::json!({}),
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
    let report = doctor::check_clients();
    let conn = db::init_connection()?;
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
    println!("Client discovery:");
    for client in report.clients {
        match client.path {
            Some(path) => println!("{:<8}: {}", client.name, path.display()),
            None => println!("{:<8}: MISSING", client.name),
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
        ProfileType::Serial => Err(anyhow!("serial connect not yet implemented")),
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
