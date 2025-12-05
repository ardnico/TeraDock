use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use eframe::egui::{self, Color32, FontId, TextStyle, Visuals};
use tracing_subscriber::EnvFilter;

use ttcore::{
    command::build_command,
    config::{AppConfig, AppPaths, ForwardingPreset, SecretBackend, ThemePreference},
    history::{HistoryEntry, HistoryStore},
    profile::{DangerLevel, ForwardDirection, Profile, ProfileSet, Protocol, SshForwarding},
    secrets::SecretStore,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths)?;
    let secret_store = SecretStore::new(&paths, &config.secrets)?;
    let profiles = ttcore::profile::ProfileSet::load(&config.profiles_path)?.profiles;
    let history_store = HistoryStore::new(&config.history_path);
    let history = history_store.load(Some(200))?;
    let last_seen = build_last_seen_map(&history);
    let preset_forms: Vec<ForwardingPresetForm> = config
        .forwarding_presets
        .iter()
        .map(ForwardingPresetForm::from_preset)
        .collect();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "TeraDock ttlaunch",
        options,
        Box::new(move |_cc| {
            Box::new(LauncherApp {
                paths: paths.clone(),
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
                edit_form: None,
                secret_store,
                active_tab: Tab::Profiles,
                preset_forms,
                selected_preset: None,
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

#[derive(Clone)]
struct ForwardingForm {
    direction: ForwardDirection,
    local_host: String,
    local_port: String,
    remote_host: String,
    remote_port: String,
}

#[derive(Clone)]
struct ForwardingPresetForm {
    name: String,
    description: String,
    rule: ForwardingForm,
}

#[derive(Clone)]
struct ProfileForm {
    original_id: String,
    id: String,
    name: String,
    host: String,
    port: String,
    protocol: Protocol,
    user: String,
    group: String,
    tags: String,
    danger_level: DangerLevel,
    pinned: bool,
    macro_path: String,
    color: String,
    description: String,
    extra_args: String,
    password: String,
    show_password: bool,
    ssh_forwardings: Vec<ForwardingForm>,
}

impl ForwardingForm {
    fn from_forwarding(f: &SshForwarding) -> Self {
        Self {
            direction: f.direction.clone(),
            local_host: f.local_host.clone().unwrap_or_else(|| "127.0.0.1".into()),
            local_port: if f.local_port == 0 {
                String::new()
            } else {
                f.local_port.to_string()
            },
            remote_host: f.remote_host.clone(),
            remote_port: if f.remote_port == 0 {
                String::new()
            } else {
                f.remote_port.to_string()
            },
        }
    }

    fn to_forwarding(&self) -> Result<SshForwarding> {
        let local_port = if self.local_port.trim().is_empty() {
            0
        } else {
            self.local_port
                .trim()
                .parse::<u16>()
                .map_err(|_| anyhow!("Invalid local port"))?
        };

        let remote_port = if self.remote_port.trim().is_empty() {
            0
        } else {
            self.remote_port
                .trim()
                .parse::<u16>()
                .map_err(|_| anyhow!("Invalid remote port"))?
        };

        Ok(SshForwarding {
            direction: self.direction.clone(),
            local_host: if self.local_host.trim().is_empty() {
                None
            } else {
                Some(self.local_host.trim().to_string())
            },
            local_port,
            remote_host: self.remote_host.trim().to_string(),
            remote_port,
        })
    }
}

impl ForwardingPresetForm {
    fn from_preset(p: &ForwardingPreset) -> Self {
        Self {
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            rule: ForwardingForm::from_forwarding(&p.rule),
        }
    }

    fn to_preset(&self) -> Result<ForwardingPreset> {
        Ok(ForwardingPreset {
            name: self.name.trim().to_string(),
            description: if self.description.trim().is_empty() {
                None
            } else {
                Some(self.description.trim().to_string())
            },
            rule: self.rule.to_forwarding()?,
        })
    }
}

impl ProfileForm {
    fn from_profile(profile: &Profile, secret_store: &SecretStore) -> Result<Self> {
        let password = if let Some(cipher) = profile.password.as_ref() {
            secret_store.decrypt(cipher)?
        } else {
            String::new()
        };

        let ssh_forwardings = profile
            .ssh_forwardings
            .iter()
            .map(ForwardingForm::from_forwarding)
            .collect();

        Ok(Self {
            original_id: profile.id.clone(),
            id: profile.id.clone(),
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: profile
                .port
                .map(|p| p.to_string())
                .unwrap_or_else(String::new),
            protocol: profile.protocol.clone(),
            user: profile.user.clone().unwrap_or_default(),
            group: profile.group.clone().unwrap_or_default(),
            tags: if profile.tags.is_empty() {
                String::new()
            } else {
                profile.tags.join(", ")
            },
            danger_level: profile.danger_level.clone(),
            pinned: profile.pinned,
            macro_path: profile
                .macro_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            color: profile.color.clone().unwrap_or_default(),
            description: profile.description.clone().unwrap_or_default(),
            extra_args: profile
                .extra_args
                .as_ref()
                .map(|v| v.join(", "))
                .unwrap_or_default(),
            password,
            show_password: false,
            ssh_forwardings,
        })
    }

    fn apply_to_profile(&self, original: &Profile, secret_store: &SecretStore) -> Result<Profile> {
        if self.id.trim().is_empty() {
            return Err(anyhow!("Profile ID is required"));
        }
        if self.name.trim().is_empty() {
            return Err(anyhow!("Profile name is required"));
        }
        if self.host.trim().is_empty() {
            return Err(anyhow!("Host is required"));
        }

        let port = if self.port.trim().is_empty() {
            None
        } else {
            Some(
                self.port
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| anyhow!("Invalid port"))?,
            )
        };

        let tags: Vec<String> = self
            .tags
            .split(',')
            .filter_map(|t| {
                let v = t.trim();
                if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                }
            })
            .collect();

        let extra_args_vec: Vec<String> = self
            .extra_args
            .split(|c| c == ',' || c == '\n')
            .filter_map(|a| {
                let v = a.trim();
                if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                }
            })
            .collect();

        let ssh_forwardings: Vec<SshForwarding> = self
            .ssh_forwardings
            .iter()
            .map(ForwardingForm::to_forwarding)
            .collect::<Result<_>>()?;

        let password = if self.password.trim().is_empty() {
            None
        } else {
            Some(secret_store.encrypt(self.password.trim().as_bytes())?)
        };

        let mut profile = original.clone();
        profile.id = self.id.trim().to_string();
        profile.name = self.name.trim().to_string();
        profile.host = self.host.trim().to_string();
        profile.port = port;
        profile.protocol = self.protocol.clone();
        profile.user = if self.user.trim().is_empty() {
            None
        } else {
            Some(self.user.trim().to_string())
        };
        profile.group = if self.group.trim().is_empty() {
            None
        } else {
            Some(self.group.trim().to_string())
        };
        profile.tags = tags;
        profile.danger_level = self.danger_level.clone();
        profile.pinned = self.pinned;
        profile.macro_path = if self.macro_path.trim().is_empty() {
            None
        } else {
            Some(self.macro_path.trim().into())
        };
        profile.color = if self.color.trim().is_empty() {
            None
        } else {
            Some(self.color.trim().to_string())
        };
        profile.description = if self.description.trim().is_empty() {
            None
        } else {
            Some(self.description.trim().to_string())
        };
        profile.extra_args = if extra_args_vec.is_empty() {
            None
        } else {
            Some(extra_args_vec)
        };
        profile.password = password;
        profile.ssh_forwardings = ssh_forwardings;
        Ok(profile)
    }
}

struct LauncherApp {
    paths: AppPaths,
    profiles: Vec<Profile>,
    filter: String,
    selected: Option<String>,
    edit_form: Option<ProfileForm>,
    config: AppConfig,
    history_store: HistoryStore,
    history: Vec<HistoryEntry>,
    last_seen: HashMap<String, DateTime<Utc>>,
    status: Option<String>,
    error: Option<String>,
    confirm: Option<ConfirmState>,
    tera_term_input: String,
    secret_store: SecretStore,
    active_tab: Tab,
    preset_forms: Vec<ForwardingPresetForm>,
    selected_preset: Option<usize>,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_ui_preferences(ctx);

        let filtered = self.filtered_profiles();
        if let Some(selected_id) = self.selected.clone() {
            if !filtered.iter().any(|p| p.id == selected_id) {
                self.selected = None;
            }
        }

        self.sync_selected_form();

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
                ui.horizontal(|ui| {
                    if ui.button("New").clicked() {
                        self.filter.clear();
                        match self.add_profile() {
                            Ok(_) => {
                                self.status = Some("New profile created".into());
                                self.error = None;
                            }
                            Err(err) => self.error = Some(err.to_string()),
                        }
                    }
                    let delete_button =
                        ui.add_enabled(self.selected.is_some(), egui::Button::new("Delete"));
                    if delete_button.clicked() {
                        match self.delete_selected() {
                            Ok(_) => {
                                self.status = Some("Profile deleted".into());
                                self.error = None;
                            }
                            Err(err) => self.error = Some(err.to_string()),
                        }
                    }
                });
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
                Tab::Settings => self.render_settings_tab(ui, ctx),
            }
        });

        if let Some(confirm) = self.confirm.take() {
            self.render_confirm_dialog(ctx, confirm);
        }
    }
}

