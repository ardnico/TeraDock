use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use rusqlite::Connection;
use tdcore::db;
use tdcore::session_log::{self, SessionLogBackendSetting, SessionLogConfig};
use tdcore::settings::{self, SettingScope, SettingScopeKind};
use tdcore::settings_registry::{self, SettingValueType};

const SESSION_LOG_KEYS: [&str; 3] = [
    session_log::SESSION_LOG_ENABLED_KEY,
    session_log::SESSION_LOG_BACKEND_KEY,
    session_log::SESSION_LOG_DIR_KEY,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsUiOutcome {
    pub saved: bool,
    pub session_log_enabled: bool,
}

pub fn run() -> Result<SettingsUiOutcome> {
    ensure_interactive_tty()?;
    let conn = db::init_connection()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_with_terminal_and_connection(&mut terminal, conn, None);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

pub(crate) fn run_in_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    profile_id: Option<String>,
) -> Result<SettingsUiOutcome> {
    let conn = db::init_connection()?;
    let outcome = run_with_terminal_and_connection(terminal, conn, profile_id)?;
    terminal.clear()?;
    Ok(outcome)
}

fn run_with_terminal_and_connection(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    conn: Connection,
    profile_id: Option<String>,
) -> Result<SettingsUiOutcome> {
    let mut state = SettingsUiState::new(conn, profile_id)?;
    loop {
        terminal.draw(|frame| render(frame, &state))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => {
                    if !should_handle_key_event(&key) {
                        continue;
                    }
                    match state.handle_key(key.code)? {
                        SettingsAction::Continue => {}
                        SettingsAction::Exit => return Ok(state.outcome()),
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

fn should_handle_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectiveSource {
    Default,
    Global,
    Env,
    Profile,
}

impl EffectiveSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Global => "global",
            Self::Env => "env",
            Self::Profile => "profile",
        }
    }

    fn overrides_global(self) -> bool {
        matches!(self, Self::Env | Self::Profile)
    }
}

#[derive(Debug, Clone)]
struct SettingsItem {
    key: String,
    description: String,
    value_type: SettingValueType,
    allowed_values: Vec<String>,
    effective_value: String,
    source: EffectiveSource,
    baseline_value: String,
    draft_value: String,
}

impl SettingsItem {
    fn dirty(&self) -> bool {
        self.draft_value != self.baseline_value
    }

    fn display_value(&self) -> &str {
        if self.dirty() {
            &self.draft_value
        } else {
            &self.effective_value
        }
    }

    fn display_source(&self) -> &'static str {
        if self.dirty() {
            "global draft"
        } else {
            self.source.as_str()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingsMode {
    Normal,
    Editing,
    ExitConfirm,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsAction {
    Continue,
    Exit,
}

pub(crate) struct SettingsUiState {
    conn: Connection,
    profile_id: Option<String>,
    items: Vec<SettingsItem>,
    diagnostics: session_log::SessionLogDiagnostics,
    cursor: usize,
    mode: SettingsMode,
    edit_buffer: String,
    status_message: String,
    saved: bool,
}

impl SettingsUiState {
    pub(crate) fn new(conn: Connection, profile_id: Option<String>) -> Result<Self> {
        let mut state = Self {
            conn,
            profile_id,
            items: Vec::new(),
            diagnostics: session_log::diagnose_config(&SessionLogConfig {
                enabled: false,
                dir: PathBuf::new(),
                backend: SessionLogBackendSetting::Auto,
            })?,
            cursor: 0,
            mode: SettingsMode::Normal,
            edit_buffer: String::new(),
            status_message: "Ready.".to_string(),
            saved: false,
        };
        state.reload()?;
        Ok(state)
    }

    fn handle_key(&mut self, code: KeyCode) -> Result<SettingsAction> {
        match self.mode {
            SettingsMode::Normal => self.handle_normal_key(code),
            SettingsMode::Editing => self.handle_edit_key(code),
            SettingsMode::ExitConfirm => Ok(self.handle_exit_confirm_key(code)),
            SettingsMode::Help => {
                if matches!(code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                    self.mode = SettingsMode::Normal;
                }
                Ok(SettingsAction::Continue)
            }
        }
    }

    fn handle_normal_key(&mut self, code: KeyCode) -> Result<SettingsAction> {
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.prev_item(),
            KeyCode::Down | KeyCode::Char('j') => self.next_item(),
            KeyCode::Left | KeyCode::Char('h') => self.cycle_current(-1)?,
            KeyCode::Right | KeyCode::Char('l') => self.cycle_current(1)?,
            KeyCode::Char(' ') => self.toggle_current_bool()?,
            KeyCode::Enter => self.enter_edit_or_cycle()?,
            KeyCode::Char('s') => self.save()?,
            KeyCode::Char('r') => {
                self.reload()?;
                self.status_message = "Reloaded settings; unsaved changes discarded.".to_string();
            }
            KeyCode::Char('d') => {
                self.refresh_diagnostics()?;
                self.status_message = "Diagnostics refreshed.".to_string();
            }
            KeyCode::Char('?') => self.mode = SettingsMode::Help,
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.dirty() {
                    self.mode = SettingsMode::ExitConfirm;
                } else {
                    return Ok(SettingsAction::Exit);
                }
            }
            _ => {}
        }
        Ok(SettingsAction::Continue)
    }

    fn handle_edit_key(&mut self, code: KeyCode) -> Result<SettingsAction> {
        match code {
            KeyCode::Esc => {
                self.mode = SettingsMode::Normal;
                self.edit_buffer.clear();
                self.status_message = "Edit cancelled.".to_string();
            }
            KeyCode::Enter => {
                let value = self.edit_buffer.trim().to_string();
                let key = self.current_item().key.clone();
                let normalized = settings_registry::validate_setting_value(&key, &value)?;
                self.current_item_mut().draft_value = normalized;
                self.mode = SettingsMode::Normal;
                self.edit_buffer.clear();
                self.status_message = "Value changed; press s to save.".to_string();
                self.refresh_diagnostics()?;
            }
            KeyCode::Backspace => {
                self.edit_buffer.pop();
            }
            KeyCode::Char(ch) => {
                self.edit_buffer.push(ch);
            }
            _ => {}
        }
        Ok(SettingsAction::Continue)
    }

    fn handle_exit_confirm_key(&mut self, code: KeyCode) -> SettingsAction {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => SettingsAction::Exit,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = SettingsMode::Normal;
                self.status_message = "Exit cancelled; press s to save.".to_string();
                SettingsAction::Continue
            }
            _ => SettingsAction::Continue,
        }
    }

    fn reload(&mut self) -> Result<()> {
        self.items = load_items(&self.conn, self.profile_id.as_deref())?;
        if self.cursor >= self.items.len() {
            self.cursor = self.items.len().saturating_sub(1);
        }
        self.refresh_diagnostics()
    }

    fn save(&mut self) -> Result<()> {
        let changes = self
            .items
            .iter()
            .filter(|item| item.dirty())
            .map(|item| {
                settings_registry::validate_setting_value(&item.key, &item.draft_value)?;
                Ok((item.key.clone(), item.draft_value.clone()))
            })
            .collect::<Result<Vec<_>>>()?;
        if changes.is_empty() {
            self.status_message = "No changes to save.".to_string();
            return Ok(());
        }
        for (key, value) in changes {
            settings::set_setting_scoped(&self.conn, &SettingScope::Global, &key, &value)?;
        }
        self.saved = true;
        self.reload()?;
        self.status_message = if self.has_override_warning() {
            "Settings saved. Selected profile/env override still controls an effective value."
                .to_string()
        } else if self.session_logging_enabled() {
            "Settings saved. Session logging enabled.".to_string()
        } else {
            "Settings saved. Session logging disabled.".to_string()
        };
        Ok(())
    }

    fn refresh_diagnostics(&mut self) -> Result<()> {
        let config = self.current_session_log_config()?;
        self.diagnostics = session_log::diagnose_config(&config)?;
        Ok(())
    }

    fn current_session_log_config(&self) -> Result<SessionLogConfig> {
        let enabled = self
            .value_for_key(session_log::SESSION_LOG_ENABLED_KEY)
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let backend_raw = self
            .value_for_key(session_log::SESSION_LOG_BACKEND_KEY)
            .unwrap_or(session_log::SESSION_LOG_BACKEND_AUTO);
        let backend = SessionLogBackendSetting::parse(backend_raw)?;
        let dir = self
            .value_for_key(session_log::SESSION_LOG_DIR_KEY)
            .map(PathBuf::from)
            .unwrap_or_default();
        Ok(SessionLogConfig {
            enabled,
            dir,
            backend,
        })
    }

    fn value_for_key(&self, key: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|item| item.key == key)
            .map(SettingsItem::display_value)
    }

    fn prev_item(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.cursor == 0 {
            self.cursor = self.items.len() - 1;
        } else {
            self.cursor -= 1;
        }
    }

    fn next_item(&mut self) {
        if !self.items.is_empty() {
            self.cursor = (self.cursor + 1) % self.items.len();
        }
    }

    fn cycle_current(&mut self, direction: i32) -> Result<()> {
        let item = self.current_item_mut();
        if item.allowed_values.len() < 2 {
            self.status_message = "Use Enter to edit this value.".to_string();
            return Ok(());
        }
        let current = item.draft_value.as_str();
        let position = item
            .allowed_values
            .iter()
            .position(|value| value == current)
            .unwrap_or(0);
        let len = item.allowed_values.len() as i32;
        let next = (position as i32 + direction).rem_euclid(len) as usize;
        item.draft_value = item.allowed_values[next].clone();
        self.status_message = "Value changed; press s to save.".to_string();
        self.refresh_diagnostics()
    }

    fn toggle_current_bool(&mut self) -> Result<()> {
        let item = self.current_item_mut();
        if !matches!(item.value_type, SettingValueType::Boolean) {
            self.status_message = "Space only toggles boolean settings.".to_string();
            return Ok(());
        }
        item.draft_value = if item.draft_value.eq_ignore_ascii_case("true") {
            "false".to_string()
        } else {
            "true".to_string()
        };
        self.status_message = "Value changed; press s to save.".to_string();
        self.refresh_diagnostics()
    }

    fn enter_edit_or_cycle(&mut self) -> Result<()> {
        let item = self.current_item();
        if matches!(item.value_type, SettingValueType::Boolean) {
            return self.toggle_current_bool();
        }
        if item.allowed_values.len() > 1 {
            return self.cycle_current(1);
        }
        let key = item.key.clone();
        let draft_value = item.draft_value.clone();
        self.edit_buffer = draft_value;
        self.mode = SettingsMode::Editing;
        self.status_message = format!("Editing {key}.");
        Ok(())
    }

    fn current_item(&self) -> &SettingsItem {
        &self.items[self.cursor]
    }

    fn current_item_mut(&mut self) -> &mut SettingsItem {
        &mut self.items[self.cursor]
    }

    fn dirty(&self) -> bool {
        self.items.iter().any(SettingsItem::dirty)
    }

    fn session_logging_enabled(&self) -> bool {
        self.diagnostics.enabled
    }

    fn has_override_warning(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.source.overrides_global() && !item.dirty())
    }

    fn outcome(&self) -> SettingsUiOutcome {
        SettingsUiOutcome {
            saved: self.saved,
            session_log_enabled: self.session_logging_enabled(),
        }
    }

    #[cfg(test)]
    fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn load_items(conn: &Connection, profile_id: Option<&str>) -> Result<Vec<SettingsItem>> {
    let mut items = Vec::new();
    for key in SESSION_LOG_KEYS {
        let schema = settings_registry::schema_for_key(key)
            .expect("session log settings should be registered");
        let default_value = session_log::default_value_for_key(conn, key)?.unwrap_or_default();
        let global_value = settings::get_setting_scoped(conn, &SettingScope::Global, key)?;
        let baseline_value = global_value
            .clone()
            .unwrap_or_else(|| default_value.clone());
        let (effective_value, source) = resolve_effective_value(
            conn,
            key,
            profile_id,
            &default_value,
            global_value.as_deref(),
        )?;
        items.push(SettingsItem {
            key: key.to_string(),
            description: schema.description.to_string(),
            value_type: schema.value_type,
            allowed_values: schema
                .allowed_values
                .iter()
                .map(|value| value.to_string())
                .collect(),
            effective_value,
            source,
            baseline_value: baseline_value.clone(),
            draft_value: baseline_value,
        });
    }
    Ok(items)
}

