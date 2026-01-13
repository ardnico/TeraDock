use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::state::{ActivePane, AppState, InputMode, ResultTab};

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

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[2]);

    render_profiles(frame, state, body[0]);
    render_right(frame, state, body[1]);

    if let Some(confirm) = state.confirm_state() {
        let area = centered_rect(70, 30, frame.size());
        frame.render_widget(Clear, area);
        let block = Block::default()
            .title("Confirm")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        let text = Text::from(vec![
            Line::from(confirm.message.clone()),
            Line::from(""),
            Line::from(format!("Type '{}' to confirm.", confirm.required_input)),
            Line::from(format!("Input: {}", confirm.input)),
            Line::from(""),
            Line::from("Press Enter to confirm, Esc to cancel."),
        ]);
        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    if state.help_open() {
        let area = centered_rect(70, 60, frame.size());
        frame.render_widget(Clear, area);
        let block = Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let text = Text::from(help_lines());
        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }
}

fn render_profiles(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    let items = state
        .filtered()
        .iter()
        .map(|profile| profile_item(profile, state.marked_profiles()))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(state.profile_cursor());
    let title = format!(
        "Profiles ({}) marked:{}",
        state.filtered().len(),
        state.marked_profiles().len()
    );
    let list = List::new(items)
        .block(pane_block(
            &title,
            state.active_pane() == ActivePane::Profiles,
        ))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_right(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    if state.details_open() {
        render_details_pane(frame, state, columns[0]);
    } else {
        render_action_pane(frame, state, columns[0]);
    }
    render_results_pane(frame, state, columns[1]);
}

fn render_action_pane(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .split(area);

    let info = Paragraph::new(action_info(state))
        .block(pane_block(
            "Action",
            state.active_pane() == ActivePane::Actions,
        ))
        .wrap(Wrap { trim: true });
    frame.render_widget(info, sections[0]);

    let mut cmdset_state = ListState::default();
    cmdset_state.select(state.cmdset_cursor());
    let cmdset_items = state.cmdsets().iter().map(cmdset_item).collect::<Vec<_>>();
    let cmdset_list = List::new(cmdset_items)
        .block(Block::default().borders(Borders::ALL).title("CommandSets"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(cmdset_list, sections[1], &mut cmdset_state);

    let preview_lines = command_preview_lines(state);
    let preview = Paragraph::new(Text::from(preview_lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Command Preview"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, sections[2]);
}

fn render_results_pane(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let titles = [
        Span::raw("stdout"),
        Span::raw("stderr"),
        Span::raw("parsed"),
        Span::raw("summary"),
    ];
    let selected = match state.result_tab() {
        ResultTab::Stdout => 0,
        ResultTab::Stderr => 1,
        ResultTab::Parsed => 2,
        ResultTab::Summary => 3,
    };
    let tabs = Tabs::new(titles.to_vec())
        .select(selected)
        .block(pane_block(
            "Results",
            state.active_pane() == ActivePane::Results,
        ))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, sections[0]);

    let content = result_content(state);
    let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, sections[1]);
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
            "Keys: / search, T type, g group, D danger, [ ] tag, x toggle tag, space mark, d details, R bulk run, r run, ? help, 1-4 tabs, q quit",
        ),
    }
}

fn action_info(state: &AppState) -> Text<'static> {
    let mut lines = Vec::new();
    if let Some(profile) = state.selected_profile() {
        lines.push(Line::from(vec![
            Span::styled(
                profile.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" ({})", profile.profile_id)),
        ]));
        lines.push(Line::from(format!(
            "{}@{}:{} [{}] danger:{}",
            profile.user, profile.host, profile.port, profile.profile_type, profile.danger_level
        )));
    } else {
        lines.push(Line::from("No profile selected"));
    }
    if let Some(cmdset) = state.selected_cmdset() {
        lines.push(Line::from(format!(
            "CommandSet: {} ({})",
            cmdset.name, cmdset.cmdset_id
        )));
    } else {
        lines.push(Line::from("CommandSet: none"));
    }
    if let Some(status) = state.status_message() {
        lines.push(Line::from(Span::styled(
            status.to_string(),
            Style::default().fg(Color::Yellow),
        )));
    }
    Text::from(lines)
}

fn command_preview_lines(state: &AppState) -> Vec<Line<'static>> {
    let preview = state.command_preview(6);
    if preview.is_empty() {
        return vec![Line::from("No preview available.".to_string())];
    }
    let mut lines = preview
        .iter()
        .take(5)
        .map(|line| Line::from(line.clone()))
        .collect::<Vec<_>>();
    if preview.len() > 5 {
        lines.push(Line::from("..."));
    }
    lines
}

