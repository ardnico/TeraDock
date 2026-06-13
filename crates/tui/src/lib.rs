//! Ratatui-based TUI for TeraDock.

mod app;
mod settings_ui;
mod state;
mod ui;

pub use app::run;
pub use settings_ui::{run as run_settings_ui, SettingsUiOutcome};
