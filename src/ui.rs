use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::service::{
    format_bytes, format_cpu_time, format_log_timestamp, priority_label, COLOR_MUTED,
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

    if show_logs {
        LayoutRegions {
            services_list: chunks[1],
            logs_panel: Some(chunks[1]),
        }
    } else {
        // Split off the 1-row column header so services_list points to the list body
        let service_chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(chunks[1]);
        LayoutRegions {
            services_list: service_chunks[1],
            logs_panel: None,
        }
    }
}

pub fn render(frame: &mut Frame, app: &mut App, live_indicator_on: bool) {
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

    // When logs are shown, give full middle area to logs; hide services list
    let (services_area, logs_area) = if app.show_logs {
        (None, Some(chunks[1]))
    } else {
        (Some(chunks[1]), None)
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
        let title = format!("SystemD {} [{}]", app.unit_type.label(), scope_label);
        let refreshed = app
            .last_refreshed
            .map(|t| format!("  (loaded {})", t.format("%b %d %H:%M:%S %Z")))
            .unwrap_or_default();
        Paragraph::new(format!("{}{}", title, refreshed))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL))
    };
    frame.render_widget(header, chunks[0]);

    // Services list (hidden when logs are full-screen)
    if let Some(services_area) = services_area {
        // Split into column header row + list body
        let service_chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(services_area);
        let header_area = service_chunks[0];
        let list_area = service_chunks[1];

        // Name column: dynamic width capped at 35 chars, +2 for padding
        const NAME_MAX: usize = 35;
        let name_width = app
            .filtered_indices
            .iter()
            .map(|&i| app.services[i].unit.len().min(NAME_MAX))
            .max()
            .unwrap_or(4)
            .max(4)
            + 2;

        // Column header
        let header_line = Line::from(Span::styled(
            format!(
                " {:<nw$}{:<10}{:<16}{:<10}{}",
                "NAME", "STATUS", "ENABLED", "LOAD", "DESCRIPTION",
                nw = name_width,
            ),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(Paragraph::new(header_line), header_area);

        if let Some(ref error) = app.error {
            let error_msg = Paragraph::new(error.as_str())
                .style(Style::default().fg(Color::Red))
                .block(Block::default().borders(Borders::ALL).title("Error"));
            frame.render_widget(error_msg, list_area);
        } else {
            let items: Vec<ListItem> = app
                .filtered_indices
                .iter()
                .map(|&i| &app.services[i])
                .map(|unit| {
                    let status_color = unit.status_color();
                    let file_state_str = unit.file_state.as_deref().unwrap_or("");
                    let mut desc = unit.description.clone();
                    if let Some(ref detail) = unit.detail {
                        desc.push_str(&format!(" ({})", detail));
                    }
                    let display_name = if unit.unit.len() > NAME_MAX {
                        format!("{}...", &unit.unit[..NAME_MAX - 3])
                    } else {
                        unit.unit.clone()
                    };
                    let spans = vec![
                        Span::styled(
                            format!("{:<nw$}", display_name, nw = name_width),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(
                            format!("{:<10}", unit.status_display()),
                            Style::default().fg(status_color),
                        ),
                        Span::styled(
                            format!("{:<16}", file_state_str),
                            Style::default().fg(file_state_color(file_state_str)),
                        ),
                        Span::styled(
                            format!("{:<10}", unit.load),
                            Style::default().fg(load_color(&unit.load)),
                        ),
                        Span::styled(desc, Style::default().fg(Color::Gray)),
                    ];
                    ListItem::new(Line::from(spans))
                })
                .collect();

            let type_label = app.unit_type.label();
            let title = if app.search_query.is_empty()
                && app.status_filter.is_none()
                && app.file_state_filter.is_none()
            {
                format!("{} ({})", type_label, app.services.len())
            } else {
                format!(
                    "{} ({}/{})",
                    type_label,
                    app.filtered_indices.len(),
                    app.services.len()
                )
            };

            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Rgb(40, 40, 80))
                        .add_modifier(Modifier::BOLD),
                );

            frame.render_stateful_widget(list, list_area, &mut app.list_state);
        }
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
        let content_width = logs_area.width.saturating_sub(2) as usize;

        // Resolve "go to bottom" sentinel against wrapped visual lines.
        let entry_heights = log_entry_visual_heights(app, content_width);
        let bottom_scroll = bottom_scroll_index(&entry_heights, visible_lines);
        if app.logs_scroll == usize::MAX {
            app.logs_scroll = bottom_scroll;
        } else if app.logs.is_empty() {
            app.logs_scroll = 0;
        } else {
            // Prevent overscrolling into trailing blank space.
            app.logs_scroll = app.logs_scroll.min(bottom_scroll);
        }

        // Track the last seen invocation ID to detect service restarts across None gaps
        let mut last_invocation_id: Option<&str> = app
            .logs
            .iter()
            .take(app.logs_scroll)
            .rev()
            .find_map(|e| e.invocation_id.as_deref());

        // Create log content with scroll, search highlighting, and boot separators
        let mut log_lines: Vec<Line> = Vec::new();
        let mut entries_shown = 0;
        for (entry_idx, entry) in app.logs.iter().enumerate().skip(app.logs_scroll) {
            if log_lines.len() >= visible_lines {
                break;
            }
            if entry_idx > 0 {
                let prev = &app.logs[entry_idx - 1];
                let (boot_changed, invocation_changed) =
                    log_boundary_before_entry(prev, entry, last_invocation_id);

                // Boot boundary separator
                if boot_changed {
                    let short_id = entry.boot_id.as_ref().map(|id| &id[..id.len().min(12)]).unwrap_or("?");
                    let boot_ts = entry
                        .timestamp
                        .map(|ts| format!(" · {}", format_log_timestamp(ts)))
                        .unwrap_or_default();
                    let label = format!(" Boot {} {} ", short_id, boot_ts);
                    let pad_total = content_width.saturating_sub(label.width());
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

                // Service restart boundary separator
                if invocation_changed {
                    let restart_ts = entry
                        .timestamp
                        .map(|ts| format!(" · {}", format_log_timestamp(ts)))
                        .unwrap_or_default();
                    let label = format!(" Restarted {} ", restart_ts);
                    let pad_total = content_width.saturating_sub(label.width());
                    let pad_left = pad_total / 2;
                    let pad_right = pad_total - pad_left;
                    let separator = format!(
                        "{}{}{}",
                        "─".repeat(pad_left),
                        label,
                        "─".repeat(pad_right),
                    );
                    let style = Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD);
                    log_lines.push(Line::from(Span::styled(separator, style)));
                    if log_lines.len() >= visible_lines {
                        break;
                    }
                }
            }
            if let Some(id) = entry.invocation_id.as_deref() {
                last_invocation_id = Some(id);
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

        let mut title_spans = vec![Span::raw(logs_title)];
        if app.live_tail {
            let pulse_on = live_indicator_on;
            let live_style = if pulse_on {
                Style::default().fg(Color::LightGreen)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            title_spans.push(Span::raw(" "));
            title_spans.push(Span::styled("[LIVE]".to_string(), live_style));
        }
        title_spans.push(Span::raw(focused_suffix));
        title_spans.push(Span::raw(scroll_info.clone()));

        let border_style = Style::default().fg(Color::Yellow);

        let logs_paragraph = Paragraph::new(log_lines)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from(title_spans))
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
        "j/k: Navigate | Enter/shortcut: Select | Esc/x: Close"
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
        "Type to search logs | Esc/Enter: Exit search | ?: Help & more"
    } else if app.show_logs && !app.log_search_query.is_empty() {
        "Esc: Back | j/k: Scroll | n/N: Next/Prev match | x: Actions | f: Follow | p: Priority | t: Time | /: Search | ?: Help & more"
    } else if app.show_logs {
        "Esc: Back | j/k: Scroll | g/G: Top/Bottom | x: Actions | f: Follow | /: Search | p: Priority | t: Time | ?: Help & more"
    } else if app.search_mode {
        "Type to search | Esc/Enter: Exit search | ?: Help & more"
    } else if !app.search_query.is_empty() || app.status_filter.is_some() || app.file_state_filter.is_some() {
        "q: Quit | /: Search | s: Status | f: File state | x: Actions | i: Details | t: Type | l: Logs | r: Refresh | u: User/System | Esc: Clear | ?: Help & more"
    } else {
        "q/Esc: Quit | /: Search | s: Status | f: File state | x: Actions | i: Details | t: Type | l: Logs | r: Refresh | u: User/System | ?: Help & more"
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

fn log_boundary_before_entry(
    prev: &LogEntry,
    current: &LogEntry,
    last_invocation_id: Option<&str>,
) -> (bool, bool) {
    let boot_changed = matches!(
        (&prev.boot_id, &current.boot_id),
        (Some(a), Some(b)) if a != b
    );
    let invocation_changed = !boot_changed
        && matches!(
            (last_invocation_id, current.invocation_id.as_deref()),
            (Some(a), Some(b)) if a != b
        );
    (boot_changed, invocation_changed)
}

fn wrapped_line_count(line: &Line<'_>, content_width: usize) -> usize {
    if content_width == 0 {
        return 1;
    }
    line.width().max(1).div_ceil(content_width)
}

fn log_entry_visual_heights(app: &App, content_width: usize) -> Vec<usize> {
    let mut heights = Vec::with_capacity(app.logs.len());
    let mut last_invocation_id: Option<&str> = None;

    for (entry_idx, entry) in app.logs.iter().enumerate() {
        let mut entry_lines = wrapped_line_count(&render_log_entry(entry, entry_idx, app), content_width);
        if entry_idx > 0 {
            let prev = &app.logs[entry_idx - 1];
            let (boot_changed, invocation_changed) =
                log_boundary_before_entry(prev, entry, last_invocation_id);
            if boot_changed || invocation_changed {
                entry_lines += 1;
            }
        }
        if let Some(id) = entry.invocation_id.as_deref() {
            last_invocation_id = Some(id);
        }
        heights.push(entry_lines);
    }

    heights
}

fn bottom_scroll_index(entry_heights: &[usize], visible_lines: usize) -> usize {
    if entry_heights.is_empty() || visible_lines == 0 {
        return 0;
    }

    let mut used = 0;
    for idx in (0..entry_heights.len()).rev() {
        let entry_lines = entry_heights[idx].max(1);
        if used + entry_lines > visible_lines {
            return if used == 0 { idx } else { idx + 1 };
        }
        used += entry_lines;
    }
    0
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

    let mut help_text: Vec<Line> = Vec::new();
    let title;

    if app.show_action_picker {
        title = "Help: Actions";
        help_text.extend(vec![
            Line::from(vec![Span::styled("Navigation", section_style)]),
            Line::from("  j / Down      Move down"),
            Line::from("  k / Up        Move up"),
            Line::from("  Enter         Select action"),
            Line::from("  s/o/r/l/e/d/D Shortcut keys"),
            Line::from(""),
            Line::from(vec![Span::styled("General", section_style)]),
            Line::from("  Esc / x       Close"),
            Line::from("  ?             Toggle this help"),
        ]);
    } else if app.show_details {
        title = "Help: Details";
        help_text.extend(vec![
            Line::from(vec![Span::styled("Navigation", section_style)]),
            Line::from("  j / Down      Scroll down"),
            Line::from("  k / Up        Scroll up"),
            Line::from("  g / Home      Go to top"),
            Line::from("  G / End       Go to bottom"),
            Line::from("  PgUp / PgDn   Page scroll"),
            Line::from(""),
            Line::from(vec![Span::styled("General", section_style)]),
            Line::from("  Esc / i       Close details"),
            Line::from("  Enter         Close details"),
            Line::from("  ?             Toggle this help"),
        ]);
    } else if app.show_logs {
        title = "Help: Logs";
        help_text.extend(vec![
            Line::from(vec![Span::styled("Navigation", section_style)]),
            Line::from("  j / Down      Scroll down"),
            Line::from("  k / Up        Scroll up"),
            Line::from("  g / Home      Go to top"),
            Line::from("  G / End       Go to bottom"),
            Line::from("  PgUp / PgDn   Page scroll"),
            Line::from("  Ctrl+u / d    Half page scroll"),
            Line::from(""),
            Line::from(vec![Span::styled("Search", section_style)]),
            Line::from("  /             Search logs"),
            Line::from("  n             Next match"),
            Line::from("  N             Previous match"),
            Line::from(""),
            Line::from(vec![Span::styled("Filters", section_style)]),
            Line::from("  p             Priority filter"),
            Line::from("  t             Time range filter"),
            Line::from(""),
            Line::from(vec![Span::styled("General", section_style)]),
            Line::from("  x             Action picker"),
            Line::from("  f             Toggle live tail (auto-refresh)"),
            Line::from("  l             Exit logs"),
            Line::from("  Esc           Clear search / Exit logs"),
            Line::from("  ?             Toggle this help"),
        ]);
    } else {
        title = "Help: Unit List";
        help_text.extend(vec![
            Line::from(vec![Span::styled("Navigation", section_style)]),
            Line::from("  j / Down      Move down"),
            Line::from("  k / Up        Move up"),
            Line::from("  g / Home      Go to top"),
            Line::from("  G / End       Go to bottom"),
            Line::from("  PgUp / PgDn   Page up/down"),
            Line::from(""),
            Line::from(vec![Span::styled("Search & Filter", section_style)]),
            Line::from("  /             Search units"),
            Line::from("  s             Status filter"),
            Line::from("  f             File state filter"),
            Line::from("  t             Unit type picker"),
            Line::from("  Esc           Clear search"),
            Line::from(""),
            Line::from(vec![Span::styled("Unit Operations", section_style)]),
            Line::from("  i / Enter     Open details"),
            Line::from("  x             Action picker"),
            Line::from("  R             Daemon reload"),
            Line::from("  l             Open logs"),
            Line::from(""),
            Line::from(vec![Span::styled("Mouse", section_style)]),
            Line::from("  Click         Select unit"),
            Line::from("  Scroll        Navigate list"),
            Line::from(""),
            Line::from(vec![Span::styled("General", section_style)]),
            Line::from("  r             Refresh units"),
            Line::from("  u             Toggle user/system"),
            Line::from("  ?             Toggle this help"),
            Line::from("  q             Quit"),
        ]);
    }

    let area = centered_rect(50, 70, frame.area());

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
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
                "dead" => COLOR_MUTED,
                "waiting" => Color::Cyan,
                "listening" => Color::Green,
                "active" => Color::Green,
                "inactive" => COLOR_MUTED,
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

    chunks[1].height.saturating_sub(2) as usize
}

/// Returns the number of visible lines in the services list
pub fn get_services_visible_lines(frame: &Frame, show_logs: bool) -> usize {
    if show_logs {
        return 0;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    chunks[1].height.saturating_sub(2) as usize
}

/// Returns the number of visible lines in the details modal
pub fn get_details_visible_lines(frame: &Frame) -> usize {
    let area = centered_rect(70, 80, frame.area());
    // Subtract 2 for borders
    area.height.saturating_sub(2) as usize
}

fn load_color(state: &str) -> Color {
    match state {
        "loaded" => Color::Green,
        "masked" => Color::Red,
        "not-found" => COLOR_MUTED,
        "error" | "bad-setting" => Color::Red,
        _ => Color::White,
    }
}

fn file_state_color(state: &str) -> Color {
    match state {
        "enabled" => Color::Green,
        "disabled" => Color::Yellow,
        "static" => COLOR_MUTED,
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
            let color = action_color(action);
            let shortcut = action.shortcut();
            let label = action.label();
            let line = Line::from(vec![
                Span::styled("  [", Style::default().fg(color)),
                Span::styled(
                    shortcut.to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("] {}", label), Style::default().fg(color)),
            ]);
            ListItem::new(line)
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

    // General section — fields matching main screen order first, then extras
    lines.push(Line::from(vec![Span::styled("General", section_style)]));
    lines.push(Line::from(vec![
        Span::styled("  Name:           ", label_style),
        Span::styled(unit_name.clone(), value_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Status:         ", label_style),
        Span::styled(props.sub_state.clone(), value_style),
    ]));
    if !props.unit_file_state.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Enabled:        ", label_style),
            Span::styled(
                props.unit_file_state.clone(),
                Style::default().fg(file_state_color(&props.unit_file_state)),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("  Load State:     ", label_style),
        Span::styled(
            props.load_state.clone(),
            Style::default().fg(load_color(&props.load_state)),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Description:    ", label_style),
        Span::styled(props.description.clone(), value_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Active State:   ", label_style),
        Span::styled(props.active_state.clone(), value_style),
    ]));
    if !props.active_enter_timestamp.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Active Since:   ", label_style),
            Span::styled(props.active_enter_timestamp.clone(), value_style),
        ]));
    }
    if !props.fragment_path.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Unit File:      ", label_style),
            Span::styled(props.fragment_path.clone(), value_style),
        ]));
    }
    lines.push(Line::from(""));

    // Timer section (only for .timer units with data)
    if unit_name.ends_with(".timer") {
        let has_timer_data = !props.timers_calendar.is_empty()
            || !props.timers_monotonic.is_empty()
            || !props.next_elapse_realtime.is_empty()
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
            if !props.next_elapse_realtime.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  Next Trigger:   ", label_style),
                    Span::styled(props.next_elapse_realtime.clone(), value_style),
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

    // Socket section (only for .socket units with data)
    if unit_name.ends_with(".socket") && !props.listen.is_empty() {
        lines.push(Line::from(vec![Span::styled("Socket", section_style)]));
        lines.push(Line::from(vec![
            Span::styled("  Listen:         ", label_style),
            Span::styled(props.listen.clone(), value_style),
        ]));
        if props.accept == "yes" {
            lines.push(Line::from(vec![
                Span::styled("  Accept:         ", label_style),
                Span::styled("yes", value_style),
            ]));
            if !props.n_accepted.is_empty() && props.n_accepted != "0" {
                lines.push(Line::from(vec![
                    Span::styled("  Accepted:       ", label_style),
                    Span::styled(props.n_accepted.clone(), value_style),
                ]));
            }
            if !props.n_connections.is_empty() && props.n_connections != "0" {
                lines.push(Line::from(vec![
                    Span::styled("  Connected:      ", label_style),
                    Span::styled(props.n_connections.clone(), value_style),
                ]));
            }
        }
        if !props.triggers.is_empty() {
            for (i, trigger) in props.triggers.iter().enumerate() {
                let label = if i == 0 { "  Triggers:       " } else { "                  " };
                lines.push(Line::from(vec![
                    Span::styled(label, label_style),
                    Span::styled(trigger.clone(), value_style),
                ]));
            }
        }
        lines.push(Line::from(""));
    }

    // Path section (only for .path units)
    if unit_name.ends_with(".path") && (!props.paths.is_empty() || !props.triggers.is_empty()) {
        lines.push(Line::from(vec![Span::styled("Path", section_style)]));
        if !props.paths.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Watch:          ", label_style),
                Span::styled(props.paths.clone(), value_style),
            ]));
        }
        if !props.triggers.is_empty() {
            for (i, trigger) in props.triggers.iter().enumerate() {
                let label = if i == 0 { "  Triggers:       " } else { "                  " };
                lines.push(Line::from(vec![
                    Span::styled(label, label_style),
                    Span::styled(trigger.clone(), value_style),
                ]));
            }
        }
        lines.push(Line::from(""));
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

    let title_name = if unit_name.len() > 35 {
        format!("{}...", &unit_name[..32])
    } else {
        unit_name.clone()
    };
    let title = format!(" {} {}", title_name, scroll_info);

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
        assert_eq!(file_state_color("static"), COLOR_MUTED);
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

    #[test]
    fn test_bottom_scroll_index_basic_window() {
        let heights = vec![1, 1, 1, 1, 1];
        assert_eq!(bottom_scroll_index(&heights, 3), 2);
    }

    #[test]
    fn test_bottom_scroll_index_skips_oversized_prefix() {
        let heights = vec![3, 1, 1];
        assert_eq!(bottom_scroll_index(&heights, 2), 1);
    }

    #[test]
    fn test_bottom_scroll_index_single_oversized_entry() {
        let heights = vec![5];
        assert_eq!(bottom_scroll_index(&heights, 2), 0);
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
        // Logs take full middle area
        assert!(regions.logs_panel.is_some());
        let logs = regions.logs_panel.unwrap();
        assert_eq!(logs.width, 100);
        // services_list gets the same rect (unused when logs shown)
        assert_eq!(regions.services_list.width, 100);
    }

    #[test]
    fn test_layout_regions_vertical_structure() {
        let area = Rect::new(0, 0, 100, 50);
        let regions = get_layout_regions(area, false);
        // Header is 3 rows, footer is 3 rows, column header is 1 row, rest is list body
        // Services list should start after header + column header (y=4)
        assert_eq!(regions.services_list.y, 4);
        assert_eq!(regions.services_list.height, 50 - 3 - 3 - 1);
    }
}
