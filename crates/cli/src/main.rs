use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tdcore::cmdset::{CmdSetStore, StepOnError};
use tdcore::configset::{ConfigFileWhen, ConfigSetStore, NewConfigFile, NewConfigSet};
use tdcore::db;
use tdcore::doctor::{self, ClientKind, ClientOverrides};
use tdcore::oplog;
use tdcore::parser::parse_output;
use tdcore::paths;
use tdcore::profile::{
    DangerLevel, NewProfile, Profile, ProfileFilters, ProfileStore, ProfileType, UpdateProfile,
};
use tdcore::secret::{NewSecret, SecretStore};
use tdcore::settings;
use tdcore::transfer::{
    build_scp_args, build_sftp_args, build_sftp_batch, TransferDirection, TransferTempDir,
    TransferVia,
};
use tdcore::util::now_ms;
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
    /// Manage config sets
    #[command(name = "configset")]
    ConfigSet {
        #[command(subcommand)]
        command: ConfigSetCommands,
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
    /// Execute a stored CommandSet over SSH
    Run {
        /// Profile ID to use
        profile_id: String,
        /// CommandSet ID to execute
        cmdset_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Connect to a profile (SSH/Telnet/Serial)
    Connect {
        /// Profile ID to connect to
        profile_id: String,
    },
    /// Upload a local file to a profile over SCP/SFTP
    Push(TransferArgs),
    /// Download a remote file from a profile over SCP/SFTP
    Pull(TransferArgs),
    /// Transfer a file between two profiles (pull -> local temp -> push)
    Xfer(XferArgs),
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

#[derive(Debug, Args)]
struct TransferArgs {
    /// Profile ID to transfer to/from
    profile_id: String,
    /// Local path (source for push, destination for pull)
    local_path: PathBuf,
    /// Remote path (destination for push, source for pull)
    remote_path: String,
    /// Transfer client (scp or sftp)
    #[arg(long, default_value = "scp")]
    via: String,
}

#[derive(Debug, Args)]
struct XferArgs {
    /// Source profile ID
    src_profile_id: String,
    /// Source remote path
    src_path: String,
    /// Destination profile ID
    dst_profile_id: String,
    /// Destination remote path
    dst_path: String,
    /// Transfer client (scp or sftp)
    #[arg(long, default_value = "scp")]
    via: String,
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
    /// Apply a config set to a profile
    Apply(ConfigApplyArgs),
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

#[derive(Debug, Subcommand)]
enum ConfigSetCommands {
    /// Add a config set
    Add(ConfigSetAddArgs),
    /// List config sets
    List,
    /// Show a config set in JSON
    Show { config_id: String },
    /// Remove a config set
    Rm { config_id: String },
}

#[derive(Debug, Args)]
struct ConfigSetAddArgs {
    /// Explicit config ID (auto-generated if omitted)
    #[arg(long)]
    config_id: Option<String>,
    #[arg(long)]
    name: String,
    #[arg(long)]
    hooks_cmdset_id: Option<String>,
    /// Config file spec: src=PATH,dest=PATH[,mode=MODE][,when=always|missing|changed]
    #[arg(long, action = ArgAction::Append)]
    file: Vec<String>,
}

#[derive(Debug, Args)]
struct ConfigApplyArgs {
    /// Profile ID to apply the config set to
    profile_id: String,
    /// Config set ID to apply
    config_id: String,
    /// Show planned changes without modifying files
    #[arg(long)]
    plan: bool,
    /// Backup existing remote files before applying
    #[arg(long)]
    backup: bool,
    /// Transfer client (scp or sftp)
    #[arg(long, default_value = "scp")]
    via: String,
}

fn main() -> Result<()> {
    let _guard = init_logging()?;
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Profile { command }) => handle_profile(command),
        Some(Commands::ConfigSet { command }) => handle_configset(command),
        Some(Commands::Config { command }) => handle_config(command),
        Some(Commands::Doctor { json }) => handle_doctor(json),
        Some(Commands::Exec {
            profile_id,
            timeout_ms,
            json,
            cmd,
        }) => handle_exec(profile_id, timeout_ms, json, cmd),
        Some(Commands::Run {
            profile_id,
            cmdset_id,
            json,
        }) => handle_run(profile_id, cmdset_id, json),
        Some(Commands::Connect { profile_id }) => handle_connect(profile_id),
        Some(Commands::Push(args)) => handle_push(args),
        Some(Commands::Pull(args)) => handle_pull(args),
        Some(Commands::Xfer(args)) => handle_xfer(args),
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
        ConfigCommands::Apply(args) => handle_config_apply(args),
    }
}

fn handle_configset(cmd: ConfigSetCommands) -> Result<()> {
    let mut store = ConfigSetStore::new(db::init_connection()?);
    match cmd {
        ConfigSetCommands::Add(args) => {
            let files = parse_config_file_specs(&args.file)?;
            if files.is_empty() {
                return Err(anyhow!("config set must include at least one --file entry"));
            }
            let created = store.insert(NewConfigSet {
                config_id: args.config_id,
                name: args.name,
                hooks_cmdset_id: args.hooks_cmdset_id,
                files,
            })?;
            info!("config set created: {}", created.config.config_id);
            println!("{}", created.config.config_id);
            Ok(())
        }
        ConfigSetCommands::List => {
            let sets = store.list()?;
            if sets.is_empty() {
                println!("(no config sets)");
                return Ok(());
            }
            for set in sets {
                let hooks = set.hooks_cmdset_id.as_deref().unwrap_or("-");
                println!("{:<16} {:<20} {}", set.config_id, set.name, hooks);
            }
            Ok(())
        }
        ConfigSetCommands::Show { config_id } => {
            match store.get(&config_id)? {
                Some(details) => {
                    let serialized = serde_json::to_string_pretty(&details)?;
                    println!("{serialized}");
                }
                None => return Err(anyhow!("config set not found: {config_id}")),
            }
            Ok(())
        }
        ConfigSetCommands::Rm { config_id } => {
            if store.delete(&config_id)? {
                info!("removed config set {}", config_id);
            } else {
                warn!("config set not found: {}", config_id);
            }
            Ok(())
        }
    }
}

fn handle_config_apply(args: ConfigApplyArgs) -> Result<()> {
    let profile_store = ProfileStore::new(db::init_connection()?);
    let config_store = ConfigSetStore::new(db::init_connection()?);
    let profile = profile_store
        .get(&args.profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.profile_id))?;
    ensure_ssh_profile(&profile, "config apply")?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    let config = config_store
        .get(&args.config_id)?
        .ok_or_else(|| anyhow!("config set not found: {}", args.config_id))?;

    let started = Instant::now();
    let via = TransferVia::from_str(&args.via)?;
    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &profile_store,
    )?;

    let needs_home = config
        .files
        .iter()
        .any(|file| file.dest.starts_with("~/"));
    let remote_home = if needs_home {
        Some(fetch_remote_home(&ssh, &profile)?)
    } else {
        None
    };

    let mut applied = 0usize;
    let mut skipped = 0usize;
    for file in &config.files {
        let local_path = PathBuf::from(&file.src);
        if !local_path.exists() {
            return Err(anyhow!("local file not found: {}", local_path.display()));
        }
        let dest = resolve_remote_dest(&file.dest, remote_home.as_deref())?;
        let status = remote_file_status(&ssh, &profile, &dest, file.when)?;
        let local_hash = if file.when == ConfigFileWhen::Changed {
            Some(sha256_file(&local_path)?)
        } else {
            None
        };
        let (should_apply, reason) = should_apply_config(file.when, status.exists, status.sha256.as_deref(), local_hash.as_deref());

        if args.plan {
            if should_apply {
                println!(
                    "PLAN apply: {} -> {} ({reason})",
                    local_path.display(),
                    dest
                );
                applied += 1;
            } else {
                println!(
                    "PLAN skip: {} -> {} ({reason})",
                    local_path.display(),
                    dest
                );
                skipped += 1;
            }
            continue;
        }

        if !should_apply {
            println!(
                "skip: {} -> {} ({reason})",
                local_path.display(),
                dest
            );
            skipped += 1;
            continue;
        }

        if args.backup && status.exists {
            let backup_path = format!("{dest}.bak.{}", now_ms());
            run_remote_command(
                &ssh,
                &profile,
                &format!(
                    "cp {} {}",
                    shell_quote(&dest),
                    shell_quote(&backup_path)
                ),
            )?;
        }

        let temp_path = format!("{dest}.tmp.{}", now_ms());
        let transfer = execute_transfer(
            &profile_store,
            &profile,
            TransferDirection::Push,
            &local_path,
            &temp_path,
            via,
        )?;
        if !transfer.ok {
            return Err(anyhow!(
                "config apply transfer failed with exit code {}",
                transfer.exit_code
            ));
        }
        run_remote_command(
            &ssh,
            &profile,
            &format!(
                "mv {} {}",
                shell_quote(&temp_path),
                shell_quote(&dest)
            ),
        )?;
        if let Some(mode) = &file.mode {
            run_remote_command(
                &ssh,
                &profile,
                &format!("chmod {} {}", mode, shell_quote(&dest)),
            )?;
        }
        println!("applied: {} -> {}", local_path.display(), dest);
        applied += 1;
    }

    profile_store.touch_last_used(&profile.profile_id)?;
    let meta_json = serde_json::json!({
        "config_id": config.config.config_id,
        "plan": args.plan,
        "backup": args.backup,
        "via": via.as_str(),
        "applied": applied,
        "skipped": skipped,
        "total": config.files.len(),
    });
    let entry = oplog::OpLogEntry {
        op: "config_apply".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(ssh.to_string_lossy().into_owned()),
        ok: true,
        exit_code: Some(0),
        duration_ms: Some(started.elapsed().as_millis() as i64),
        meta_json: Some(meta_json),
    };
    oplog::log_operation(profile_store.conn(), entry)?;
    Ok(())
}

