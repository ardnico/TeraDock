use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{anyhow, Result};

use tdcore::cmdset::{CmdSet, CmdSetStore};
use tdcore::cmdset_runner::{run_cmdset_ssh, CmdSetRunRequest, CmdSetRunResult};
use tdcore::doctor::ClientKind;
use tdcore::oplog::{self, OpLogEntry};
use tdcore::profile::{DangerLevel, Profile, ProfileFilters, ProfileStore, ProfileType};
use tdcore::settings::{self, ResolvedSettingDetail, ResolvedSettingSource};
use tdcore::ssh::{self, SshBuildError, SshInvocationMode, SshInvocationRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Profiles,
    Actions,
    Results,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTab {
    Stdout,
    Stderr,
    Parsed,
    Summary,
}

#[derive(Debug, Clone)]
pub struct RunSummaryItem {
    pub profile_id: String,
    pub profile_name: String,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub total: usize,
    pub ok_count: usize,
    pub fail_count: usize,
    pub items: Vec<RunSummaryItem>,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub ok: bool,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub parsed_pretty: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshSessionCommand {
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub profile_type: ProfileType,
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub safe_metadata: serde_json::Value,
}

impl RunResult {
    fn from_error(err: anyhow::Error) -> Self {
        Self {
            ok: false,
            exit_code: -1,
            duration_ms: 0,
            stdout: String::new(),
            stderr: String::new(),
            parsed_pretty: "{}".to_string(),
            error: Some(err.to_string()),
        }
    }

    fn from_cmdset_run(run: CmdSetRunResult) -> Self {
        let steps_json = run
            .steps
            .iter()
            .map(|step| {
                serde_json::json!({
                    "ord": step.ord,
                    "cmd": step.cmd,
                    "ok": step.ok,
                    "exit_code": step.exit_code,
                    "stdout": step.stdout,
                    "stderr": step.stderr,
                    "duration_ms": step.duration_ms,
                    "parsed": step.parsed,
                })
            })
            .collect::<Vec<_>>();
        let parsed_json = serde_json::json!({ "steps": steps_json });
        let parsed_pretty =
            serde_json::to_string_pretty(&parsed_json).unwrap_or_else(|_| "{}".into());
        Self {
            ok: run.ok,
            exit_code: run.exit_code,
            duration_ms: run.duration_ms,
            stdout: run.stdout,
            stderr: run.stderr,
            parsed_pretty,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    RunCmdSet {
        profile_id: String,
        cmdset_id: String,
    },
    RunCmdSetBulk {
        profile_ids: Vec<String>,
        cmdset_id: String,
    },
    OpenSshSession {
        profile_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub message: String,
    pub required_input: String,
    pub input: String,
    pub action: PendingAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmedAction {
    Continue,
    OpenSshSession,
}

pub struct AppState {
    store: ProfileStore,
    cmdset_store: CmdSetStore,
    filters: ProfileFilters,
    filtered: Vec<Profile>,
    groups: Vec<String>,
    tags: Vec<String>,
    tag_cursor: usize,
    mode: InputMode,
    search_input: String,
    profile_cursor: usize,
    cmdsets: Vec<CmdSet>,
    cmdset_cursor: usize,
    active_pane: ActivePane,
    result_tab: ResultTab,
    confirm: Option<ConfirmState>,
    last_result: Option<RunResult>,
    last_summary: Option<RunSummary>,
    marked_profiles: BTreeSet<String>,
    details_open: bool,
    details_lines: Vec<String>,
    details_scroll: usize,
    help_open: bool,
    status_message: Option<String>,
    confirmed_ssh_session_profile_id: Option<String>,
}

impl AppState {
    pub fn new(store: ProfileStore, cmdset_store: CmdSetStore) -> Result<Self> {
        let profiles = store.list()?;
        let groups = collect_groups(&profiles);
        let tags = collect_tags(&profiles);
        let filters = ProfileFilters::default();
        let filtered = store.list_filtered(&filters)?;
        let cmdsets = cmdset_store.list()?;
        Ok(Self {
            store,
            cmdset_store,
            filters,
            filtered,
            groups,
            tags,
            tag_cursor: 0,
            mode: InputMode::Normal,
            search_input: String::new(),
            profile_cursor: 0,
            cmdsets,
            cmdset_cursor: 0,
            active_pane: ActivePane::Profiles,
            result_tab: ResultTab::Stdout,
            confirm: None,
            last_result: None,
            last_summary: None,
            marked_profiles: BTreeSet::new(),
            details_open: false,
            details_lines: Vec::new(),
            details_scroll: 0,
            help_open: false,
            status_message: None,
            confirmed_ssh_session_profile_id: None,
        })
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn active_pane(&self) -> ActivePane {
        self.active_pane
    }

    pub fn result_tab(&self) -> ResultTab {
        self.result_tab
    }

    pub fn last_summary(&self) -> Option<&RunSummary> {
        self.last_summary.as_ref()
    }

    pub fn details_open(&self) -> bool {
        self.details_open
    }

    pub fn details_lines(&self) -> &[String] {
        &self.details_lines
    }

    pub fn details_scroll(&self) -> usize {
        self.details_scroll
    }

    pub fn help_open(&self) -> bool {
        self.help_open
    }

    pub fn filters(&self) -> &ProfileFilters {
        &self.filters
    }

    pub fn filtered(&self) -> &[Profile] {
        &self.filtered
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn tag_cursor(&self) -> Option<&str> {
        self.tags.get(self.tag_cursor).map(String::as_str)
    }

    pub fn search_input(&self) -> &str {
        &self.search_input
    }

    pub fn profile_cursor(&self) -> Option<usize> {
        if self.filtered.is_empty() {
            None
        } else {
            Some(self.profile_cursor.min(self.filtered.len() - 1))
        }
    }

    pub fn cmdset_cursor(&self) -> Option<usize> {
        if self.cmdsets.is_empty() {
            None
        } else {
            Some(self.cmdset_cursor.min(self.cmdsets.len() - 1))
        }
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        self.profile_cursor().and_then(|idx| self.filtered.get(idx))
    }

    pub fn selected_cmdset(&self) -> Option<&CmdSet> {
        self.cmdset_cursor().and_then(|idx| self.cmdsets.get(idx))
    }

    pub fn confirm_state(&self) -> Option<&ConfirmState> {
        self.confirm.as_ref()
    }

    pub fn last_result(&self) -> Option<&RunResult> {
        self.last_result.as_ref()
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn action_hint(&self) -> String {
        if self.filtered.is_empty() {
            return "No profiles match filters; press c to clear filters.".to_string();
        }
        let Some(profile) = self.selected_profile() else {
            return "No profile selected.".to_string();
        };
        if profile.profile_type != ProfileType::Ssh {
            return format!(
                "Selected profile is {}; s and CommandSet run require SSH.",
                profile.profile_type
            );
        }
        if self.cmdsets.is_empty() {
            return format!(
                "Ready: s opens SSH session for '{}'; no CommandSets available.",
                profile.profile_id
            );
        }
        let Some(cmdset) = self.selected_cmdset() else {
            return format!(
                "Ready: s opens SSH session for '{}'; no CommandSet selected.",
                profile.profile_id
            );
        };
        if self.marked_profiles.is_empty() {
            format!(
                "Ready: s opens SSH session; r runs '{}' on '{}'; Space marks profiles for bulk R.",
                cmdset.cmdset_id, profile.profile_id
            )
        } else {
            format!(
                "Ready: s opens SSH session; r runs selected; R runs '{}' on {} marked profiles.",
                cmdset.cmdset_id,
                self.marked_profiles.len()
            )
        }
    }

    pub fn cmdsets(&self) -> &[CmdSet] {
        &self.cmdsets
    }

    pub fn marked_profiles(&self) -> &BTreeSet<String> {
        &self.marked_profiles
    }

    pub fn enter_search(&mut self) {
        self.mode = InputMode::Search;
        self.search_input = self.filters.query.clone().unwrap_or_default();
    }

    pub fn exit_search(&mut self) -> Result<()> {
        self.mode = InputMode::Normal;
        self.update_query()
    }

    pub fn push_search_char(&mut self, ch: char) -> Result<()> {
        self.search_input.push(ch);
        self.update_query()
    }

    pub fn pop_search_char(&mut self) -> Result<()> {
        self.search_input.pop();
        self.update_query()
    }

    pub fn clear_filters(&mut self) -> Result<()> {
        self.filters = ProfileFilters::default();
        self.search_input.clear();
        self.refresh()
    }

    pub fn cycle_profile_type(&mut self) -> Result<()> {
        self.filters.profile_type = match self.filters.profile_type {
            None => Some(ProfileType::Ssh),
            Some(ProfileType::Ssh) => Some(ProfileType::Telnet),
            Some(ProfileType::Telnet) => Some(ProfileType::Serial),
            Some(ProfileType::Serial) => None,
        };
        self.refresh()
    }

    pub fn cycle_danger(&mut self) -> Result<()> {
        self.filters.danger = match self.filters.danger {
            None => Some(DangerLevel::Normal),
            Some(DangerLevel::Normal) => Some(DangerLevel::High),
            Some(DangerLevel::High) => Some(DangerLevel::Critical),
            Some(DangerLevel::Critical) => None,
        };
        self.refresh()
    }

    pub fn cycle_group(&mut self) -> Result<()> {
        if self.groups.is_empty() {
            self.filters.group = None;
            return self.refresh();
        }
        let next = match &self.filters.group {
            None => Some(self.groups[0].clone()),
            Some(current) => match self
                .groups
                .iter()
                .position(|g| g.eq_ignore_ascii_case(current))
            {
                Some(idx) if idx + 1 < self.groups.len() => Some(self.groups[idx + 1].clone()),
                _ => None,
            },
        };
        self.filters.group = next;
        self.refresh()
    }

    pub fn tag_cursor_next(&mut self) {
        if self.tags.is_empty() {
            return;
        }
        self.tag_cursor = (self.tag_cursor + 1) % self.tags.len();
    }

    pub fn tag_cursor_prev(&mut self) {
        if self.tags.is_empty() {
            return;
        }
        if self.tag_cursor == 0 {
            self.tag_cursor = self.tags.len() - 1;
        } else {
            self.tag_cursor -= 1;
        }
    }

    pub fn toggle_tag(&mut self) -> Result<()> {
        if self.tags.is_empty() {
            return Ok(());
        }
        let tag = &self.tags[self.tag_cursor];
        if let Some(pos) = self
            .filters
            .tags
            .iter()
            .position(|t| t.eq_ignore_ascii_case(tag))
        {
            self.filters.tags.remove(pos);
        } else {
            self.filters.tags.push(tag.clone());
        }
        self.refresh()
    }

    pub fn cycle_pane(&mut self) {
        self.active_pane = match self.active_pane {
            ActivePane::Profiles => ActivePane::Actions,
            ActivePane::Actions => ActivePane::Results,
            ActivePane::Results => ActivePane::Profiles,
        };
    }

    pub fn next_profile(&mut self) -> Result<()> {
        if !self.filtered.is_empty() {
            self.profile_cursor = (self.profile_cursor + 1) % self.filtered.len();
        }
        if self.details_open {
            self.refresh_details()?;
        }
        Ok(())
    }

    pub fn prev_profile(&mut self) -> Result<()> {
        if self.filtered.is_empty() {
            return Ok(());
        }
        if self.profile_cursor == 0 {
            self.profile_cursor = self.filtered.len() - 1;
        } else {
            self.profile_cursor -= 1;
        }
        if self.details_open {
            self.refresh_details()?;
        }
        Ok(())
    }

    pub fn next_cmdset(&mut self) {
        if !self.cmdsets.is_empty() {
            self.cmdset_cursor = (self.cmdset_cursor + 1) % self.cmdsets.len();
        }
    }

    pub fn prev_cmdset(&mut self) {
        if self.cmdsets.is_empty() {
            return;
        }
        if self.cmdset_cursor == 0 {
            self.cmdset_cursor = self.cmdsets.len() - 1;
        } else {
            self.cmdset_cursor -= 1;
        }
    }

    pub fn next_result_tab(&mut self) {
        self.result_tab = match self.result_tab {
            ResultTab::Stdout => ResultTab::Stderr,
            ResultTab::Stderr => ResultTab::Parsed,
            ResultTab::Parsed => ResultTab::Summary,
            ResultTab::Summary => ResultTab::Stdout,
        };
    }

    pub fn prev_result_tab(&mut self) {
        self.result_tab = match self.result_tab {
            ResultTab::Stdout => ResultTab::Summary,
            ResultTab::Stderr => ResultTab::Stdout,
            ResultTab::Parsed => ResultTab::Stderr,
            ResultTab::Summary => ResultTab::Parsed,
        };
    }

    pub fn set_result_tab(&mut self, tab: ResultTab) {
        self.result_tab = tab;
    }

    pub fn cancel_confirm(&mut self) {
        self.confirm = None;
        self.status_message = Some("Confirmation cancelled.".to_string());
    }

    pub fn confirm_action(&mut self) -> Result<ConfirmedAction> {
        let Some(confirm) = self.confirm.as_ref() else {
            return Ok(ConfirmedAction::Continue);
        };
        if confirm.input != confirm.required_input {
            self.status_message = Some(format!("Type '{}' to confirm.", confirm.required_input));
            return Ok(ConfirmedAction::Continue);
        }
        let confirm = self.confirm.take().expect("confirm state should exist");
        match confirm.action {
            PendingAction::RunCmdSet {
                profile_id,
                cmdset_id,
            } => {
                self.execute_cmdset_run(&profile_id, &cmdset_id)?;
                Ok(ConfirmedAction::Continue)
            }
            PendingAction::RunCmdSetBulk {
                profile_ids,
                cmdset_id,
            } => {
                self.execute_cmdset_run_bulk(&profile_ids, &cmdset_id)?;
                Ok(ConfirmedAction::Continue)
            }
            PendingAction::OpenSshSession { profile_id } => {
                self.confirmed_ssh_session_profile_id = Some(profile_id);
                Ok(ConfirmedAction::OpenSshSession)
            }
        }
    }

    pub fn push_confirm_char(&mut self, ch: char) {
        if let Some(confirm) = &mut self.confirm {
            confirm.input.push(ch);
        }
    }

    pub fn pop_confirm_char(&mut self) {
        if let Some(confirm) = &mut self.confirm {
            confirm.input.pop();
        }
    }

    pub fn request_run(&mut self) -> Result<()> {
        let (profile_id, cmdset_id, danger_level, profile_label) = {
            let Some(profile) = self.selected_profile() else {
                self.status_message =
                    Some("No profile selected; clear filters or add a profile.".to_string());
                return Ok(());
            };
            let Some(cmdset) = self.selected_cmdset() else {
                self.status_message = Some(
                    "No CommandSet selected; run td init --with-samples or import one.".to_string(),
                );
                return Ok(());
            };
            (
                profile.profile_id.clone(),
                cmdset.cmdset_id.clone(),
                profile.danger_level,
                format!("{}@{}:{}", profile.user, profile.host, profile.port),
            )
        };
        if danger_level == DangerLevel::Critical {
            self.confirm = Some(ConfirmState {
                message: format!(
                    "Critical profile '{}'. Type the profile id to run CommandSet '{}' on {}.",
                    profile_id, cmdset_id, profile_label
                ),
                required_input: profile_id.clone(),
                input: String::new(),
                action: PendingAction::RunCmdSet {
                    profile_id,
                    cmdset_id,
                },
            });
            return Ok(());
        }
        self.execute_cmdset_run(&profile_id, &cmdset_id)
    }

    pub fn request_bulk_run(&mut self) -> Result<()> {
        if self.marked_profiles.is_empty() {
            self.status_message =
                Some("No profiles marked; press Space on profiles before bulk run.".to_string());
            return Ok(());
        }
        let Some(cmdset_id) = self
            .selected_cmdset()
            .map(|cmdset| cmdset.cmdset_id.clone())
        else {
            self.status_message = Some(
                "No CommandSet selected; run td init --with-samples or import one.".to_string(),
            );
            return Ok(());
        };
        let mut profile_ids: Vec<String> = self.marked_profiles.iter().cloned().collect();
        profile_ids.sort();
        let mut critical_ids = Vec::new();
        for profile_id in &profile_ids {
            if let Some(profile) = self.store.get(profile_id)? {
                if profile.danger_level == DangerLevel::Critical {
                    critical_ids.push(profile.profile_id);
                }
            }
        }
        if !critical_ids.is_empty() {
            let required = critical_ids.join(",");
            self.confirm = Some(ConfirmState {
                message: format!(
                    "Critical profiles in bulk run: {}. Type the comma-separated IDs exactly to continue.",
                    required
                ),
                required_input: required,
                input: String::new(),
                action: PendingAction::RunCmdSetBulk {
                    profile_ids,
                    cmdset_id: cmdset_id.clone(),
                },
            });
            return Ok(());
        }
        self.execute_cmdset_run_bulk(&profile_ids, &cmdset_id)
    }

    pub fn build_ssh_session_command(&mut self) -> Result<Option<SshSessionCommand>> {
        let confirmed_profile_id = self.confirmed_ssh_session_profile_id.take();
        let Some(profile) = self.selected_profile().cloned() else {
            self.status_message =
                Some("No profile selected; clear filters or add a profile.".to_string());
            return Ok(None);
        };
        if profile.profile_type != ProfileType::Ssh {
            self.status_message = Some(format!(
                "Selected profile is {}; SSH session requires an SSH profile.",
                profile.profile_type
            ));
            return Ok(None);
        }
        if profile.danger_level == DangerLevel::Critical
            && confirmed_profile_id.as_deref() != Some(profile.profile_id.as_str())
        {
            self.confirm = Some(ConfirmState {
                message: format!(
                    "Critical profile '{}'. Type the profile id to open SSH session to {}@{}:{}.",
                    profile.profile_id, profile.user, profile.host, profile.port
                ),
                required_input: profile.profile_id.clone(),
                input: String::new(),
                action: PendingAction::OpenSshSession {
                    profile_id: profile.profile_id,
                },
            });
            return Ok(None);
        }
        let invocation = match ssh::build_ssh_invocation(
            &self.store,
            SshInvocationRequest {
                profile_id: &profile.profile_id,
                source: "tui",
                mode: SshInvocationMode::Interactive,
            },
        ) {
            Ok(invocation) => invocation,
            Err(err) => {
                self.status_message = Some(ssh_build_status_message(&err));
                return Ok(None);
            }
        };
        Ok(Some(SshSessionCommand {
            profile_id: invocation.target.profile_id,
            host: invocation.target.host,
            port: invocation.target.port,
            user: invocation.target.user,
            profile_type: ProfileType::Ssh,
            executable: invocation.client_path,
            args: invocation.args,
            safe_metadata: invocation.safe_metadata,
        }))
    }

    pub fn record_ssh_session_result(
        &mut self,
        session: &SshSessionCommand,
        ok: bool,
        exit_code: Option<i32>,
        duration_ms: i64,
    ) -> Result<()> {
        self.store.touch_last_used(&session.profile_id)?;
        oplog::log_operation(
            self.store.conn(),
            OpLogEntry {
                op: oplog::SSH_SESSION_OP.into(),
                profile_id: Some(session.profile_id.clone()),
                client_used: Some(session.executable.to_string_lossy().into_owned()),
                ok,
                exit_code,
                duration_ms: Some(duration_ms),
                meta_json: Some(ssh_session_meta_json(session, None)),
            },
        )?;
        self.status_message = Some(ssh_session_result_message(ok, exit_code));
        Ok(())
    }

    pub fn record_ssh_session_launch_failure(
        &mut self,
        session: &SshSessionCommand,
        error: &str,
        duration_ms: i64,
    ) -> Result<()> {
        self.store.touch_last_used(&session.profile_id)?;
        oplog::log_operation(
            self.store.conn(),
            OpLogEntry {
                op: oplog::SSH_SESSION_OP.into(),
                profile_id: Some(session.profile_id.clone()),
                client_used: Some(session.executable.to_string_lossy().into_owned()),
                ok: false,
                exit_code: None,
                duration_ms: Some(duration_ms),
                meta_json: Some(ssh_session_meta_json(session, Some(error))),
            },
        )?;
        Ok(())
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    fn execute_cmdset_run(&mut self, profile_id: &str, cmdset_id: &str) -> Result<()> {
        let result = self.try_execute_cmdset_run(profile_id, cmdset_id);
        match result {
            Ok(run) => {
                self.status_message = Some(format!(
                    "Run {} in {}ms (exit {}).",
                    if run.ok { "succeeded" } else { "failed" },
                    run.duration_ms,
                    run.exit_code
                ));
                self.last_result = Some(run);
                self.last_summary = None;
            }
            Err(err) => {
                self.status_message = Some(format!("Run failed: {err}"));
                self.last_result = Some(RunResult::from_error(err));
                self.last_summary = None;
            }
        }
        Ok(())
    }

    fn execute_cmdset_run_bulk(&mut self, profile_ids: &[String], cmdset_id: &str) -> Result<()> {
        let mut items = Vec::new();
        for profile_id in profile_ids {
            let profile = self.store.get(profile_id)?;
            let Some(profile) = profile else {
                items.push(RunSummaryItem {
                    profile_id: profile_id.clone(),
                    profile_name: "(missing)".to_string(),
                    ok: false,
                    exit_code: None,
                    error: Some("profile not found".to_string()),
                });
                continue;
            };
            let result = self.try_execute_cmdset_run(&profile.profile_id, cmdset_id);
            match result {
                Ok(run) => {
                    items.push(RunSummaryItem {
                        profile_id: profile.profile_id.clone(),
                        profile_name: profile.name.clone(),
                        ok: run.ok,
                        exit_code: Some(run.exit_code),
                        error: run.error.clone(),
                    });
                    self.last_result = Some(run);
                }
                Err(err) => {
                    items.push(RunSummaryItem {
                        profile_id: profile.profile_id.clone(),
                        profile_name: profile.name.clone(),
                        ok: false,
                        exit_code: None,
                        error: Some(err.to_string()),
                    });
                }
            }
        }
        let ok_count = items.iter().filter(|item| item.ok).count();
        let total = items.len();
        let fail_count = total - ok_count;
        self.last_summary = Some(RunSummary {
            total,
            ok_count,
            fail_count,
            items,
        });
        self.result_tab = ResultTab::Summary;
        self.status_message = Some(format!(
            "Bulk run finished: {ok_count} ok, {fail_count} failed."
        ));
        Ok(())
    }

    fn try_execute_cmdset_run(&mut self, profile_id: &str, cmdset_id: &str) -> Result<RunResult> {
        let profile = self
            .store
            .get(profile_id)?
            .ok_or_else(|| anyhow!("profile not found: {profile_id}"))?;
        if profile.profile_type != ProfileType::Ssh {
            return Err(anyhow!("run only supports SSH profiles for now"));
        }
        let ssh = ssh::resolve_client_for(
            ClientKind::Ssh,
            profile.client_overrides.as_ref(),
            self.store.conn(),
        )?;
        let auth = ssh::ssh_auth_context(self.store.conn())?;
        let run = run_cmdset_ssh(
            &self.store,
            &self.cmdset_store,
            CmdSetRunRequest {
                profile_id,
                cmdset_id,
                ssh: &ssh,
                ssh_auth_args: &auth.args,
            },
            |_| Ok(()),
        )?;
        Ok(RunResult::from_cmdset_run(run))
    }

    pub fn command_preview(&self, limit: usize) -> Vec<String> {
        let Some(profile) = self.selected_profile() else {
            return Vec::new();
        };
        let Some(cmdset) = self.selected_cmdset() else {
            return Vec::new();
        };
        let steps = self.cmdset_store.list_steps(&cmdset.cmdset_id);
        let Ok(steps) = steps else {
            return vec!["Failed to load command steps.".to_string()];
        };
        let ssh = ssh::resolve_client_for(
            ClientKind::Ssh,
            profile.client_overrides.as_ref(),
            self.store.conn(),
        );
        let Ok(ssh) = ssh else {
            return vec!["SSH client not found.".to_string()];
        };
        let auth = ssh::ssh_auth_context(self.store.conn());
        let auth_args = auth.map(|context| context.args).unwrap_or_default();
        steps
            .into_iter()
            .take(limit)
            .map(|step| {
                let cmd = mask_sensitive_tokens(&step.cmd);
                format!(
                    "{} {}@{} {}",
                    ssh::format_ssh_invocation(&ssh, profile.port, &auth_args),
                    profile.user,
                    profile.host,
                    cmd
                )
            })
            .collect()
    }

    fn update_query(&mut self) -> Result<()> {
        let trimmed = self.search_input.trim();
        self.filters.query = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        self.refresh()
    }

    fn refresh(&mut self) -> Result<()> {
        self.filtered = self.store.list_filtered(&self.filters)?;
        if self.filtered.is_empty() {
            self.profile_cursor = 0;
        } else if self.profile_cursor >= self.filtered.len() {
            self.profile_cursor = self.filtered.len() - 1;
        }
        if self.details_open {
            self.refresh_details()?;
        }
        Ok(())
    }

    pub fn toggle_mark(&mut self) {
        let Some(profile_id) = self
            .selected_profile()
            .map(|profile| profile.profile_id.clone())
        else {
            return;
        };
        if self.marked_profiles.contains(&profile_id) {
            self.marked_profiles.remove(&profile_id);
        } else {
            self.marked_profiles.insert(profile_id);
        }
    }

    pub fn toggle_details(&mut self) -> Result<()> {
        self.details_open = !self.details_open;
        if self.details_open {
            self.refresh_details()?;
        }
        Ok(())
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn scroll_details_up(&mut self) {
        if self.details_scroll > 0 {
            self.details_scroll -= 1;
        }
    }

    pub fn scroll_details_down(&mut self) {
        if self.details_scroll + 1 < self.details_lines.len() {
            self.details_scroll += 1;
        }
    }

    fn refresh_details(&mut self) -> Result<()> {
        let Some(profile) = self.selected_profile() else {
            self.details_lines = vec!["No profile selected.".to_string()];
            self.details_scroll = 0;
            return Ok(());
        };
        let env_name =
            settings::get_current_env(self.store.conn())?.unwrap_or_else(|| "none".to_string());
        let details =
            settings::resolve_settings_for_profile(self.store.conn(), &profile.profile_id, None)?;
        self.details_lines = format_resolved_details(
            profile.profile_id.as_str(),
            profile.name.as_str(),
            &env_name,
            &details,
        );
        self.details_scroll = 0;
        Ok(())
    }
}

fn format_resolved_details(
    profile_id: &str,
    profile_name: &str,
    env_name: &str,
    details: &[ResolvedSettingDetail],
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("Profile: {profile_name} ({profile_id})"));
    lines.push(format!("Current env: {env_name}"));
    lines.push(String::new());
    for detail in details {
        let resolved = detail.resolved_value.as_deref().unwrap_or("(unset)");
        let source = detail
            .resolved_source
            .as_ref()
            .map(ResolvedSettingSource::as_str)
            .unwrap_or("none");
        lines.push(format!("{} = {} ({})", detail.key, resolved, source));
        lines.push(format!(
            "  command={} profile={} env={} global={}",
            display_opt(detail.command_value.as_deref()),
            display_opt(detail.profile_value.as_deref()),
            display_opt(detail.env_value.as_deref()),
            display_opt(detail.global_value.as_deref())
        ));
        lines.push(String::new());
    }
    lines
}

fn display_opt(value: Option<&str>) -> &str {
    value.unwrap_or("(unset)")
}

fn ssh_session_result_message(ok: bool, exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) if ok => "SSH session ended.".to_string(),
        Some(code) => format!("SSH session ended with exit code {code}."),
        None => "SSH session ended without exit code.".to_string(),
    }
}

fn ssh_build_status_message(err: &SshBuildError) -> String {
    match err {
        SshBuildError::ClientNotFound { .. } => format!("SSH client not found: {err}"),
        SshBuildError::InvalidAuthOrder(_) | SshBuildError::SettingsError(_) => {
            format!("Failed to build SSH auth options: {err}")
        }
        SshBuildError::ProfileNotFound(_) | SshBuildError::UnsupportedProfileType { .. } => {
            format!("Failed to build SSH session command: {err}")
        }
    }
}

fn ssh_session_meta_json(
    session: &SshSessionCommand,
    launch_error: Option<&str>,
) -> serde_json::Value {
    let mut meta = session.safe_metadata.clone();
    if let Some(error) = launch_error {
        meta["launch_error"] = serde_json::Value::String(error.to_string());
    }
    meta
}

fn collect_groups(profiles: &[Profile]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for profile in profiles {
        if let Some(group) = &profile.group {
            set.insert(group.to_string());
        }
    }
    set.into_iter().collect()
}

fn collect_tags(profiles: &[Profile]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for profile in profiles {
        for tag in &profile.tags {
            set.insert(tag.to_string());
        }
    }
    set.into_iter().collect()
}

fn mask_sensitive_tokens(input: &str) -> String {
    let mut tokens: Vec<String> = input
        .split_whitespace()
        .map(|token| token.to_string())
        .collect();
    let mut idx = 0;
    while idx < tokens.len() {
        let token = tokens[idx].clone();
        if is_sensitive_flag(&token) && idx + 1 < tokens.len() {
            tokens[idx + 1] = "****".to_string();
            idx += 2;
            continue;
        }
        if let Some(masked) = mask_sensitive_kv(&token) {
            tokens[idx] = masked;
        }
        idx += 1;
    }
    tokens.join(" ")
}

fn is_sensitive_flag(token: &str) -> bool {
    matches!(
        token,
        "--password" | "--pass" | "--token" | "--secret" | "--api-key" | "--apikey" | "--key"
    )
}

fn mask_sensitive_kv(token: &str) -> Option<String> {
    let (key, value) = token.split_once('=')?;
    if value.is_empty() {
        return None;
    }
    let lowered = key.to_lowercase();
    if lowered.contains("password")
        || lowered.contains("secret")
        || lowered.contains("token")
        || lowered.contains("apikey")
        || lowered.contains("api_key")
    {
        Some(format!("{key}=****"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    use tdcore::cmdset::CmdSetStore;
    use tdcore::db;
    use tdcore::doctor::ClientOverrides;
    use tdcore::profile::{NewProfile, ProfileStore};

    fn empty_cmdset_store() -> CmdSetStore {
        CmdSetStore::new(db::init_in_memory().unwrap())
    }

    fn state_with_profiles(profiles: Vec<NewProfile>) -> AppState {
        let store = ProfileStore::new(db::init_in_memory().unwrap());
        for profile in profiles {
            store.insert(profile).unwrap();
        }
        AppState::new(store, empty_cmdset_store()).unwrap()
    }

    fn base_profile(profile_type: ProfileType) -> NewProfile {
        NewProfile {
            profile_id: Some("p_test".to_string()),
            name: "Test Profile".to_string(),
            profile_type,
            host: "example.com".to_string(),
            port: 2222,
            user: "alice".to_string(),
            danger_level: DangerLevel::Normal,
            group: None,
            tags: Vec::new(),
            note: None,
            initial_send: None,
            client_overrides: None,
        }
    }

    fn fake_ssh_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "teradock-tui-fake-ssh-{name}-{}{}",
            std::process::id(),
            if cfg!(windows) { ".cmd" } else { "" }
        ));
        fs::write(&path, "fake ssh").unwrap();
        path
    }

    fn sample_ssh_session_command() -> SshSessionCommand {
        SshSessionCommand {
            profile_id: "p_test".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            user: "alice".to_string(),
            profile_type: ProfileType::Ssh,
            executable: PathBuf::from("ssh"),
            args: Vec::new(),
            safe_metadata: serde_json::json!({
                "mode": "interactive",
                "source": "tui",
                "host": "example.com",
                "port": 2222,
                "user": "alice",
                "profile_type": "ssh",
            }),
        }
    }

    #[test]
    fn builds_selected_ssh_session_command() {
        let fake_ssh = fake_ssh_path("build");
        let mut profile = base_profile(ProfileType::Ssh);
        profile.client_overrides = Some(ClientOverrides {
            ssh: Some(fake_ssh.to_string_lossy().into_owned()),
            ..Default::default()
        });
        let mut state = state_with_profiles(vec![profile]);

        let command = state
            .build_ssh_session_command()
            .unwrap()
            .expect("ssh session command");

        assert_eq!(command.profile_id, "p_test");
        assert_eq!(command.host, "example.com");
        assert_eq!(command.port, 2222);
        assert_eq!(command.user, "alice");
        assert_eq!(command.profile_type, ProfileType::Ssh);
        assert_eq!(command.executable, fake_ssh);
        assert_eq!(command.args[0], OsStr::new("-p"));
        assert_eq!(command.args[1], OsStr::new("2222"));
        assert_eq!(
            command.args.last().unwrap(),
            OsStr::new("alice@example.com")
        );
        let _ = fs::remove_file(command.executable);
    }

    #[test]
    fn rejects_ssh_session_when_no_profile_is_selected() {
        let mut state = state_with_profiles(Vec::new());

        let command = state.build_ssh_session_command().unwrap();

        assert!(command.is_none());
        assert_eq!(
            state.status_message(),
            Some("No profile selected; clear filters or add a profile.")
        );
    }

    #[test]
    fn rejects_ssh_session_for_non_ssh_profile() {
        let mut state = state_with_profiles(vec![base_profile(ProfileType::Telnet)]);

        let command = state.build_ssh_session_command().unwrap();

        assert!(command.is_none());
        assert_eq!(
            state.status_message(),
            Some("Selected profile is telnet; SSH session requires an SSH profile.")
        );
    }

    #[test]
    fn critical_ssh_session_requires_confirmation() {
        let fake_ssh = fake_ssh_path("critical");
        let mut profile = base_profile(ProfileType::Ssh);
        profile.danger_level = DangerLevel::Critical;
        profile.client_overrides = Some(ClientOverrides {
            ssh: Some(fake_ssh.to_string_lossy().into_owned()),
            ..Default::default()
        });
        let mut state = state_with_profiles(vec![profile]);

        let command = state.build_ssh_session_command().unwrap();

        assert!(command.is_none());
        assert!(state.confirm_state().is_some());
        for ch in "p_test".chars() {
            state.push_confirm_char(ch);
        }
        assert_eq!(
            state.confirm_action().unwrap(),
            ConfirmedAction::OpenSshSession
        );

        let command = state
            .build_ssh_session_command()
            .unwrap()
            .expect("confirmed ssh session command");

        assert_eq!(command.profile_id, "p_test");
        let _ = fs::remove_file(command.executable);
    }

    #[test]
    fn cancelling_confirmation_sets_status_message() {
        let mut state = state_with_profiles(vec![base_profile(ProfileType::Ssh)]);
        state.confirm = Some(ConfirmState {
            message: "Confirm".to_string(),
            required_input: "p_test".to_string(),
            input: String::new(),
            action: PendingAction::OpenSshSession {
                profile_id: "p_test".to_string(),
            },
        });

        state.cancel_confirm();

        assert!(state.confirm_state().is_none());
        assert_eq!(state.status_message(), Some("Confirmation cancelled."));
    }

    #[test]
    fn formats_ssh_session_result_status_messages() {
        assert_eq!(
            ssh_session_result_message(true, Some(0)),
            "SSH session ended."
        );
        assert_eq!(
            ssh_session_result_message(false, Some(255)),
            "SSH session ended with exit code 255."
        );
        assert_eq!(
            ssh_session_result_message(false, None),
            "SSH session ended without exit code."
        );
    }

    #[test]
    fn records_ssh_session_result_to_oplog() {
        let mut state = state_with_profiles(vec![base_profile(ProfileType::Ssh)]);
        let session = sample_ssh_session_command();

        state
            .record_ssh_session_result(&session, false, None, 42)
            .unwrap();

        let (op, ok, exit_code, duration_ms, meta_json): (
            String,
            i64,
            Option<i32>,
            Option<i64>,
            String,
        ) = state
            .store
            .conn()
            .query_row(
                "SELECT op, ok, exit_code, duration_ms, meta_json FROM op_logs",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        let profile = state.store.get("p_test").unwrap().expect("profile exists");
        let meta: serde_json::Value = serde_json::from_str(&meta_json).unwrap();

        assert_eq!(op, tdcore::oplog::SSH_SESSION_OP);
        assert_eq!(ok, 0);
        assert_eq!(exit_code, None);
        assert_eq!(duration_ms, Some(42));
        assert!(profile.last_used_at.is_some());
        assert_eq!(meta["mode"], "interactive");
        assert_eq!(meta["source"], "tui");
        assert_eq!(meta["host"], "example.com");
        assert_eq!(meta["port"], 2222);
        assert_eq!(meta["user"], "alice");
        assert_eq!(meta["profile_type"], "ssh");
        assert!(meta.get("launch_error").is_none());
    }

    #[test]
    fn records_ssh_session_launch_failure_to_oplog() {
        let mut state = state_with_profiles(vec![base_profile(ProfileType::Ssh)]);
        let session = sample_ssh_session_command();

        state
            .record_ssh_session_launch_failure(&session, "permission denied", 7)
            .unwrap();

        let (ok, exit_code, meta_json): (i64, Option<i32>, String) = state
            .store
            .conn()
            .query_row(
                "SELECT ok, exit_code, meta_json FROM op_logs WHERE op = ?1",
                [tdcore::oplog::SSH_SESSION_OP],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&meta_json).unwrap();

        assert_eq!(ok, 0);
        assert_eq!(exit_code, None);
        assert_eq!(meta["launch_error"], "permission denied");
    }
}