fn resolve_effective_value(
    conn: &Connection,
    key: &str,
    profile_id: Option<&str>,
    default_value: &str,
    global_value: Option<&str>,
) -> Result<(String, EffectiveSource)> {
    if let Some(profile_id) = profile_id {
        if settings_registry::scope_supported(key, SettingScopeKind::Profile)? {
            let profile_scope = SettingScope::Profile(profile_id.to_string());
            if let Some(value) = settings::get_setting_scoped(conn, &profile_scope, key)? {
                return Ok((value, EffectiveSource::Profile));
            }
        }
        if settings_registry::scope_supported(key, SettingScopeKind::Env)? {
            if let Some(env_name) = settings::get_current_env(conn)? {
                let env_scope = SettingScope::Env(env_name);
                if let Some(value) = settings::get_setting_scoped(conn, &env_scope, key)? {
                    return Ok((value, EffectiveSource::Env));
                }
            }
        }
    }
    if let Some(value) = global_value {
        return Ok((value.to_string(), EffectiveSource::Global));
    }
    Ok((default_value.to_string(), EffectiveSource::Default))
}

fn render(frame: &mut Frame<'_>, state: &SettingsUiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.size());

    let header = Paragraph::new(header_lines(state)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("TeraDock Settings"),
    );
    frame.render_widget(header, layout[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(layout[1]);
    render_settings_list(frame, state, body[0]);
    render_diagnostics(frame, state, body[1]);

    let footer = Paragraph::new(footer_lines(state))
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, layout[2]);

    match state.mode {
        SettingsMode::Editing => render_edit_popup(frame, state),
        SettingsMode::ExitConfirm => render_exit_confirm(frame),
        SettingsMode::Help => render_help_popup(frame),
        SettingsMode::Normal => {}
    }
}

fn header_lines(state: &SettingsUiState) -> Text<'static> {
    let context = state
        .profile_id
        .as_ref()
        .map(|profile_id| format!("Context profile: {profile_id}"))
        .unwrap_or_else(|| "Context profile: none".to_string());
    Text::from(vec![
        Line::from("Session Logging | SSH / Connection (read-only) | UI / Safety (read-only) | Paths (read-only) | Advanced (read-only)"),
        Line::from(context),
    ])
}