fn parse_config_file_specs(specs: &[String]) -> Result<Vec<NewConfigFile>> {
    let mut files = Vec::new();
    for spec in specs {
        files.push(parse_config_file_spec(spec)?);
    }
    Ok(files)
}

fn parse_config_file_spec(spec: &str) -> Result<NewConfigFile> {
    let mut src = None;
    let mut dest = None;
    let mut mode = None;
    let mut when = ConfigFileWhen::Always;

    for pair in spec.split(',') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts
            .next()
            .ok_or_else(|| anyhow!("invalid file spec (missing '='): {spec}"))?
            .trim();
        match key {
            "src" => src = Some(value.to_string()),
            "dest" => dest = Some(value.to_string()),
            "mode" => mode = Some(value.to_string()),
            "when" => when = ConfigFileWhen::parse(value)?,
            _ => return Err(anyhow!("unknown file spec key: {key}")),
        }
    }

    let src = src.ok_or_else(|| anyhow!("file spec missing src: {spec}"))?;
    let dest = dest.ok_or_else(|| anyhow!("file spec missing dest: {spec}"))?;
    Ok(NewConfigFile {
        src,
        dest,
        mode,
        when,
    })
}

fn resolve_remote_dest(dest: &str, remote_home: Option<&str>) -> Result<String> {
    if dest.starts_with("~/") {
        let home = remote_home.ok_or_else(|| anyhow!("remote home not resolved"))?;
        let suffix = dest.trim_start_matches("~/");
        Ok(format!("{home}/{suffix}"))
    } else {
        Ok(dest.to_string())
    }
}

