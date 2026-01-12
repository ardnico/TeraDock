use std::collections::{BTreeSet, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use directories::BaseDirs;
use wait_timeout::ChildExt;

use tdcore::cmdset::{CmdSet, CmdSetStore, StepOnError};
use tdcore::doctor::{self, ClientKind};
use tdcore::oplog;
use tdcore::parser::{parse_output, ParserSpec};
use tdcore::profile::{DangerLevel, Profile, ProfileFilters, ProfileStore, ProfileType};
use tdcore::settings;

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
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub ord: i64,
    pub cmd: String,
    pub ok: bool,
    pub exit_code: i32,
    pub duration_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub parsed: serde_json::Value,
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
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    RunCmdSet {
        profile_id: String,
        cmdset_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub message: String,
    pub action: PendingAction,
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
    status_message: Option<String>,
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
            status_message: None,
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

    pub fn cmdsets(&self) -> &[CmdSet] {
        &self.cmdsets
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

    pub fn next_profile(&mut self) {
        if !self.filtered.is_empty() {
            self.profile_cursor = (self.profile_cursor + 1) % self.filtered.len();
        }
    }

    pub fn prev_profile(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        if self.profile_cursor == 0 {
            self.profile_cursor = self.filtered.len() - 1;
        } else {
            self.profile_cursor -= 1;
        }
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
            ResultTab::Parsed => ResultTab::Stdout,
        };
    }

    pub fn prev_result_tab(&mut self) {
        self.result_tab = match self.result_tab {
            ResultTab::Stdout => ResultTab::Parsed,
            ResultTab::Stderr => ResultTab::Stdout,
            ResultTab::Parsed => ResultTab::Stderr,
        };
    }

    pub fn set_result_tab(&mut self, tab: ResultTab) {
        self.result_tab = tab;
    }

    pub fn cancel_confirm(&mut self) {
        self.confirm = None;
    }

    pub fn confirm_action(&mut self) -> Result<()> {
        let Some(confirm) = self.confirm.take() else {
            return Ok(());
        };
        match confirm.action {
            PendingAction::RunCmdSet {
                profile_id,
                cmdset_id,
            } => self.execute_cmdset_run(&profile_id, &cmdset_id),
        }
    }

    pub fn request_run(&mut self) -> Result<()> {
        let (profile_id, cmdset_id, danger_level, profile_label) = {
            let Some(profile) = self.selected_profile() else {
                self.status_message = Some("No profile selected.".to_string());
                return Ok(());
            };
            let Some(cmdset) = self.selected_cmdset() else {
                self.status_message = Some("No CommandSet selected.".to_string());
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
                    "Profile '{}' is critical. Run CommandSet '{}' on {} ?",
                    profile_id, cmdset_id, profile_label
                ),
                action: PendingAction::RunCmdSet {
                    profile_id,
                    cmdset_id,
                },
            });
            return Ok(());
        }
        self.execute_cmdset_run(&profile_id, &cmdset_id)
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
            }
            Err(err) => {
                self.status_message = Some(format!("Run failed: {err}"));
                self.last_result = Some(RunResult::from_error(err));
            }
        }
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
        let cmdset = self
            .cmdset_store
            .get(cmdset_id)?
            .ok_or_else(|| anyhow!("cmdset not found: {cmdset_id}"))?;
        let steps = self.cmdset_store.list_steps(&cmdset.cmdset_id)?;
        if steps.is_empty() {
            return Err(anyhow!("cmdset has no steps: {cmdset_id}"));
        }

        let ssh = resolve_client_for(
            ClientKind::Ssh,
            profile.client_overrides.as_ref(),
            &self.store,
        )?;
        let auth = ssh_auth_context(self.store.conn())?;

        let run_started = Instant::now();
        let mut stdout_all = String::new();
        let mut stderr_all = String::new();
        let mut step_results = Vec::new();
        let mut overall_ok = true;
        let mut last_exit_code = 0;

        for step in steps {
            let mut command = build_ssh_command(&ssh, &profile, &auth.args, &step.cmd);
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
                ParserSpec::Regex(id) => self.cmdset_store.get_parser(id)?,
                _ => None,
            };
            let parsed = parse_output(&step.parser_spec, &stdout_text, parser_def.as_ref())?;

            step_results.push(StepResult {
                ord: step.ord,
                cmd: step.cmd,
                ok,
                exit_code,
                stdout: stdout_text,
                stderr: stderr_text,
                duration_ms,
                parsed,
            });

            if !ok && step.on_error == StepOnError::Stop {
                break;
            }
        }

        let duration_ms = run_started.elapsed().as_millis() as i64;
        self.store.touch_last_used(&profile.profile_id)?;
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
        oplog::log_operation(self.store.conn(), entry)?;

        let steps_json = step_results
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

        Ok(RunResult {
            ok: overall_ok,
            exit_code: last_exit_code,
            duration_ms,
            stdout: stdout_all,
            stderr: stderr_all,
            parsed_pretty,
            error: None,
        })
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
        let ssh = resolve_client_for(
            ClientKind::Ssh,
            profile.client_overrides.as_ref(),
            &self.store,
        );
        let Ok(ssh) = ssh else {
            return vec!["SSH client not found.".to_string()];
        };
        let auth = ssh_auth_context(self.store.conn());
        let auth_args = auth.map(|context| context.args).unwrap_or_default();
        steps
            .into_iter()
            .take(limit)
            .map(|step| {
                let cmd = mask_sensitive_tokens(&step.cmd);
                format!(
                    "{} {}@{} {}",
                    format_ssh_invocation(&ssh, profile.port, &auth_args),
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
        Ok(())
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

struct SshAuthAvailability {
    agent: bool,
    keys: bool,
}

struct SshAuthContext {
    args: Vec<OsString>,
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
    normalize_auth_order(order)
}

fn default_auth_order() -> Vec<SshAuthMethod> {
    vec![
        SshAuthMethod::Agent,
        SshAuthMethod::Keys,
        SshAuthMethod::Password,
    ]
}

fn load_ssh_auth_order(conn: &rusqlite::Connection) -> Result<Vec<SshAuthMethod>> {
    match settings::get_ssh_auth_order(conn)? {
        Some(raw) => parse_auth_order_setting(&raw)
            .map_err(|err| anyhow!("invalid ssh auth order setting: {err}")),
        None => Ok(default_auth_order()),
    }
}

fn detect_ssh_auth_availability() -> SshAuthAvailability {
    let agent = std::env::var_os("SSH_AUTH_SOCK")
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
                let available = match method {
                    SshAuthMethod::Agent => availability.agent,
                    SshAuthMethod::Keys => availability.keys,
                    _ => false,
                };
                if !publickey_added && available {
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

fn ssh_auth_context(conn: &rusqlite::Connection) -> Result<SshAuthContext> {
    let order = load_ssh_auth_order(conn)?;
    let availability = detect_ssh_auth_availability();
    let args = build_ssh_auth_args(&order, &availability);
    Ok(SshAuthContext { args })
}

fn resolve_client_for(
    kind: ClientKind,
    profile_overrides: Option<&tdcore::doctor::ClientOverrides>,
    store: &ProfileStore,
) -> Result<PathBuf> {
    let global_overrides = settings::get_client_overrides(store.conn())?;
    doctor::resolve_client_with_overrides(kind, profile_overrides, global_overrides.as_ref())
        .ok_or_else(|| anyhow!("{} client not found via overrides or PATH", kind.as_str()))
}

fn build_ssh_command(
    ssh: &PathBuf,
    profile: &Profile,
    auth_args: &[OsString],
    cmd: &str,
) -> Command {
    let mut command = Command::new(ssh);
    command
        .arg("-p")
        .arg(profile.port.to_string())
        .args(auth_args)
        .arg(format!("{}@{}", profile.user, profile.host))
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Output> {
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

fn format_ssh_invocation(ssh: &PathBuf, port: u16, auth_args: &[OsString]) -> String {
    let mut parts = vec![
        ssh.to_string_lossy().to_string(),
        "-p".to_string(),
        port.to_string(),
    ];
    parts.extend(
        auth_args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string()),
    );
    parts.join(" ")
}

fn mask_sensitive_tokens(input: &str) -> String {
    let mut tokens: Vec<String> = input
        .split_whitespace()
        .map(|token| token.to_string())
        .collect();
    let mut idx = 0;
    while idx < tokens.len() {
        let token = tokens[idx].clone();
        if is_sensitive_flag(&token) {
            if idx + 1 < tokens.len() {
                tokens[idx + 1] = "****".to_string();
                idx += 2;
                continue;
            }
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
