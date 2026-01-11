use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use directories::BaseDirs;
use rusqlite::Connection;
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tdcore::agent;
use tdcore::cmdset::{CmdSetStore, StepOnError};
use tdcore::configset::{ConfigFileWhen, ConfigSetStore, NewConfigFile, NewConfigSet};
use tdcore::db;
use tdcore::doctor::{self, ClientKind, ClientOverrides};
use tdcore::import_export::{self, ConflictStrategy, ExportDocument, ImportReport};
use tdcore::oplog;
use tdcore::parser::parse_output;
use tdcore::paths;
use tdcore::profile::{
    DangerLevel, NewProfile, Profile, ProfileFilters, ProfileStore, ProfileType, UpdateProfile,
};
use tdcore::secret::{NewSecret, SecretStore};
use tdcore::settings;
use tdcore::settings::SettingScope;
use tdcore::settings_registry;
use tdcore::tester::{self, SshBatchCommand, TestOptions};
use tdcore::tunnel::{Forward, ForwardKind, ForwardStore, NewSession, SessionKind, SessionStore};
use tdcore::transfer::{TransferDirection, TransferTempDir, TransferVia};
use tdcore::util::now_ms;
use tracing::{info, warn};
use tracing_subscriber::prelude::*;
use wait_timeout::ChildExt;
use zeroize::Zeroizing;

mod transfer;

use crate::transfer::{ensure_insecure_allowed, execute_transfer, run_transfer_with_log};

const INITIAL_SEND_DELAY: Duration = Duration::from_millis(300);

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
    /// Manage configuration (client overrides, settings)
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Manage environment presets
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },
    /// Inspect and manage SSH agent keys
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
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
        /// One-time string to send right after connect (overrides profile)
        #[arg(long)]
        initial_send: Option<String>,
    },
    /// Manage SSH tunnels
    Tunnel {
        #[command(subcommand)]
        command: TunnelCommands,
    },
    /// Test connectivity to a profile
    Test {
        /// Profile ID to test
        profile_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Include SSH BatchMode auth probe (SSH profiles only)
        #[arg(long)]
        ssh: bool,
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
    /// Export profiles, command sets, configs, and secrets metadata as JSON
    Export(ExportArgs),
    /// Import profiles, command sets, configs, and secrets metadata from JSON
    Import(ImportArgs),
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
    initial_send: Option<String>,
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
    initial_send: Option<String>,
    #[arg(long)]
    clear_initial_send: bool,
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
    /// Transfer client (scp, sftp, or ftp)
    #[arg(long, default_value = "scp")]
    via: String,
    /// Acknowledge FTP is insecure when using --via ftp
    #[arg(long)]
    i_know_its_insecure: bool,
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
    /// Transfer client (scp, sftp, or ftp)
    #[arg(long, default_value = "scp")]
    via: String,
    /// Acknowledge FTP is insecure when using --via ftp
    #[arg(long)]
    i_know_its_insecure: bool,
}

#[derive(Debug, Subcommand)]
enum TunnelCommands {
    /// Start a tunnel for a profile
    Start(TunnelStartArgs),
    /// Stop a tunnel session
    Stop {
        /// Session ID to stop
        session_id: String,
    },
    /// Show tunnel session status
    Status(TunnelStatusArgs),
}

#[derive(Debug, Args)]
struct TunnelStartArgs {
    /// Profile ID to use
    profile_id: String,
    /// Forward name to apply (repeatable)
    #[arg(long = "forward")]
    forward: Vec<String>,
}

#[derive(Debug, Args)]
struct TunnelStatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
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
    /// Show the configuration schema
    Schema(ConfigSchemaArgs),
    /// List available configuration keys
    Keys,
    /// Get a configuration value
    Get(ConfigGetArgs),
    /// Set a configuration value
    Set(ConfigSetArgs),
    /// Set or clear global client overrides (ssh/scp/sftp/ftp/telnet)
    SetClient(ClientOverrideArgs),
    /// Show current global client overrides
    ShowClient,
    /// Clear all global client overrides
    ClearClient,
    /// Set the SSH authentication order (agent/keys/password)
    SetAuthOrder(AuthOrderArgs),
    /// Show the SSH authentication order
    ShowAuthOrder,
    /// Clear the SSH authentication order
    ClearAuthOrder,
    /// Apply a config set to a profile
    Apply(ConfigApplyArgs),
}

#[derive(Debug, Subcommand)]
enum EnvCommands {
    /// List available env presets
    List,
    /// Set the current env preset
    Use { name: String },
    /// Show settings for an env preset
    Show { name: String },
    /// Set a configuration value in an env preset (NAME.KEY VALUE)
    Set(EnvSetArgs),
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// Show SSH agent status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List keys loaded in ssh-agent
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a key to ssh-agent
    Add { key_path: PathBuf },
    /// Remove all keys from ssh-agent
    Clear,
}

#[derive(Debug, Args)]
struct EnvSetArgs {
    /// Env setting key in the form NAME.KEY
    name_key: String,
    /// Setting value
    value: String,
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
    /// Override ftp client path
    #[arg(long)]
    ftp: Option<String>,
    /// Override telnet client path
    #[arg(long)]
    telnet: Option<String>,
    /// Clear all overrides before applying provided values
    #[arg(long)]
    clear_all: bool,
}

#[derive(Debug, Args)]
struct AuthOrderArgs {
    /// Preferred SSH auth order (comma-delimited: agent,keys,password)
    #[arg(long, value_delimiter = ',')]
    order: Vec<SshAuthMethod>,
}

#[derive(Debug, Args)]
struct ConfigSchemaArgs {
    /// Optional setting key to show schema for
    key: Option<String>,
}

#[derive(Debug, Args)]
struct ConfigGetArgs {
    /// Setting key
    key: String,
    /// Setting scope (global, env:NAME, or profile:ID)
    #[arg(long, default_value = "global")]
    scope: String,
    /// Resolve the value from the scope and fall back to global if unset
    #[arg(long)]
    resolved: bool,
}

