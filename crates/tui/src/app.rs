use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tdcore::cmdset::CmdSetStore;
use tdcore::db;
use tdcore::profile::ProfileStore;

use crate::state::{ActivePane, AppState, InputMode, ResultTab};
use crate::ui;

pub fn run() -> Result<()> {
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
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        return Ok(());
                    }
                    match state.mode() {
                        InputMode::Search => handle_search_key(state, key.code)?,
                        InputMode::Normal => {
                            if handle_normal_key(state, key.code)? {
                                return Ok(());
                            }
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}

fn handle_search_key(state: &mut AppState, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Esc | KeyCode::Enter => state.exit_search(),
        KeyCode::Backspace => state.pop_search_char(),
        KeyCode::Char(ch) => state.push_search_char(ch),
        _ => Ok(()),
    }
}

fn handle_normal_key(state: &mut AppState, code: KeyCode) -> Result<bool> {
    if state.confirm_state().is_some() {
        return handle_confirm_key(state, code).map(|_| false);
    }
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('/') => state.enter_search(),
        KeyCode::Char('t') => state.cycle_profile_type()?,
        KeyCode::Char('g') => state.cycle_group()?,
        KeyCode::Char('d') => state.cycle_danger()?,
        KeyCode::Char('c') => state.clear_filters()?,
        KeyCode::Char('[') => state.tag_cursor_prev(),
        KeyCode::Char(']') => state.tag_cursor_next(),
        KeyCode::Char(' ') => state.toggle_tag()?,
        KeyCode::Tab => state.cycle_pane(),
        KeyCode::Up | KeyCode::Char('k') => match state.active_pane() {
            ActivePane::Profiles => state.prev_profile(),
            ActivePane::Actions => state.prev_cmdset(),
            ActivePane::Results => {}
        },
        KeyCode::Down | KeyCode::Char('j') => match state.active_pane() {
            ActivePane::Profiles => state.next_profile(),
            ActivePane::Actions => state.next_cmdset(),
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
        KeyCode::Char('r') | KeyCode::Enter => state.request_run()?,
        _ => {}
    }
    Ok(false)
}

fn handle_confirm_key(state: &mut AppState, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Char('y') | KeyCode::Enter => state.confirm_action(),
        KeyCode::Char('n') | KeyCode::Esc => {
            state.cancel_confirm();
            Ok(())
        }
        _ => Ok(()),
    }
}