fn render_settings_list(frame: &mut Frame<'_>, state: &SettingsUiState, area: Rect) {
    let items = state
        .items
        .iter()
        .map(setting_list_item)
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Session Logging"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn setting_list_item(item: &SettingsItem) -> ListItem<'static> {
    let dirty = if item.dirty() { "*" } else { " " };
    let value = if matches!(item.value_type, SettingValueType::Boolean) {
        let mark = if item.display_value().eq_ignore_ascii_case("true") {
            "[x]"
        } else {
            "[ ]"
        };
        format!("{mark} {}", item.display_value())
    } else if item.allowed_values.len() > 1 {
        format!("<{}>", item.display_value())
    } else {
        item.display_value().to_string()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::raw(format!("{dirty} ")),
            Span::styled(
                format!("{:<24}", item.key),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{:<18}", value)),
            Span::styled(
                format!("source: {}", item.display_source()),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(Span::styled(
            format!("   {}", item.description),
            Style::default().fg(Color::DarkGray),
        )),
    ];
    if item.source.overrides_global() {
        lines.push(Line::from(Span::styled(
            "   Global edits may not affect this context until the override is changed.",
            Style::default().fg(Color::Yellow),
        )));
    }
    ListItem::new(lines)
}

fn render_diagnostics(frame: &mut Frame<'_>, state: &SettingsUiState, area: Rect) {
    let diagnostics = &state.diagnostics;
    let mut lines = diagnostic_rows(diagnostics)
        .into_iter()
        .map(|(label, value)| diag_line(label, &value))
        .collect::<Vec<_>>();
    if !diagnostics.hints.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Hint:",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for hint in &diagnostics.hints {
            lines.push(Line::from(format!("  {hint}")));
        }
    }
    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Diagnostics"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn diagnostic_rows(
    diagnostics: &session_log::SessionLogDiagnostics,
) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Enabled", diagnostics.enabled.to_string()),
        ("Backend setting", diagnostics.backend_setting.clone()),
        ("Resolved backend", diagnostics.resolved_backend.clone()),
        ("Platform", diagnostics.platform.clone()),
        (
            "Platform support",
            diagnostics.platform_supported.to_string(),
        ),
        (
            "PowerShell",
            command_found(
                diagnostics.powershell_command.as_ref(),
                diagnostics.powershell_command_note.as_deref(),
            ),
        ),
        (
            "ssh",
            command_found(
                diagnostics.ssh_command.as_ref(),
                diagnostics.ssh_command_note.as_deref(),
            ),
        ),
        (
            "script",
            diagnostics
                .script_command
                .as_ref()
                .map(|_| "found".to_string())
                .or_else(|| diagnostics.script_command_note.clone())
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        (
            "Log directory",
            diagnostics.log_directory.display().to_string(),
        ),
        (
            "Writable",
            diagnostics
                .log_directory_writable
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        (
            "Last session log",
            diagnostics
                .last_session_log
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
        ("Status", diagnostics.status.clone()),
    ];
    if let Some(reason) = &diagnostics.fallback_reason {
        rows.push(("Reason", display_fallback_reason(reason)));
    }
    if let Some(reliability) = &diagnostics.content_capture_reliability {
        rows.push(("Capture reliability", reliability.clone()));
    }
    if let Some(warning) = &diagnostics.warning {
        rows.push(("Warning", warning.clone()));
    }
    rows
}

fn command_found(path: Option<&PathBuf>, note: Option<&str>) -> String {
    if path.is_some() {
        "found".to_string()
    } else {
        note.unwrap_or("unknown").to_string()
    }
}

fn display_fallback_reason(reason: &str) -> String {
    if reason == session_log::SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY {
        "conpty backend required for full SSH logging".to_string()
    } else {
        reason.to_string()
    }
}

fn diag_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{label:<18}")),
        Span::styled(value.to_string(), Style::default().fg(Color::Cyan)),
    ])
}