#[derive(Debug, Args)]
#[command(disable_help_flag = true)]
struct ConfigSetArgs {
    /// Setting key
    key: Option<String>,
    /// Setting value
    value: Option<String>,
    /// Setting scope (global, env:NAME, or profile:ID)
    #[arg(long, default_value = "global")]
    scope: String,
    /// Resolve the value after setting (falls back to global if unset)
    #[arg(long)]
    resolved: bool,
    /// Show schema for the provided key
    #[arg(long)]
    help: bool,
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
    /// Transfer client (scp, sftp, or ftp)
    #[arg(long, default_value = "scp")]
    via: String,
    /// Acknowledge FTP is insecure when using --via ftp
    #[arg(long)]
    i_know_its_insecure: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
enum SshAuthMethod {
    Agent,
    Keys,
    Password,
}

impl SshAuthMethod {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Keys => "keys",
            Self::Password => "password",
        }
    }
}

#[derive(Debug, Args)]
struct ExportArgs {
    /// Include decrypted secret values in the export
    #[arg(long)]
    include_secrets: bool,
    /// Write output to a file instead of stdout
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ImportArgs {
    /// Conflict strategy for name collisions (reject or rename)
    #[arg(long, default_value = "reject")]
    conflict: ConflictArg,
    /// Path to an export JSON file (reads stdin if omitted)
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ConflictArg {
    Reject,
    Rename,
}

fn main() -> Result<()> {
    let _guard = init_logging()?;
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Profile { command }) => handle_profile(command),
        Some(Commands::ConfigSet { command }) => handle_configset(command),
        Some(Commands::Config { command }) => handle_config(command),
        Some(Commands::Env { command }) => handle_env(command),
        Some(Commands::Agent { command }) => handle_agent(command),
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
        Some(Commands::Connect {
            profile_id,
            initial_send,
        }) => handle_connect(profile_id, initial_send),
        Some(Commands::Tunnel { command }) => handle_tunnel(command),
        Some(Commands::Test {
            profile_id,
            json,
            ssh,
        }) => handle_test(profile_id, json, ssh),
        Some(Commands::Push(args)) => handle_push(args),
        Some(Commands::Pull(args)) => handle_pull(args),
        Some(Commands::Xfer(args)) => handle_xfer(args),
        Some(Commands::Secret { command }) => handle_secret(command),
        Some(Commands::Export(args)) => handle_export(args),
        Some(Commands::Import(args)) => handle_import(args),
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn handle_export(args: ExportArgs) -> Result<()> {
    let master = if args.include_secrets {
        let store = SecretStore::new(db::init_connection()?);
        Some(load_master_prompt(&store)?)
    } else {
        None
    };
    let conn = db::init_connection()?;
    let json = import_export::export_to_json(&conn, args.include_secrets, master.as_ref())?;
    if let Some(path) = args.output {
        std::fs::write(&path, json)?;
        info!("export written to {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn handle_import(args: ImportArgs) -> Result<()> {
    let json = read_import_payload(args.path.as_deref())?;
    let document: ExportDocument = serde_json::from_str(&json)?;
    let needs_master = document.secrets.iter().any(|secret| secret.value.is_some());
    let master = if needs_master {
        let store = SecretStore::new(db::init_connection()?);
        Some(load_master_prompt(&store)?)
    } else {
        None
    };
    let mut conn = db::init_connection()?;
    let report = import_export::import_document(
        &mut conn,
        document,
        match args.conflict {
            ConflictArg::Reject => ConflictStrategy::Reject,
            ConflictArg::Rename => ConflictStrategy::Rename,
        },
        master.as_ref(),
    )?;
    print_import_report(&report);
    Ok(())
}

fn read_import_payload(path: Option<&Path>) -> Result<String> {
    if let Some(path) = path {
        return Ok(std::fs::read_to_string(path)?);
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

fn print_import_report(report: &ImportReport) {
    println!(
        "imported: profiles={}, cmdsets={}, parsers={}, configs={}, secrets={}, secrets_skipped={}",
        report.profiles,
        report.cmdsets,
        report.parsers,
        report.configs,
        report.secrets,
        report.secrets_skipped
    );
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
                initial_send: args.initial_send,
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
            let initial_send = if args.clear_initial_send {
                Some(None)
            } else {
                args.initial_send.map(Some)
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
                    initial_send,
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
        ConfigCommands::Schema(args) => handle_config_schema(args),
        ConfigCommands::Keys => handle_config_keys(),
        ConfigCommands::Get(args) => handle_config_get(&conn, args),
        ConfigCommands::Set(args) => handle_config_set(&conn, args),
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
            if let Some(path) = args.ftp {
                overrides.ftp = Some(path);
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
        ConfigCommands::SetAuthOrder(args) => {
            let order = normalize_auth_order(args.order)?;
            let serialized = format_auth_order(&order);
            settings::set_ssh_auth_order(&conn, &serialized)?;
            info!("updated ssh auth order");
            println!("{serialized}");
            Ok(())
        }
        ConfigCommands::ShowAuthOrder => {
            let order = load_ssh_auth_order(&conn)?;
            println!("{}", format_auth_order(&order));
            Ok(())
        }
        ConfigCommands::ClearAuthOrder => {
            settings::clear_ssh_auth_order(&conn)?;
            info!("cleared ssh auth order");
            println!("ssh auth order cleared");
            Ok(())
        }
        ConfigCommands::Apply(args) => handle_config_apply(args),
    }
}

fn handle_env(cmd: EnvCommands) -> Result<()> {
    let conn = db::init_connection()?;
    match cmd {
        EnvCommands::List => {
            let mut envs = settings::list_env_names(&conn)?;
            let current = settings::get_current_env(&conn)?;
            if let Some(current_name) = current.as_ref() {
                if !envs.iter().any(|name| name == current_name) {
                    envs.push(current_name.clone());
                }
            }
            envs.sort();
            if envs.is_empty() {
                println!("(no envs)");
                return Ok(());
            }
            for env_name in envs {
                if current.as_deref() == Some(env_name.as_str()) {
                    println!("* {env_name}");
                } else {
                    println!("  {env_name}");
                }
            }
            Ok(())
        }
        EnvCommands::Use { name } => {
            let name = normalize_env_name(&name)?;
            settings::set_current_env(&conn, &name)?;
            println!("current env: {name}");
            Ok(())
        }
        EnvCommands::Show { name } => {
            let name = normalize_env_name(&name)?;
            let scope = SettingScope::Env(name);
            let entries = settings::list_settings_scoped(&conn, &scope)?;
            if entries.is_empty() {
                println!("(no settings)");
                return Ok(());
            }
            for (key, value) in entries {
                println!("{key}={value}");
            }
            Ok(())
        }
        EnvCommands::Set(args) => {
            let (env_name, key) = parse_env_key(&args.name_key)?;
            ensure_known_setting(&key)?;
            ensure_scope_supported(&key, settings::SettingScopeKind::Env)?;
            let normalized = match settings_registry::validate_setting_value(&key, &args.value) {
                Ok(normalized) => normalized,
                Err(err) => {
                    let schema = schema_output_for_key(&key)?;
                    return Err(anyhow!("invalid value for '{key}': {err}\n\n{schema}"));
                }
            };
            let scope = SettingScope::Env(env_name.clone());
            settings::set_setting_scoped(&conn, &scope, &key, &normalized)?;
            println!("{env_name}.{key}={normalized}");
            Ok(())
        }
    }
}

fn handle_agent(cmd: AgentCommands) -> Result<()> {
    match cmd {
        AgentCommands::Status { json } => {
            let status = agent::status();
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
                return Ok(());
            }
            if let Some(sock) = &status.auth_sock {
                println!("SSH_AUTH_SOCK: {sock}");
            } else {
                println!("SSH_AUTH_SOCK: (not set)");
            }
            if let Some(count) = status.key_count {
                println!("keys: {count}");
            } else {
                println!("keys: (unknown)");
            }
            if let Some(error) = &status.error {
                println!("ssh-add: {error}");
            }
            Ok(())
        }
        AgentCommands::List { json } => {
            let list = agent::list();
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            if let Some(error) = list.error {
                println!("{error}");
                return Ok(());
            }
            if list.keys.is_empty() {
                println!("(no keys loaded)");
                return Ok(());
            }
            for key in list.keys {
                println!("{key}");
            }
            Ok(())
        }
        AgentCommands::Add { key_path } => {
            ensure_agent_socket_available()?;
            if !confirm_agent_add(&key_path)? {
                println!("aborted");
                return Ok(());
            }
            let output = agent::run_add(&key_path)?;
            handle_ssh_add_output(output, "ssh-add add")?;
            println!("ssh-add: key added");
            Ok(())
        }
        AgentCommands::Clear => {
            ensure_agent_socket_available()?;
            if !confirm_agent_clear()? {
                println!("aborted");
                return Ok(());
            }
            let output = agent::run_clear()?;
            handle_ssh_add_output(output, "ssh-add clear")?;
            println!("ssh-add: keys cleared");
            Ok(())
        }
    }
}

fn handle_config_schema(args: ConfigSchemaArgs) -> Result<()> {
    let output = if let Some(key) = args.key {
        schema_output_for_key(&key)?
    } else {
        let schemas = settings_registry::list_schemas();
        serde_json::to_string_pretty(&schemas)?
    };
    println!("{output}");
    Ok(())
}

fn handle_config_keys() -> Result<()> {
    for key in settings_registry::list_keys() {
        println!("{key}");
    }
    Ok(())
}

fn handle_config_get(conn: &Connection, args: ConfigGetArgs) -> Result<()> {
    ensure_known_setting(&args.key)?;
    let scope = SettingScope::parse(&args.scope)
        .map_err(|err| anyhow!("invalid scope '{}': {err}", args.scope))?;
    ensure_scope_supported(&args.key, scope.kind())?;
    let value = if args.resolved {
        settings::get_setting_resolved(conn, &scope, &args.key)?
    } else {
        settings::get_setting_scoped(conn, &scope, &args.key)?
    };
    if let Some(value) = value {
        println!("{}={}", args.key, value);
    } else {
        println!("{}=", args.key);
    }
    Ok(())
}

fn handle_config_set(conn: &Connection, args: ConfigSetArgs) -> Result<()> {
    if args.help {
        if let Some(key) = args.key.as_deref() {
            println!("{}", schema_output_for_key(key)?);
        } else {
            let schemas = settings_registry::list_schemas();
            println!("{}", serde_json::to_string_pretty(&schemas)?);
        }
        return Ok(());
    }
    let key = args
        .key
        .ok_or_else(|| anyhow!("missing config key (use --help for schema)"))?;
    ensure_known_setting(&key)?;
    let value = args
        .value
        .ok_or_else(|| anyhow!("missing value for config key '{key}'"))?;
    let scope = SettingScope::parse(&args.scope)
        .map_err(|err| anyhow!("invalid scope '{}': {err}", args.scope))?;
    ensure_scope_supported(&key, scope.kind())?;
    let normalized = match settings_registry::validate_setting_value(&key, &value) {
        Ok(normalized) => normalized,
        Err(err) => {
            let schema = schema_output_for_key(&key)?;
            return Err(anyhow!("invalid value for '{key}': {err}\n\n{schema}"));
        }
    };
    settings::set_setting_scoped(conn, &scope, &key, &normalized)?;
    let output_value = if args.resolved {
        settings::get_setting_resolved(conn, &scope, &key)?.unwrap_or(normalized)
    } else {
        normalized
    };
    println!("{key}={output_value}");
    Ok(())
}

fn ensure_known_setting(key: &str) -> Result<()> {
    if settings_registry::schema_for_key(key).is_none() {
        return Err(anyhow!(
            "unknown config key: {key}\nknown keys: {}",
            settings_registry::list_keys().join(", ")
        ));
    }
    Ok(())
}

fn ensure_scope_supported(key: &str, scope: settings::SettingScopeKind) -> Result<()> {
    if !settings_registry::scope_supported(key, scope)? {
        return Err(anyhow!(
            "config key '{key}' does not support {} scope",
            format_scope_kind(scope)
        ));
    }
    Ok(())
}

fn format_scope_kind(scope: settings::SettingScopeKind) -> &'static str {
    match scope {
        settings::SettingScopeKind::Global => "global",
        settings::SettingScopeKind::Env => "env",
        settings::SettingScopeKind::Profile => "profile",
    }
}

fn schema_output_for_key(key: &str) -> Result<String> {
    let schema = settings_registry::schema_for_key(key)
        .ok_or_else(|| anyhow!("unknown config key: {key}"))?;
    Ok(serde_json::to_string_pretty(schema)?)
}

fn normalize_env_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("env name cannot be empty"));
    }
    Ok(trimmed.to_string())
}

fn parse_env_key(raw: &str) -> Result<(String, String)> {
    let mut parts = raw.splitn(2, '.');
    let env = parts.next().unwrap_or("").trim();
    let key = parts.next().unwrap_or("").trim();
    if env.is_empty() || key.is_empty() {
        return Err(anyhow!("env setting must be in the form NAME.KEY"));
    }
    Ok((env.to_string(), key.to_string()))
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
    let allow_insecure_transfers = settings::get_allow_insecure_transfers(profile_store.conn())?;
    ensure_insecure_allowed(via, allow_insecure_transfers, args.i_know_its_insecure)?;
    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &profile_store,
    )?;
    let transfer_client = resolve_client_for(
        via.client_kind(),
        profile.client_overrides.as_ref(),
        &profile_store,
    )?;
    let auth = ssh_auth_context(profile_store.conn())?;
    emit_ssh_auth_messages(&auth);

    let needs_home = config.files.iter().any(|file| file.dest.starts_with("~/"));
    let remote_home = if needs_home {
        Some(fetch_remote_home(&ssh, &profile, &auth)?)
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
        let status = remote_file_status(&ssh, &profile, &auth, &dest, file.when)?;
        let local_hash = if file.when == ConfigFileWhen::Changed {
            Some(sha256_file(&local_path)?)
        } else {
            None
        };
        let (should_apply, reason) = should_apply_config(
            file.when,
            status.exists,
            status.sha256.as_deref(),
            local_hash.as_deref(),
        );

        if args.plan {
            if should_apply {
                println!(
                    "PLAN apply: {} -> {} ({reason})",
                    local_path.display(),
                    dest
                );
                applied += 1;
            } else {
                println!("PLAN skip: {} -> {} ({reason})", local_path.display(), dest);
                skipped += 1;
            }
            continue;
        }

        if !should_apply {
            println!("skip: {} -> {} ({reason})", local_path.display(), dest);
            skipped += 1;
            continue;
        }

        if args.backup && status.exists {
            let backup_path = format!("{dest}.bak.{}", now_ms());
            run_remote_command(
                &ssh,
                &profile,
                &auth,
                &format!("cp {} {}", shell_quote(&dest), shell_quote(&backup_path)),
            )?;
        }

        let temp_path = format!("{dest}.tmp.{}", now_ms());
        let transfer = execute_transfer(
            &profile,
            TransferDirection::Push,
            &local_path,
            &temp_path,
            via,
            transfer_client.clone(),
            &auth.args,
            allow_insecure_transfers,
            args.i_know_its_insecure,
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
            &auth,
            &format!("mv {} {}", shell_quote(&temp_path), shell_quote(&dest)),
        )?;
        if let Some(mode) = &file.mode {
            run_remote_command(
                &ssh,
                &profile,
                &auth,
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
        "insecure": via.is_insecure(),
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

struct SshAuthAvailability {
    agent: bool,
    keys: bool,
}

struct SshAuthContext {
    order: Vec<SshAuthMethod>,
    args: Vec<OsString>,
    hint: Option<String>,
    warn_password_fallback: bool,
}

fn normalize_auth_order(order: Vec<SshAuthMethod>) -> Result<Vec<SshAuthMethod>> {
    if order.is_empty() {
        return Err(anyhow!("auth order cannot be empty"));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for method in order {
        if !seen.insert(method) {
            return Err(anyhow!(
                "auth order contains duplicate '{}'",
                method.as_str()
            ));
        }
        normalized.push(method);
    }
    Ok(normalized)
}

fn parse_auth_order_setting(raw: &str) -> Result<Vec<SshAuthMethod>> {
    if raw.trim().is_empty() {
        return Err(anyhow!("auth order setting is empty"));
    }
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for item in raw.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let method = match trimmed {
            "agent" => SshAuthMethod::Agent,
            "keys" => SshAuthMethod::Keys,
            "password" => SshAuthMethod::Password,
            _ => return Err(anyhow!("unknown auth method '{trimmed}'")),
        };
        if !seen.insert(method) {
            return Err(anyhow!("auth order contains duplicate '{trimmed}'"));
        }
        order.push(method);
    }
    if order.is_empty() {
        return Err(anyhow!("auth order setting is empty"));
    }
    Ok(order)
}

fn format_auth_order(order: &[SshAuthMethod]) -> String {
    order
        .iter()
        .map(SshAuthMethod::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn default_auth_order() -> Vec<SshAuthMethod> {
    vec![
        SshAuthMethod::Agent,
        SshAuthMethod::Keys,
        SshAuthMethod::Password,
    ]
}

fn load_ssh_auth_order(conn: &Connection) -> Result<Vec<SshAuthMethod>> {
    match settings::get_ssh_auth_order(conn)? {
        Some(raw) => parse_auth_order_setting(&raw)
            .map_err(|err| anyhow!("invalid ssh auth order setting: {err}")),
        None => Ok(default_auth_order()),
    }
}

fn detect_ssh_auth_availability() -> SshAuthAvailability {
    let agent = env::var_os("SSH_AUTH_SOCK")
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    let keys = if let Some(dirs) = BaseDirs::new() {
        let ssh_dir = dirs.home_dir().join(".ssh");
        [
            "id_ed25519",
            "id_rsa",
            "id_ecdsa",
            "id_ed25519_sk",
            "id_ecdsa_sk",
            "id_dsa",
            "identity",
        ]
        .iter()
        .any(|name| ssh_dir.join(name).exists())
    } else {
        false
    };
    SshAuthAvailability { agent, keys }
}

fn build_ssh_auth_args(
    order: &[SshAuthMethod],
    availability: &SshAuthAvailability,
) -> Vec<OsString> {
    let mut preferred = Vec::new();
    let mut publickey_added = false;
    for method in order {
        match method {
            SshAuthMethod::Agent | SshAuthMethod::Keys => {
                if !publickey_added {
                    preferred.push("publickey");
                    publickey_added = true;
                }
            }
            SshAuthMethod::Password => {
                preferred.push("keyboard-interactive");
                preferred.push("password");
            }
        }
    }
    let mut args = Vec::new();
    if !preferred.is_empty() {
        args.push(OsString::from("-o"));
        args.push(OsString::from(format!(
            "PreferredAuthentications={}",
            preferred.join(",")
        )));
    }
    if !availability.agent || !order.contains(&SshAuthMethod::Agent) {
        args.push(OsString::from("-o"));
        args.push(OsString::from("IdentityAgent=none"));
    }
    args
}

fn is_auth_method_available(method: SshAuthMethod, availability: &SshAuthAvailability) -> bool {
    match method {
        SshAuthMethod::Agent => availability.agent,
        SshAuthMethod::Keys => availability.keys,
        SshAuthMethod::Password => true,
    }
}

fn ssh_auth_context(conn: &Connection) -> Result<SshAuthContext> {
    let order = load_ssh_auth_order(conn)?;
    let availability = detect_ssh_auth_availability();
    let args = build_ssh_auth_args(&order, &availability);
    let hint = match order.first().copied() {
        Some(SshAuthMethod::Agent) if !availability.agent => Some(
            "Hint: SSH auth order prefers agent; start ssh-agent or set SSH_AUTH_SOCK to avoid password prompts.".to_string(),
        ),
        Some(SshAuthMethod::Keys) if !availability.keys => Some(
            "Hint: SSH auth order prefers keys; add a key under ~/.ssh or update the auth order.".to_string(),
        ),
        _ => None,
    };
    let first_available = order
        .iter()
        .copied()
        .find(|method| is_auth_method_available(*method, &availability));
    let warn_password_fallback = matches!(first_available, Some(SshAuthMethod::Password))
        && order.first().copied() != Some(SshAuthMethod::Password);
    Ok(SshAuthContext {
        order,
        args,
        hint,
        warn_password_fallback,
    })
}

fn emit_ssh_auth_messages(auth: &SshAuthContext) {
    if let Some(hint) = &auth.hint {
        eprintln!("{hint}");
    }
    if auth.warn_password_fallback {
        eprintln!(
            "Warning: falling back to password auth (order: {}).",
            format_auth_order(&auth.order)
        );
    }
}

struct RemoteFileStatus {
    exists: bool,
    sha256: Option<String>,
}

fn remote_file_status(
    ssh: &Path,
    profile: &Profile,
    auth: &SshAuthContext,
    dest: &str,
    when: ConfigFileWhen,
) -> Result<RemoteFileStatus> {
    match when {
        ConfigFileWhen::Always => Ok(RemoteFileStatus {
            exists: remote_exists(ssh, profile, auth, dest)?,
            sha256: None,
        }),
        ConfigFileWhen::Missing => Ok(RemoteFileStatus {
            exists: remote_exists(ssh, profile, auth, dest)?,
            sha256: None,
        }),
        ConfigFileWhen::Changed => {
            let output = run_remote_command(
                ssh,
                profile,
                auth,
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

fn remote_exists(ssh: &Path, profile: &Profile, auth: &SshAuthContext, dest: &str) -> Result<bool> {
    let output = run_remote_command(
        ssh,
        profile,
        auth,
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

fn fetch_remote_home(ssh: &Path, profile: &Profile, auth: &SshAuthContext) -> Result<String> {
    let output = run_remote_command(ssh, profile, auth, "printf %s \"$HOME\"")?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.is_empty() {
        return Err(anyhow!("failed to resolve remote home"));
    }
    Ok(home)
}

fn run_remote_command(
    ssh: &Path,
    profile: &Profile,
    auth: &SshAuthContext,
    cmd: &str,
) -> Result<std::process::Output> {
    let output = Command::new(ssh)
        .arg("-p")
        .arg(profile.port.to_string())
        .args(&auth.args)
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

    let ssh = resolve_client_for(ClientKind::Ssh, profile.client_overrides.as_ref(), &store)?;
    let auth = ssh_auth_context(store.conn())?;
    emit_ssh_auth_messages(&auth);
    let mut command = Command::new(&ssh);
    command
        .arg("-p")
        .arg(profile.port.to_string())
        .args(&auth.args)
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
    let auth = ssh_auth_context(profile_store.conn())?;
    emit_ssh_auth_messages(&auth);
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
            .args(&auth.args)
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
    if let Some(sock) = &report.agent.auth_sock {
        println!("SSH agent: {sock}");
    } else {
        println!("SSH agent: (not set)");
    }
    if let Some(count) = report.agent.key_count {
        println!("SSH agent keys: {count}");
    } else {
        println!("SSH agent keys: (unknown)");
    }
    if let Some(error) = &report.agent.error {
        println!("SSH agent error: {error}");
    }
    let note = if global_overrides.is_some() {
        " (global overrides applied)"
    } else {
        ""
    };
    println!("Client discovery{note}:");
    for client in report.clients {
        match client.path {
            Some(path) => println!("{:<8}: {} [{}]", client.name, path.display(), client.source),
            None => println!("{:<8}: MISSING [{}]", client.name, client.source),
        }
    }
    if !report.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in report.warnings {
            println!("- {} ({})", warning.message, warning.code);
        }
    }
    if !report.errors.is_empty() {
        println!();
        println!("Errors:");
        for error in report.errors {
            println!("- {} ({})", error.message, error.code);
        }
    }
    Ok(())
}

fn handle_connect(profile_id: String, initial_send: Option<String>) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    let initial_send = initial_send.or_else(|| profile.initial_send.clone());

    let overrides = profile.client_overrides.clone();
    match profile.profile_type {
        ProfileType::Ssh => {
            let ssh = resolve_client_for(ClientKind::Ssh, overrides.as_ref(), &store)?;
            let auth = ssh_auth_context(store.conn())?;
            emit_ssh_auth_messages(&auth);
            connect_ssh(&store, profile, ssh, &auth)
        }
        ProfileType::Telnet => {
            let telnet = resolve_client_for(ClientKind::Telnet, overrides.as_ref(), &store)?;
            connect_telnet(&store, profile, telnet, initial_send)
        }
        ProfileType::Serial => connect_serial(&store, profile, initial_send),
    }
}

fn handle_tunnel(command: TunnelCommands) -> Result<()> {
    match command {
        TunnelCommands::Start(args) => handle_tunnel_start(args),
        TunnelCommands::Stop { session_id } => handle_tunnel_stop(session_id),
        TunnelCommands::Status(args) => handle_tunnel_status(args),
    }
}

fn handle_tunnel_start(args: TunnelStartArgs) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&args.profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.profile_id))?;
    ensure_ssh_profile(&profile, "tunnel start")?;

    let forward_store = ForwardStore::new(store.conn().try_clone()?);
    let forwards = if args.forward.is_empty() {
        forward_store.list_for_profile(&profile.profile_id)?
    } else {
        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        for name in args.forward {
            if !seen.insert(name.clone()) {
                continue;
            }
            let forward = forward_store
                .get_by_name(&profile.profile_id, &name)?
                .ok_or_else(|| anyhow!("forward not found: {name}"))?;
            selected.push(forward);
        }
        selected
    };
    if forwards.is_empty() {
        return Err(anyhow!(
            "no forwards defined for profile {}",
            profile.profile_id
        ));
    }

    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &store,
    )?;
    let auth = ssh_auth_context(store.conn())?;
    emit_ssh_auth_messages(&auth);

    let mut command = Command::new(&ssh);
    command
        .arg("-N")
        .arg("-p")
        .arg(profile.port.to_string())
        .args(&auth.args);
    for forward in &forwards {
        append_forward(&mut command, forward)?;
    }
    command
        .arg(format!("{}@{}", profile.user, profile.host))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = command.spawn().context("failed to start ssh tunnel")?;
    let pid = child.id();
    drop(child);

    store.touch_last_used(&profile.profile_id)?;
    let session_store = SessionStore::new(store.conn().try_clone()?);
    let session = session_store.insert(NewSession {
        kind: SessionKind::Tunnel,
        profile_id: profile.profile_id.clone(),
        pid: Some(pid),
        forwards: forwards.iter().map(|forward| forward.name.clone()).collect(),
    })?;
    println!("tunnel started: {} (pid {})", session.session_id, pid);
    Ok(())
}

fn handle_tunnel_stop(session_id: String) -> Result<()> {
    let store = SessionStore::new(db::init_connection()?);
    let session = store
        .get(&session_id)?
        .ok_or_else(|| anyhow!("session not found: {session_id}"))?;
    if let Some(pid) = session.pid {
        terminate_pid(pid).with_context(|| format!("failed to stop pid {pid}"))?;
    }
    store.remove(&session_id)?;
    println!("tunnel stopped: {session_id}");
    Ok(())
}

fn handle_tunnel_status(args: TunnelStatusArgs) -> Result<()> {
    let store = SessionStore::new(db::init_connection()?);
    let cleaned = store.cleanup_dead()?;
    let sessions = store.list()?;

    if args.json {
        let json = serde_json::json!({
            "sessions": sessions.iter().map(session_json).collect::<Vec<_>>(),
            "cleaned": cleaned.iter().map(session_json).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    if !cleaned.is_empty() {
        println!("cleaned {} dead sessions", cleaned.len());
    }
    if sessions.is_empty() {
        println!("no active tunnel sessions");
        return Ok(());
    }
    for session in sessions {
        let pid = session
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "{}\tprofile={}\tpid={}\tforwards={}",
            session.session_id,
            session.profile_id,
            pid,
            session.forwards.join(",")
        );
    }
    Ok(())
}

fn append_forward(command: &mut Command, forward: &Forward) -> Result<()> {
    match forward.kind {
        ForwardKind::Local | ForwardKind::Remote => {
            let dest = forward
                .dest
                .as_ref()
                .ok_or_else(|| anyhow!("forward {} is missing destination", forward.name))?;
            command
                .arg(forward.kind.as_flag())
                .arg(format!("{}:{}", forward.listen, dest));
        }
        ForwardKind::Dynamic => {
            command.arg(forward.kind.as_flag()).arg(&forward.listen);
        }
    }
    Ok(())
}

fn terminate_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .context("failed to execute kill")?;
        if !status.success() {
            return Err(anyhow!("kill failed for pid {pid}"));
        }
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .status()
            .context("failed to execute taskkill")?;
        if !status.success() {
            return Err(anyhow!("taskkill failed for pid {pid}"));
        }
    }
    Ok(())
}

fn session_json(session: &tdcore::tunnel::Session) -> serde_json::Value {
    serde_json::json!({
        "session_id": session.session_id,
        "kind": session.kind.to_string(),
        "profile_id": session.profile_id,
        "pid": session.pid,
        "started_at": session.started_at,
        "forwards": session.forwards,
    })
}

fn handle_test(profile_id: String, json: bool, ssh: bool) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if !tester::is_network_profile(&profile) {
        return Err(anyhow!("test only supports ssh/telnet profiles for now"));
    }
    if ssh && profile.profile_type != ProfileType::Ssh {
        return Err(anyhow!("--ssh is only supported for SSH profiles"));
    }

    let mut options = TestOptions::default();
    let mut client_used = None;
    if ssh {
        let ssh_path =
            resolve_client_for(ClientKind::Ssh, profile.client_overrides.as_ref(), &store)?;
        let auth = ssh_auth_context(store.conn())?;
        emit_ssh_auth_messages(&auth);
        client_used = Some(ssh_path.to_string_lossy().into_owned());
        options = options.with_ssh(SshBatchCommand::new(
            ssh_path,
            profile.user.clone(),
            profile.host.clone(),
            profile.port,
            auth.args,
            Duration::from_secs(5),
        ));
    }

    let report = tester::run_profile_test(&profile, &options);
    store.touch_last_used(&profile.profile_id)?;
    let meta_json = serde_json::to_value(&report)?;
    let entry = oplog::OpLogEntry {
        op: "test".into(),
        profile_id: Some(profile.profile_id.clone()),
        client_used,
        ok: report.ok,
        exit_code: report.ssh_exit_code(),
        duration_ms: Some(report.duration_ms),
        meta_json: Some(meta_json),
    };
    oplog::log_operation(store.conn(), entry)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!(
        "Test results for {} ({}): {}:{}",
        report.profile_id, report.profile_type, report.host, report.port
    );
    for check in &report.checks {
        let status = if check.skipped {
            "SKIPPED"
        } else if check.ok {
            "OK"
        } else {
            "FAIL"
        };
        let duration = check
            .duration_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "-".to_string());
        let detail = check.detail.as_deref().unwrap_or("");
        if detail.is_empty() {
            println!("{:<8} {:<7} ({duration})", check.name, status);
        } else {
            println!("{:<8} {:<7} ({duration}) {}", check.name, status, detail);
        }
    }
    if report.ok {
        Ok(())
    } else {
        Err(anyhow!("test failed"))
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
    let allow_insecure_transfers = settings::get_allow_insecure_transfers(store.conn())?;
    let auth = ssh_auth_context(store.conn())?;
    emit_ssh_auth_messages(&auth);
    let client = resolve_client_for(via.client_kind(), profile.client_overrides.as_ref(), &store)?;
    run_transfer_with_log(
        &store,
        &profile,
        TransferDirection::Push,
        &args.local_path,
        &args.remote_path,
        via,
        client,
        &auth.args,
        allow_insecure_transfers,
        args.i_know_its_insecure,
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
    let allow_insecure_transfers = settings::get_allow_insecure_transfers(store.conn())?;
    let auth = ssh_auth_context(store.conn())?;
    emit_ssh_auth_messages(&auth);
    let client = resolve_client_for(via.client_kind(), profile.client_overrides.as_ref(), &store)?;
    run_transfer_with_log(
        &store,
        &profile,
        TransferDirection::Pull,
        &args.local_path,
        &args.remote_path,
        via,
        client,
        &auth.args,
        allow_insecure_transfers,
        args.i_know_its_insecure,
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
    let allow_insecure_transfers = settings::get_allow_insecure_transfers(store.conn())?;
    let auth = ssh_auth_context(store.conn())?;
    emit_ssh_auth_messages(&auth);
    let src_client = resolve_client_for(
        via.client_kind(),
        src_profile.client_overrides.as_ref(),
        &store,
    )?;
    let dst_client = resolve_client_for(
        via.client_kind(),
        dst_profile.client_overrides.as_ref(),
        &store,
    )?;
    let temp_dir = TransferTempDir::new("xfer")?;
    let temp_file = temp_dir.path().join(filename_from_remote(&args.src_path));

    let started = Instant::now();
    let pull = execute_transfer(
        &src_profile,
        TransferDirection::Pull,
        &temp_file,
        &args.src_path,
        via,
        src_client,
        &auth.args,
        allow_insecure_transfers,
        args.i_know_its_insecure,
    )?;
    let mut push = None;
    let mut ok = pull.ok;
    let mut exit_code = pull.exit_code;

    if pull.ok {
        let push_outcome = execute_transfer(
            &dst_profile,
            TransferDirection::Push,
            &temp_file,
            &args.dst_path,
            via,
            dst_client,
            &auth.args,
            allow_insecure_transfers,
            args.i_know_its_insecure,
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
        "insecure": via.is_insecure(),
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

fn connect_ssh(
    store: &ProfileStore,
    profile: Profile,
    ssh: PathBuf,
    auth: &SshAuthContext,
) -> Result<()> {
    let mut cmd = Command::new(&ssh);
    cmd.arg("-p")
        .arg(profile.port.to_string())
        .args(&auth.args)
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

fn connect_telnet(
    store: &ProfileStore,
    profile: Profile,
    telnet: PathBuf,
    initial_send: Option<String>,
) -> Result<()> {
    let mut cmd = Command::new(&telnet);
    cmd.arg(&profile.host).arg(profile.port.to_string());
    if let Some(initial_send) = initial_send {
        spawn_tty_initial_send(initial_send);
    }
    let started = Instant::now();
    let status = cmd
        .spawn()
        .context("failed to launch telnet")?
        .wait()
        .context("failed to wait for telnet")?;
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

fn connect_serial(
    store: &ProfileStore,
    profile: Profile,
    initial_send: Option<String>,
) -> Result<()> {
    let port_name = profile.host.clone();
    let baud_rate = profile.port as u32;
    let mut port = serialport::new(&port_name, baud_rate)
        .timeout(Duration::from_millis(20))
        .open()
        .with_context(|| format!("failed to open serial port {port_name} at {baud_rate}"))?;
    let started = Instant::now();
    let result = run_serial_session(&mut port, initial_send);
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

fn run_serial_session(
    port: &mut Box<dyn serialport::SerialPort>,
    initial_send: Option<String>,
) -> Result<()> {
    let _raw = RawModeGuard::enter()?;
    if let Some(initial_send) = initial_send {
        spawn_serial_initial_send(port, initial_send);
    }
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

fn spawn_tty_initial_send(initial_send: String) {
    thread::spawn(move || {
        thread::sleep(INITIAL_SEND_DELAY);
        match OpenOptions::new().write(true).open("/dev/tty") {
            Ok(mut tty) => {
                if let Err(err) = tty
                    .write_all(initial_send.as_bytes())
                    .and_then(|_| tty.flush())
                {
                    warn!(?err, "failed to write initial send to tty");
                }
            }
            Err(err) => {
                warn!(?err, "failed to open tty for initial send");
            }
        }
    });
}

fn spawn_serial_initial_send(port: &mut Box<dyn serialport::SerialPort>, initial_send: String) {
    let mut port_writer = match port.try_clone() {
        Ok(writer) => writer,
        Err(err) => {
            warn!(?err, "failed to clone serial port for initial send");
            return;
        }
    };
    thread::spawn(move || {
        thread::sleep(INITIAL_SEND_DELAY);
        if let Err(err) = port_writer
            .write_all(initial_send.as_bytes())
            .and_then(|_| port_writer.flush())
        {
            warn!(?err, "failed to write initial send to serial port");
        }
    });
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

fn filename_from_remote(remote_path: &str) -> PathBuf {
    Path::new(remote_path)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("transfer.bin"))
}

fn ensure_agent_socket_available() -> Result<()> {
    let status = agent::status();
    if status.auth_sock.is_some() {
        Ok(())
    } else {
        Err(anyhow!(
            "SSH_AUTH_SOCK is not set; start ssh-agent before using td agent"
        ))
    }
}

fn confirm_agent_add(key_path: &Path) -> Result<bool> {
    println!("About to add key to ssh-agent: {}", key_path.display());
    print!("Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("yes"))
}

fn confirm_agent_clear() -> Result<bool> {
    println!("About to remove all keys from ssh-agent.");
    print!("Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("yes") {
        return Ok(false);
    }
    print!("Type 'clear' to confirm: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("clear"))
}

fn handle_ssh_add_output(output: std::process::Output, label: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut detail = String::new();
    if !stderr.is_empty() {
        detail.push_str(&format!("stderr: {stderr}"));
    }
    if !stdout.is_empty() {
        if !detail.is_empty() {
            detail.push('\n');
        }
        detail.push_str(&format!("stdout: {stdout}"));
    }
    if detail.is_empty() {
        detail = format!("exit status: {}", output.status);
    }
    Err(anyhow!("{label} failed: {detail}"))
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
    fn parses_config_set_allow_insecure_transfers() {
        let cli = Cli::try_parse_from(["td", "config", "set", "allow_insecure_transfers", "true"])
            .expect("parses config set");

        match cli.command {
            Some(Commands::Config {
                command: ConfigCommands::Set(args),
            }) => {
                assert_eq!(args.key.as_deref(), Some("allow_insecure_transfers"));
                assert_eq!(args.value.as_deref(), Some("true"));
            }
            _ => panic!("expected config set command"),
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
            "td", "config", "apply", "p1", "cfg_main", "--plan", "--backup", "--via", "sftp",
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
                assert!(!args.i_know_its_insecure);
            }
            _ => panic!("expected config apply command"),
        }
    }

    #[test]
    fn parses_env_set() {
        let cli = Cli::try_parse_from(["td", "env", "set", "work.ssh_auth_order", "agent,keys"])
            .expect("parses env set");

        match cli.command {
            Some(Commands::Env {
                command: EnvCommands::Set(args),
            }) => {
                assert_eq!(args.name_key, "work.ssh_auth_order");
                assert_eq!(args.value, "agent,keys");
            }
            _ => panic!("expected env set command"),
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
        let cli = Cli::try_parse_from(["td", "run", "p1", "c_main", "--json"]).expect("parses run");

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

    #[test]
    fn parses_test_command() {
        let cli =
            Cli::try_parse_from(["td", "test", "p1", "--json", "--ssh"]).expect("parses test");

        match cli.command {
            Some(Commands::Test {
                profile_id,
                json,
                ssh,
            }) => {
                assert_eq!(profile_id, "p1");
                assert!(json);
                assert!(ssh);
            }
            _ => panic!("expected test command"),
        }
    }

    #[test]
    fn parses_export_command() {
        let cli =
            Cli::try_parse_from(["td", "export", "--include-secrets", "--output", "out.json"])
                .expect("parses export");

        match cli.command {
            Some(Commands::Export(args)) => {
                assert!(args.include_secrets);
                assert_eq!(args.output.as_deref(), Some(Path::new("out.json")));
            }
            _ => panic!("expected export command"),
        }
    }

    #[test]
    fn parses_import_command() {
        let cli = Cli::try_parse_from(["td", "import", "--conflict", "rename", "in.json"])
            .expect("parses import");

        match cli.command {
            Some(Commands::Import(args)) => {
                assert!(matches!(args.conflict, ConflictArg::Rename));
                assert_eq!(args.path.as_deref(), Some(Path::new("in.json")));
            }
            _ => panic!("expected import command"),
        }
    }
}
