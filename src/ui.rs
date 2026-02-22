use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::service::{
    format_bytes, format_cpu_time, format_log_timestamp, format_relative_time, priority_label,
    LogEntry, TimeRange, UnitAction, FILE_STATE_OPTIONS, PRIORITY_LABELS, TIME_RANGES, UNIT_TYPES,
};

/// Layout regions for mouse hit testing
pub struct LayoutRegions {
    pub services_list: Rect,
    pub logs_panel: Option<Rect>,
}

/// Get layout regions for mouse hit testing
pub fn get_layout_regions(area: Rect, show_logs: bool) -> LayoutRegions {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);

    let (services_area, logs_area) = if show_logs {
        let middle = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(chunks[1]);
        (middle[0], Some(middle[1]))
    } else {
        (chunks[1], None)
    };

    LayoutRegions {
        services_list: services_area,
        logs_panel: logs_area,
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    // Load logs for selected service if selection changed (only if logs are visible)
    if app.show_logs {
        app.load_logs_for_selected();
    }

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    // Conditionally split middle section for logs panel
    let (services_area, logs_area) = if app.show_logs {
        let middle_chunks = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(chunks[1]);
        (middle_chunks[0], Some(middle_chunks[1]))
    } else {
        (chunks[1], None)
    };

    // Header / Search bar
    let header = if app.log_search_mode {
        let match_info = if app.log_search_matches.is_empty() {
            if app.log_search_query.is_empty() {
                String::new()
            } else {
                " (no matches)".to_string()
            }
        } else {
            format!(
                " ({}/{})",
                app.log_search_match_index.map_or(0, |i| i + 1),
                app.log_search_matches.len()
            )
        };
        let search_text = format!("/{}_{}",  app.log_search_query, match_info);
        Paragraph::new(search_text)
            .style(Style::default().fg(Color::Magenta))
            .block(Block::default().borders(Borders::ALL).title("Log Search"))
    } else if !app.log_search_query.is_empty() && app.show_logs {
        let match_info = format!(
            "Log search: \"{}\" ({} matches) | n/N: Next/Prev",
            app.log_search_query,
            app.log_search_matches.len()
        );
        Paragraph::new(match_info)
            .style(Style::default().fg(Color::Magenta))
            .block(Block::default().borders(Borders::ALL))
    } else if app.search_mode {
        let search_text = format!("/{}_", app.search_query);
        Paragraph::new(search_text)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("Search"))
    } else if !app.search_query.is_empty() || app.status_filter.is_some() || app.file_state_filter.is_some() {
        let mut info_parts = Vec::new();
        if !app.search_query.is_empty() {
            info_parts.push(format!("Search: {}", app.search_query));
        }
        if let Some(ref status) = app.status_filter {
            info_parts.push(format!("Status: {}", status));
        }
        if let Some(ref fs) = app.file_state_filter {
            info_parts.push(format!("File state: {}", fs));
        }
        let info = format!("{} ({} matches)", info_parts.join(" | "), app.filtered_indices.len());
        Paragraph::new(info)
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::ALL))
    } else if let Some(ref msg) = app.status_message {
        Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL))
    } else {
        let scope_label = if app.user_mode { "User" } else { "System" };
        Paragraph::new(format!("SystemD {} [{}]", app.unit_type.label(), scope_label))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL))
    };
    frame.render_widget(header, chunks[0]);

    // Services list
    if let Some(ref error) = app.error {
        let error_msg = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Error"));
        frame.render_widget(error_msg, services_area);
    } else {
        let items: Vec<ListItem> = app
            .filtered_indices
            .iter()
            .map(|&i| &app.services[i])
            .map(|unit| {
                let status_color = unit.status_color();
                let mut spans = vec![
                    Span::styled(
                        format!("{:8}", unit.status_display()),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&unit.unit, Style::default().fg(Color::White)),
                ];
                if let Some(ref detail) = unit.detail {
                    spans.push(Span::styled(
                        format!("  ({})", detail),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                if let Some(ref fs) = unit.file_state {
                    spans.push(Span::styled(
                        format!("  [{}]", fs),
                        Style::default().fg(file_state_color(fs)),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();

        let type_label = app.unit_type.label();
        let title = if app.search_query.is_empty() && app.status_filter.is_none() && app.file_state_filter.is_none() {
            format!("{} ({})", type_label, app.services.len())
        } else {
            format!(
                "{} ({}/{})",
                type_label,
                app.filtered_indices.len(),
                app.services.len()
            )
        };

        let services_border_style = if app.show_logs {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(services_border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, services_area, &mut app.list_state);
    }

    // Logs panel (only if visible)
    if let Some(logs_area) = logs_area {
        let mut logs_title = if let Some(ref service_name) = app.last_selected_service {
            format!("Logs: {}", service_name)
        } else {
            "Logs".to_string()
        };

        if let Some(p) = app.log_priority_filter {
            logs_title.push_str(&format!(" [p:{}]", priority_label(p)));
        }
        if app.log_time_range != TimeRange::All {
            logs_title.push_str(&format!(" [t:{}]", app.log_time_range.label()));
        }

        let focused_suffix = " [FOCUSED]";

        // Calculate visible area (subtract 2 for borders)
        let visible_lines = logs_area.height.saturating_sub(2) as usize;

        // Create log content with scroll, search highlighting, and boot separators
        let mut log_lines: Vec<Line> = Vec::new();
        let mut entries_shown = 0;
        for (entry_idx, entry) in app.logs.iter().enumerate().skip(app.logs_scroll) {
            if log_lines.len() >= visible_lines {
                break;
            }
            // Boot boundary separator
            if entry_idx > 0
                && let (Some(prev), Some(cur)) = (
                    &app.logs[entry_idx - 1].boot_id,
                    &entry.boot_id,
                )
                && prev != cur
            {
                let content_width = logs_area.width.saturating_sub(2) as usize;
                let short_id = &cur[..cur.len().min(12)];
                let boot_ts = entry
                    .timestamp
                    .map(|ts| format!(" · {}", format_log_timestamp(ts)))
                    .unwrap_or_default();
                let label = format!(" Boot {} {} ", short_id, boot_ts);
                let pad_total = content_width.saturating_sub(label.len());
                let pad_left = pad_total / 2;
                let pad_right = pad_total - pad_left;
                let separator = format!(
                    "{}{}{}",
                    "─".repeat(pad_left),
                    label,
                    "─".repeat(pad_right),
                );
                let style = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                log_lines.push(Line::from(Span::styled(separator, style)));
                if log_lines.len() >= visible_lines {
                    break;
                }
            }
            log_lines.push(render_log_entry(entry, entry_idx, app));
            entries_shown += 1;
        }

        let scroll_info = if !app.logs.is_empty() {
            format!(
                " [{}-{}/{}]",
                app.logs_scroll + 1,
                app.logs_scroll + entries_shown,
                app.logs.len()
            )
        } else {
            String::new()
        };

        let border_style = Style::default().fg(Color::Yellow);

        let logs_paragraph = Paragraph::new(log_lines)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("{}{}{}", logs_title, focused_suffix, scroll_info))
                    .border_style(border_style),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(logs_paragraph, logs_area);
    }

    // Footer with keybindings
    let footer_text = if app.show_help {
        "Press any key to close"
    } else if app.show_confirm && app.action_in_progress {
        "Executing..."
    } else if app.show_confirm && app.action_result.is_some() {
        "Press any key to dismiss"
    } else if app.show_confirm {
        "Y: Confirm | N/Esc: Cancel"
    } else if app.show_action_picker {
        "j/k: Navigate | Enter: Select | Esc/x: Close"
    } else if app.show_details {
        "j/k: Scroll | g/G: Top/Bottom | PgUp/PgDn: Page | Esc/i: Close"
    } else if app.show_status_picker {
        "j/k: Navigate | Enter: Select | Esc/s: Close"
    } else if app.show_type_picker {
        "j/k: Navigate | Enter: Select | Esc/t: Close"
    } else if app.show_priority_picker {
        "j/k: Navigate | Enter: Select | Esc/p: Close"
    } else if app.show_time_picker {
        "j/k: Navigate | Enter: Select | Esc/T: Close"
    } else if app.show_file_state_picker {
        "j/k: Navigate | Enter: Select | Esc/f: Close"
    } else if app.log_search_mode {
        "Type to search logs | Esc/Enter: Exit search | ?: Help"
    } else if app.show_logs && !app.log_search_query.is_empty() {
        "l: Exit logs | j/k: Scroll | n/N: Next/Prev match | p: Priority | T: Time | /: Search | ?: Help"
    } else if app.show_logs {
        "l: Exit logs | j/k: Scroll | g/G: Top/Bottom | /: Search | p: Priority | T: Time | ?: Help"
    } else if app.search_mode {
        "Type to search | Esc/Enter: Exit search | ?: Help"
    } else if !app.search_query.is_empty() || app.status_filter.is_some() || app.file_state_filter.is_some() {
        "q: Quit | /: Search | s: Status | f: File state | x: Actions | i: Details | t: Type | l: Logs | u: User/System | Esc: Clear | ?: Help"
    } else {
        "q/Esc: Quit | /: Search | s: Status | f: File state | x: Actions | i: Details | t: Type | l: Logs | u: User/System | ?: Help"
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);

    // Status picker overlay
    if app.show_status_picker {
        render_status_picker(frame, app);
    }

    // Type picker overlay
    if app.show_type_picker {
        render_type_picker(frame, app);
    }

    // Priority picker overlay
    if app.show_priority_picker {
        render_priority_picker(frame, app);
    }

    // Time range picker overlay
    if app.show_time_picker {
        render_time_picker(frame, app);
    }

    // File state picker overlay
    if app.show_file_state_picker {
        render_file_state_picker(frame, app);
    }

    // Action picker overlay
    if app.show_action_picker {
        render_action_picker(frame, app);
    }

    // Confirmation dialog overlay
    if app.show_confirm {
        render_confirm_dialog(frame, app);
    }

    // Details modal (on top of pickers)
    if app.show_details {
        render_details_modal(frame, app);
    }

    // Help overlay
    if app.show_help {
        render_help(frame, app);
    }
}

fn priority_color(p: u8) -> (Color, bool) {
    match p {
        0..=2 => (Color::Red, true),    // emerg/alert/crit - bold
        3 => (Color::Red, false),        // err
        4 => (Color::Yellow, false),     // warning
        5 => (Color::Cyan, false),       // notice
        6 => (Color::White, false),      // info
        7 => (Color::DarkGray, false),   // debug
        _ => (Color::White, false),
    }
}

fn render_log_entry<'a>(entry: &LogEntry, line_idx: usize, app: &App) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();

    // Timestamp
    if let Some(ts) = entry.timestamp {
        let formatted = format_log_timestamp(ts);
        if !formatted.is_empty() {
            spans.push(Span::styled(
                formatted,
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::raw(" "));
        }
    }

    // Priority label
    let (msg_color, msg_bold) = entry
        .priority
        .map(priority_color)
        .unwrap_or((Color::White, false));

    if let Some(p) = entry.priority {
        let label = priority_label(p);
        let (color, bold) = priority_color(p);
        let mut style = Style::default().fg(color);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(format!("[{}]", label), style));
        spans.push(Span::raw(" "));
    }

    // Identifier/PID
    match (&entry.identifier, &entry.pid) {
        (Some(ident), Some(pid)) => {
            spans.push(Span::styled(
                format!("({}/{}): ", ident, pid),
                Style::default().fg(Color::DarkGray),
            ));
        }
        (Some(ident), None) => {
            spans.push(Span::styled(
                format!("{}: ", ident),
                Style::default().fg(Color::DarkGray),
            ));
        }
        (None, Some(pid)) => {
            spans.push(Span::styled(
                format!("({}): ", pid),
                Style::default().fg(Color::DarkGray),
            ));
        }
        (None, None) => {}
    }

    // Message with severity coloring and search highlighting
    let mut base_style = Style::default().fg(msg_color);
    if msg_bold {
        base_style = base_style.add_modifier(Modifier::BOLD);
    }

    let message_spans = highlight_search_in_message(&entry.message, line_idx, app, base_style);
    spans.extend(message_spans);

    Line::from(spans)
}

fn highlight_search_in_message<'a>(
    message: &str,
    line_idx: usize,
    app: &App,
    base_style: Style,
) -> Vec<Span<'a>> {
    if app.log_search_query.is_empty() {
        return vec![Span::styled(message.to_string(), base_style)];
    }

    let query_lower = app.log_search_query.to_lowercase();
    let msg_lower = message.to_lowercase();

    if !msg_lower.contains(&query_lower) {
        return vec![Span::styled(message.to_string(), base_style)];
    }

    let is_current_match = app.log_search_match_index.is_some_and(|mi| {
        app.log_search_matches
            .get(mi)
            .is_some_and(|&idx| idx == line_idx)
    });

    let highlight_style = if is_current_match {
        Style::default().bg(Color::Yellow).fg(Color::Black)
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::Yellow)
    };

    let mut spans = Vec::new();
    let mut pos = 0;
    let query_len = app.log_search_query.len();

    while pos < message.len() {
        if let Some(match_start) = msg_lower[pos..].find(&query_lower) {
            let abs_start = pos + match_start;
            if abs_start > pos {
                spans.push(Span::styled(message[pos..abs_start].to_string(), base_style));
            }
            spans.push(Span::styled(
                message[abs_start..abs_start + query_len].to_string(),
                highlight_style,
            ));
            pos = abs_start + query_len;
        } else {
            spans.push(Span::styled(message[pos..].to_string(), base_style));
            break;
        }
    }

    spans
}

fn render_help(frame: &mut Frame, app: &App) {
    let section_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let mut help_text = vec![
        Line::from(vec![Span::styled("Navigation", section_style)]),
        Line::from("  j / Down      Move down"),
        Line::from("  k / Up        Move up"),
        Line::from("  g / Home      Go to top"),
        Line::from("  G / End       Go to bottom"),
        Line::from(""),
        Line::from(vec![Span::styled("Search & Filter", section_style)]),
        Line::from("  /             Start search"),
        Line::from("  s             Open status filter"),
        Line::from("  f             Open file state filter"),
        Line::from("  t             Open unit type picker"),
        Line::from("  p             Log priority filter"),
        Line::from("  T             Log time range filter"),
        Line::from("  Esc           Clear search/filter"),
        Line::from(""),
        Line::from(vec![Span::styled("Unit Details", section_style)]),
        Line::from("  i / Enter     Open details modal"),
        Line::from("  j/k           Scroll details"),
        Line::from("  g/G           Top/Bottom of details"),
        Line::from("  Esc/i/Enter   Close details"),
        Line::from(""),
        Line::from(vec![Span::styled("Unit Actions", section_style)]),
        Line::from("  x             Open action picker"),
        Line::from("  R             Daemon reload (direct)"),
        Line::from(""),
        Line::from(vec![Span::styled("Logs Panel", section_style)]),
        Line::from("  l             Toggle logs panel"),
        Line::from("  PgUp/PgDn     Scroll list/logs"),
        Line::from("  Ctrl+u/d      Scroll logs half page"),
        Line::from(""),
    ];

    if app.show_logs {
        help_text.extend(vec![
            Line::from(vec![Span::styled("Log Focus Mode", section_style)]),
            Line::from("  j/k / Up/Down Scroll logs"),
            Line::from("  g / Home      Go to top of logs"),
            Line::from("  G / End       Go to bottom of logs"),
            Line::from("  /             Search within logs"),
            Line::from("  n / N         Next/Prev search match"),
            Line::from("  p             Log priority filter"),
            Line::from("  T             Log time range filter"),
            Line::from("  l             Exit log mode"),
            Line::from("  Esc           Clear log search"),
            Line::from(""),
        ]);
    }

    help_text.extend(vec![
        Line::from(vec![Span::styled("Mouse", section_style)]),
        Line::from("  Click         Select service"),
        Line::from("  Scroll        Navigate list/logs"),
        Line::from(""),
        Line::from(vec![Span::styled("Other", section_style)]),
        Line::from("  r             Refresh services"),
        Line::from("  u             Toggle user/system"),
        Line::from("  ?             Toggle this help"),
        Line::from("  q / Esc       Quit"),
    ]);

    let area = centered_rect(50, 70, frame.area());

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .style(Style::default().bg(Color::Black)),
        );

    frame.render_widget(Clear, area);
    frame.render_widget(help, area);
}

fn render_status_picker(frame: &mut Frame, app: &mut App) {
    let options = app.unit_type.status_options();
    let items: Vec<ListItem> = options
        .iter()
        .map(|&opt| {
            let color = match opt {
                "All" => Color::Cyan,
                "running" => Color::Green,
                "exited" => Color::Yellow,
                "failed" => Color::Red,
                "dead" => Color::DarkGray,
                "waiting" => Color::Cyan,
                "listening" => Color::Green,
                "active" => Color::Green,
                "inactive" => Color::DarkGray,
                "elapsed" => Color::Yellow,
                _ => Color::White,
            };
            let is_active = match (&app.status_filter, opt) {
                (None, "All") => true,
                (Some(f), o) => f == o,
                _ => false,
            };
            let marker = if is_active { " *" } else { "" };
            let text = format!("  {}{}", opt, marker);
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Status Filter")
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(30, options.len() as u16 + 2, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.status_picker_state);
}

fn render_type_picker(frame: &mut Frame, app: &mut App) {
    let items: Vec<ListItem> = UNIT_TYPES
        .iter()
        .map(|&ut| {
            let is_active = ut == app.unit_type;
            let marker = if is_active { " *" } else { "" };
            let text = format!("  {}{}", ut.label(), marker);
            ListItem::new(text).style(Style::default().fg(Color::Cyan))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Unit Type")
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(30, UNIT_TYPES.len() as u16 + 2, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.type_picker_state);
}

fn render_priority_picker(frame: &mut Frame, app: &mut App) {
    // 9 items: "All" + 8 priority levels
    let mut items: Vec<ListItem> = Vec::with_capacity(9);

    // "All" option
    let all_active = app.log_priority_filter.is_none();
    let all_marker = if all_active { " *" } else { "" };
    items.push(
        ListItem::new(format!("  All{}", all_marker))
            .style(Style::default().fg(Color::Cyan)),
    );

    // Priority levels 0-7
    for (i, &label) in PRIORITY_LABELS.iter().enumerate() {
        let p = i as u8;
        let is_active = app.log_priority_filter == Some(p);
        let marker = if is_active { " *" } else { "" };
        let (color, bold) = priority_color(p);
        let mut style = Style::default().fg(color);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        items.push(ListItem::new(format!("  {} (0-{}){}",  label, i, marker)).style(style));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Log Priority Filter")
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(30, 11, frame.area()); // 9 items + 2 border
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.priority_picker_state);
}

fn render_time_picker(frame: &mut Frame, app: &mut App) {
    let items: Vec<ListItem> = TIME_RANGES
        .iter()
        .map(|&tr| {
            let is_active = tr == app.log_time_range;
            let marker = if is_active { " *" } else { "" };
            let text = format!("  {}{}", tr.label(), marker);
            ListItem::new(text).style(Style::default().fg(Color::Cyan))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Log Time Range")
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(30, TIME_RANGES.len() as u16 + 2, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.time_picker_state);
}

fn centered_fixed_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(
        x,
        y,
        width.min(area.width),
        height.min(area.height),
    )
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Returns the number of visible lines in the logs panel
pub fn get_logs_visible_lines(frame: &Frame, show_logs: bool) -> usize {
    if !show_logs {
        return 0;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let middle_chunks = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ])
    .split(chunks[1]);

    middle_chunks[1].height.saturating_sub(2) as usize
}

/// Returns the number of visible lines in the services list
pub fn get_services_visible_lines(frame: &Frame, show_logs: bool) -> usize {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let services_area = if show_logs {
        let middle_chunks = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(chunks[1]);
        middle_chunks[0]
    } else {
        chunks[1]
    };

    services_area.height.saturating_sub(2) as usize
}

/// Returns the number of visible lines in the details modal
pub fn get_details_visible_lines(frame: &Frame) -> usize {
    let area = centered_rect(70, 80, frame.area());
    // Subtract 2 for borders
    area.height.saturating_sub(2) as usize
}

fn file_state_color(state: &str) -> Color {
    match state {
        "enabled" => Color::Green,
        "disabled" => Color::Yellow,
        "static" => Color::DarkGray,
        "masked" => Color::Red,
        "indirect" => Color::Cyan,
        _ => Color::White,
    }
}

fn render_file_state_picker(frame: &mut Frame, app: &mut App) {
    let items: Vec<ListItem> = FILE_STATE_OPTIONS
        .iter()
        .map(|&opt| {
            let color = match opt {
                "All" => Color::Cyan,
                other => file_state_color(other),
            };
            let is_active = match (&app.file_state_filter, opt) {
                (None, "All") => true,
                (Some(f), o) => f == o,
                _ => false,
            };
            let marker = if is_active { " *" } else { "" };
            let text = format!("  {}{}", opt, marker);
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("File State Filter")
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(30, FILE_STATE_OPTIONS.len() as u16 + 2, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.file_state_picker_state);
}

fn action_color(action: &UnitAction) -> Color {
    match action {
        UnitAction::Start => Color::Green,
        UnitAction::Stop => Color::Red,
        UnitAction::Restart => Color::Yellow,
        UnitAction::Reload => Color::Cyan,
        UnitAction::Enable => Color::Green,
        UnitAction::Disable => Color::Yellow,
        UnitAction::DaemonReload => Color::Magenta,
    }
}

fn render_action_picker(frame: &mut Frame, app: &mut App) {
    let unit_name = app
        .selected_unit()
        .map(|u| u.unit.clone())
        .unwrap_or_default();

    let items: Vec<ListItem> = app
        .available_actions
        .iter()
        .map(|action| {
            let text = format!("  {}", action.label());
            ListItem::new(text).style(Style::default().fg(action_color(action)))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Actions: {}", unit_name))
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let area = centered_fixed_rect(40, app.available_actions.len() as u16 + 2, frame.area());
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(list, area, &mut app.action_picker_state);
}

fn render_confirm_dialog(frame: &mut Frame, app: &App) {
    let (action, unit_name) = match (&app.confirm_action, &app.confirm_unit_name) {
        (Some(a), Some(n)) => (a, n),
        _ => return,
    };

    let (text, title) = if let Some(ref result) = app.action_result {
        // Show result
        let (msg, color) = match result {
            Ok(msg) => (msg.as_str(), Color::Green),
            Err(msg) => (msg.as_str(), Color::Red),
        };
        let text = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                msg.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Press any key to dismiss",
                Style::default().fg(Color::DarkGray),
            )]),
        ];
        let title = if result.is_ok() {
            "Action Succeeded"
        } else {
            "Action Failed"
        };
        (text, title)
    } else if app.action_in_progress {
        // Show progress
        let text = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                action.progress_label().to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(""),
        ];
        (text, "Executing")
    } else {
        // Show confirmation prompt
        let message = action.confirmation_message(unit_name);
        let text = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                message,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(" Confirm  "),
                Span::styled("[N/Esc]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(" Cancel"),
            ]),
        ];
        (text, "Confirm Action")
    };

    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().bg(Color::Black)),
        )
        .alignment(ratatui::layout::Alignment::Center);

    let area = centered_fixed_rect(50, 6, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn render_details_modal(frame: &mut Frame, app: &mut App) {
    let props = match &app.detail_properties {
        Some(p) => p.clone(),
        None => return,
    };
    let unit_name = app.detail_unit_name.clone().unwrap_or_default();

    let mut lines: Vec<Line> = Vec::new();

    let section_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::Cyan);
    let value_style = Style::default().fg(Color::White);

    // General section
    lines.push(Line::from(vec![Span::styled("General", section_style)]));
    lines.push(Line::from(vec![
        Span::styled("  Description:    ", label_style),
        Span::styled(props.description.clone(), value_style),
    ]));
    if !props.fragment_path.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Unit File:      ", label_style),
            Span::styled(props.fragment_path.clone(), value_style),
        ]));
    }
    if !props.unit_file_state.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Enabled State:  ", label_style),
            Span::styled(
                props.unit_file_state.clone(),
                Style::default().fg(file_state_color(&props.unit_file_state)),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("  Active State:   ", label_style),
        Span::styled(
            format!("{} ({})", props.active_state, props.sub_state),
            value_style,
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Load State:     ", label_style),
        Span::styled(props.load_state.clone(), value_style),
    ]));
    lines.push(Line::from(""));

    // Timer section (only for .timer units with data)
    if unit_name.ends_with(".timer") {
        let has_timer_data = !props.timers_calendar.is_empty()
            || !props.timers_monotonic.is_empty()
            || props.next_elapse_realtime.is_some()
            || (!props.last_trigger_usec.is_empty() && props.last_trigger_usec != "n/a");

        if has_timer_data {
            lines.push(Line::from(vec![Span::styled("Timer", section_style)]));
            for spec in &props.timers_calendar {
                lines.push(Line::from(vec![
                    Span::styled("  Schedule:       ", label_style),
                    Span::styled(spec.clone(), value_style),
                ]));
            }
            for spec in &props.timers_monotonic {
                lines.push(Line::from(vec![
                    Span::styled("  Schedule:       ", label_style),
                    Span::styled(spec.clone(), value_style),
                ]));
            }
            if let Some(next_usec) = props.next_elapse_realtime {
                lines.push(Line::from(vec![
                    Span::styled("  Next Trigger:   ", label_style),
                    Span::styled(format_relative_time(next_usec), value_style),
                ]));
            }
            if !props.last_trigger_usec.is_empty() && props.last_trigger_usec != "n/a" {
                lines.push(Line::from(vec![
                    Span::styled("  Last Trigger:   ", label_style),
                    Span::styled(props.last_trigger_usec.clone(), value_style),
                ]));
            }
            if !props.result.is_empty() {
                let result_color = if props.result == "success" {
                    Color::Green
                } else {
                    Color::Red
                };
                lines.push(Line::from(vec![
                    Span::styled("  Result:         ", label_style),
                    Span::styled(props.result.clone(), Style::default().fg(result_color)),
                ]));
            }
            if props.persistent == "yes" {
                lines.push(Line::from(vec![
                    Span::styled("  Persistent:     ", label_style),
                    Span::styled(props.persistent.clone(), value_style),
                ]));
            }
            if !props.accuracy_usec.is_empty() && props.accuracy_usec != "0" {
                lines.push(Line::from(vec![
                    Span::styled("  Accuracy:       ", label_style),
                    Span::styled(props.accuracy_usec.clone(), value_style),
                ]));
            }
            if !props.randomized_delay_usec.is_empty() && props.randomized_delay_usec != "0" {
                lines.push(Line::from(vec![
                    Span::styled("  Random Delay:   ", label_style),
                    Span::styled(props.randomized_delay_usec.clone(), value_style),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    // Process section (only if PID > 0)
    if props.main_pid > 0 {
        lines.push(Line::from(vec![Span::styled("Process", section_style)]));
        lines.push(Line::from(vec![
            Span::styled("  Main PID:       ", label_style),
            Span::styled(props.main_pid.to_string(), value_style),
        ]));
        if !props.exec_main_start_timestamp.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Started:        ", label_style),
                Span::styled(props.exec_main_start_timestamp.clone(), value_style),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Resources section (only if any data)
    if props.memory_current.is_some() || props.cpu_usage_nsec.is_some() {
        lines.push(Line::from(vec![Span::styled("Resources", section_style)]));
        if let Some(mem) = props.memory_current {
            lines.push(Line::from(vec![
                Span::styled("  Memory:         ", label_style),
                Span::styled(format_bytes(mem), value_style),
            ]));
        }
        if let Some(cpu) = props.cpu_usage_nsec {
            lines.push(Line::from(vec![
                Span::styled("  CPU Time:       ", label_style),
                Span::styled(format_cpu_time(cpu), value_style),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Dependencies section
    let dep_sections: Vec<(&str, &Vec<String>)> = vec![
        ("Requires", &props.requires),
        ("Wants", &props.wants),
        ("After", &props.after),
        ("Before", &props.before),
        ("Conflicts", &props.conflicts),
        ("TriggeredBy", &props.triggered_by),
        ("Triggers", &props.triggers),
    ];

    let has_deps = dep_sections.iter().any(|(_, deps)| !deps.is_empty());
    if has_deps {
        lines.push(Line::from(vec![Span::styled(
            "Dependencies",
            section_style,
        )]));
        for (label, deps) in &dep_sections {
            if deps.is_empty() {
                continue;
            }
            render_dep_lines(&mut lines, label, deps, label_style, value_style);
        }
    }

    // Store content height for scroll bounds
    app.detail_content_height = lines.len();

    let area = centered_rect(70, 80, frame.area());
    let visible_height = area.height.saturating_sub(2) as usize;

    let scroll_info = if lines.len() > visible_height {
        let start = app.detail_scroll + 1;
        let end = (app.detail_scroll + visible_height).min(lines.len());
        format!(" [{}-{}/{}]", start, end, lines.len())
    } else {
        String::new()
    };

    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(app.detail_scroll)
        .take(visible_height)
        .collect();

    let title = format!(" {} {}", unit_name, scroll_info);

    let paragraph = Paragraph::new(visible_lines)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().bg(Color::Black)),
        );

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn render_dep_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    label: &str,
    deps: &[String],
    label_style: Style,
    value_style: Style,
) {
    let joined = deps.join(", ");
    if joined.len() <= 50 {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:16}", format!("{}:", label)), label_style),
            Span::styled(joined, value_style),
        ]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            format!("  {}:", label),
            label_style,
        )]));
        for dep in deps {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(dep.clone(), value_style),
            ]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Phase 4 — file_state_color

    #[test]
    fn test_file_state_color_enabled() {
        assert_eq!(file_state_color("enabled"), Color::Green);
    }

    #[test]
    fn test_file_state_color_disabled() {
        assert_eq!(file_state_color("disabled"), Color::Yellow);
    }

    #[test]
    fn test_file_state_color_static() {
        assert_eq!(file_state_color("static"), Color::DarkGray);
    }

    #[test]
    fn test_file_state_color_masked() {
        assert_eq!(file_state_color("masked"), Color::Red);
    }

    #[test]
    fn test_file_state_color_indirect() {
        assert_eq!(file_state_color("indirect"), Color::Cyan);
    }

    #[test]
    fn test_file_state_color_unknown() {
        assert_eq!(file_state_color("something"), Color::White);
    }

    // Phase 3 — priority_color

    #[test]
    fn test_priority_color_0() {
        assert_eq!(priority_color(0), (Color::Red, true));
    }

    #[test]
    fn test_priority_color_1() {
        assert_eq!(priority_color(1), (Color::Red, true));
    }

    #[test]
    fn test_priority_color_2() {
        assert_eq!(priority_color(2), (Color::Red, true));
    }

    #[test]
    fn test_priority_color_3() {
        assert_eq!(priority_color(3), (Color::Red, false));
    }

    #[test]
    fn test_priority_color_4() {
        assert_eq!(priority_color(4), (Color::Yellow, false));
    }

    #[test]
    fn test_priority_color_5() {
        assert_eq!(priority_color(5), (Color::Cyan, false));
    }

    #[test]
    fn test_priority_color_6() {
        assert_eq!(priority_color(6), (Color::White, false));
    }

    #[test]
    fn test_priority_color_7() {
        assert_eq!(priority_color(7), (Color::DarkGray, false));
    }

    #[test]
    fn test_priority_color_8() {
        assert_eq!(priority_color(8), (Color::White, false));
    }

    #[test]
    fn test_priority_color_255() {
        assert_eq!(priority_color(255), (Color::White, false));
    }

    // Layout geometry — centered_fixed_rect

    #[test]
    fn test_centered_fixed_rect_centered() {
        let area = Rect::new(0, 0, 100, 50);
        let result = centered_fixed_rect(30, 10, area);
        assert_eq!(result.x, 35);
        assert_eq!(result.y, 20);
        assert_eq!(result.width, 30);
        assert_eq!(result.height, 10);
    }

    #[test]
    fn test_centered_fixed_rect_larger_than_area() {
        let area = Rect::new(0, 0, 20, 10);
        let result = centered_fixed_rect(30, 15, area);
        assert_eq!(result.width, 20);
        assert_eq!(result.height, 10);
    }

    #[test]
    fn test_centered_fixed_rect_with_offset() {
        let area = Rect::new(10, 5, 100, 50);
        let result = centered_fixed_rect(30, 10, area);
        assert_eq!(result.x, 10 + (100 - 30) / 2);
        assert_eq!(result.y, 5 + (50 - 10) / 2);
        assert_eq!(result.width, 30);
        assert_eq!(result.height, 10);
    }

    #[test]
    fn test_centered_fixed_rect_exact_fit() {
        let area = Rect::new(0, 0, 30, 10);
        let result = centered_fixed_rect(30, 10, area);
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 0);
        assert_eq!(result.width, 30);
        assert_eq!(result.height, 10);
    }

    // Layout geometry — centered_rect

    #[test]
    fn test_centered_rect_50_50() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(50, 50, area);
        // The centered rect should be roughly 50% of the area
        assert!(result.width > 0);
        assert!(result.height > 0);
        assert!(result.width <= 100);
        assert!(result.height <= 100);
        // Should be approximately centered
        assert!(result.x > 0);
        assert!(result.y > 0);
    }

    // Layout geometry — get_layout_regions

    #[test]
    fn test_layout_regions_no_logs() {
        let area = Rect::new(0, 0, 100, 50);
        let regions = get_layout_regions(area, false);
        // Services list should take full width
        assert_eq!(regions.services_list.width, 100);
        assert!(regions.logs_panel.is_none());
    }

    #[test]
    fn test_layout_regions_with_logs() {
        let area = Rect::new(0, 0, 100, 50);
        let regions = get_layout_regions(area, true);
        // Services takes ~40%, logs ~60%
        assert!(regions.services_list.width < 100);
        assert!(regions.logs_panel.is_some());
        let logs = regions.logs_panel.unwrap();
        assert!(logs.width > 0);
        assert_eq!(
            regions.services_list.width + logs.width,
            100
        );
    }

    #[test]
    fn test_layout_regions_vertical_structure() {
        let area = Rect::new(0, 0, 100, 50);
        let regions = get_layout_regions(area, false);
        // Header is 3 rows, footer is 3 rows, middle is the rest
        // Services list should start after header (y=3) and end before footer
        assert_eq!(regions.services_list.y, 3);
        assert_eq!(regions.services_list.height, 50 - 3 - 3);
    }
}
