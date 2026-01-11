use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::state::{AppState, InputMode};

pub fn render(frame: &mut Frame<'_>, state: &AppState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(frame.size());

    let filter_line = Paragraph::new(filters_line(state)).wrap(Wrap { trim: true });
    frame.render_widget(filter_line, layout[0]);

    let hint_line = Paragraph::new(hints_line(state));
    frame.render_widget(hint_line, layout[1]);

    let items = state
        .filtered()
        .iter()
        .map(profile_item)
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Profiles"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(list, layout[2]);
}

fn filters_line(state: &AppState) -> Line<'static> {
    let type_value = state
        .filters()
        .profile_type
        .map(|t| t.to_string())
        .unwrap_or_else(|| "any".to_string());
    let group_value = state
        .filters()
        .group
        .clone()
        .unwrap_or_else(|| "any".to_string());
    let danger_value = state
        .filters()
        .danger
        .map(|d| d.to_string())
        .unwrap_or_else(|| "any".to_string());
    let tags_value = if state.filters().tags.is_empty() {
        "any".to_string()
    } else {
        state.filters().tags.join(",")
    };
    let query_value = state
        .filters()
        .query
        .clone()
        .unwrap_or_else(|| "none".to_string());
    let tag_focus = state.tag_cursor().unwrap_or("none");

    Line::from(vec![
        pill("Type", &type_value, state.filters().profile_type.is_some()),
        spacer(),
        pill("Group", &group_value, state.filters().group.is_some()),
        spacer(),
        pill("Danger", &danger_value, state.filters().danger.is_some()),
        spacer(),
        pill("Tags", &tags_value, !state.filters().tags.is_empty()),
        spacer(),
        pill("Query", &query_value, state.filters().query.is_some()),
        spacer(),
        pill("Tag Focus", tag_focus, !state.tags().is_empty()),
    ])
}

fn hints_line(state: &AppState) -> Line<'static> {
    match state.mode() {
        InputMode::Search => Line::from(vec![
            Span::styled(
                format!("Search: {}", state.search_input()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  (Enter/Esc to stop)"),
        ]),
        InputMode::Normal => Line::from(
            "Keys: / search, t type, g group, d danger, [ ] tag, space toggle tag, c clear, q quit",
        ),
    }
}

fn profile_item(profile: &tdcore::profile::Profile) -> ListItem<'static> {
    let mut meta = format!(
        "{}@{}:{} [{}]",
        profile.user, profile.host, profile.port, profile.profile_type
    );
    if let Some(group) = &profile.group {
        meta.push_str(&format!(" group:{}", group));
    }
    if !profile.tags.is_empty() {
        meta.push_str(&format!(" tags:{}", profile.tags.join(",")));
    }
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("{} ", profile.name),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("({}) ", profile.profile_id)),
        Span::styled(meta, Style::default().fg(Color::DarkGray)),
    ]))
}

fn pill(label: &str, value: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    Span::styled(format!("[{}: {}]", label, value), style)
}

fn spacer() -> Span<'static> {
    Span::raw(" ")
}
