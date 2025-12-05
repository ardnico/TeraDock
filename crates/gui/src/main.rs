use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use eframe::egui::{self, Color32};
use tracing_subscriber::EnvFilter;

use ttcore::{
    command::build_command,
    config::{AppConfig, AppPaths},
    history::{HistoryEntry, HistoryStore},
    profile::{Profile, ProfileSet},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths)?;
    let profiles = ttcore::profile::ProfileSet::load(&config.profiles_path)?.profiles;
    let history_store = HistoryStore::new(&config.history_path);
    let history = history_store.load(Some(200))?;
    let last_seen = build_last_seen_map(&history);

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "TeraDock ttlaunch",
        options,
        Box::new(|_cc| {
            Box::new(LauncherApp {
                profiles,
                filter: String::new(),
                selected: None,
                config,
                history_store,
                history,
                last_seen,
                status: None,
                error: None,
                confirm: None,
                tera_term_input: String::new(),
                active_tab: Tab::Profiles,
            })
        }),
    )
    .map_err(|err| anyhow!(err.to_string()))?;

    Ok(())
}

#[derive(Debug, Clone)]
struct ConfirmState {
    profile: Profile,
    started: Instant,
}

impl ConfirmState {
    fn new(profile: Profile) -> Self {
        Self {
            profile,
            started: Instant::now(),
        }
    }

    fn ready(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(3)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Profiles,
    History,
    Settings,
}

struct LauncherApp {
    profiles: Vec<Profile>,
    filter: String,
    selected: Option<String>,
    config: AppConfig,
    history_store: HistoryStore,
    history: Vec<HistoryEntry>,
    last_seen: HashMap<String, DateTime<Utc>>,
    status: Option<String>,
    error: Option<String>,
    confirm: Option<ConfirmState>,
    tera_term_input: String,
    active_tab: Tab,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let filtered = self.filtered_profiles();
        if let Some(selected_id) = self.selected.clone() {
            if !filtered.iter().any(|p| p.id == selected_id) {
                self.selected = None;
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                let resp = ui.text_edit_singleline(&mut self.filter);
                if resp.changed() {
                    ctx.request_repaint();
                }
                ui.separator();
                ui.label(format!("{} profiles", filtered.len()));
            });
        });

        egui::SidePanel::left("profiles_left")
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Profiles");
                ui.separator();
                for profile in filtered.iter() {
                    let selected = self
                        .selected
                        .as_ref()
                        .map(|id| id == &profile.id)
                        .unwrap_or(false);
                    if ui
                        .selectable_label(selected, format!("{} ({})", profile.name, profile.id))
                        .clicked()
                    {
                        self.selected = Some(profile.id.clone());
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Profiles, "Profiles");
                ui.selectable_value(&mut self.active_tab, Tab::History, "History");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "Settings");
            });
            ui.separator();

            match self.active_tab {
                Tab::Profiles => self.render_profiles_tab(ui, ctx, &filtered),
                Tab::History => self.render_history_tab(ui),
                Tab::Settings => self.render_settings_tab(ui),
            }
        });

        if let Some(confirm) = self.confirm.take() {
            self.render_confirm_dialog(ctx, confirm);
        }
    }
}

impl LauncherApp {
    fn filtered_profiles(&self) -> Vec<Profile> {
        let mut list: Vec<Profile> = if self.filter.trim().is_empty() {
            self.profiles.clone()
        } else {
            let filter = self.filter.clone();
            self.profiles
                .iter()
                .filter(|p| p.matches_filter(&filter))
                .cloned()
                .collect()
        };
        list.sort_by(|a, b| match b.pinned.cmp(&a.pinned) {
            std::cmp::Ordering::Equal => {
                let la = self.last_seen.get(&a.id);
                let lb = self.last_seen.get(&b.id);
                match lb.cmp(&la) {
                    std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    ord => ord,
                }
            }
            ord => ord,
        });
        list
    }

