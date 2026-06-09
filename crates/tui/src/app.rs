use std::io::{self, IsTerminal};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tdcore::cmdset::CmdSetStore;
use tdcore::db;
use tdcore::profile::ProfileStore;

use crate::state::{
    ActivePane, AppState, ConfirmedAction, InputMode, ResultTab, SshSessionCommand,
};
use crate::ui;

pub fn run() -> Result<()> {
    ensure_interactive_tty()?;
    let conn = db::init_connection()?;
    let store = ProfileStore::new(conn);
    let cmdset_store = CmdSetStore::new(db::init_connection()?);
    let mut state = AppState::new(store, cmdset_store)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &mut state);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => {
                    if !should_handle_key_event(&key) {
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        return Ok(());
                    }
                    match state.mode() {
                        InputMode::Search => handle_search_key(state, key.code)?,
                        InputMode::Normal => match handle_normal_key(state, key.code)? {
                            UiAction::Continue => {}
                            UiAction::Quit => return Ok(()),
                            UiAction::OpenSshSession => {
                                handle_ssh_session_request(terminal, state)?;
                            }
                        },
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

fn handle_search_key(state: &mut AppState, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Esc | KeyCode::Enter => state.exit_search(),
        KeyCode::Backspace => state.pop_search_char(),
        KeyCode::Char(ch) => state.push_search_char(ch),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiAction {
    Continue,
    Quit,
    OpenSshSession,
}

fn handle_normal_key(state: &mut AppState, code: KeyCode) -> Result<UiAction> {
    if state.confirm_state().is_some() {
        return handle_confirm_key(state, code);
    }
    match code {
        KeyCode::Char('q') => return Ok(UiAction::Quit),
        KeyCode::Char('/') => state.enter_search(),
        KeyCode::Char('T') => state.cycle_profile_type()?,
        KeyCode::Char('g') => state.cycle_group()?,
        KeyCode::Char('D') => state.cycle_danger()?,
        KeyCode::Char('c') => state.clear_filters()?,
        KeyCode::Char('[') => state.tag_cursor_prev(),
        KeyCode::Char(']') => state.tag_cursor_next(),
        KeyCode::Char('x') => state.toggle_tag()?,
        KeyCode::Char(' ') => state.toggle_mark(),
        KeyCode::Tab => state.cycle_pane(),
        KeyCode::Char('d') => state.toggle_details()?,
        KeyCode::Char('?') => state.toggle_help(),
        KeyCode::Up | KeyCode::Char('k') => match state.active_pane() {
            ActivePane::Profiles => state.prev_profile()?,
            ActivePane::Actions => {
                if state.details_open() {
                    state.scroll_details_up();
                } else {
                    state.prev_cmdset();
                }
            }
            ActivePane::Results => {}
        },
        KeyCode::Down | KeyCode::Char('j') => match state.active_pane() {
            ActivePane::Profiles => state.next_profile()?,
            ActivePane::Actions => {
                if state.details_open() {
                    state.scroll_details_down();
                } else {
                    state.next_cmdset();
                }
            }
            ActivePane::Results => {}
        },
        KeyCode::Left | KeyCode::Char('h') => match state.active_pane() {
            ActivePane::Results => state.prev_result_tab(),
            ActivePane::Actions | ActivePane::Profiles => {}
        },
        KeyCode::Right | KeyCode::Char('l') => match state.active_pane() {
            ActivePane::Results => state.next_result_tab(),
            ActivePane::Actions | ActivePane::Profiles => {}
        },
        KeyCode::Char('1') => state.set_result_tab(ResultTab::Stdout),
        KeyCode::Char('2') => state.set_result_tab(ResultTab::Stderr),
        KeyCode::Char('3') => state.set_result_tab(ResultTab::Parsed),
        KeyCode::Char('4') => state.set_result_tab(ResultTab::Summary),
        KeyCode::Char('r') | KeyCode::Enter => state.request_run()?,
        KeyCode::Char('R') => state.request_bulk_run()?,
        KeyCode::Char('s') => return Ok(UiAction::OpenSshSession),
        _ => {}
    }
    Ok(UiAction::Continue)
}

fn handle_confirm_key(state: &mut AppState, code: KeyCode) -> Result<UiAction> {
    match code {
        KeyCode::Enter => match state.confirm_action()? {
            ConfirmedAction::Continue => Ok(UiAction::Continue),
            ConfirmedAction::OpenSshSession => Ok(UiAction::OpenSshSession),
        },
        KeyCode::Backspace => {
            state.pop_confirm_char();
            Ok(UiAction::Continue)
        }
        KeyCode::Char(ch) => {
            state.push_confirm_char(ch);
            Ok(UiAction::Continue)
        }
        KeyCode::Esc => {
            state.cancel_confirm();
            Ok(UiAction::Continue)
        }
        _ => Ok(UiAction::Continue),
    }
}

fn handle_ssh_session_request(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> Result<()> {
    let Some(session) = state.build_ssh_session_command()? else {
        return Ok(());
    };
    match run_interactive_ssh_session(terminal, &session)? {
        SshSessionRunResult::Completed(outcome) => {
            if let Err(err) = state.record_ssh_session_result(
                &session,
                outcome.ok,
                outcome.exit_code,
                outcome.duration_ms,
            ) {
                state.set_status_message(format!(
                    "SSH session ended, but failed to record result: {err}"
                ));
            }
        }
        SshSessionRunResult::LaunchFailed { error, duration_ms } => {
            let error_message = error.to_string();
            let mut status_message = format!("Failed to launch SSH session: {error_message}");
            if let Err(err) =
                state.record_ssh_session_launch_failure(&session, &error_message, duration_ms)
            {
                status_message.push_str(&format!("; failed to record SSH session failure: {err}"));
            }
            state.set_status_message(status_message);
        }
    }
    Ok(())
}

struct SshSessionOutcome {
    ok: bool,
    exit_code: Option<i32>,
    duration_ms: i64,
}

enum SshSessionRunResult {
    Completed(SshSessionOutcome),
    LaunchFailed {
        error: anyhow::Error,
        duration_ms: i64,
    },
}

fn run_interactive_ssh_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &SshSessionCommand,
) -> Result<SshSessionRunResult> {
    suspend_tui_terminal(terminal)?;
    let started = Instant::now();
    let status = Command::new(&session.executable)
        .args(&session.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to launch ssh");
    let duration_ms = started.elapsed().as_millis() as i64;
    resume_tui_terminal(terminal).context("failed to restore TUI after SSH session")?;

    match status {
        Ok(status) => Ok(SshSessionRunResult::Completed(SshSessionOutcome {
            ok: status.success(),
            exit_code: status.code(),
            duration_ms,
        })),
        Err(error) => Ok(SshSessionRunResult::LaunchFailed { error, duration_ms }),
    }
}

fn ensure_interactive_tty() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("td ui requires an interactive TTY; interactive SSH sessions require a TTY");
    }
    Ok(())
}

fn suspend_tui_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn resume_tui_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.clear()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{handle_normal_key, should_handle_key_event, UiAction};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use tdcore::cmdset::CmdSetStore;
    use tdcore::db;
    use tdcore::profile::ProfileStore;

    use crate::state::AppState;

    fn empty_state() -> AppState {
        AppState::new(
            ProfileStore::new(db::init_in_memory().unwrap()),
            CmdSetStore::new(db::init_in_memory().unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn handles_press_events() {
        let key = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Press);
        assert!(should_handle_key_event(&key));
    }

    #[test]
    fn handles_repeat_events() {
        let key = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Repeat);
        assert!(should_handle_key_event(&key));
    }

    #[test]
    fn ignores_release_events() {
        let key = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Release);
        assert!(!should_handle_key_event(&key));
    }

    #[test]
    fn s_key_requests_ssh_session() {
        let mut state = empty_state();

        let action = handle_normal_key(&mut state, KeyCode::Char('s')).unwrap();

        assert_eq!(action, UiAction::OpenSshSession);
    }
}