impl LauncherApp {
    fn apply_ui_preferences(&self, ctx: &egui::Context) {
        match self.config.ui.theme {
            ThemePreference::Dark => ctx.set_visuals(Visuals::dark()),
            ThemePreference::Light => ctx.set_visuals(Visuals::light()),
            ThemePreference::System => ctx.set_visuals(Visuals::default()),
        };

        let mut style = (*ctx.style()).clone();
        let body_font = if self.config.ui.font_family.eq_ignore_ascii_case("monospace") {
            FontId::monospace(self.config.ui.text_size)
        } else {
            FontId::proportional(self.config.ui.text_size)
        };

        style.text_styles.insert(TextStyle::Body, body_font.clone());
        style
            .text_styles
            .insert(TextStyle::Button, body_font.clone());
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::monospace(self.config.ui.text_size),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::proportional(self.config.ui.text_size * 1.2),
        );
        ctx.set_style(style);
    }

    fn refresh_secret_store(&mut self) {
        match SecretStore::new(&self.paths, &self.config.secrets) {
            Ok(store) => {
                self.secret_store = store;
                self.status = Some("Secret backend updated".into());
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
    }

    fn sync_selected_form(&mut self) {
        if let Some(selected_id) = self.selected.clone() {
            let needs_refresh = self
                .edit_form
                .as_ref()
                .map(|f| f.original_id != selected_id)
                .unwrap_or(true);
            if needs_refresh {
                if let Some(profile) = self.profiles.iter().find(|p| p.id == selected_id) {
                    match ProfileForm::from_profile(profile, &self.secret_store) {
                        Ok(form) => {
                            self.edit_form = Some(form);
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err.to_string());
                            self.edit_form = None;
                        }
                    }
                }
            }
        }
    }

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
        _filtered: &[Profile],
    ) {
        if self.selected.is_none() {
            ui.label("Select a profile from the left or create a new one.");
            return;
        }

        if let Some(form) = self.edit_form.as_mut() {
            ui.heading("Profile editor");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("ID");
                ui.text_edit_singleline(&mut form.id);
                ui.label("Name");
                ui.text_edit_singleline(&mut form.name);
            });
            ui.horizontal(|ui| {
                ui.label("Host");
                ui.text_edit_singleline(&mut form.host);
                ui.label("Port");
                ui.text_edit_singleline(&mut form.port);
                egui::ComboBox::from_label("Protocol")
                    .selected_text(format!("{:?}", form.protocol))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.protocol, Protocol::Ssh, "SSH");
                        ui.selectable_value(&mut form.protocol, Protocol::Telnet, "Telnet");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("User");
                ui.text_edit_singleline(&mut form.user);
                ui.label("Group");
                ui.text_edit_singleline(&mut form.group);
            });
            ui.horizontal(|ui| {
                ui.label("Tags (comma separated)");
                ui.text_edit_singleline(&mut form.tags);
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("Danger")
                    .selected_text(format!("{:?}", form.danger_level))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.danger_level, DangerLevel::Normal, "Normal");
                        ui.selectable_value(&mut form.danger_level, DangerLevel::Warn, "Warn");
                        ui.selectable_value(
                            &mut form.danger_level,
                            DangerLevel::Critical,
                            "Critical",
                        );
                    });
                ui.checkbox(&mut form.pinned, "Pinned");
            });
            ui.horizontal(|ui| {
                ui.label("Color (hex)");
                ui.text_edit_singleline(&mut form.color);
            });

            ui.label("Description");
            ui.text_edit_multiline(&mut form.description);
            ui.label("Macro path");
            ui.text_edit_singleline(&mut form.macro_path);
            ui.label("Extra args (comma or newline)");
            ui.text_edit_multiline(&mut form.extra_args);

            ui.separator();
            ui.label("Access password");
            ui.horizontal(|ui| {
                let mut edit = egui::TextEdit::singleline(&mut form.password);
                edit = edit.password(!form.show_password);
                ui.add(edit);
                ui.checkbox(&mut form.show_password, "Show");
                if ui.button("Clear").clicked() {
                    form.password.clear();
                }
            });

            ui.separator();
            ui.heading("SSH forwarding");
            if !self.preset_forms.is_empty() {
                ui.horizontal(|ui| {
                    let selected_label = self
                        .selected_preset
                        .and_then(|idx| self.preset_forms.get(idx))
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Select preset".to_string());
                    egui::ComboBox::from_label("Presets")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for (idx, preset) in self.preset_forms.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        self.selected_preset == Some(idx),
                                        preset.name.clone(),
                                    )
                                    .clicked()
                                {
                                    self.selected_preset = Some(idx);
                                }
                            }
                        });
                    if ui.button("Apply preset").clicked() {
                        if let Some(idx) = self.selected_preset {
                            if let Some(preset) = self.preset_forms.get(idx) {
                                if let Ok(rule) = preset.rule.to_forwarding() {
                                    form.ssh_forwardings
                                        .push(ForwardingForm::from_forwarding(&rule));
                                }
                            }
                        }
                    }
                });
            }
            if form.ssh_forwardings.is_empty() {
                ui.label("No forwarding rules");
            }
            let mut remove_idx: Option<usize> = None;
            for (idx, fwd) in form.ssh_forwardings.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label(format!("Rule {}", idx + 1))
                        .selected_text(format!("{:?}", fwd.direction))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut fwd.direction,
                                ForwardDirection::Local,
                                "Local",
                            );
                            ui.selectable_value(
                                &mut fwd.direction,
                                ForwardDirection::Remote,
                                "Remote",
                            );
                            ui.selectable_value(
                                &mut fwd.direction,
                                ForwardDirection::Dynamic,
                                "Dynamic",
                            );
                        });
                    ui.label("Local host");
                    ui.text_edit_singleline(&mut fwd.local_host);
                    ui.label("Local port");
                    ui.text_edit_singleline(&mut fwd.local_port);
                    if ui.button("Remove").clicked() {
                        remove_idx = Some(idx);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Remote host");
                    ui.text_edit_singleline(&mut fwd.remote_host);
                    ui.label("Remote port");
                    ui.text_edit_singleline(&mut fwd.remote_port);
                });
                ui.separator();
            }
            if let Some(idx) = remove_idx {
                form.ssh_forwardings.remove(idx);
            }
            if ui.button("Add forwarding").clicked() {
                form.ssh_forwardings.push(ForwardingForm {
                    direction: ForwardDirection::Local,
                    local_host: "127.0.0.1".into(),
                    local_port: String::new(),
                    remote_host: String::new(),
                    remote_port: String::new(),
                });
            }

            ui.separator();
            if let Some(ts) = self.last_seen.get(&form.original_id) {
                ui.label(format!(
                    "Last connected: {}",
                    ts.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
                ));
            }

            ui.horizontal(|ui| {
                if ui.button("Save profile").clicked() {
                    match self.persist_form() {
                        Ok(_) => {
                            self.status = Some("Profile saved".into());
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err.to_string());
                        }
                    }
                }
                if ui.button("Reset changes").clicked() {
                    self.sync_selected_form();
                }
                if ui.button("Connect").clicked() {
                    match self.persist_form() {
                        Ok(profile) => {
                            if profile.is_dangerous() {
                                self.confirm = Some(ConfirmState::new(profile));
                            } else if let Err(err) = self.execute_connect(profile) {
                                self.error = Some(err.to_string());
                            }
                            ctx.request_repaint();
                        }
                        Err(err) => {
                            self.error = Some(err.to_string());
                        }
                    }
                }
            });

            if let Some(status) = &self.status {
                ui.label(status);
            }
            if let Some(err) = &self.error {
                ui.colored_label(Color32::RED, err);
            }
        } else {
            ui.label("Failed to load profile details.");
        }
    }

    fn persist_form(&mut self) -> Result<Profile> {
        let form = self
            .edit_form
            .clone()
            .ok_or_else(|| anyhow!("No profile selected"))?;
        if self
            .profiles
            .iter()
            .any(|p| p.id == form.id && p.id != form.original_id)
        {
            return Err(anyhow!("Profile ID already exists"));
        }
        let idx = self
            .profiles
            .iter()
            .position(|p| p.id == form.original_id)
            .ok_or_else(|| anyhow!("Profile not found"))?;
        let updated = form.apply_to_profile(&self.profiles[idx], &self.secret_store)?;
        let old_id = self.profiles[idx].id.clone();
        self.profiles[idx] = updated.clone();

        if old_id != updated.id {
            if let Some(ts) = self.last_seen.remove(&old_id) {
                self.last_seen.insert(updated.id.clone(), ts);
            }
        }

        self.save_profiles()?;
        self.selected = Some(updated.id.clone());
        self.edit_form = Some(ProfileForm::from_profile(&updated, &self.secret_store)?);
        Ok(updated)
    }

    fn add_profile(&mut self) -> Result<()> {
        let id = self.generate_profile_id();
        let profile = Profile {
            id: id.clone(),
            name: format!("Profile {}", id),
            host: String::new(),
            port: None,
            protocol: Protocol::default(),
            user: None,
            group: None,
            tags: Vec::new(),
            danger_level: DangerLevel::default(),
            pinned: false,
            macro_path: None,
            color: None,
            description: None,
            extra_args: None,
            password: None,
            ssh_forwardings: Vec::new(),
        };
        self.selected = Some(id.clone());
        self.profiles.push(profile.clone());
        self.edit_form = Some(ProfileForm::from_profile(&profile, &self.secret_store)?);
        Ok(())
    }

    fn delete_selected(&mut self) -> Result<()> {
        let selected_id = self
            .selected
            .clone()
            .ok_or_else(|| anyhow!("No profile selected"))?;
        if let Some(pos) = self.profiles.iter().position(|p| p.id == selected_id) {
            self.profiles.remove(pos);
            self.last_seen.remove(&selected_id);
            self.save_profiles()?;
            self.selected = self.profiles.first().map(|p| p.id.clone());
            self.edit_form = None;
            self.sync_selected_form();
            Ok(())
        } else {
            Err(anyhow!("Profile not found"))
        }
    }

    fn generate_profile_id(&self) -> String {
        let mut idx = 1;
        loop {
            let candidate = format!("profile-{}", idx);
            if !self.profiles.iter().any(|p| p.id == candidate) {
                return candidate;
            }
            idx += 1;
        }
    }

    fn persist_form(&mut self) -> Result<Profile> {
        let form = self
            .edit_form
            .clone()
            .ok_or_else(|| anyhow!("No profile selected"))?;
        if self
            .profiles
            .iter()
            .any(|p| p.id == form.id && p.id != form.original_id)
        {
            return Err(anyhow!("Profile ID already exists"));
        }
        let idx = self
            .profiles
            .iter()
            .position(|p| p.id == form.original_id)
            .ok_or_else(|| anyhow!("Profile not found"))?;
        let updated = form.apply_to_profile(&self.profiles[idx], &self.secret_store)?;
        let old_id = self.profiles[idx].id.clone();
        self.profiles[idx] = updated.clone();

        if old_id != updated.id {
            if let Some(ts) = self.last_seen.remove(&old_id) {
                self.last_seen.insert(updated.id.clone(), ts);
            }
        }

        self.save_profiles()?;
        self.selected = Some(updated.id.clone());
        self.edit_form = Some(ProfileForm::from_profile(&updated, &self.secret_store)?);
        Ok(updated)
    }

    fn add_profile(&mut self) -> Result<()> {
        let id = self.generate_profile_id();
        let profile = Profile {
            id: id.clone(),
            name: format!("Profile {}", id),
            host: String::new(),
            port: None,
            protocol: Protocol::default(),
            user: None,
            group: None,
            tags: Vec::new(),
            danger_level: DangerLevel::default(),
            pinned: false,
            macro_path: None,
            color: None,
            description: None,
            extra_args: None,
            password: None,
            ssh_forwardings: Vec::new(),
        };
        self.selected = Some(id.clone());
        self.profiles.push(profile.clone());
        self.edit_form = Some(ProfileForm::from_profile(&profile, &self.secret_store)?);
        Ok(())
    }

    fn delete_selected(&mut self) -> Result<()> {
        let selected_id = self
            .selected
            .clone()
            .ok_or_else(|| anyhow!("No profile selected"))?;
        if let Some(pos) = self.profiles.iter().position(|p| p.id == selected_id) {
            self.profiles.remove(pos);
            self.last_seen.remove(&selected_id);
            self.save_profiles()?;
            self.selected = self.profiles.first().map(|p| p.id.clone());
            self.edit_form = None;
            self.sync_selected_form();
            Ok(())
        } else {
            Err(anyhow!("Profile not found"))
        }
    }

    fn generate_profile_id(&self) -> String {
        let mut idx = 1;
        loop {
            let candidate = format!("profile-{}", idx);
            if !self.profiles.iter().any(|p| p.id == candidate) {
                return candidate;
            }
            idx += 1;
        }

        self.save_profiles()?;
        self.selected = Some(updated.id.clone());
        self.edit_form = Some(ProfileForm::from_profile(&updated, &self.secret_store)?);
        Ok(updated)
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

    fn render_settings_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
        ui.label("Tera Term path");
        if self.tera_term_input.is_empty() {
            self.tera_term_input = self.config.tera_term_path.display().to_string();
        }
        ui.text_edit_singleline(&mut self.tera_term_input);
        if !std::path::Path::new(&self.tera_term_input).exists() {
            ui.colored_label(Color32::YELLOW, "Path does not exist on this system");
        }

        ui.separator();
        ui.heading("Secrets");
        egui::ComboBox::from_label("Secret backend")
            .selected_text(format!("{:?}", self.config.secrets.backend))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.config.secrets.backend,
                    SecretBackend::FileKey,
                    "Local file key",
                );
                ui.selectable_value(
                    &mut self.config.secrets.backend,
                    SecretBackend::WindowsCredentialManager,
                    "Windows Credential Manager",
                );
                ui.selectable_value(
                    &mut self.config.secrets.backend,
                    SecretBackend::WindowsDpapi,
                    "Windows DPAPI",
                );
            });
        ui.label("Credential target / label");
        ui.text_edit_singleline(&mut self.config.secrets.credential_target);

        ui.separator();
        ui.heading("Forwarding presets");
        let mut remove_preset: Option<usize> = None;
        for (idx, preset) in self.preset_forms.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut preset.name);
                    if ui.button("Remove").clicked() {
                        remove_preset = Some(idx);
                    }
                });
                ui.label("Description");
                ui.text_edit_singleline(&mut preset.description);
                ui.label("Rule");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Direction")
                        .selected_text(format!("{:?}", preset.rule.direction))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut preset.rule.direction,
                                ForwardDirection::Local,
                                "Local",
                            );
                            ui.selectable_value(
                                &mut preset.rule.direction,
                                ForwardDirection::Remote,
                                "Remote",
                            );
                            ui.selectable_value(
                                &mut preset.rule.direction,
                                ForwardDirection::Dynamic,
                                "Dynamic",
                            );
                        });
                    ui.label("Local host");
                    ui.text_edit_singleline(&mut preset.rule.local_host);
                    ui.label("Local port");
                    ui.text_edit_singleline(&mut preset.rule.local_port);
                });
                ui.horizontal(|ui| {
                    ui.label("Remote host");
                    ui.text_edit_singleline(&mut preset.rule.remote_host);
                    ui.label("Remote port");
                    ui.text_edit_singleline(&mut preset.rule.remote_port);
                });
            });
        }
        if let Some(idx) = remove_preset {
            self.preset_forms.remove(idx);
        }
        if ui.button("Add preset").clicked() {
            self.preset_forms.push(ForwardingPresetForm {
                name: format!("preset-{}", self.preset_forms.len() + 1),
                description: String::new(),
                rule: ForwardingForm {
                    direction: ForwardDirection::Local,
                    local_host: "127.0.0.1".into(),
                    local_port: String::new(),
                    remote_host: String::new(),
                    remote_port: String::new(),
                },
            });
        }

        ui.separator();
        ui.heading("Appearance");
        let mut appearance_changed = false;
        egui::ComboBox::from_label("Theme")
            .selected_text(match self.config.ui.theme {
                ThemePreference::System => "System".to_string(),
                ThemePreference::Light => "Light".to_string(),
                ThemePreference::Dark => "Dark".to_string(),
            })
            .show_ui(ui, |ui| {
                appearance_changed |= ui
                    .selectable_value(&mut self.config.ui.theme, ThemePreference::System, "System")
                    .changed();
                appearance_changed |= ui
                    .selectable_value(&mut self.config.ui.theme, ThemePreference::Light, "Light")
                    .changed();
                appearance_changed |= ui
                    .selectable_value(&mut self.config.ui.theme, ThemePreference::Dark, "Dark")
                    .changed();
            });

        egui::ComboBox::from_label("Font")
            .selected_text(
                if self.config.ui.font_family.eq_ignore_ascii_case("monospace") {
                    "Monospace".to_string()
                } else {
                    "Proportional".to_string()
                },
            )
            .show_ui(ui, |ui| {
                appearance_changed |= ui
                    .selectable_value(
                        &mut self.config.ui.font_family,
                        "proportional".into(),
                        "Proportional",
                    )
                    .changed();
                appearance_changed |= ui
                    .selectable_value(
                        &mut self.config.ui.font_family,
                        "monospace".into(),
                        "Monospace",
                    )
                    .changed();
            });

        appearance_changed |= ui
            .add(egui::Slider::new(&mut self.config.ui.text_size, 10.0..=32.0).text("Text size"))
            .changed();

        if appearance_changed {
            self.apply_ui_preferences(ctx);
        }

        if ui.button("Save settings").clicked() {
            self.config.tera_term_path = std::path::PathBuf::from(self.tera_term_input.trim());
            match self
                .preset_forms
                .iter()
                .map(ForwardingPresetForm::to_preset)
                .collect::<Result<Vec<_>>>()
            {
                Ok(presets) => {
                    self.config.forwarding_presets = presets;
                    if let Err(err) = self.config.save(&self.paths.settings_path) {
                        self.error = Some(err.to_string());
                    } else {
                        self.status = Some("Settings saved".into());
                        self.refresh_secret_store();
                    }
                }
                Err(err) => {
                    self.error = Some(err.to_string());
                }
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
        let password = if let Some(cipher) = profile.password.as_ref() {
            Some(self.secret_store.decrypt(cipher)?)
        } else {
            None
        };
        let spec = build_command(&profile, &self.config, password.as_deref());
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