struct RemoteFileStatus {
    exists: bool,
    sha256: Option<String>,
}

fn remote_file_status(
    ssh: &Path,
    profile: &Profile,
    dest: &str,
    when: ConfigFileWhen,
) -> Result<RemoteFileStatus> {
    match when {
        ConfigFileWhen::Always => Ok(RemoteFileStatus {
            exists: remote_exists(ssh, profile, dest)?,
            sha256: None,
        }),
        ConfigFileWhen::Missing => Ok(RemoteFileStatus {
            exists: remote_exists(ssh, profile, dest)?,
            sha256: None,
        }),
        ConfigFileWhen::Changed => {
            let output = run_remote_command(
                ssh,
                profile,
                &format!(
                    "if test -f {path}; then sha256sum {path} | awk '{{print $1}}'; else echo MISSING; fi",
                    path = shell_quote(dest)
                ),
            )?;
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout == "MISSING" || stdout.is_empty() {
                Ok(RemoteFileStatus {
                    exists: false,
                    sha256: None,
                })
            } else {
                Ok(RemoteFileStatus {
                    exists: true,
                    sha256: Some(stdout),
                })
            }
        }
    }
}

fn remote_exists(ssh: &Path, profile: &Profile, dest: &str) -> Result<bool> {
    let output = run_remote_command(
        ssh,
        profile,
        &format!(
            "if test -f {path}; then echo EXISTS; else echo MISSING; fi",
            path = shell_quote(dest)
        ),
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match stdout.as_str() {
        "EXISTS" => Ok(true),
        "MISSING" => Ok(false),
        _ => Err(anyhow!("unexpected exists response: {stdout}")),
    }
}

fn should_apply_config(
    when: ConfigFileWhen,
    exists: bool,
    remote_hash: Option<&str>,
    local_hash: Option<&str>,
) -> (bool, &'static str) {
    match when {
        ConfigFileWhen::Always => (true, "always"),
        ConfigFileWhen::Missing => {
            if exists {
                (false, "exists")
            } else {
                (true, "missing")
            }
        }
        ConfigFileWhen::Changed => {
            if !exists {
                return (true, "missing");
            }
            match (remote_hash, local_hash) {
                (Some(remote), Some(local)) if remote != local => (true, "changed"),
                (Some(_), Some(_)) => (false, "unchanged"),
                _ => (true, "unknown"),
            }
        }
    }
}

fn fetch_remote_home(ssh: &Path, profile: &Profile) -> Result<String> {
    let output = run_remote_command(ssh, profile, "printf %s \"$HOME\"")?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.is_empty() {
        return Err(anyhow!("failed to resolve remote home"));
    }
    Ok(home)
}

fn run_remote_command(ssh: &Path, profile: &Profile, cmd: &str) -> Result<std::process::Output> {
    let output = Command::new(ssh)
        .arg("-p")
        .arg(profile.port.to_string())
        .arg(format!("{}@{}", profile.user, profile.host))
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to execute ssh")?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("remote command failed: {stderr}"))
    }
}

fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

fn sha256_file(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to execute sha256sum")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("sha256sum failed: {stderr}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hash = stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("sha256sum output missing hash"))?;
    Ok(hash.to_string())
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

fn handle_run(profile_id: String, cmdset_id: String, json_output: bool) -> Result<()> {
    let profile_store = ProfileStore::new(db::init_connection()?);
    let cmdset_store = CmdSetStore::new(db::init_connection()?);
    let profile = profile_store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if profile.profile_type != ProfileType::Ssh {
        return Err(anyhow!("run only supports SSH profiles for now"));
    }
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    if cmdset_store.get(&cmdset_id)?.is_none() {
        return Err(anyhow!("cmdset not found: {cmdset_id}"));
    }
    let steps = cmdset_store.list_steps(&cmdset_id)?;
    if steps.is_empty() {
        return Err(anyhow!("cmdset has no steps: {cmdset_id}"));
    }

    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &profile_store,
    )?;
    let run_started = Instant::now();
    let mut stdout_all = String::new();
    let mut stderr_all = String::new();
    let mut step_results = Vec::new();
    let mut overall_ok = true;
    let mut last_exit_code = 0;

    for step in steps {
        let mut command = Command::new(&ssh);
        command
            .arg("-p")
            .arg(profile.port.to_string())
            .arg(format!("{}@{}", profile.user, profile.host))
            .arg(&step.cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let step_started = Instant::now();
        let output = match step.timeout_ms {
            Some(ms) => run_with_timeout(command, Duration::from_millis(ms))
                .map_err(|e| anyhow!("step {} timed out after {ms}ms: {e}", step.ord))?,
            None => command.output().context("failed to execute ssh")?,
        };
        let duration_ms = step_started.elapsed().as_millis() as i64;
        let exit_code = output.status.code().unwrap_or_default();
        let ok = output.status.success();
        last_exit_code = exit_code;
        if !ok {
            overall_ok = false;
        }

        let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
        stdout_all.push_str(&stdout_text);
        stderr_all.push_str(&stderr_text);

        let parser_def = match &step.parser_spec {
            tdcore::parser::ParserSpec::Regex(id) => cmdset_store.get_parser(id)?,
            _ => None,
        };
        let parsed = parse_output(&step.parser_spec, &stdout_text, parser_def.as_ref())?;

        step_results.push(serde_json::json!({
            "ord": step.ord,
            "cmd": step.cmd,
            "ok": ok,
            "exit_code": exit_code,
            "stdout": stdout_text,
            "stderr": stderr_text,
            "duration_ms": duration_ms,
            "parsed": parsed,
        }));

        if !json_output {
            io::stdout().write_all(output.stdout.as_slice())?;
            io::stderr().write_all(output.stderr.as_slice())?;
        }

        if !ok && step.on_error == StepOnError::Stop {
            break;
        }
    }

    let duration_ms = run_started.elapsed().as_millis() as i64;
    profile_store.touch_last_used(&profile.profile_id)?;
    let meta_json = serde_json::json!({
        "cmdset_id": cmdset_id,
        "steps_executed": step_results.len(),
    });
    let entry = oplog::OpLogEntry {
        op: "run".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used: Some(ssh.to_string_lossy().into_owned()),
        ok: overall_ok,
        exit_code: Some(last_exit_code),
        duration_ms: Some(duration_ms),
        meta_json: Some(meta_json),
    };
    oplog::log_operation(profile_store.conn(), entry)?;

    if json_output {
        let json = serde_json::json!({
            "ok": overall_ok,
            "exit_code": last_exit_code,
            "stdout": stdout_all,
            "stderr": stderr_all,
            "duration_ms": duration_ms,
            "parsed": {
                "steps": step_results,
            }
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    if !overall_ok {
        return Err(anyhow!("run failed with exit code {last_exit_code}"));
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

fn handle_push(args: TransferArgs) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&args.profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.profile_id))?;
    ensure_ssh_profile(&profile, "push")?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    let via = TransferVia::from_str(&args.via)?;
    run_transfer_with_log(
        &store,
        &profile,
        TransferDirection::Push,
        &args.local_path,
        &args.remote_path,
        via,
        "push",
    )
}

fn handle_pull(args: TransferArgs) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&args.profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.profile_id))?;
    ensure_ssh_profile(&profile, "pull")?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    let via = TransferVia::from_str(&args.via)?;
    run_transfer_with_log(
        &store,
        &profile,
        TransferDirection::Pull,
        &args.local_path,
        &args.remote_path,
        via,
        "pull",
    )
}

