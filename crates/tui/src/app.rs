use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tdcore::db;
use tdcore::profile::ProfileStore;

use crate::state::{AppState, InputMode};
use crate::ui;

pub fn run() -> Result<()> {
    let conn = db::init_connection()?;
    let store = ProfileStore::new(conn);
    let mut state = AppState::new(store)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, terminal::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &mut state);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        terminal::DisableMouseCapture
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
        _ => {}
    }
    Ok(false)
}