    fn render_profiles_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        filtered: &[Profile],
    ) {
        if let Some(selected_id) = self.selected.clone() {
            if let Some(profile) = filtered.iter().find(|p| p.id == selected_id) {
                ui.heading(&profile.name);
                ui.label(format!("Host: {}", profile.host));
                if let Some(port) = profile.port {
                    ui.label(format!("Port: {}", port));
                }
                ui.label(format!("Protocol: {:?}", profile.protocol));
                if let Some(ts) = self.last_seen.get(&profile.id) {
                    ui.label(format!(
                        "Last connected: {}",
                        ts.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
                    ));
                }
                if let Some(user) = &profile.user {
                    ui.label(format!("User: {}", user));
                }
                if let Some(group) = &profile.group {
                    ui.label(format!("Group: {}", group));
                }
                ui.horizontal(|ui| {
                    ui.label("Pinned:");
                    ui.colored_label(
                        if profile.pinned {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::GRAY
                        },
                        if profile.pinned { "yes" } else { "no" },
                    );
                    if ui
                        .button(if profile.pinned { "Unpin" } else { "Pin" })
                        .clicked()
                    {
                        if let Err(err) = self.update_pin(&profile.id, !profile.pinned) {
                            self.error = Some(err.to_string());
                        }
                    }
                });
                if !profile.tags.is_empty() {
                    ui.label(format!("Tags: {}", profile.tags.join(", ")));
                }
                if let Some(desc) = &profile.description {
                    ui.label(desc);
                }
                if profile.is_dangerous() {
                    ui.colored_label(Color32::RED, "Dangerous connection");
                }

                ui.separator();
                if ui.button("Connect").clicked() {
                    if profile.is_dangerous() {
                        self.confirm = Some(ConfirmState::new(profile.clone()));
                    } else {
                        if let Err(err) = self.execute_connect(profile.clone()) {
                            self.error = Some(err.to_string());
                        }
                        ctx.request_repaint();
                    }
                }
                if let Some(status) = &self.status {
                    ui.label(status);
                }
                if let Some(err) = &self.error {
                    ui.colored_label(Color32::RED, err);
                }
            } else {
                ui.label("No profile selected.");
            }
        } else {
            ui.label("Select a profile from the left.");
        }
    }

    fn render_history_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recent history");
        ui.separator();
        for entry in &self.history {
            let color = if entry.success {
                Color32::LIGHT_GREEN
            } else {
                Color32::RED
            };
            ui.colored_label(
                color,
                format!(
                    "{} | {} | {} ({}) {}",
                    entry
                        .timestamp
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M:%S"),
                    if entry.success { "OK" } else { "FAIL" },
                    entry.profile_name,
                    entry.profile_id,
                    entry
                        .message
                        .as_ref()
                        .map(|m| format!("- {}", m))
                        .unwrap_or_default()
                ),
            );
        }
    }

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("Tera Term path");
        if self.tera_term_input.is_empty() {
            self.tera_term_input = self.config.tera_term_path.display().to_string();
        }
        ui.text_edit_singleline(&mut self.tera_term_input);
        if !std::path::Path::new(&self.tera_term_input).exists() {
            ui.colored_label(Color32::YELLOW, "Path does not exist on this system");
        }
        if ui.button("Save settings").clicked() {
            self.config.tera_term_path = std::path::PathBuf::from(self.tera_term_input.trim());
            if let Err(err) = self
                .config
                .save(&AppPaths::discover().unwrap().settings_path)
            {
                self.error = Some(err.to_string());
            } else {
                self.status = Some("Settings saved".into());
            }
        }
        ui.separator();
        ui.label(format!("Profiles: {}", self.config.profiles_path.display()));
        ui.label(format!("History: {}", self.config.history_path.display()));
    }

    fn render_confirm_dialog(&mut self, ctx: &egui::Context, confirm: ConfirmState) {
        let mut open = true;
        let ready = confirm.ready();
        egui::Window::new("Confirm connection")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.colored_label(Color32::RED, "Dangerous connection");
                ui.label(format!("{}", confirm.profile.name));
                ui.label(format!("Host: {}", confirm.profile.host));
                ui.label("Wait 3 seconds before enabling execution");
                let remaining = if ready {
                    0
                } else {
                    3i64.saturating_sub(confirm.started.elapsed().as_secs() as i64)
                };
                ui.label(format!("{} seconds remaining", remaining));
                let button = ui.add_enabled(ready, egui::Button::new("Run"));
                if button.clicked() {
                    if let Err(err) = self.execute_connect(confirm.profile.clone()) {
                        self.error = Some(err.to_string());
                    }
                }
            });
        if open {
            self.confirm = Some(confirm);
        }
    }

    fn execute_connect(&mut self, profile: Profile) -> Result<()> {
        let spec = build_command(&profile, &self.config);
        match Command::new(&spec.program).args(&spec.args).spawn() {
            Ok(child) => {
                self.status = Some(format!(
                    "Spawned {} (pid {})",
                    spec.program.display(),
                    child.id()
                ));
                let entry = HistoryEntry::new(
                    profile.id.clone(),
                    profile.name.clone(),
                    true,
                    Some("spawned".into()),
                    false,
                );
                self.history_store.append(&entry)?;
                self.history.insert(0, entry);
                self.last_seen.insert(profile.id.clone(), Utc::now());
            }
            Err(err) => {
                self.error = Some(err.to_string());
                let entry = HistoryEntry::new(
                    profile.id.clone(),
                    profile.name.clone(),
                    false,
                    Some(err.to_string()),
                    false,
                );
                self.history_store.append(&entry)?;
                self.history.insert(0, entry);
                self.last_seen.insert(profile.id.clone(), Utc::now());
            }
        }
        Ok(())
    }

    fn update_pin(&mut self, profile_id: &str, pinned: bool) -> Result<()> {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == profile_id) {
            p.pinned = pinned;
            self.save_profiles()?;
            self.status = Some(if pinned { "Pinned" } else { "Unpinned" }.into());
        }
        Ok(())
    }

    fn save_profiles(&self) -> Result<()> {
        let set = ProfileSet {
            profiles: self.profiles.clone(),
        };
        set.save(&self.config.profiles_path)?;
        Ok(())
    }
}

fn build_last_seen_map(history: &[HistoryEntry]) -> HashMap<String, DateTime<Utc>> {
    let mut map = HashMap::new();
    for entry in history {
        map.entry(entry.profile_id.clone())
            .and_modify(|existing| {
                if entry.timestamp > *existing {
                    *existing = entry.timestamp;
                }
            })
            .or_insert(entry.timestamp);
    }
    map
}