fn footer_lines(state: &SettingsUiState) -> Text<'static> {
    let dirty = if state.dirty() { "dirty" } else { "clean" };
    Text::from(vec![
        Line::from("Up/Down move | Left/Right change | Space toggle | Enter edit | s save | r reload | d diagnostics | ? help | q/Esc exit"),
        Line::from(vec![
            Span::styled(format!("State: {dirty}"), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::raw(state.status_message.clone()),
        ]),
    ])
}

fn render_edit_popup(frame: &mut Frame<'_>, state: &SettingsUiState) {
    let area = centered_rect(72, 28, frame.size());
    frame.render_widget(Clear, area);
    let item = state.current_item();
    let text = Text::from(vec![
        Line::from(format!("{}:", item.key)),
        Line::from(""),
        Line::from(state.edit_buffer.clone()),
        Line::from(""),
        Line::from("Enter saves the edit in memory; press s on the main screen to persist."),
        Line::from("Esc cancels."),
    ]);
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Edit Value"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_exit_confirm(frame: &mut Frame<'_>) {
    let area = centered_rect(64, 24, frame.size());
    frame.render_widget(Clear, area);
    let text = Text::from(vec![
        Line::from("Unsaved changes exist."),
        Line::from(""),
        Line::from("Press y to exit without saving."),
        Line::from("Press n or Esc to return."),
    ]);
    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Discard Changes"),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_help_popup(frame: &mut Frame<'_>) {
    let area = centered_rect(74, 56, frame.size());
    frame.render_widget(Clear, area);
    let text = Text::from(vec![
        Line::from("Settings UI"),
        Line::from(""),
        Line::from("  Up/Down     move between settings"),
        Line::from("  Left/Right  cycle enum values"),
        Line::from("  Space       toggle booleans"),
        Line::from("  Enter       edit strings/paths"),
        Line::from("  s           save global settings"),
        Line::from("  r           reload and discard changes"),
        Line::from("  d           refresh diagnostics"),
        Line::from("  q/Esc       exit"),
        Line::from(""),
        Line::from("Only global scope is saved here. The source column shows when a profile or env override is currently winning."),
        Line::from("Session logs can contain secrets shown in terminal output."),
        Line::from("Press ? or Esc to close help."),
    ]);
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(rect);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn ensure_interactive_tty() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("td config ui requires an interactive TTY");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use tdcore::db;

    #[test]
    fn boolean_toggle_marks_dirty() {
        let conn = db::init_in_memory().unwrap();
        let mut state = SettingsUiState::new(conn, None).unwrap();

        state.toggle_current_bool().unwrap();

        assert!(state.current_item().dirty());
        assert_eq!(state.current_item().draft_value, "true");
    }

    #[test]
    fn enum_cycle_changes_backend() {
        let conn = db::init_in_memory().unwrap();
        let mut state = SettingsUiState::new(conn, None).unwrap();
        state.cursor = 1;

        state.cycle_current(1).unwrap();

        assert!(state.current_item().dirty());
        assert_eq!(state.current_item().draft_value, "script");
    }

    #[test]
    fn save_writes_global_scope() {
        let conn = db::init_in_memory().unwrap();
        let mut state = SettingsUiState::new(conn, None).unwrap();

        state.toggle_current_bool().unwrap();
        state.save().unwrap();

        let value = settings::get_setting_scoped(
            state.conn(),
            &SettingScope::Global,
            session_log::SESSION_LOG_ENABLED_KEY,
        )
        .unwrap();
        assert_eq!(value.as_deref(), Some("true"));
        assert!(!state.current_item().dirty());
        assert!(state.outcome().saved);
    }

    #[test]
    fn reload_discards_dirty_value() {
        let conn = db::init_in_memory().unwrap();
        let mut state = SettingsUiState::new(conn, None).unwrap();

        state.toggle_current_bool().unwrap();
        state.reload().unwrap();

        assert_eq!(state.current_item().draft_value, "false");
        assert!(!state.current_item().dirty());
    }

    #[test]
    fn settings_ui_ignores_key_release_events() {
        let key = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Release);

        assert!(!should_handle_key_event(&key));
    }

    #[test]
    fn conpty_fallback_reason_is_human_readable() {
        assert_eq!(
            display_fallback_reason(session_log::SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY),
            "conpty backend required for full SSH logging"
        );
    }

    #[test]
    fn diagnostics_rows_show_windows_auto_not_ready() {
        let mut diagnostics = diagnostics_fixture();
        diagnostics.backend_setting = session_log::SESSION_LOG_BACKEND_AUTO.to_string();
        diagnostics.resolved_backend = session_log::SESSION_LOG_BACKEND_NO_LOG.to_string();
        diagnostics.platform = "windows".to_string();
        diagnostics.platform_supported = false;
        diagnostics.fallback_reason =
            Some(session_log::SESSION_LOG_REASON_WINDOWS_REQUIRES_CONPTY.to_string());
        diagnostics.status = "not_ready".to_string();

        let rows = diagnostic_rows(&diagnostics);

        assert_eq!(
            row_value(&rows, "Resolved backend"),
            Some(session_log::SESSION_LOG_BACKEND_NO_LOG)
        );
        assert_eq!(row_value(&rows, "Status"), Some("not_ready"));
        assert_eq!(
            row_value(&rows, "Reason"),
            Some("conpty backend required for full SSH logging")
        );
    }

    #[test]
    fn diagnostics_rows_show_explicit_powershell_degraded() {
        let mut diagnostics = diagnostics_fixture();
        diagnostics.backend_setting =
            session_log::SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT.to_string();
        diagnostics.resolved_backend =
            session_log::SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT.to_string();
        diagnostics.platform = "windows".to_string();
        diagnostics.platform_supported = true;
        diagnostics.content_capture_reliability =
            Some(session_log::SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT.to_string());
        diagnostics.warning =
            Some(session_log::SESSION_LOG_DIAGNOSTIC_WARNING_POWERSHELL_TRANSCRIPT.to_string());
        diagnostics.status = "degraded".to_string();

        let rows = diagnostic_rows(&diagnostics);

        assert_eq!(
            row_value(&rows, "Resolved backend"),
            Some(session_log::SESSION_LOG_BACKEND_POWERSHELL_TRANSCRIPT)
        );
        assert_eq!(row_value(&rows, "Status"), Some("degraded"));
        assert_eq!(
            row_value(&rows, "Capture reliability"),
            Some(session_log::SESSION_LOG_CONTENT_CAPTURE_BEST_EFFORT)
        );
        assert_eq!(
            row_value(&rows, "Warning"),
            Some(session_log::SESSION_LOG_DIAGNOSTIC_WARNING_POWERSHELL_TRANSCRIPT)
        );
    }

    #[test]
    fn diagnostics_rows_show_explicit_conpty_degraded() {
        let mut diagnostics = diagnostics_fixture();
        diagnostics.backend_setting = session_log::SESSION_LOG_BACKEND_CONPTY.to_string();
        diagnostics.resolved_backend = session_log::SESSION_LOG_BACKEND_CONPTY.to_string();
        diagnostics.platform = "windows".to_string();
        diagnostics.platform_supported = true;
        diagnostics.content_capture_reliability =
            Some(session_log::SESSION_LOG_BACKEND_STATUS_EXPERIMENTAL_READY.to_string());
        diagnostics.warning =
            Some(session_log::SESSION_LOG_DIAGNOSTIC_WARNING_CONPTY_EXPERIMENTAL.to_string());
        diagnostics.status = "degraded".to_string();

        let rows = diagnostic_rows(&diagnostics);

        assert_eq!(
            row_value(&rows, "Resolved backend"),
            Some(session_log::SESSION_LOG_BACKEND_CONPTY)
        );
        assert_eq!(row_value(&rows, "Status"), Some("degraded"));
        assert_eq!(
            row_value(&rows, "Capture reliability"),
            Some(session_log::SESSION_LOG_BACKEND_STATUS_EXPERIMENTAL_READY)
        );
        assert_eq!(
            row_value(&rows, "Warning"),
            Some(session_log::SESSION_LOG_DIAGNOSTIC_WARNING_CONPTY_EXPERIMENTAL)
        );
    }

    fn diagnostics_fixture() -> session_log::SessionLogDiagnostics {
        session_log::SessionLogDiagnostics {
            enabled: true,
            backend_setting: session_log::SESSION_LOG_BACKEND_AUTO.to_string(),
            resolved_backend: session_log::SESSION_LOG_BACKEND_SCRIPT.to_string(),
            script_command: Some(PathBuf::from("script")),
            script_command_note: None,
            powershell_command: None,
            powershell_command_note: Some("not checked".to_string()),
            ssh_command: Some(PathBuf::from("ssh")),
            ssh_command_note: None,
            log_directory: PathBuf::from("session-logs"),
            log_directory_exists: true,
            log_directory_writable: Some(true),
            last_session_log: None,
            platform: "unix".to_string(),
            platform_supported: true,
            fallback_reason: None,
            content_capture_reliability: None,
            warning: None,
            status: "ready".to_string(),
            hints: Vec::new(),
        }
    }

    fn row_value<'a>(rows: &'a [(&'static str, String)], label: &str) -> Option<&'a str> {
        rows.iter()
            .find(|(row_label, _)| *row_label == label)
            .map(|(_, value)| value.as_str())
    }
}