fn handle_xfer(args: XferArgs) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let src_profile = store
        .get(&args.src_profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.src_profile_id))?;
    let dst_profile = store
        .get(&args.dst_profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.dst_profile_id))?;
    ensure_ssh_profile(&src_profile, "xfer")?;
    ensure_ssh_profile(&dst_profile, "xfer")?;
    if src_profile.danger_level == DangerLevel::Critical && !confirm_danger(&src_profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    if dst_profile.danger_level == DangerLevel::Critical && !confirm_danger(&dst_profile)? {
        println!("Aborted by user.");
        return Ok(());
    }

    let via = TransferVia::from_str(&args.via)?;
    let temp_dir = TransferTempDir::new("xfer")?;
    let temp_file = temp_dir
        .path()
        .join(filename_from_remote(&args.src_path));

    let started = Instant::now();
    let pull = execute_transfer(
        &store,
        &src_profile,
        TransferDirection::Pull,
        &temp_file,
        &args.src_path,
        via,
    )?;
    let mut push = None;
    let mut ok = pull.ok;
    let mut exit_code = pull.exit_code;

    if pull.ok {
        let push_outcome = execute_transfer(
            &store,
            &dst_profile,
            TransferDirection::Push,
            &temp_file,
            &args.dst_path,
            via,
        )?;
        ok = push_outcome.ok;
        exit_code = push_outcome.exit_code;
        push = Some(push_outcome);
    }

    store.touch_last_used(&src_profile.profile_id)?;
    store.touch_last_used(&dst_profile.profile_id)?;
    let duration_ms = started.elapsed().as_millis() as i64;
    let meta_json = serde_json::json!({
        "via": via.as_str(),
        "src_profile_id": src_profile.profile_id,
        "dst_profile_id": dst_profile.profile_id,
        "src_path": args.src_path,
        "dst_path": args.dst_path,
        "pull_exit_code": pull.exit_code,
        "push_exit_code": push.as_ref().map(|outcome| outcome.exit_code),
        "pull_client": pull.client_used.to_string_lossy().to_string(),
        "push_client": push
            .as_ref()
            .map(|outcome| outcome.client_used.to_string_lossy().to_string()),
    });
    let entry = oplog::OpLogEntry {
        op: "xfer".into(),
        profile_id: None,
        client_used: None,
        ok,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        meta_json: Some(meta_json),
    };
    oplog::log_operation(store.conn(), entry)?;

    if ok {
        Ok(())
    } else {
        Err(anyhow!("xfer failed with exit code {exit_code}"))
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

fn ensure_ssh_profile(profile: &Profile, op: &str) -> Result<()> {
    if profile.profile_type != ProfileType::Ssh {
        Err(anyhow!("{op} only supports SSH profiles for now"))
    } else {
        Ok(())
    }
}

struct TransferOutcome {
    ok: bool,
    exit_code: i32,
    duration_ms: i64,
    client_used: PathBuf,
}

fn run_transfer_with_log(
    store: &ProfileStore,
    profile: &Profile,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
    via: TransferVia,
    op: &str,
) -> Result<()> {
    let outcome = execute_transfer(store, profile, direction, local_path, remote_path, via)?;
    store.touch_last_used(&profile.profile_id)?;
    let meta_json = serde_json::json!({
        "via": via.as_str(),
        "direction": match direction {
            TransferDirection::Push => "push",
            TransferDirection::Pull => "pull",
        },
        "local_path": local_path.display().to_string(),
        "remote_path": remote_path,
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

fn execute_transfer(
    store: &ProfileStore,
    profile: &Profile,
    direction: TransferDirection,
    local_path: &Path,
    remote_path: &str,
    via: TransferVia,
) -> Result<TransferOutcome> {
    let client = resolve_client_for(
        via.client_kind(),
        profile.client_overrides.as_ref(),
        store,
    )?;
    let mut cmd = Command::new(&client);
    let _batch_guard: Option<TransferTempDir>;
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
    };
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
    })
}

fn filename_from_remote(remote_path: &str) -> PathBuf {
    Path::new(remote_path)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("transfer.bin"))
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
    fn parses_configset_add_with_file() {
        let cli = Cli::try_parse_from([
            "td",
            "configset",
            "add",
            "--name",
            "dotfiles",
            "--file",
            "src=./.bashrc,dest=~/.bashrc,mode=644,when=changed",
        ])
        .expect("parses configset add");

        match cli.command {
            Some(Commands::ConfigSet {
                command: ConfigSetCommands::Add(args),
            }) => {
                assert_eq!(args.name, "dotfiles");
                assert_eq!(args.file.len(), 1);
            }
            _ => panic!("expected configset add command"),
        }
    }

    #[test]
    fn parses_config_apply() {
        let cli = Cli::try_parse_from([
            "td",
            "config",
            "apply",
            "p1",
            "cfg_main",
            "--plan",
            "--backup",
            "--via",
            "sftp",
        ])
        .expect("parses config apply");

        match cli.command {
            Some(Commands::Config {
                command: ConfigCommands::Apply(args),
            }) => {
                assert_eq!(args.profile_id, "p1");
                assert_eq!(args.config_id, "cfg_main");
                assert!(args.plan);
                assert!(args.backup);
                assert_eq!(args.via, "sftp");
            }
            _ => panic!("expected config apply command"),
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

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from(["td", "run", "p1", "c_main", "--json"])
            .expect("parses run");

        match cli.command {
            Some(Commands::Run {
                profile_id,
                cmdset_id,
                json,
            }) => {
                assert_eq!(profile_id, "p1");
                assert_eq!(cmdset_id, "c_main");
                assert!(json);
            }
            _ => panic!("expected run command"),
        }
    }
}
