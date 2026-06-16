use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
#[cfg(windows)]
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
#[cfg(windows)]
use portable_pty::{CommandBuilder, PtySize};
use rusqlite::Connection;
use std::fmt::Display;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tdcore::agent;
use tdcore::cmdset::{CmdSetStore, NewCmdSet, NewCmdStep, StepOnError};
use tdcore::cmdset_runner::{run_cmdset_ssh, CmdSetRunRequest};
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
use tdcore::session_log::{
    self, SessionLogFiles, SessionLogPlan, SessionLogReference,
    SESSION_LOG_REASON_METADATA_WRITE_FAILED, SESSION_LOG_REASON_POWERSHELL_LAUNCH_FAILED,
    SESSION_LOG_REASON_SCRIPT_LAUNCH_FAILED,
};
use tdcore::settings;
use tdcore::settings::SettingScope;
use tdcore::settings_registry;
use tdcore::ssh::{self, SshAuthContext, SshInvocation, SshInvocationMode, SshInvocationRequest};
use tdcore::tester::{self, SshBatchCommand, TestOptions};
use tdcore::transfer::{TransferDirection, TransferTempDir, TransferVia};
use tdcore::tunnel::{ForwardKind, ForwardStore, NewSession, SessionKind, SessionStore};
use tdcore::util::now_ms;
use time::OffsetDateTime;
use tracing::{info, warn};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::prelude::*;
use tui as tdtui;
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
    /// Initialize local TeraDock data and optionally install safe samples
    Init(InitArgs),
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
        /// Parser spec to apply to stdout (raw/json/regex:<id>)
        #[arg(long)]
        parser: Option<String>,
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
    /// Show recently used interactive SSH session profiles
    Recent {
        /// Maximum number of profiles to show
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Inspect saved interactive SSH session logs
    Session {
        #[command(subcommand)]
        command: SessionCommands,
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
    /// Launch the terminal UI
    Ui,
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
struct InitArgs {
    /// Install safe read-only sample CommandSets
    #[arg(long)]
    with_samples: bool,
}

#[derive(Debug, Subcommand)]
enum SessionCommands {
    /// Diagnose interactive SSH session logging
    Doctor(SessionDoctorArgs),
    /// Run an experimental Windows ConPTY SSH logging proof of concept
    ConptyTest(SessionConptyTestArgs),
    /// List saved interactive SSH session logs
    List(SessionListArgs),
    /// Show metadata for a saved interactive SSH session log
    Show(SessionShowArgs),
    /// Print the terminal log path for a saved session
    Path { session_id: String },
}

#[derive(Debug, Args)]
struct SessionConptyTestArgs {
    profile_id: String,
    /// Seconds to wait for the first ConPTY output byte before aborting; 0 disables the timeout
    #[arg(long, default_value_t = 10)]
    startup_timeout_sec: u64,
    /// Print sanitized ConPTY PoC startup and bridge phase diagnostics
    #[arg(long)]
    debug: bool,
}

#[derive(Debug, Args)]
struct SessionDoctorArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct SessionListArgs {
    /// Maximum number of sessions to show
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Output as JSON
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct SessionShowArgs {
    /// Session log ID
    session_id: String,
    /// Output metadata as JSON
    #[arg(long)]
    json: bool,
    /// Print the last N log lines after metadata
    #[arg(long)]
    tail: Option<usize>,
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
    /// Open the interactive settings UI
    Ui,
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
    order: Vec<CliSshAuthMethod>,
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
enum CliSshAuthMethod {
    Agent,
    Keys,
    Password,
}

impl From<CliSshAuthMethod> for ssh::SshAuthMethod {
    fn from(method: CliSshAuthMethod) -> Self {
        match method {
            CliSshAuthMethod::Agent => Self::Agent,
            CliSshAuthMethod::Keys => Self::Keys,
            CliSshAuthMethod::Password => Self::Password,
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
        Some(Commands::Init(args)) => handle_init(args),
        Some(Commands::Exec {
            profile_id,
            timeout_ms,
            json,
            parser,
            cmd,
        }) => handle_exec(profile_id, timeout_ms, json, parser, cmd),
        Some(Commands::Run {
            profile_id,
            cmdset_id,
            json,
        }) => handle_run(profile_id, cmdset_id, json),
        Some(Commands::Connect {
            profile_id,
            initial_send,
        }) => handle_connect(profile_id, initial_send),
        Some(Commands::Recent { limit, json }) => handle_recent(limit, json),
        Some(Commands::Session { command }) => handle_session(command),
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
        Some(Commands::Ui) => handle_ui(),
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

fn handle_ui() -> Result<()> {
    tdtui::run()
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

fn handle_init(args: InitArgs) -> Result<()> {
    let config_dir = paths::config_dir()?;
    let database_path = paths::database_path()?;
    let conn = db::init_connection()?;
    let schema_version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    println!("TeraDock local data is ready.");
    println!("Config dir: {}", config_dir.display());
    println!("Database: {}", database_path.display());
    println!("Schema version: {schema_version}");

    if args.with_samples {
        let mut cmdset_store = CmdSetStore::new(conn);
        let installed = install_sample_cmdsets(&mut cmdset_store)?;
        println!();
        println!("Sample CommandSets:");
        for item in installed {
            println!("  {} {}", item.status.as_str(), item.cmdset_id);
        }
    }

    println!();
    println!("Next commands:");
    println!("  td doctor");
    if !args.with_samples {
        println!("  td init --with-samples");
    }
    println!("  td profile add --name lab1 --host 192.0.2.10 --user admin --danger high");
    println!("  td run <profile_id> linux-basic-check");
    println!("  td ui");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SampleInstallResult {
    cmdset_id: String,
    status: SampleInstallStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleInstallStatus {
    Created,
    Skipped,
}

impl SampleInstallStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Skipped => "skipped",
        }
    }
}

fn install_sample_cmdsets(store: &mut CmdSetStore) -> Result<Vec<SampleInstallResult>> {
    let sample = linux_basic_check_sample();
    let cmdset_id = sample
        .cmdset_id
        .clone()
        .expect("sample cmdset id should be explicit");
    if store.get(&cmdset_id)?.is_some() {
        return Ok(vec![SampleInstallResult {
            cmdset_id,
            status: SampleInstallStatus::Skipped,
        }]);
    }
    let created = store.insert(sample)?;
    Ok(vec![SampleInstallResult {
        cmdset_id: created.cmdset_id,
        status: SampleInstallStatus::Created,
    }])
}

fn linux_basic_check_sample() -> NewCmdSet {
    let commands = [
        "uname -a",
        "uptime",
        "df -h",
        "free -m",
        "systemctl --failed || true",
    ];
    NewCmdSet {
        cmdset_id: Some("linux-basic-check".to_string()),
        name: "Linux basic check".to_string(),
        vars: None,
        steps: commands
            .iter()
            .map(|cmd| NewCmdStep {
                cmd: (*cmd).to_string(),
                timeout_ms: Some(10_000),
                on_error: StepOnError::Continue,
                parser_spec: tdcore::parser::ParserSpec::Raw,
            })
            .collect(),
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
        ConfigCommands::Ui => {
            tdtui::run_settings_ui()?;
            Ok(())
        }
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

fn handle_env(cmd: EnvCommands) -> Result<()> {
    let conn = db::init_connection()?;
    match cmd {
        EnvCommands::List => {
            let current = settings::get_current_env(&conn)?;
            let envs = settings::list_env_names(&conn)?;
            if envs.is_empty() {
                if let Some(current) = current {
                    println!("{current} *");
                } else {
                    println!("(no env presets)");
                }
                return Ok(());
            }
            for env in envs {
                if current.as_deref() == Some(env.as_str()) {
                    println!("{env} *");
                } else {
                    println!("{env}");
                }
            }
            Ok(())
        }
        EnvCommands::Use { name } => {
            let name = normalize_env_name(&name)?;
            settings::set_current_env(&conn, &name)?;
            println!("{name}");
            Ok(())
        }
        EnvCommands::Show { name } => {
            let name = normalize_env_name(&name)?;
            let scope = SettingScope::Env(name);
            let settings = settings::list_settings_scoped(&conn, &scope)?;
            if settings.is_empty() {
                println!("(no settings)");
                return Ok(());
            }
            for (key, value) in settings {
                println!("{key}={value}");
            }
            Ok(())
        }
        EnvCommands::Set(args) => {
            let (name, key) = parse_env_key(&args.name_key)?;
            let name = normalize_env_name(&name)?;
            ensure_known_setting(&key)?;
            ensure_scope_supported(&key, settings::SettingScopeKind::Env)?;
            let normalized = match settings_registry::validate_setting_value(&key, &args.value) {
                Ok(normalized) => normalized,
                Err(err) => {
                    let schema = schema_output_for_key(&key)?;
                    return Err(anyhow!("invalid value for '{key}': {err}\n\n{schema}"));
                }
            };
            let scope = SettingScope::Env(name);
            settings::set_setting_scoped(&conn, &scope, &key, &normalized)?;
            println!("{key}={normalized}");
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
    let mut value = if args.resolved {
        settings::get_setting_resolved(conn, &scope, &args.key)?
    } else {
        settings::get_setting_scoped(conn, &scope, &args.key)?
    };
    if args.resolved && value.is_none() {
        value = default_resolved_config_value(conn, &args.key)?;
    }
    if let Some(value) = value {
        println!("{}={}", args.key, value);
    } else {
        println!("{}=", args.key);
    }
    Ok(())
}

fn default_resolved_config_value(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(session_log::default_value_for_key(conn, key)?)
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
    let via = TransferVia::parse(&args.via)?;
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

fn normalize_auth_order(order: Vec<CliSshAuthMethod>) -> Result<Vec<ssh::SshAuthMethod>> {
    ssh::normalize_auth_order(order.into_iter().map(Into::into).collect()).map_err(Into::into)
}

fn format_auth_order(order: &[ssh::SshAuthMethod]) -> String {
    ssh::format_auth_order(order)
}

fn load_ssh_auth_order(conn: &Connection) -> Result<Vec<ssh::SshAuthMethod>> {
    ssh::load_ssh_auth_order(conn).map_err(Into::into)
}

fn ssh_auth_context(conn: &Connection) -> Result<SshAuthContext> {
    ssh::ssh_auth_context(conn).map_err(Into::into)
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
    parser: Option<String>,
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

    let invocation = ssh::build_ssh_invocation(
        &store,
        SshInvocationRequest {
            profile_id: &profile_id,
            source: "cli",
            mode: SshInvocationMode::Exec,
        },
    )?;
    emit_ssh_auth_messages(&invocation.auth_context);
    let mut command = Command::new(&invocation.client_path);
    command
        .args(&invocation.args)
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
        client_used: Some(invocation.client_path.to_string_lossy().into_owned()),
        ok,
        exit_code: Some(exit_code),
        duration_ms: Some(duration_ms),
        meta_json: None,
    };
    oplog::log_operation(store.conn(), entry)?;

    if json_output {
        let stdout_text = String::from_utf8_lossy(&output.stdout);
        let parsed = if let Some(parser_spec) = parser {
            let spec = tdcore::parser::ParserSpec::parse(&parser_spec)?;
            let parser_def = match &spec {
                tdcore::parser::ParserSpec::Regex(id) => {
                    let cmdset_store = CmdSetStore::new(db::init_connection()?);
                    cmdset_store.get_parser(id)?
                }
                _ => None,
            };
            parse_output(&spec, &stdout_text, parser_def.as_ref())?
        } else {
            serde_json::from_str::<serde_json::Value>(&stdout_text)
                .unwrap_or_else(|_| serde_json::json!({}))
        };
        let json = serde_json::json!({
            "ok": ok,
            "exit_code": exit_code,
            "stdout": stdout_text,
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
    let invocation = ssh::build_ssh_invocation(
        &profile_store,
        SshInvocationRequest {
            profile_id: &profile_id,
            source: "cli",
            mode: SshInvocationMode::CommandSet,
        },
    )?;
    emit_ssh_auth_messages(&invocation.auth_context);
    let result = run_cmdset_ssh(
        &profile_store,
        &cmdset_store,
        CmdSetRunRequest {
            profile_id: &profile_id,
            cmdset_id: &cmdset_id,
            ssh: &invocation.client_path,
            ssh_auth_args: &invocation.auth_context.args,
        },
        |step| -> tdcore::error::Result<()> {
            if !json_output {
                io::stdout().write_all(step.stdout.as_bytes())?;
                io::stderr().write_all(step.stderr.as_bytes())?;
            }
            Ok(())
        },
    )?;

    if json_output {
        let json = serde_json::json!({
            "ok": result.ok,
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "duration_ms": result.duration_ms,
            "parsed": {
                "steps": result.steps,
            }
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    if !result.ok {
        return Err(anyhow!("run failed with exit code {}", result.exit_code));
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
    match profile.profile_type {
        ProfileType::Ssh => {
            let invocation = ssh::build_ssh_invocation(
                &store,
                SshInvocationRequest {
                    profile_id: &profile.profile_id,
                    source: "cli",
                    mode: SshInvocationMode::Interactive,
                },
            )?;
            emit_ssh_auth_messages(&invocation.auth_context);
            connect_ssh(&store, invocation)
        }
        ProfileType::Telnet => {
            let telnet = resolve_client_for(
                ClientKind::Telnet,
                profile.client_overrides.as_ref(),
                &store,
            )?;
            connect_telnet(&store, profile, telnet, initial_send)
        }
        ProfileType::Serial => connect_serial(&store, profile, initial_send),
    }
}

fn handle_recent(limit: usize, json: bool) -> Result<()> {
    if limit == 0 {
        return Err(anyhow!("--limit must be greater than 0"));
    }
    let conn = db::init_connection()?;
    let recent = oplog::recent_ssh_sessions(&conn, limit)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&recent)?);
        return Ok(());
    }
    if recent.is_empty() {
        println!("(no recent SSH sessions)");
        return Ok(());
    }
    println!(
        "{:<16} {:<20} {:<28} {:<6} {:<8} {:<20} status",
        "profile_id", "name", "endpoint", "type", "danger", "last_connected"
    );
    for item in recent {
        let endpoint = format!("{}@{}:{}", item.user, item.host, item.port);
        println!(
            "{:<16} {:<20} {:<28} {:<6} {:<8} {:<20} {}",
            item.profile_id,
            item.name,
            endpoint,
            item.profile_type,
            item.danger_level,
            format_unix_ms_utc(item.last_connected_at),
            format_recent_status(item.last_ok, item.last_exit_code.as_ref())
        );
    }
    Ok(())
}

fn handle_session(cmd: SessionCommands) -> Result<()> {
    match cmd {
        SessionCommands::ConptyTest(args) => {
            let store = ProfileStore::new(db::init_connection()?);
            handle_session_conpty_test(&store, args)
        }
        SessionCommands::Doctor(args) => {
            let conn = db::init_connection()?;
            handle_session_doctor(&conn, args)
        }
        SessionCommands::List(args) => {
            let conn = db::init_connection()?;
            handle_session_list(&conn, args)
        }
        SessionCommands::Show(args) => {
            let conn = db::init_connection()?;
            handle_session_show(&conn, args)
        }
        SessionCommands::Path { session_id } => {
            let conn = db::init_connection()?;
            let metadata = session_log::get_session_log(&conn, &session_id)?;
            let Some(log_path) = metadata.log_path else {
                return Err(anyhow!("session has no terminal log path: {session_id}"));
            };
            println!("{}", log_path.display());
            Ok(())
        }
    }
}

#[cfg(not(windows))]
fn handle_session_conpty_test(_store: &ProfileStore, _args: SessionConptyTestArgs) -> Result<()> {
    Err(anyhow!(
        "unsupported: ConPTY session logging is only available on Windows"
    ))
}

#[cfg(windows)]
fn handle_session_conpty_test(store: &ProfileStore, args: SessionConptyTestArgs) -> Result<()> {
    let debug = args.debug || teradock_debug_enabled();
    let profile_id = args.profile_id;
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    ensure_ssh_profile(&profile, "session conpty-test")?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }
    let invocation = build_conpty_test_invocation(store, &profile_id)?;
    emit_ssh_auth_messages(&invocation.auth_context);
    println!("ConPTY session logging PoC is experimental.");
    println!("ConPTY is not selected by auto and is not integrated with the TUI.");
    println!(
        "TeraDock does not mask terminal output; passwords, tokens, or secrets shown on screen may be captured."
    );
    println!("Starting ConPTY SSH session...");
    println!(
        "Profile: {} ({}@{}:{})",
        invocation.target.profile_id,
        invocation.target.user,
        invocation.target.host,
        invocation.target.port
    );
    let files = session_log::prepare_conpty_session_files(store.conn())?;
    println!("Log path: {}", files.log_path.display());
    conpty_debug(
        debug,
        format_args!("selected profile id: {}", invocation.target.profile_id),
    );
    conpty_debug(
        debug,
        format_args!(
            "resolved ssh client path: {}",
            invocation.client_path.display()
        ),
    );
    conpty_debug(debug, "backend: conpty");
    conpty_debug(
        debug,
        format_args!("log path: {}", files.log_path.display()),
    );
    println!(
        "Spawning {}...",
        invocation
            .client_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ssh.exe")
    );
    println!("Waiting for SSH output...");
    io::stdout().flush()?;

    let options = ConptyRunOptions {
        debug,
        startup_timeout: conpty_startup_timeout(args.startup_timeout_sec),
    };
    match run_conpty_logged_cli_ssh(&invocation, &files, options) {
        CliSshRunResult::Completed(outcome) => {
            store.touch_last_used(&invocation.target.profile_id)?;
            let entry = oplog::OpLogEntry {
                op: oplog::SSH_SESSION_OP.into(),
                profile_id: Some(invocation.target.profile_id.clone()),
                client_used: Some(invocation.client_path.to_string_lossy().into_owned()),
                ok: outcome.ok,
                exit_code: outcome.exit_code,
                duration_ms: Some(outcome.duration_ms),
                meta_json: Some(ssh_connect_meta(&invocation, None, &outcome.session_log)),
            };
            oplog::log_operation(store.conn(), entry)?;
            println!();
            if let Some(session_id) = &outcome.session_log.session_id {
                println!("Saved ConPTY session log: {session_id}");
            }
            match outcome.exit_code {
                Some(code) => println!("SSH exit code: {code}"),
                None => println!("SSH exit code: unavailable"),
            }
            if outcome.ok {
                Ok(())
            } else if let Some(code) = outcome.exit_code {
                Err(anyhow!("ssh exited with code {code}"))
            } else {
                Err(anyhow!("ssh ended without exit code"))
            }
        }
        CliSshRunResult::LaunchFailed {
            error,
            duration_ms,
            session_log,
        } => {
            let error_message = error.to_string();
            store.touch_last_used(&invocation.target.profile_id)?;
            let entry = oplog::OpLogEntry {
                op: oplog::SSH_SESSION_OP.into(),
                profile_id: Some(invocation.target.profile_id.clone()),
                client_used: Some(invocation.client_path.to_string_lossy().into_owned()),
                ok: false,
                exit_code: None,
                duration_ms: Some(duration_ms),
                meta_json: Some(ssh_connect_meta(
                    &invocation,
                    Some(&error_message),
                    &session_log,
                )),
            };
            oplog::log_operation(store.conn(), entry)?;
            Err(error)
        }
    }
}

fn build_conpty_test_invocation(store: &ProfileStore, profile_id: &str) -> Result<SshInvocation> {
    ssh::build_ssh_invocation(
        store,
        SshInvocationRequest {
            profile_id,
            source: "cli-conpty-test",
            mode: SshInvocationMode::Interactive,
        },
    )
    .map_err(Into::into)
}

fn teradock_debug_enabled() -> bool {
    match std::env::var("TERADOCK_DEBUG") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn conpty_debug(enabled: bool, message: impl Display) {
    if enabled {
        eprintln!("debug: {message}");
    }
}

fn conpty_startup_timeout(seconds: u64) -> Option<Duration> {
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

fn handle_session_doctor(conn: &Connection, args: SessionDoctorArgs) -> Result<()> {
    let diagnostics = session_log::diagnose(conn, None)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&diagnostics)?);
        return Ok(());
    }
    print_session_diagnostics(&diagnostics);
    Ok(())
}

fn print_session_diagnostics(diagnostics: &session_log::SessionLogDiagnostics) {
    println!("Session logging diagnostics");
    println!();
    println!("enabled: {}", diagnostics.enabled);
    println!("backend setting: {}", diagnostics.backend_setting);
    println!("resolved backend: {}", diagnostics.resolved_backend);
    println!(
        "script command: {}",
        diagnostics
            .script_command
            .as_ref()
            .map(|path| path.display().to_string())
            .or_else(|| diagnostics.script_command_note.clone())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "powershell command: {}",
        diagnostics
            .powershell_command
            .as_ref()
            .map(|path| path.display().to_string())
            .or_else(|| diagnostics.powershell_command_note.clone())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "ssh command: {}",
        diagnostics
            .ssh_command
            .as_ref()
            .map(|path| path.display().to_string())
            .or_else(|| diagnostics.ssh_command_note.clone())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("log directory: {}", diagnostics.log_directory.display());
    println!("log directory exists: {}", diagnostics.log_directory_exists);
    println!(
        "log directory writable: {}",
        diagnostics
            .log_directory_writable
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "last session log: {}",
        diagnostics.last_session_log.as_deref().unwrap_or("none")
    );
    println!("platform: {}", diagnostics.platform);
    println!("platform supported: {}", diagnostics.platform_supported);
    if let Some(reason) = &diagnostics.fallback_reason {
        println!("fallback reason: {reason}");
    }
    if let Some(reliability) = &diagnostics.content_capture_reliability {
        println!("content capture reliability: {reliability}");
    }
    if let Some(warning) = &diagnostics.warning {
        println!("warning: {warning}");
    }
    if diagnostics.platform == "windows" {
        println!();
        println!("ConPTY backend: experimental_ready");
        println!("ConPTY PoC command: td session conpty-test <profile_id>");
        println!(
            "Reason: manual smoke succeeded, but TUI integration and broader Windows validation are pending."
        );
    }
    if diagnostics.status == "ready" {
        println!();
        println!("Status: ready");
    } else if diagnostics.status != "disabled" {
        println!();
        println!("Status: {}", diagnostics.status);
    }
    if !diagnostics.hints.is_empty() {
        println!();
        println!("Hints:");
        for hint in &diagnostics.hints {
            println!("- {hint}");
        }
    }
}

fn handle_session_list(conn: &Connection, args: SessionListArgs) -> Result<()> {
    if args.limit == 0 {
        return Err(anyhow!("--limit must be greater than 0"));
    }
    let mut sessions = session_log::list_session_logs(conn)?;
    sessions.truncate(args.limit);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
        return Ok(());
    }
    if sessions.is_empty() {
        println!("(no saved SSH session logs)");
        return Ok(());
    }
    println!(
        "{:<14} {:<16} {:<28} {:<20} {:<12} {:<18} log_path",
        "session_id", "profile_id", "endpoint", "started", "duration", "status"
    );
    for item in sessions {
        let endpoint = format!("{}@{}:{}", item.user, item.host, item.port);
        let log_path = format_session_log_path(item.log_path.as_deref());
        println!(
            "{:<14} {:<16} {:<28} {:<20} {:<12} {:<18} {}",
            table_cell(&item.session_id, 14),
            table_cell(&item.profile_id, 16),
            table_cell(&endpoint, 28),
            table_cell(&format_unix_ms_utc(item.started_at), 20),
            table_cell(&format_duration_ms(item.duration_ms), 12),
            table_cell(&format_session_status(&item), 18),
            log_path
        );
    }
    Ok(())
}

fn handle_session_show(conn: &Connection, args: SessionShowArgs) -> Result<()> {
    let metadata = session_log::get_session_log(conn, &args.session_id)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&metadata)?);
        return Ok(());
    }

    println!("session_id: {}", metadata.session_id);
    println!("profile_id: {}", metadata.profile_id);
    println!(
        "endpoint: {}@{}:{}",
        metadata.user, metadata.host, metadata.port
    );
    println!("started_at: {}", format_unix_ms_utc(metadata.started_at));
    println!("ended_at: {}", format_unix_ms_utc(metadata.ended_at));
    println!("duration: {}", format_duration_ms(metadata.duration_ms));
    println!(
        "exit_code: {}",
        metadata
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("backend: {}", metadata.backend);
    println!("status: {}", metadata.status);
    if let Some(reason) = &metadata.reason {
        println!("reason: {reason}");
    }
    if let Some(phase) = &metadata.failure_phase {
        println!("failure_phase: {phase}");
    }
    if let Some(reason) = &metadata.failure_reason {
        println!("failure_reason: {reason}");
    }
    println!(
        "log_path: {}",
        format_session_log_path(metadata.log_path.as_deref())
    );
    println!("metadata_path: {}", metadata.metadata_path.display());
    for line in session_capture_lines(&metadata) {
        println!("{line}");
    }

    if let Some(tail) = args.tail {
        if tail == 0 {
            return Err(anyhow!("--tail must be greater than 0"));
        }
        let Some(log_path) = metadata.log_path.as_ref() else {
            return Err(anyhow!("session has no terminal log path"));
        };
        print_log_tail(log_path, tail)?;
    }
    Ok(())
}

fn session_capture_lines(metadata: &session_log::SessionLogMetadata) -> Vec<String> {
    let mut lines = Vec::new();
    if metadata.backend == session_log::SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT
        || metadata.backend == session_log::SESSION_LOG_BACKEND_CONPTY
    {
        lines.push("backend_status: degraded".to_string());
    }
    if let Some(capture) = &metadata.content_capture {
        lines.push(format!("content_capture: {capture}"));
    }
    if let Some(reliable) = metadata.content_capture_reliable {
        lines.push(format!("content_capture_reliable: {reliable}"));
    }
    if let Some(warning) = &metadata.backend_warning {
        lines.push(format!("backend_warning: {warning}"));
    }
    if let Some(status) = &metadata.content_capture_status {
        lines.push(format!("Content capture: {status}"));
    }
    if let Some(warning) = &metadata.content_capture_warning {
        lines.push(format!("Warning: {warning}"));
    }
    lines
}

fn print_log_tail(log_path: &Path, tail: usize) -> Result<()> {
    let raw = std::fs::read(log_path)
        .with_context(|| format!("failed to read session log {}", log_path.display()))?;
    let display_text = session_log_display_text(&raw);
    let lines = display_text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(tail);
    println!();
    println!("log_tail:");
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

fn session_log_display_text(raw: &[u8]) -> String {
    #[cfg(windows)]
    {
        String::from_utf8_lossy(&sanitize_conpty_log_bytes(raw)).into_owned()
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(raw).into_owned()
    }
}

#[cfg(windows)]
fn sanitize_conpty_log_bytes(raw: &[u8]) -> Vec<u8> {
    let mut sanitizer = ConptyLogSanitizer::default();
    let mut sanitized = sanitizer.push(raw);
    sanitized.extend(sanitizer.finish());
    sanitized
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
        ok: report.errors.is_empty() && report.clients.iter().all(|client| client.path.is_some()),
        exit_code: None,
        duration_ms: None,
        meta_json: Some(meta_json),
    };
    oplog::log_operation(&conn, entry)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("Doctor report:");
    for client in &report.clients {
        let path = client
            .path
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "MISSING".to_string());
        println!("{:<6} {:<14} {}", client.name, client.source, path);
    }
    if let Some(sock) = &report.agent.auth_sock {
        println!("SSH_AUTH_SOCK: {sock}");
    } else {
        println!("SSH_AUTH_SOCK: (not set)");
    }
    if let Some(count) = report.agent.key_count {
        println!("ssh-agent keys: {count}");
    }
    if let Some(error) = &report.agent.error {
        println!("ssh-add: {error}");
    }
    if !report.warnings.is_empty() {
        println!("Warnings:");
        for warning in &report.warnings {
            println!("- {}: {}", warning.code, warning.message);
        }
    }
    if !report.errors.is_empty() {
        println!("Errors:");
        for error in &report.errors {
            println!("- {}: {}", error.code, error.message);
        }
    }
    Ok(())
}

fn format_recent_status(ok: bool, exit_code: Option<&i32>) -> String {
    match (ok, exit_code) {
        (true, Some(code)) => format!("ok exit {code}"),
        (false, Some(code)) => format!("failed exit {code}"),
        (true, None) => "ok without exit code".to_string(),
        (false, None) => "failed without exit code".to_string(),
    }
}

fn format_unix_ms_utc(ts_ms: i64) -> String {
    let secs = ts_ms.div_euclid(1000);
    let Ok(dt) = OffsetDateTime::from_unix_timestamp(secs) else {
        return ts_ms.to_string();
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

fn format_duration_ms(duration_ms: i64) -> String {
    if duration_ms >= 1000 {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    } else {
        format!("{duration_ms}ms")
    }
}

fn format_session_status(metadata: &session_log::SessionLogMetadata) -> String {
    match metadata.exit_code {
        Some(code) => format!("{} exit {}", metadata.status, code),
        None => metadata.status.clone(),
    }
}

fn table_cell(value: &str, width: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('~');
    truncated
}

fn format_session_log_path(path: Option<&Path>) -> String {
    let Some(path) = path else {
        return "<none>".to_string();
    };
    if !is_displayable_session_log_path(path) {
        return "<none>".to_string();
    }
    path.display().to_string()
}

fn is_displayable_session_log_path(path: &Path) -> bool {
    let raw = path.as_os_str().to_string_lossy();
    if raw.trim().is_empty() || raw.contains('\n') || raw.contains('\r') {
        return false;
    }
    path.is_absolute()
        || path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("log"))
}

fn handle_test(profile_id: String, json: bool, include_ssh: bool) -> Result<()> {
    let store = ProfileStore::new(db::init_connection()?);
    let profile = store
        .get(&profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
    if !tester::is_network_profile(&profile) {
        return Err(anyhow!("test only supports SSH or telnet profiles"));
    }

    let mut options = TestOptions::default();
    let mut client_used = None;
    if include_ssh {
        if profile.profile_type != ProfileType::Ssh {
            return Err(anyhow!("--ssh is only supported for SSH profiles"));
        }
        let auth = ssh_auth_context(store.conn())?;
        emit_ssh_auth_messages(&auth);
        let ssh = resolve_client_for(ClientKind::Ssh, profile.client_overrides.as_ref(), &store)?;
        client_used = Some(ssh.to_string_lossy().into_owned());
        let batch = SshBatchCommand::new(
            ssh,
            profile.user.clone(),
            profile.host.clone(),
            profile.port,
            auth.args,
            Duration::from_secs(5),
        );
        options = options.with_ssh(batch);
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

fn handle_tunnel(cmd: TunnelCommands) -> Result<()> {
    match cmd {
        TunnelCommands::Start(args) => handle_tunnel_start(args),
        TunnelCommands::Stop { session_id } => handle_tunnel_stop(&session_id),
        TunnelCommands::Status(args) => handle_tunnel_status(args),
    }
}

fn handle_tunnel_start(args: TunnelStartArgs) -> Result<()> {
    if args.forward.is_empty() {
        return Err(anyhow!("tunnel start requires at least one --forward"));
    }
    let profile_store = ProfileStore::new(db::init_connection()?);
    let forward_store = ForwardStore::new(db::init_connection()?);
    let session_store = SessionStore::new(db::init_connection()?);
    let profile = profile_store
        .get(&args.profile_id)?
        .ok_or_else(|| anyhow!("profile not found: {}", args.profile_id))?;
    ensure_ssh_profile(&profile, "tunnel")?;
    if profile.danger_level == DangerLevel::Critical && !confirm_danger(&profile)? {
        println!("Aborted by user.");
        return Ok(());
    }

    let mut forwards = Vec::new();
    for name in &args.forward {
        let forward = forward_store
            .get_by_name(&profile.profile_id, name)?
            .ok_or_else(|| anyhow!("forward not found: {name}"))?;
        forwards.push(forward);
    }

    let ssh = resolve_client_for(
        ClientKind::Ssh,
        profile.client_overrides.as_ref(),
        &profile_store,
    )?;
    let auth = ssh_auth_context(profile_store.conn())?;
    emit_ssh_auth_messages(&auth);

    let mut cmd = Command::new(&ssh);
    cmd.arg("-N")
        .arg("-p")
        .arg(profile.port.to_string())
        .args(&auth.args);
    for forward in &forwards {
        let spec = match forward.kind {
            ForwardKind::Dynamic => forward.listen.clone(),
            ForwardKind::Local | ForwardKind::Remote => format!(
                "{}:{}",
                forward.listen,
                forward
                    .dest
                    .as_ref()
                    .ok_or_else(|| anyhow!("forward {} missing destination", forward.name))?
            ),
        };
        cmd.arg(forward.kind.as_flag()).arg(spec);
    }
    cmd.arg(format!("{}@{}", profile.user, profile.host))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().context("failed to launch ssh tunnel")?;
    let session = session_store.insert(NewSession {
        kind: SessionKind::Tunnel,
        profile_id: profile.profile_id.clone(),
        pid: Some(child.id()),
        forwards: forwards
            .iter()
            .map(|forward| forward.name.clone())
            .collect(),
    })?;
    println!(
        "started tunnel session {} (pid {})",
        session.session_id,
        session.pid.unwrap_or_default()
    );
    Ok(())
}

fn handle_tunnel_stop(session_id: &str) -> Result<()> {
    let session_store = SessionStore::new(db::init_connection()?);
    let session = session_store
        .get(session_id)?
        .ok_or_else(|| anyhow!("session not found: {session_id}"))?;
    if let Some(pid) = session.pid {
        terminate_pid(pid)?;
    }
    session_store.remove(session_id)?;
    println!("stopped tunnel session {session_id}");
    Ok(())
}

fn handle_tunnel_status(args: TunnelStatusArgs) -> Result<()> {
    let session_store = SessionStore::new(db::init_connection()?);
    let cleaned = session_store.cleanup_dead()?;
    let sessions = session_store.list()?;

    if args.json {
        let payload = serde_json::json!({
            "cleaned": cleaned.iter().map(|session| session.session_id.clone()).collect::<Vec<_>>(),
            "sessions": sessions.iter().map(|session| serde_json::json!({
                "session_id": session.session_id,
                "profile_id": session.profile_id,
                "pid": session.pid,
                "started_at": session.started_at,
                "forwards": session.forwards,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if sessions.is_empty() {
        println!("(no tunnel sessions)");
        return Ok(());
    }
    if !cleaned.is_empty() {
        println!("cleaned {} dead session(s)", cleaned.len());
    }
    for session in sessions {
        let pid = session
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<12} {:<10} {:<8} {:?}",
            session.session_id, session.profile_id, pid, session.forwards
        );
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
            return Err(anyhow!("failed to terminate pid {pid}"));
        }
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .context("failed to execute taskkill")?;
        if !status.success() {
            return Err(anyhow!("failed to terminate pid {pid}"));
        }
    }
    Ok(())
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
    let via = TransferVia::parse(&args.via)?;
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
    let via = TransferVia::parse(&args.via)?;
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

    let via = TransferVia::parse(&args.via)?;
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

struct CliSshOutcome {
    ok: bool,
    exit_code: Option<i32>,
    duration_ms: i64,
    session_log: SessionLogReference,
}

enum CliSshRunResult {
    Completed(CliSshOutcome),
    LaunchFailed {
        error: anyhow::Error,
        duration_ms: i64,
        session_log: SessionLogReference,
    },
}

fn connect_ssh(store: &ProfileStore, invocation: SshInvocation) -> Result<()> {
    let plan = session_log::plan_for_target_with_ssh(
        store.conn(),
        &invocation.target,
        &invocation.client_path,
    );
    emit_session_log_notice(&plan);
    let result = match &plan {
        SessionLogPlan::Script {
            script_path,
            files,
            launch_failure_policy,
        } => run_script_logged_cli_ssh(&invocation, script_path, files, *launch_failure_policy),
        SessionLogPlan::PowerShellTranscript {
            powershell_path,
            files,
            launch_failure_policy,
        } => run_powershell_transcript_cli_ssh(
            &invocation,
            powershell_path,
            files,
            *launch_failure_policy,
        ),
        SessionLogPlan::Error { reason } => CliSshRunResult::LaunchFailed {
            error: anyhow!("session logging backend is not ready: {reason}"),
            duration_ms: 0,
            session_log: SessionLogReference::not_saved(reason.to_string()),
        },
        SessionLogPlan::Disabled | SessionLogPlan::NoLog { .. } => {
            run_plain_cli_ssh(&invocation, plan.not_saved_reference())
        }
    };
    match result {
        CliSshRunResult::Completed(outcome) => {
            store.touch_last_used(&invocation.target.profile_id)?;
            let entry = oplog::OpLogEntry {
                op: "connect".into(),
                profile_id: Some(invocation.target.profile_id.clone()),
                client_used: Some(invocation.client_path.to_string_lossy().into_owned()),
                ok: outcome.ok,
                exit_code: outcome.exit_code,
                duration_ms: Some(outcome.duration_ms),
                meta_json: Some(ssh_connect_meta(&invocation, None, &outcome.session_log)),
            };
            oplog::log_operation(store.conn(), entry)?;
            if outcome.ok {
                Ok(())
            } else if let Some(code) = outcome.exit_code {
                Err(anyhow!("ssh exited with code {code}"))
            } else {
                Err(anyhow!("ssh ended without exit code"))
            }
        }
        CliSshRunResult::LaunchFailed {
            error,
            duration_ms,
            session_log,
        } => {
            let error_message = error.to_string();
            store.touch_last_used(&invocation.target.profile_id)?;
            let entry = oplog::OpLogEntry {
                op: "connect".into(),
                profile_id: Some(invocation.target.profile_id.clone()),
                client_used: Some(invocation.client_path.to_string_lossy().into_owned()),
                ok: false,
                exit_code: None,
                duration_ms: Some(duration_ms),
                meta_json: Some(ssh_connect_meta(
                    &invocation,
                    Some(&error_message),
                    &session_log,
                )),
            };
            oplog::log_operation(store.conn(), entry)?;
            Err(error)
        }
    }
}

fn run_plain_cli_ssh(
    invocation: &SshInvocation,
    session_log: SessionLogReference,
) -> CliSshRunResult {
    let mut cmd = Command::new(&invocation.client_path);
    cmd.args(&invocation.args);
    let started = Instant::now();
    let status = cmd.status().context("failed to launch ssh");
    let duration_ms = started.elapsed().as_millis() as i64;

    match status {
        Ok(status) => CliSshRunResult::Completed(CliSshOutcome {
            ok: status.success(),
            exit_code: status.code(),
            duration_ms,
            session_log,
        }),
        Err(error) => CliSshRunResult::LaunchFailed {
            error,
            duration_ms,
            session_log,
        },
    }
}

fn run_script_logged_cli_ssh(
    invocation: &SshInvocation,
    script_path: &Path,
    files: &SessionLogFiles,
    launch_failure_policy: session_log::SessionLogLaunchFailurePolicy,
) -> CliSshRunResult {
    let script = session_log::build_script_invocation(
        script_path,
        files,
        &invocation.client_path,
        &invocation.args,
    );
    let log_started_at = now_ms();
    let started = Instant::now();
    let status = Command::new(&script.executable)
        .args(&script.args)
        .status()
        .context("failed to launch script");
    let duration_ms = started.elapsed().as_millis() as i64;

    match status {
        Ok(status) => {
            let exit_code = status.code();
            let session_log = match session_log::complete_script_session(
                files,
                &invocation.target,
                log_started_at,
                duration_ms,
                exit_code,
            ) {
                Ok(metadata) => SessionLogReference::saved(metadata.session_id),
                Err(err) => SessionLogReference::not_saved(format!(
                    "{SESSION_LOG_REASON_METADATA_WRITE_FAILED}: {err}"
                )),
            };
            CliSshRunResult::Completed(CliSshOutcome {
                ok: status.success(),
                exit_code,
                duration_ms,
                session_log,
            })
        }
        Err(error) => {
            if launch_failure_policy.fallback_to_plain() {
                eprintln!(
                    "TeraDock session logging failed to start ({error}); continuing without logging."
                );
                run_plain_cli_ssh(
                    invocation,
                    SessionLogReference::not_saved(SESSION_LOG_REASON_SCRIPT_LAUNCH_FAILED),
                )
            } else {
                CliSshRunResult::LaunchFailed {
                    error,
                    duration_ms,
                    session_log: SessionLogReference::not_saved(
                        SESSION_LOG_REASON_SCRIPT_LAUNCH_FAILED,
                    ),
                }
            }
        }
    }
}

fn run_powershell_transcript_cli_ssh(
    invocation: &SshInvocation,
    powershell_path: &Path,
    files: &SessionLogFiles,
    launch_failure_policy: session_log::SessionLogLaunchFailurePolicy,
) -> CliSshRunResult {
    let powershell = session_log::build_powershell_transcript_invocation(
        powershell_path,
        files,
        &invocation.client_path,
        &invocation.args,
        launch_failure_policy,
    );
    let log_started_at = now_ms();
    let started = Instant::now();
    let status = Command::new(&powershell.executable)
        .args(&powershell.args)
        .status()
        .context("failed to launch PowerShell");
    let duration_ms = started.elapsed().as_millis() as i64;

    match status {
        Ok(status) => {
            let exit_code = status.code();
            let session_log = match session_log::complete_powershell_transcript_session(
                files,
                &invocation.target,
                log_started_at,
                duration_ms,
                exit_code,
            ) {
                Ok(metadata) => SessionLogReference::saved(metadata.session_id),
                Err(err) if !launch_failure_policy.fallback_to_plain() => {
                    return CliSshRunResult::LaunchFailed {
                        error: anyhow!("session logging failed: {err}"),
                        duration_ms,
                        session_log: SessionLogReference::not_saved(format!(
                            "{SESSION_LOG_REASON_METADATA_WRITE_FAILED}: {err}"
                        )),
                    };
                }
                Err(err) => SessionLogReference::not_saved(format!(
                    "{SESSION_LOG_REASON_METADATA_WRITE_FAILED}: {err}"
                )),
            };
            CliSshRunResult::Completed(CliSshOutcome {
                ok: status.success(),
                exit_code,
                duration_ms,
                session_log,
            })
        }
        Err(error) => {
            if launch_failure_policy.fallback_to_plain() {
                eprintln!(
                    "TeraDock session logging failed to start ({error}); continuing without logging."
                );
                run_plain_cli_ssh(
                    invocation,
                    SessionLogReference::not_saved(SESSION_LOG_REASON_POWERSHELL_LAUNCH_FAILED),
                )
            } else {
                CliSshRunResult::LaunchFailed {
                    error,
                    duration_ms,
                    session_log: SessionLogReference::not_saved(
                        SESSION_LOG_REASON_POWERSHELL_LAUNCH_FAILED,
                    ),
                }
            }
        }
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ConptyRunOptions {
    debug: bool,
    startup_timeout: Option<Duration>,
}

#[cfg(windows)]
struct ConptyChildReport {
    exit_code: Option<i32>,
    first_output_received: bool,
}

#[cfg(windows)]
struct ConptyOutputThread {
    handle: thread::JoinHandle<()>,
    result_rx: mpsc::Receiver<io::Result<u64>>,
}

#[cfg(windows)]
struct ConptyInputThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

#[cfg(windows)]
struct ConptyWaitThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

#[cfg(windows)]
struct ConptyTimerThread {
    handle: thread::JoinHandle<()>,
    done_rx: mpsc::Receiver<()>,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConptySyntheticResponse {
    CursorPosition,
    DeviceStatusOk,
}

#[cfg(windows)]
impl ConptySyntheticResponse {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::CursorPosition => b"\x1b[1;1R",
            Self::DeviceStatusOk => b"\x1b[0n",
        }
    }

    fn debug_label(self) -> &'static str {
        match self {
            Self::CursorPosition => "cursor_position",
            Self::DeviceStatusOk => "device_status_ok",
        }
    }
}

#[cfg(windows)]
enum ConptyInputCommand {
    WriteSynthetic { response: ConptySyntheticResponse },
}

#[cfg(windows)]
#[derive(Debug)]
enum ConptyEvent {
    FirstOutput { bytes: usize },
    OutputChunk { bytes: usize },
    ChildExited { exit_code: Option<i32> },
    StartupTimeout,
    UserAbort,
    OutputError { message: String },
    InputError { message: String },
}

#[cfg(windows)]
#[derive(Debug)]
enum ConptyLoopMessage {
    Event(ConptyEvent),
    ChildWaitError { message: String },
}

#[cfg(windows)]
fn send_conpty_event(tx: &mpsc::Sender<ConptyLoopMessage>, event: ConptyEvent) {
    let _ = tx.send(ConptyLoopMessage::Event(event));
}

#[cfg(windows)]
struct ConptyChildFailure {
    status: &'static str,
    phase: &'static str,
    reason: &'static str,
    error: anyhow::Error,
}

#[cfg(windows)]
impl ConptyChildFailure {
    fn failed(phase: &'static str, reason: &'static str, error: anyhow::Error) -> Self {
        Self {
            status: session_log::SESSION_LOG_STATUS_FAILED,
            phase,
            reason,
            error,
        }
    }

    fn aborted(phase: &'static str, reason: &'static str, error: anyhow::Error) -> Self {
        Self {
            status: session_log::SESSION_LOG_STATUS_ABORTED,
            phase,
            reason,
            error,
        }
    }

    fn into_error(self) -> anyhow::Error {
        anyhow!(
            "ConPTY SSH {status} during {phase}: {reason}: {error}",
            status = self.status,
            phase = self.phase,
            reason = self.reason,
            error = self.error
        )
    }
}

#[cfg(windows)]
fn run_conpty_logged_cli_ssh(
    invocation: &SshInvocation,
    files: &SessionLogFiles,
    options: ConptyRunOptions,
) -> CliSshRunResult {
    let log_started_at = now_ms();
    let started = Instant::now();
    let status = run_conpty_ssh_child(invocation, files, options);
    let duration_ms = started.elapsed().as_millis() as i64;

    match status {
        Ok(report) => {
            let exit_code = report.exit_code;
            let session_log = match session_log::complete_conpty_session(
                files,
                &invocation.target,
                log_started_at,
                duration_ms,
                exit_code,
            ) {
                Ok(metadata) => {
                    conpty_debug(
                        options.debug,
                        format_args!("metadata write result: saved {}", metadata.session_id),
                    );
                    SessionLogReference::saved(metadata.session_id)
                }
                Err(err) => {
                    conpty_debug(
                        options.debug,
                        format_args!("metadata write result: failed: {err}"),
                    );
                    SessionLogReference::not_saved(format!(
                        "{SESSION_LOG_REASON_METADATA_WRITE_FAILED}: {err}"
                    ))
                }
            };
            let exit_code_text = exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unavailable".to_string());
            conpty_debug(
                options.debug,
                format_args!("exit phase: code {exit_code_text}"),
            );
            if !report.first_output_received {
                conpty_debug(options.debug, "first output received: no");
            }
            CliSshRunResult::Completed(CliSshOutcome {
                ok: exit_code == Some(0),
                exit_code,
                duration_ms,
                session_log,
            })
        }
        Err(failure) => {
            let failure_status = failure.status;
            conpty_debug(
                options.debug,
                format_args!("failure phase: {} ({})", failure.phase, failure.reason),
            );
            if failure_status == session_log::SESSION_LOG_STATUS_ABORTED {
                conpty_debug(options.debug, "writing aborted metadata");
            }
            let session_log = match session_log::complete_conpty_failure_session(
                files,
                &invocation.target,
                log_started_at,
                duration_ms,
                session_log::SessionLogFailureMetadata {
                    status: failure.status,
                    failure_phase: failure.phase,
                    failure_reason: failure.reason,
                    exit_code: None,
                },
            ) {
                Ok(metadata) => {
                    conpty_debug(
                        options.debug,
                        format_args!("metadata write result: saved {}", metadata.session_id),
                    );
                    SessionLogReference::saved(metadata.session_id)
                }
                Err(err) => {
                    conpty_debug(
                        options.debug,
                        format_args!("metadata write result: failed: {err}"),
                    );
                    SessionLogReference::not_saved(format!(
                        "{SESSION_LOG_REASON_METADATA_WRITE_FAILED}: {err}"
                    ))
                }
            };
            if session_log.saved {
                eprintln!("Session metadata saved with status={failure_status}.");
            }
            CliSshRunResult::LaunchFailed {
                error: failure.into_error(),
                duration_ms,
                session_log,
            }
        }
    }
}

#[cfg(windows)]
fn run_conpty_ssh_child(
    invocation: &SshInvocation,
    files: &SessionLogFiles,
    options: ConptyRunOptions,
) -> std::result::Result<ConptyChildReport, ConptyChildFailure> {
    conpty_debug(options.debug, "child spawn phase: create log");
    let log_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&files.log_path)
        .map_err(|err| {
            ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_CREATE_LOG,
                session_log::SESSION_LOG_FAILURE_REASON_CREATE_LOG_FAILED,
                err.into(),
            )
        })?;
    conpty_debug(options.debug, "child spawn phase: open pty");
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system.openpty(current_pty_size()).map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    let master = pair.master;
    let slave = pair.slave;
    let reader = master.try_clone_reader().map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    let writer = master.take_writer().map_err(|err| {
        ConptyChildFailure::failed(
            session_log::SESSION_LOG_FAILURE_PHASE_OPEN_PTY,
            session_log::SESSION_LOG_FAILURE_REASON_OPEN_PTY_FAILED,
            err,
        )
    })?;
    conpty_debug(options.debug, "child spawn phase: enter raw mode");
    let raw_mode = match RawModeGuard::enter() {
        Ok(guard) => guard,
        Err(err) => {
            drop(log_file);
            return Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_ENTER_RAW_MODE,
                session_log::SESSION_LOG_FAILURE_REASON_RAW_MODE_FAILED,
                err,
            ));
        }
    };
    let mut cmd = CommandBuilder::new(invocation.client_path.as_os_str());
    cmd.args(&invocation.args);
    conpty_debug(options.debug, "child spawn phase: spawn child");
    let child = match slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(err) => {
            drop(raw_mode);
            drop(log_file);
            return Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_SPAWN_CHILD,
                session_log::SESSION_LOG_FAILURE_REASON_SPAWN_CHILD_FAILED,
                err,
            ));
        }
    };
    conpty_debug(options.debug, "child spawned");
    drop(slave);

    let first_output_received = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = mpsc::channel();
    let (input_command_tx, input_command_rx) = mpsc::channel();
    let output_thread = spawn_conpty_output_thread(
        reader,
        log_file,
        Arc::clone(&first_output_received),
        event_tx.clone(),
        input_command_tx,
        options,
    );
    let mut child_killer = child.clone_killer();
    let timeout_child_killer = child_killer.clone_killer();
    let wait_thread =
        spawn_conpty_wait_thread(child, Arc::clone(&cancel), event_tx.clone(), options);
    let input_thread = spawn_conpty_input_thread(
        writer,
        master,
        Arc::clone(&cancel),
        input_command_rx,
        event_tx.clone(),
        options,
    );
    let timeout_thread = spawn_conpty_startup_timeout_thread(
        options.startup_timeout,
        Arc::clone(&first_output_received),
        Arc::clone(&cancel),
        event_tx.clone(),
        timeout_child_killer,
        options,
    );
    drop(event_tx);

    let mut output_bytes = 0_u64;
    let startup_deadline = options
        .startup_timeout
        .and_then(|timeout| Instant::now().checked_add(timeout));
    let loop_result = loop {
        let Some(message) =
            recv_conpty_loop_message(&event_rx, &first_output_received, startup_deadline)
        else {
            break Err(ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                anyhow!("ConPTY event loop ended without child status"),
            ));
        };
        match message {
            ConptyLoopMessage::Event(ConptyEvent::FirstOutput { bytes }) => {
                conpty_debug(
                    options.debug,
                    format_args!("first output received: {bytes} bytes"),
                );
            }
            ConptyLoopMessage::Event(ConptyEvent::OutputChunk { bytes }) => {
                output_bytes += bytes as u64;
            }
            ConptyLoopMessage::Event(ConptyEvent::ChildExited { exit_code }) => {
                let exit_code_text = exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_string());
                conpty_debug(
                    options.debug,
                    format_args!("child exited: code {exit_code_text}"),
                );
                break Ok(exit_code);
            }
            ConptyLoopMessage::Event(ConptyEvent::StartupTimeout) => {
                if first_output_received.load(Ordering::SeqCst) {
                    conpty_debug(options.debug, "startup timeout ignored after first output");
                    continue;
                }
                let timeout = options.startup_timeout.unwrap_or(Duration::from_secs(0));
                conpty_debug(
                    options.debug,
                    format_args!("startup timeout after {} seconds", timeout.as_secs()),
                );
                conpty_debug(options.debug, "first output received: no");
                eprintln!();
                eprintln!(
                    "Error: no ConPTY output received within {} seconds.",
                    timeout.as_secs()
                );
                eprintln!("Aborting ConPTY child...");
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_WAITING_INITIAL_OUTPUT,
                    session_log::SESSION_LOG_FAILURE_REASON_INITIAL_OUTPUT_TIMEOUT,
                    anyhow!(
                        "no ConPTY output received within {} seconds",
                        timeout.as_secs()
                    ),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::UserAbort) => {
                conpty_debug(options.debug, "user abort received");
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::aborted(
                    session_log::SESSION_LOG_FAILURE_PHASE_USER_ABORT,
                    session_log::SESSION_LOG_FAILURE_REASON_CTRL_C,
                    anyhow!("aborted by Ctrl-C"),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::OutputError { message }) => {
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_OUTPUT_BRIDGE,
                    session_log::SESSION_LOG_FAILURE_REASON_OUTPUT_BRIDGE_FAILED,
                    anyhow!(message),
                ));
            }
            ConptyLoopMessage::Event(ConptyEvent::InputError { message }) => {
                kill_conpty_child(&mut child_killer, options);
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_INPUT_BRIDGE,
                    session_log::SESSION_LOG_FAILURE_REASON_INPUT_BRIDGE_FAILED,
                    anyhow!(message),
                ));
            }
            ConptyLoopMessage::ChildWaitError { message } => {
                kill_conpty_child(&mut child_killer, options);
                eprintln!("Warning: ConPTY child process exit could not be confirmed.");
                break Err(ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                    session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                    anyhow!(message),
                ));
            }
        }
    };

    cancel.store(true, Ordering::SeqCst);
    conpty_debug(options.debug, "dropping pty handles");
    conpty_debug(options.debug, "joining threads best-effort");
    let input_shutdown = join_conpty_input_thread(input_thread, Duration::from_millis(500));
    drop(raw_mode);
    conpty_debug(options.debug, "terminal restored");
    match input_shutdown {
        Ok(()) => conpty_debug(
            options.debug,
            "thread shutdown status: input bridge stopped",
        ),
        Err(err) => conpty_debug(
            options.debug,
            format_args!("thread shutdown status: input bridge incomplete: {err}"),
        ),
    }

    let wait_shutdown = join_conpty_wait_thread_best_effort(wait_thread, Duration::from_secs(3));
    match &wait_shutdown {
        Ok(()) => conpty_debug(options.debug, "thread shutdown status: child wait stopped"),
        Err(err) => {
            conpty_debug(
                options.debug,
                format_args!("thread shutdown status: child wait incomplete: {err}"),
            );
            eprintln!("Warning: ConPTY child process did not confirm exit after cleanup.");
        }
    }
    let timeout_shutdown =
        join_conpty_timer_thread_best_effort(timeout_thread, Duration::from_millis(200));
    match &timeout_shutdown {
        Ok(()) => conpty_debug(
            options.debug,
            "thread shutdown status: startup timer stopped",
        ),
        Err(err) => conpty_debug(
            options.debug,
            format_args!("thread shutdown status: startup timer incomplete: {err}"),
        ),
    }

    let exit_code = match loop_result {
        Ok(exit_code) => {
            wait_shutdown.map_err(|err| {
                ConptyChildFailure::failed(
                    session_log::SESSION_LOG_FAILURE_PHASE_CHILD_WAIT,
                    session_log::SESSION_LOG_FAILURE_REASON_CHILD_WAIT_FAILED,
                    err,
                )
            })?;
            exit_code
        }
        Err(failure) => {
            if let Err(err) =
                join_conpty_output_thread_best_effort(output_thread, Duration::from_millis(1000))
            {
                conpty_debug(
                    options.debug,
                    format_args!("thread shutdown status: output reader incomplete: {err}"),
                );
            } else {
                conpty_debug(
                    options.debug,
                    "thread shutdown status: output reader stopped",
                );
            }
            return Err(failure);
        }
    };
    let output_total = join_conpty_output_thread(output_thread, Duration::from_millis(1000))
        .map_err(|err| {
            ConptyChildFailure::failed(
                session_log::SESSION_LOG_FAILURE_PHASE_OUTPUT_BRIDGE,
                session_log::SESSION_LOG_FAILURE_REASON_OUTPUT_BRIDGE_FAILED,
                err,
            )
        })?;
    conpty_debug(
        options.debug,
        "thread shutdown status: output reader stopped",
    );
    conpty_debug(
        options.debug,
        format_args!(
            "output reader completed: {output_total} bytes; event loop observed {output_bytes} bytes"
        ),
    );
    Ok(ConptyChildReport {
        exit_code,
        first_output_received: first_output_received.load(Ordering::SeqCst),
    })
}

#[cfg(windows)]
fn recv_conpty_loop_message(
    event_rx: &mpsc::Receiver<ConptyLoopMessage>,
    first_output_received: &AtomicBool,
    startup_deadline: Option<Instant>,
) -> Option<ConptyLoopMessage> {
    if first_output_received.load(Ordering::SeqCst) {
        return event_rx.recv().ok();
    }
    let Some(deadline) = startup_deadline else {
        return event_rx.recv().ok();
    };
    let now = Instant::now();
    if now >= deadline {
        return Some(ConptyLoopMessage::Event(ConptyEvent::StartupTimeout));
    }
    match event_rx.recv_timeout(deadline.duration_since(now)) {
        Ok(message) => Some(message),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Some(ConptyLoopMessage::Event(ConptyEvent::StartupTimeout))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => None,
    }
}

#[cfg(windows)]
fn spawn_conpty_output_thread(
    mut reader: Box<dyn Read + Send>,
    log_file: std::fs::File,
    first_output_received: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    input_command_tx: mpsc::Sender<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> ConptyOutputThread {
    let (result_tx, result_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = tee_conpty_output(
            &mut reader,
            log_file,
            first_output_received,
            &event_tx,
            &input_command_tx,
            options,
        );
        if let Err(err) = &result {
            send_conpty_event(
                &event_tx,
                ConptyEvent::OutputError {
                    message: err.to_string(),
                },
            );
        }
        let _ = result_tx.send(result);
    });
    ConptyOutputThread { handle, result_rx }
}

#[cfg(windows)]
fn spawn_conpty_input_thread(
    mut writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    cancel: Arc<AtomicBool>,
    input_command_rx: mpsc::Receiver<ConptyInputCommand>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> ConptyInputThread {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        conpty_debug(options.debug, "input bridge started");
        let result = run_conpty_input_bridge(
            &mut writer,
            master.as_ref(),
            &cancel,
            &input_command_rx,
            &event_tx,
            options,
        );
        if let Err(err) = result {
            send_conpty_event(
                &event_tx,
                ConptyEvent::InputError {
                    message: err.to_string(),
                },
            );
        }
        drop(writer);
        drop(master);
        let _ = done_tx.send(());
    });
    ConptyInputThread { handle, done_rx }
}

#[cfg(windows)]
fn run_conpty_input_bridge(
    writer: &mut Box<dyn Write + Send>,
    master: &dyn portable_pty::MasterPty,
    cancel: &Arc<AtomicBool>,
    input_command_rx: &mpsc::Receiver<ConptyInputCommand>,
    event_tx: &mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> Result<()> {
    while !cancel.load(Ordering::SeqCst) {
        drain_conpty_input_commands(writer, input_command_rx, options)?;
        if !event::poll(Duration::from_millis(20))
            .context("failed to poll terminal input for ConPTY")?
        {
            continue;
        }
        match event::read().context("failed to read terminal input for ConPTY")? {
            Event::Key(key) => {
                if key_event_is_ctrl_c(key) {
                    conpty_debug(options.debug, "ctrl-c input event received");
                    cancel.store(true, Ordering::SeqCst);
                    send_conpty_event(event_tx, ConptyEvent::UserAbort);
                    return Ok(());
                }
                if let Some(bytes) = key_event_to_pty_bytes(key) {
                    writer
                        .write_all(&bytes)
                        .context("failed to write terminal input to ConPTY")?;
                    writer
                        .flush()
                        .context("failed to flush terminal input to ConPTY")?;
                }
            }
            Event::Resize(cols, rows) => {
                master
                    .resize(pty_size(cols, rows))
                    .context("failed to resize ConPTY")?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(windows)]
fn drain_conpty_input_commands(
    writer: &mut Box<dyn Write + Send>,
    input_command_rx: &mpsc::Receiver<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> Result<()> {
    while let Ok(ConptyInputCommand::WriteSynthetic { response }) = input_command_rx.try_recv() {
        writer
            .write_all(response.bytes())
            .context("failed to write synthetic terminal response to ConPTY")?;
        writer
            .flush()
            .context("failed to flush synthetic terminal response to ConPTY")?;
        conpty_debug(
            options.debug,
            format_args!(
                "synthetic terminal response sent: {}",
                response.debug_label()
            ),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn spawn_conpty_wait_thread(
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    cancel: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    options: ConptyRunOptions,
) -> ConptyWaitThread {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        conpty_debug(options.debug, "child wait started");
        let mut cancel_seen_at: Option<Instant> = None;
        let message = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let exit_code = Some(conpty_exit_code(&status));
                    let exit_code_text = exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unavailable".to_string());
                    conpty_debug(
                        options.debug,
                        format_args!("child exited: code {exit_code_text}"),
                    );
                    break ConptyLoopMessage::Event(ConptyEvent::ChildExited { exit_code });
                }
                Ok(None) => {}
                Err(err) => {
                    break ConptyLoopMessage::ChildWaitError {
                        message: err.to_string(),
                    }
                }
            }
            if cancel.load(Ordering::SeqCst) {
                let first_seen = cancel_seen_at.get_or_insert_with(Instant::now);
                if first_seen.elapsed() >= Duration::from_secs(2) {
                    break ConptyLoopMessage::ChildWaitError {
                        message: "ConPTY child did not exit after shutdown request".to_string(),
                    };
                }
            }
            thread::sleep(Duration::from_millis(20));
        };
        let _ = event_tx.send(message);
        let _ = done_tx.send(());
    });
    ConptyWaitThread { handle, done_rx }
}

#[cfg(windows)]
fn spawn_conpty_startup_timeout_thread(
    timeout: Option<Duration>,
    first_output_received: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ConptyLoopMessage>,
    mut child_killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    options: ConptyRunOptions,
) -> Option<ConptyTimerThread> {
    let Some(timeout) = timeout else {
        conpty_debug(options.debug, "startup timeout disabled");
        return None;
    };
    conpty_debug(
        options.debug,
        format_args!("startup timeout armed: {} seconds", timeout.as_secs()),
    );
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if cancel.load(Ordering::SeqCst) || first_output_received.load(Ordering::SeqCst) {
                let _ = done_tx.send(());
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if !cancel.load(Ordering::SeqCst) && !first_output_received.load(Ordering::SeqCst) {
            conpty_debug(options.debug, "startup watchdog killing child");
            if let Err(err) = child_killer.kill() {
                conpty_debug(
                    options.debug,
                    format_args!("startup watchdog child kill failed: {err}"),
                );
            }
            send_conpty_event(&event_tx, ConptyEvent::StartupTimeout);
        }
        let _ = done_tx.send(());
    });
    Some(ConptyTimerThread { handle, done_rx })
}

#[cfg(windows)]
fn kill_conpty_child(
    child_killer: &mut Box<dyn portable_pty::ChildKiller + Send + Sync>,
    options: ConptyRunOptions,
) {
    conpty_debug(options.debug, "killing child");
    if let Err(err) = child_killer.kill() {
        conpty_debug(options.debug, format_args!("child kill failed: {err}"));
    } else {
        conpty_debug(options.debug, "child killed");
    }
}

#[cfg(windows)]
fn join_conpty_input_thread(thread: ConptyInputThread, timeout: Duration) -> Result<()> {
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY input bridge thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY input bridge did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

#[cfg(windows)]
fn join_conpty_wait_thread_best_effort(thread: ConptyWaitThread, timeout: Duration) -> Result<()> {
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY child wait thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY child wait did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

#[cfg(windows)]
fn join_conpty_timer_thread_best_effort(
    thread: Option<ConptyTimerThread>,
    timeout: Duration,
) -> Result<()> {
    let Some(thread) = thread else {
        return Ok(());
    };
    match thread.done_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => match thread.handle.join() {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("ConPTY startup timer thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY startup timer did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

#[cfg(windows)]
fn join_conpty_output_thread(output_thread: ConptyOutputThread, timeout: Duration) -> Result<u64> {
    match output_thread.result_rx.recv_timeout(timeout) {
        Ok(result) => {
            match output_thread.handle.join() {
                Ok(()) => {}
                Err(_) => return Err(anyhow!("ConPTY output thread panicked")),
            }
            result.context("ConPTY output tee failed")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => match output_thread.handle.join() {
            Ok(()) => Ok(0),
            Err(_) => Err(anyhow!("ConPTY output thread panicked")),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
            "ConPTY output reader did not stop within {} ms",
            timeout.as_millis()
        )),
    }
}

#[cfg(windows)]
fn join_conpty_output_thread_best_effort(
    output_thread: ConptyOutputThread,
    timeout: Duration,
) -> Result<Option<u64>> {
    match join_conpty_output_thread(output_thread, timeout) {
        Ok(total) => Ok(Some(total)),
        Err(err) => Err(err),
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConptyOutputParseState {
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

#[cfg(windows)]
#[derive(Debug, Default, PartialEq, Eq)]
struct ConptyOutputInspection {
    has_visible_output: bool,
    synthetic_responses: Vec<ConptySyntheticResponse>,
}

#[cfg(windows)]
struct ConptyOutputInspector {
    state: ConptyOutputParseState,
    csi_body: Vec<u8>,
}

#[cfg(windows)]
impl Default for ConptyOutputInspector {
    fn default() -> Self {
        Self {
            state: ConptyOutputParseState::Ground,
            csi_body: Vec::new(),
        }
    }
}

#[cfg(windows)]
impl ConptyOutputInspector {
    fn inspect(&mut self, bytes: &[u8]) -> ConptyOutputInspection {
        let mut inspection = ConptyOutputInspection::default();
        for &byte in bytes {
            match self.state {
                ConptyOutputParseState::Ground => match byte {
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    0x9b => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    0x9d => self.state = ConptyOutputParseState::Osc,
                    0x00..=0x1f | 0x7f | 0x80..=0x9f => {}
                    _ => inspection.has_visible_output = true,
                },
                ConptyOutputParseState::Escape => match byte {
                    b'[' => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    b']' => self.state = ConptyOutputParseState::Osc,
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    _ => self.state = ConptyOutputParseState::Ground,
                },
                ConptyOutputParseState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        if let Some(response) =
                            conpty_synthetic_response_for_csi(&self.csi_body, byte)
                        {
                            inspection.synthetic_responses.push(response);
                        }
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Ground;
                    } else if self.csi_body.len() < 64 {
                        self.csi_body.push(byte);
                    }
                }
                ConptyOutputParseState::Osc => match byte {
                    0x07 => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => {}
                },
                ConptyOutputParseState::OscEscape => match byte {
                    b'\\' => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => self.state = ConptyOutputParseState::Osc,
                },
            }
        }
        inspection
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConptyLogCell {
    ch: char,
    width: usize,
}

#[cfg(windows)]
struct ConptyLogSanitizer {
    state: ConptyOutputParseState,
    csi_body: Vec<u8>,
    pending_text: Vec<u8>,
    line: Vec<ConptyLogCell>,
    cursor_col: usize,
    row: usize,
}

#[cfg(windows)]
impl Default for ConptyLogSanitizer {
    fn default() -> Self {
        Self {
            state: ConptyOutputParseState::Ground,
            csi_body: Vec::new(),
            pending_text: Vec::new(),
            line: Vec::new(),
            cursor_col: 0,
            row: 1,
        }
    }
}

#[cfg(windows)]
impl ConptyLogSanitizer {
    fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            match self.state {
                ConptyOutputParseState::Ground => match byte {
                    0x1b => {
                        self.flush_pending_text(&mut out);
                        self.state = ConptyOutputParseState::Escape;
                    }
                    0x9b => {
                        self.flush_pending_text(&mut out);
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    0x9d => {
                        self.flush_pending_text(&mut out);
                        self.state = ConptyOutputParseState::Osc;
                    }
                    b'\n' => {
                        self.flush_pending_text(&mut out);
                        self.flush_line(&mut out);
                    }
                    b'\r' => {
                        self.flush_pending_text(&mut out);
                        self.cursor_col = 0;
                    }
                    b'\t' => {
                        self.flush_pending_text(&mut out);
                        let next_tab = ((self.cursor_col / 8) + 1) * 8;
                        while self.cursor_col < next_tab {
                            self.write_char(' ');
                        }
                    }
                    0x07 | 0x00..=0x08 | 0x0b..=0x1f | 0x7f => {
                        self.flush_pending_text(&mut out);
                    }
                    _ => self.pending_text.push(byte),
                },
                ConptyOutputParseState::Escape => match byte {
                    b'[' => {
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Csi;
                    }
                    b']' => self.state = ConptyOutputParseState::Osc,
                    0x1b => self.state = ConptyOutputParseState::Escape,
                    _ => self.state = ConptyOutputParseState::Ground,
                },
                ConptyOutputParseState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.apply_csi(byte, &mut out);
                        self.csi_body.clear();
                        self.state = ConptyOutputParseState::Ground;
                    } else if self.csi_body.len() < 64 {
                        self.csi_body.push(byte);
                    }
                }
                ConptyOutputParseState::Osc => match byte {
                    0x07 => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => {}
                },
                ConptyOutputParseState::OscEscape => match byte {
                    b'\\' => self.state = ConptyOutputParseState::Ground,
                    0x1b => self.state = ConptyOutputParseState::OscEscape,
                    _ => self.state = ConptyOutputParseState::Osc,
                },
            }
        }
        self.flush_pending_text(&mut out);
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.flush_pending_text(&mut out);
        if !self.line.is_empty() {
            self.flush_line(&mut out);
        }
        out
    }

    fn apply_csi(&mut self, final_byte: u8, out: &mut Vec<u8>) {
        match final_byte {
            b'C' => {
                let count = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = self.cursor_col.saturating_add(count.max(1));
            }
            b'D' => {
                let count = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = self.cursor_col.saturating_sub(count.max(1));
            }
            b'G' => {
                let col = csi_params(&self.csi_body).first().copied().unwrap_or(1);
                self.cursor_col = col.saturating_sub(1);
            }
            b'H' | b'f' => {
                let params = csi_params(&self.csi_body);
                let row = params.first().copied().unwrap_or(1).max(1);
                let col = params.get(1).copied().unwrap_or(1).max(1);
                if row > self.row && !self.line.is_empty() {
                    self.flush_line(out);
                }
                self.row = row;
                self.cursor_col = col - 1;
            }
            b'K' => {
                let mode = csi_params(&self.csi_body).first().copied().unwrap_or(0);
                match mode {
                    0 => self.truncate_line_to_cursor(),
                    1 => {
                        let suffix = self.split_line_at_cursor();
                        self.line = suffix;
                        self.cursor_col = 0;
                    }
                    2 => {
                        self.line.clear();
                        self.cursor_col = 0;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn flush_pending_text(&mut self, _out: &mut Vec<u8>) {
        if self.pending_text.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.pending_text).into_owned();
        self.pending_text.clear();
        for ch in text.chars() {
            self.write_char(ch);
        }
    }

    fn write_char(&mut self, ch: char) {
        let width = display_width(ch);
        if width == 0 {
            return;
        }
        self.pad_to_cursor();
        self.truncate_line_to_cursor();
        self.line.push(ConptyLogCell { ch, width });
        self.cursor_col = self.cursor_col.saturating_add(width);
    }

    fn pad_to_cursor(&mut self) {
        let current_width = self.line_width();
        if self.cursor_col <= current_width {
            return;
        }
        for _ in 0..(self.cursor_col - current_width) {
            self.line.push(ConptyLogCell { ch: ' ', width: 1 });
        }
    }

    fn truncate_line_to_cursor(&mut self) {
        let keep = self.index_for_col(self.cursor_col);
        self.line.truncate(keep);
    }

    fn split_line_at_cursor(&self) -> Vec<ConptyLogCell> {
        let start = self.index_for_col(self.cursor_col);
        self.line[start..].to_vec()
    }

    fn index_for_col(&self, col: usize) -> usize {
        let mut width = 0;
        for (index, cell) in self.line.iter().enumerate() {
            if width >= col || width.saturating_add(cell.width) > col {
                return index;
            }
            width += cell.width;
        }
        self.line.len()
    }

    fn line_width(&self) -> usize {
        self.line.iter().map(|cell| cell.width).sum()
    }

    fn flush_line(&mut self, out: &mut Vec<u8>) {
        for cell in &self.line {
            let mut encoded = [0_u8; 4];
            out.extend_from_slice(cell.ch.encode_utf8(&mut encoded).as_bytes());
        }
        out.push(b'\n');
        self.line.clear();
        self.cursor_col = 0;
        self.row = self.row.saturating_add(1);
    }
}

#[cfg(windows)]
fn csi_params(body: &[u8]) -> Vec<usize> {
    let raw = String::from_utf8_lossy(body);
    raw.trim_start_matches('?')
        .split(';')
        .filter_map(|part| part.parse::<usize>().ok())
        .collect()
}

#[cfg(windows)]
fn display_width(ch: char) -> usize {
    if ch == '\u{0}' || ch.is_control() {
        0
    } else if is_wide_char(ch) {
        2
    } else {
        1
    }
}

#[cfg(windows)]
fn is_wide_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x115f
            | 0x2329..=0x232a
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f300..=0x1f64f
            | 0x1f900..=0x1f9ff
            | 0x20000..=0x3fffd
    )
}

#[cfg(windows)]
fn conpty_synthetic_response_for_csi(
    body: &[u8],
    final_byte: u8,
) -> Option<ConptySyntheticResponse> {
    match (body, final_byte) {
        (b"6", b'n') => Some(ConptySyntheticResponse::CursorPosition),
        (b"5", b'n') => Some(ConptySyntheticResponse::DeviceStatusOk),
        _ => None,
    }
}

#[cfg(windows)]
fn tee_conpty_output(
    reader: &mut Box<dyn Read + Send>,
    mut log_file: std::fs::File,
    first_output_received: Arc<AtomicBool>,
    event_tx: &mpsc::Sender<ConptyLoopMessage>,
    input_command_tx: &mpsc::Sender<ConptyInputCommand>,
    options: ConptyRunOptions,
) -> io::Result<u64> {
    conpty_debug(options.debug, "output reader started");
    let mut stdout = io::stdout();
    let mut buffer = [0_u8; 8192];
    let mut total_bytes = 0_u64;
    let mut inspector = ConptyOutputInspector::default();
    let mut log_sanitizer = ConptyLogSanitizer::default();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                total_bytes += n as u64;
                let inspection = inspector.inspect(&buffer[..n]);
                for response in inspection.synthetic_responses {
                    conpty_debug(
                        options.debug,
                        format_args!("terminal query detected: {}", response.debug_label()),
                    );
                    let _ = input_command_tx.send(ConptyInputCommand::WriteSynthetic { response });
                }
                if inspection.has_visible_output
                    && !first_output_received.swap(true, Ordering::SeqCst)
                {
                    send_conpty_event(event_tx, ConptyEvent::FirstOutput { bytes: n });
                } else if !inspection.has_visible_output
                    && !first_output_received.load(Ordering::SeqCst)
                {
                    conpty_debug(
                        options.debug,
                        format_args!("startup control output ignored: {n} bytes"),
                    );
                }
                send_conpty_event(event_tx, ConptyEvent::OutputChunk { bytes: n });
                let sanitized = log_sanitizer.push(&buffer[..n]);
                if !sanitized.is_empty() {
                    log_file.write_all(&sanitized)?;
                    log_file.flush()?;
                }
                stdout.write_all(&buffer[..n])?;
                stdout.flush()?;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::BrokenPipe
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::UnexpectedEof
                ) =>
            {
                break
            }
            Err(err) => return Err(err),
        }
    }
    let sanitized = log_sanitizer.finish();
    if !sanitized.is_empty() {
        log_file.write_all(&sanitized)?;
        log_file.flush()?;
    }
    conpty_debug(
        options.debug,
        format_args!("output reader ended: {total_bytes} bytes"),
    );
    Ok(total_bytes)
}

#[cfg(windows)]
fn current_pty_size() -> PtySize {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    pty_size(cols, rows)
}

#[cfg(windows)]
fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[cfg(windows)]
fn conpty_exit_code(status: &portable_pty::ExitStatus) -> i32 {
    i32::try_from(status.exit_code()).unwrap_or(i32::MAX)
}

#[cfg(windows)]
fn key_event_to_pty_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    match key.code {
        KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ctrl_char_to_byte(ch).map(|byte| vec![byte])
        }
        KeyCode::Char(ch) => {
            let mut encoded = [0_u8; 4];
            Some(ch.encode_utf8(&mut encoded).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(b"\r".to_vec()),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(b"\t".to_vec()),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        _ => None,
    }
}

#[cfg(windows)]
fn key_event_is_ctrl_c(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(windows)]
fn ctrl_char_to_byte(ch: char) -> Option<u8> {
    let upper = ch.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some((upper as u8) - b'A' + 1)
    } else if ch == ' ' {
        Some(0)
    } else {
        None
    }
}

fn emit_session_log_notice(plan: &SessionLogPlan) {
    if let Some(notice) = plan.notice() {
        eprintln!("{notice}");
        if matches!(
            plan,
            SessionLogPlan::Script { .. } | SessionLogPlan::PowerShellTranscript { .. }
        ) {
            eprintln!(
                "TeraDock does not mask terminal output; passwords, tokens, or secrets shown on screen may be captured."
            );
        }
    }
}

fn ssh_connect_meta(
    invocation: &SshInvocation,
    launch_error: Option<&str>,
    session_log: &SessionLogReference,
) -> serde_json::Value {
    let mut meta = invocation.safe_metadata.clone();
    if let Some(error) = launch_error {
        meta["launch_error"] = serde_json::Value::String(error.to_string());
    }
    session_log::add_reference_to_meta(&mut meta, session_log);
    meta
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
    ssh::resolve_client_for(kind, profile_overrides, store.conn()).map_err(Into::into)
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
    let debug_enabled = teradock_debug_enabled();

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(non_blocking)
        .with_filter(teradock_log_filter(debug_enabled, false)?);
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_filter(teradock_log_filter(debug_enabled, true)?);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .context("failed to initialize logging")?;

    Ok(guard)
}

fn teradock_log_filter(debug_enabled: bool, console: bool) -> Result<EnvFilter> {
    let directive = match (debug_enabled, console) {
        (true, true) => "warn,td=debug,tdcore=debug",
        (true, false) => "warn,td=debug,tdcore=debug",
        (false, true) => "warn",
        (false, false) => "warn,td=info,tdcore=info",
    };
    EnvFilter::try_new(directive).context("failed to configure tracing filter")
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
    fn parses_config_ui() {
        let cli = Cli::try_parse_from(["td", "config", "ui"]).expect("parses config ui");

        match cli.command {
            Some(Commands::Config {
                command: ConfigCommands::Ui,
            }) => {}
            _ => panic!("expected config ui command"),
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
            "--parser",
            "json",
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
                parser,
                cmd,
            }) => {
                assert_eq!(profile_id, "p1");
                assert_eq!(timeout_ms, Some(5000));
                assert!(json);
                assert_eq!(parser.as_deref(), Some("json"));
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
    fn parses_recent_command() {
        let cli =
            Cli::try_parse_from(["td", "recent", "--limit", "5", "--json"]).expect("parses recent");

        match cli.command {
            Some(Commands::Recent { limit, json }) => {
                assert_eq!(limit, 5);
                assert!(json);
            }
            _ => panic!("expected recent command"),
        }
    }

    #[test]
    fn parses_session_list_command() {
        let cli = Cli::try_parse_from(["td", "session", "list", "--limit", "5", "--json"])
            .expect("parses session list");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::List(args),
            }) => {
                assert_eq!(args.limit, 5);
                assert!(args.json);
            }
            _ => panic!("expected session list command"),
        }
    }

    #[test]
    fn parses_session_doctor_command() {
        let cli = Cli::try_parse_from(["td", "session", "doctor", "--json"])
            .expect("parses session doctor");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::Doctor(args),
            }) => {
                assert!(args.json);
            }
            _ => panic!("expected session doctor command"),
        }
    }

    #[test]
    fn parses_session_path_command() {
        let cli = Cli::try_parse_from(["td", "session", "path", "sl_abc123"])
            .expect("parses session path");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::Path { session_id },
            }) => {
                assert_eq!(session_id, "sl_abc123");
            }
            _ => panic!("expected session path command"),
        }
    }

    #[test]
    fn parses_session_conpty_test_command() {
        let cli = Cli::try_parse_from(["td", "session", "conpty-test", "p_test"])
            .expect("parses session conpty-test");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::ConptyTest(args),
            }) => {
                assert_eq!(args.profile_id, "p_test");
                assert_eq!(args.startup_timeout_sec, 10);
                assert!(!args.debug);
            }
            _ => panic!("expected session conpty-test command"),
        }
    }

    #[test]
    fn parses_session_conpty_test_debug_command() {
        let cli = Cli::try_parse_from(["td", "session", "conpty-test", "p_test", "--debug"])
            .expect("parses session conpty-test --debug");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::ConptyTest(args),
            }) => {
                assert_eq!(args.profile_id, "p_test");
                assert_eq!(args.startup_timeout_sec, 10);
                assert!(args.debug);
            }
            _ => panic!("expected session conpty-test command"),
        }
    }

    #[test]
    fn parses_session_conpty_test_startup_timeout_command() {
        let cli = Cli::try_parse_from([
            "td",
            "session",
            "conpty-test",
            "p_test",
            "--startup-timeout-sec",
            "0",
        ])
        .expect("parses session conpty-test --startup-timeout-sec");

        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::ConptyTest(args),
            }) => {
                assert_eq!(args.profile_id, "p_test");
                assert_eq!(args.startup_timeout_sec, 0);
                assert_eq!(conpty_startup_timeout(args.startup_timeout_sec), None);
            }
            _ => panic!("expected session conpty-test command"),
        }
    }

    #[test]
    fn conpty_test_invocation_rejects_non_ssh_profile() {
        let store = ProfileStore::new(db::init_in_memory().unwrap());
        store
            .insert(NewProfile {
                profile_id: Some("p_telnet".to_string()),
                name: "Telnet".to_string(),
                profile_type: ProfileType::Telnet,
                host: "example.com".to_string(),
                port: 23,
                user: "alice".to_string(),
                danger_level: DangerLevel::Normal,
                group: None,
                tags: Vec::new(),
                note: None,
                initial_send: None,
                client_overrides: None,
            })
            .unwrap();

        let err = build_conpty_test_invocation(&store, "p_telnet").unwrap_err();

        assert!(err.to_string().contains("SSH requires an SSH profile"));
    }

    #[cfg(not(windows))]
    #[test]
    fn conpty_test_is_unsupported_on_non_windows() {
        let store = ProfileStore::new(db::init_in_memory().unwrap());

        let err = handle_session_conpty_test(
            &store,
            SessionConptyTestArgs {
                profile_id: "p_test".to_string(),
                startup_timeout_sec: 10,
                debug: false,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsupported"));
    }

    #[cfg(windows)]
    #[test]
    fn conpty_key_mapping_handles_text_arrows_and_ctrl_c() {
        use crossterm::event::KeyEventState;

        assert_eq!(
            key_event_to_pty_bytes(KeyEvent {
                code: KeyCode::Char('x'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }),
            Some(b"x".to_vec())
        );
        assert_eq!(
            key_event_to_pty_bytes(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }),
            Some(vec![0x03])
        );
        assert_eq!(
            key_event_to_pty_bytes(KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }),
            Some(b"\x1b[D".to_vec())
        );
    }

    #[cfg(windows)]
    #[test]
    fn conpty_pty_size_clamps_zero_dimensions() {
        let size = pty_size(0, 0);

        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
        assert_eq!(size.pixel_width, 0);
        assert_eq!(size.pixel_height, 0);
    }

    #[cfg(windows)]
    #[test]
    fn conpty_exit_code_saturates_to_metadata_range() {
        assert_eq!(
            conpty_exit_code(&portable_pty::ExitStatus::with_exit_code(0)),
            0
        );
        assert_eq!(
            conpty_exit_code(&portable_pty::ExitStatus::with_exit_code(255)),
            255
        );
        assert_eq!(
            conpty_exit_code(&portable_pty::ExitStatus::with_exit_code(u32::MAX)),
            i32::MAX
        );
    }

    #[cfg(windows)]
    #[test]
    fn conpty_output_inspector_ignores_cursor_position_query_as_startup_output() {
        let mut inspector = ConptyOutputInspector::default();

        let inspection = inspector.inspect(b"\x1b[6n");

        assert!(!inspection.has_visible_output);
        assert_eq!(
            inspection.synthetic_responses,
            vec![ConptySyntheticResponse::CursorPosition]
        );
    }

    #[cfg(windows)]
    #[test]
    fn conpty_output_inspector_handles_split_cursor_position_query() {
        let mut inspector = ConptyOutputInspector::default();

        let first = inspector.inspect(b"\x1b[");
        let second = inspector.inspect(b"6n");

        assert!(!first.has_visible_output);
        assert!(first.synthetic_responses.is_empty());
        assert!(!second.has_visible_output);
        assert_eq!(
            second.synthetic_responses,
            vec![ConptySyntheticResponse::CursorPosition]
        );
    }

    #[cfg(windows)]
    #[test]
    fn conpty_output_inspector_treats_prompt_text_as_startup_output() {
        let mut inspector = ConptyOutputInspector::default();

        let inspection = inspector.inspect(b"\x1b[?2004hPassword: ");

        assert!(inspection.has_visible_output);
        assert!(inspection.synthetic_responses.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn conpty_loop_receive_synthesizes_startup_timeout_without_worker_event() {
        let (_tx, rx) = mpsc::channel();
        let first_output_received = AtomicBool::new(false);

        let message = recv_conpty_loop_message(
            &rx,
            &first_output_received,
            Some(Instant::now() + Duration::from_millis(1)),
        )
        .expect("startup timeout message");

        assert!(matches!(
            message,
            ConptyLoopMessage::Event(ConptyEvent::StartupTimeout)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn conpty_log_sanitizer_strips_color_and_clear_sequences() {
        let mut sanitizer = ConptyLogSanitizer::default();

        let mut out = sanitizer.push(b"drwx <ESC>");
        out.extend(sanitizer.push(b"\x1b[34m.\x1b[m/\x1b[K\n"));

        assert_eq!(String::from_utf8(out).unwrap(), "drwx <ESC>./\n");
    }

    #[cfg(windows)]
    #[test]
    fn conpty_log_sanitizer_turns_cursor_forward_into_spacing() {
        let mut sanitizer = ConptyLogSanitizer::default();

        let out = sanitizer.push(b"nico@ao03:~$\x1b[1Cdf -h\r\n");

        assert_eq!(String::from_utf8(out).unwrap(), "nico@ao03:~$ df -h\n");
    }

    #[cfg(windows)]
    #[test]
    fn conpty_log_sanitizer_applies_prompt_line_clear_redraw() {
        let mut sanitizer = ConptyLogSanitizer {
            row: 18,
            ..Default::default()
        };

        let mut out = sanitizer.push("nico@ao03:~$ ｌｌ".as_bytes());
        out.extend(sanitizer.push(b"\x1b[18;16H\x1b[K\x1b[18;14H\x1b[K\x07ll\r"));
        out.extend(sanitizer.push(b"\x1b[?2004l\n"));

        assert_eq!(String::from_utf8(out).unwrap(), "nico@ao03:~$ ll\n");
    }

    #[cfg(windows)]
    #[test]
    fn conpty_log_sanitizer_splits_absolute_cursor_rows_into_lines() {
        let mut sanitizer = ConptyLogSanitizer::default();

        let mut out = sanitizer.push(b"Welcome\x1b[3;1H * Documentation\n");
        out.extend(sanitizer.finish());

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Welcome\n * Documentation\n"
        );
    }

    #[cfg(windows)]
    #[test]
    fn conpty_log_sanitizer_preserves_utf8_japanese_text() {
        let mut sanitizer = ConptyLogSanitizer::default();

        let out = sanitizer.push("6月 15日 日本語\n".as_bytes());

        assert_eq!(String::from_utf8(out).unwrap(), "6月 15日 日本語\n");
    }

    #[cfg(windows)]
    #[test]
    fn session_log_display_text_sanitizes_existing_raw_conpty_logs() {
        let raw = "Welcome\x1b[18;1H\
                   nico@ao03:~$ ｌｌ\x1b[18;16H\x1b[K\x1b[18;14H\x1b[K\x07ll\r\x1b[?2004l\n\
                   nico@ao03:~$\x1b[1Cdf -h\r\n\
                   drwx \x1b[34m.\x1b[m/\x1b[K\n";

        let display = session_log_display_text(raw.as_bytes());

        assert_eq!(
            display,
            "Welcome\nnico@ao03:~$ ll\nnico@ao03:~$ df -h\ndrwx ./\n"
        );
    }

    #[test]
    fn session_show_capture_lines_include_host_only_warning() {
        let metadata = session_log::SessionLogMetadata {
            session_id: "sl_abc123".to_string(),
            profile_id: "p_test".to_string(),
            user: "alice".to_string(),
            host: "example.com".to_string(),
            port: 22,
            started_at: 1000,
            ended_at: 2000,
            duration_ms: 1000,
            exit_code: Some(0),
            backend: session_log::SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT.to_string(),
            log_path: Some(PathBuf::from("sl_abc123.log")),
            metadata_path: PathBuf::from("sl_abc123.json"),
            status: "completed".to_string(),
            reason: None,
            failure_phase: None,
            failure_reason: None,
            content_capture: Some(session_log::SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT.to_string()),
            content_capture_reliable: Some(false),
            backend_warning: Some(
                session_log::SESSION_LOG_BACKEND_WARNING_POWERSHELL_TRANSCRIPT.to_string(),
            ),
            content_capture_status: Some(
                session_log::SESSION_LOG_CAPTURE_STATUS_HOST_ONLY_OR_EMPTY.to_string(),
            ),
            content_capture_warning: Some(
                session_log::SESSION_LOG_CAPTURE_WARNING_NO_SSH_CONTENT.to_string(),
            ),
        };

        let lines = session_capture_lines(&metadata);

        assert!(lines.iter().any(|line| line == "backend_status: degraded"));
        assert!(lines
            .iter()
            .any(|line| line == "content_capture: best_effort"));
        assert!(lines
            .iter()
            .any(|line| line == "content_capture_reliable: false"));
        assert!(lines
            .iter()
            .any(|line| line == "Content capture: host_only_or_empty"));
        assert!(lines
            .iter()
            .any(|line| line == "Warning: No SSH terminal content appears to have been captured."));
    }

    #[test]
    fn session_show_capture_lines_include_conpty_degraded_warning() {
        let metadata = session_log::SessionLogMetadata {
            session_id: "sl_abc123".to_string(),
            profile_id: "p_test".to_string(),
            user: "alice".to_string(),
            host: "example.com".to_string(),
            port: 22,
            started_at: 100,
            ended_at: 200,
            duration_ms: 100,
            exit_code: Some(0),
            backend: session_log::SESSION_LOG_BACKEND_CONPTY.to_string(),
            log_path: Some(PathBuf::from("sl_abc123.log")),
            metadata_path: PathBuf::from("sl_abc123.json"),
            status: "completed".to_string(),
            reason: None,
            failure_phase: None,
            failure_reason: None,
            content_capture: Some(session_log::SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT.to_string()),
            content_capture_reliable: Some(false),
            backend_warning: Some(
                session_log::SESSION_LOG_BACKEND_WARNING_CONPTY_EXPERIMENTAL.to_string(),
            ),
            content_capture_status: None,
            content_capture_warning: None,
        };

        let lines = session_capture_lines(&metadata);

        assert!(lines.iter().any(|line| line == "backend_status: degraded"));
        assert!(lines
            .iter()
            .any(|line| line == "backend_warning: conpty_backend_is_experimental_poc"));
    }

    #[test]
    fn session_log_path_display_ignores_warning_text() {
        assert_eq!(
            format_session_log_path(Some(Path::new(
                "not selected by auto and is not integrated with the TUI."
            ))),
            "<none>"
        );
        assert_eq!(
            format_session_log_path(Some(Path::new("sl_abc123.log"))),
            "sl_abc123.log"
        );
        assert_eq!(format_session_log_path(None), "<none>");
    }

    #[test]
    fn session_list_table_cells_are_bounded() {
        assert_eq!(table_cell("completed exit 0", 18), "completed exit 0");
        assert_eq!(table_cell("abcdefghijklmnopqrstuvwxyz", 8), "abcdefg~");
    }

    #[test]
    fn session_log_config_defaults_are_resolved() {
        let conn = db::init_in_memory().unwrap();

        assert_eq!(
            default_resolved_config_value(&conn, session_log::SESSION_LOG_ENABLED_KEY).unwrap(),
            Some("false".to_string())
        );
        assert_eq!(
            default_resolved_config_value(&conn, session_log::SESSION_LOG_BACKEND_KEY).unwrap(),
            Some("auto".to_string())
        );
    }

    #[test]
    fn parses_init_with_samples() {
        let cli = Cli::try_parse_from(["td", "init", "--with-samples"]).expect("parses init");

        match cli.command {
            Some(Commands::Init(args)) => {
                assert!(args.with_samples);
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn sample_cmdset_install_is_idempotent() {
        let conn = db::init_in_memory().unwrap();
        let mut store = CmdSetStore::new(conn);

        let first = install_sample_cmdsets(&mut store).unwrap();
        let second = install_sample_cmdsets(&mut store).unwrap();

        assert_eq!(first[0].status, SampleInstallStatus::Created);
        assert_eq!(second[0].status, SampleInstallStatus::Skipped);
        let steps = store.list_steps("linux-basic-check").unwrap();
        assert_eq!(steps.len(), 5);
    }
}
