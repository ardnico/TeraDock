use std::collections::HashMap;
use std::io::{self, Write};
use std::process::Command;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use ttcore::{
    command::{build_command, confirmation_message},
    config::{AppConfig, AppPaths},
    history::{format_history_entry, HistoryEntry, HistoryStore},
    profile::ProfileSet,
    secrets::SecretStore,
    Error, Result,
};

#[derive(Parser, Debug)]
#[command(name = "ttlaunch", author, version, about = "Tera Term launcher CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available profiles
    List {
        #[arg(long)]
        json: bool,
    },
    /// Connect to a profile
    Connect {
        profile_id: String,
        #[arg(long)]
        force: bool,
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Show recent history
    History {
        #[arg(long)]
        limit: Option<usize>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths)?;
    let secret_store = SecretStore::new(&paths.secret_key_path)?;
    let history = HistoryStore::new(&config.history_path);

    match cli.command {
        Commands::List { json } => list_profiles(&config, json),
        Commands::Connect {
            profile_id,
            force,
            dry_run,
        } => match connect(
            &config,
            &history,
            &secret_store,
            &profile_id,
            force,
            dry_run,
        ) {
            Ok(_) => Ok(()),
            Err(Error::ProfileNotFound(_)) => std::process::exit(2),
            Err(Error::MissingConfig(_)) => std::process::exit(64),
            Err(_) => std::process::exit(3),
        },
        Commands::History { limit } => show_history(&history, limit),
    }
}

fn load_profiles(config: &AppConfig) -> Result<ProfileSet> {
    Ok(ProfileSet::load(&config.profiles_path)?)
}

fn list_profiles(config: &AppConfig, json: bool) -> Result<()> {
    let set = load_profiles(config)?;
    let last_seen = build_last_seen_map(&HistoryStore::new(&config.history_path))?;
    let mut profiles = set.profiles;
    profiles.sort_by(|a, b| match b.pinned.cmp(&a.pinned) {
        std::cmp::Ordering::Equal => {
            let la = last_seen.get(&a.id);
            let lb = last_seen.get(&b.id);
            match lb.cmp(&la) {
                std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ord => ord,
            }
        }
        ord => ord,
    });
    if json {
        let text = serde_json::to_string_pretty(&profiles)?;
        println!("{}", text);
    } else {
        println!("ID\tName\tDanger\tPinned\tLast Connected");
        for p in profiles {
            let last = last_seen
                .get(&p.id)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{}\t{}\t{:?}\t{}\t{}",
                p.id,
                p.name,
                p.danger_level,
                if p.pinned { "yes" } else { "no" },
                last
            );
        }
    }
    Ok(())
}

fn build_last_seen_map(store: &HistoryStore) -> Result<HashMap<String, DateTime<Utc>>> {
    let mut map = HashMap::new();
    for entry in store.load(None)? {
        map.entry(entry.profile_id.clone())
            .and_modify(|existing| {
                if entry.timestamp > *existing {
                    *existing = entry.timestamp;
                }
            })
            .or_insert(entry.timestamp);
    }
    Ok(map)
}

fn connect(
    config: &AppConfig,
    history: &HistoryStore,
    secret_store: &SecretStore,
    profile_id: &str,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let set = load_profiles(config)?;
    let profile = set
        .find(profile_id)
        .ok_or_else(|| Error::ProfileNotFound(profile_id.to_string()))?;

    if profile.is_dangerous() && !force {
        println!("{}", confirmation_message(&profile));
        if !confirm()? {
            println!("Aborted by user");
            return Ok(());
        }
    }

    let password = if let Some(cipher) = profile.password.as_ref() {
        Some(secret_store.decrypt(cipher)?)
    } else {
        None
    };

    let spec = build_command(&profile, config, password.as_deref());

    if dry_run {
        print_command(&spec);
        history.append(&HistoryEntry::new(
            profile.id,
            profile.name,
            true,
            Some("dry-run".into()),
            force,
        ))?;
        return Ok(());
    }

    match Command::new(&spec.program).args(&spec.args).spawn() {
        Ok(child) => {
            println!("Spawned {} with pid {}", spec.program.display(), child.id());
            history.append(&HistoryEntry::new(
                profile.id,
                profile.name,
                true,
                Some("spawned".into()),
                force,
            ))?;
            Ok(())
        }
        Err(err) => {
            eprintln!("Failed to start {}: {}", spec.program.display(), err);
            history.append(&HistoryEntry::new(
                profile.id,
                profile.name,
                false,
                Some(err.to_string()),
                force,
            ))?;
            Err(err.into())
        }
    }
}

fn confirm() -> Result<bool> {
    print!("Proceed? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn print_command(spec: &ttcore::command::CommandSpec) {
    print!("{} ", spec.program.display());
    for arg in &spec.args {
        print!("{} ", arg.to_string_lossy());
    }
    println!();
}

fn show_history(store: &HistoryStore, limit: Option<usize>) -> Result<()> {
    let entries = store.load(limit)?;
    for entry in entries {
        println!("{}", format_history_entry(&entry));
    }
    Ok(())
}