fn result_content(state: &AppState) -> Text<'static> {
    if let ResultTab::Summary = state.result_tab() {
        return summary_content(state);
    }
    let Some(result) = state.last_result() else {
        return Text::from("No results yet. Run a CommandSet to see output.".to_string());
    };
    if let Some(error) = &result.error {
        return Text::from(format!("Error: {error}"));
    }
    let content = match state.result_tab() {
        ResultTab::Stdout => {
            if result.stdout.is_empty() {
                "(stdout empty)".to_string()
            } else {
                result.stdout.clone()
            }
        }
        ResultTab::Stderr => {
            if result.stderr.is_empty() {
                "(stderr empty)".to_string()
            } else {
                result.stderr.clone()
            }
        }
        ResultTab::Parsed => result.parsed_pretty.clone(),
        ResultTab::Summary => String::new(),
    };
    Text::from(content)
}

fn summary_content(state: &AppState) -> Text<'static> {
    let Some(summary) = state.last_summary() else {
        return Text::from("No bulk run summary available.".to_string());
    };
    let mut lines = Vec::new();
    lines.push(Line::from(format!(
        "Bulk run summary: {} total, {} ok, {} failed",
        summary.total, summary.ok_count, summary.fail_count
    )));
    lines.push(Line::from(""));
    for item in &summary.items {
        let status = if item.ok { "ok" } else { "fail" };
        let exit = item
            .exit_code
            .map(|code| format!("exit {}", code))
            .unwrap_or_else(|| "exit ?".to_string());
        let mut line = format!(
            "{} ({}) - {} {}",
            item.profile_name, item.profile_id, status, exit
        );
        if let Some(error) = &item.error {
            line.push_str(&format!(" ({error})"));
        }
        lines.push(Line::from(line));
    }
    Text::from(lines)
}

fn cmdset_item(cmdset: &tdcore::cmdset::CmdSet) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            cmdset.name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" ({})", cmdset.cmdset_id)),
    ]))
}

fn profile_item(
    profile: &tdcore::profile::Profile,
    marked: &std::collections::BTreeSet<String>,
) -> ListItem<'static> {
    let mut meta = format!(
        "{}@{}:{} [{}] danger:{}",
        profile.user, profile.host, profile.port, profile.profile_type, profile.danger_level
    );
    if let Some(group) = &profile.group {
        meta.push_str(&format!(" group:{}", group));
    }
    if !profile.tags.is_empty() {
        meta.push_str(&format!(" tags:{}", profile.tags.join(",")));
    }
    let mark = if marked.contains(&profile.profile_id) {
        Span::styled("[*] ", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("[ ] ")
    };
    ListItem::new(Line::from(vec![
        mark,
        Span::styled(
            format!("{} ", profile.name),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("({}) ", profile.profile_id)),
        Span::styled(meta, Style::default().fg(Color::DarkGray)),
    ]))
}

fn pane_block(title: &str, active: bool) -> Block<'static> {
    let style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let title = title.to_string();
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(style)
}

fn render_details_pane(frame: &mut Frame<'_>, state: &AppState, area: Rect) {
    let lines = detail_lines(state);
    let paragraph = Paragraph::new(Text::from(lines))
        .block(pane_block(
            "Details (Resolved)",
            state.active_pane() == ActivePane::Actions,
        ))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn detail_lines(state: &AppState) -> Vec<Line<'static>> {
    let lines = state.details_lines();
    if lines.is_empty() {
        return vec![Line::from("No details available.".to_string())];
    }
    let start = state.details_scroll().min(lines.len());
    lines
        .iter()
        .skip(start)
        .map(|line| Line::from(line.clone()))
        .collect()
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Navigation"),
        Line::from("  /           search"),
        Line::from("  Tab         cycle panes"),
        Line::from("  Up/Down     move selection"),
        Line::from(""),
        Line::from("Actions"),
        Line::from("  r / Enter   run CommandSet"),
        Line::from("  R           run CommandSet on marked profiles"),
        Line::from("  d           toggle resolved details"),
        Line::from("  Space       mark/unmark profile"),
        Line::from(""),
        Line::from("Filters"),
        Line::from("  T           cycle profile type filter"),
        Line::from("  g           cycle group filter"),
        Line::from("  D           cycle danger filter"),
        Line::from("  [ / ]       tag cursor"),
        Line::from("  x           toggle tag filter"),
        Line::from("  c           clear filters"),
        Line::from(""),
        Line::from("Results"),
        Line::from("  1/2/3/4     stdout/stderr/parsed/summary tabs"),
        Line::from(""),
        Line::from("Other"),
        Line::from("  ?           toggle help"),
        Line::from("  q           quit"),
    ]
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
