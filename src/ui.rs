use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::service::UNIT_TYPES;

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
    } else if !app.search_query.is_empty() || app.status_filter.is_some() {
        let mut info_parts = Vec::new();
        if !app.search_query.is_empty() {
            info_parts.push(format!("Search: {}", app.search_query));
        }
        if let Some(ref status) = app.status_filter {
            info_parts.push(format!("Status: {}", status));
        }
        let info = format!("{} ({} matches)", info_parts.join(" | "), app.filtered_indices.len());
        Paragraph::new(info)
            .style(Style::default().fg(Color::Green))
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
                ListItem::new(Line::from(spans))
            })
            .collect();

        let type_label = app.unit_type.label();
        let title = if app.search_query.is_empty() && app.status_filter.is_none() {
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
        let logs_title = if let Some(ref service_name) = app.last_selected_service {
            format!("Logs: {}", service_name)
        } else {
            "Logs".to_string()
        };

        let focused_suffix = " [FOCUSED]";

        // Calculate visible area (subtract 2 for borders)
        let visible_lines = logs_area.height.saturating_sub(2) as usize;

        // Create log content with scroll and search highlighting
        let log_lines: Vec<Line> = app
            .logs
            .iter()
            .enumerate()
            .skip(app.logs_scroll)
            .take(visible_lines)
            .map(|(line_idx, line)| {
                highlight_search_in_line(line, line_idx, app)
            })
            .collect();

        let scroll_info = if !app.logs.is_empty() {
            format!(
                " [{}-{}/{}]",
                app.logs_scroll + 1,
                (app.logs_scroll + visible_lines).min(app.logs.len()),
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
    let footer_text = if app.log_search_mode {
        "Type to search logs | Esc/Enter: Exit search | ?: Help"
    } else if app.show_logs && !app.log_search_query.is_empty() {
        "l: Exit logs | j/k: Scroll | n/N: Next/Prev match | t: Type | u: User/System | Esc: Clear | ?: Help"
    } else if app.show_logs {
        "l: Exit logs | j/k: Scroll | g/G: Top/Bottom | /: Search logs | t: Type | u: User/System | ?: Help"
    } else if app.search_mode {
        "Type to search | Esc/Enter: Exit search | ?: Help"
    } else if !app.search_query.is_empty() || app.status_filter.is_some() {
        "q: Quit | /: Search | s: Status | t: Type | l: Logs | u: User/System | Esc: Clear | ?: Help"
    } else {
        "q/Esc: Quit | /: Search | s: Status | t: Type | l: Logs | u: User/System | ?: Help"
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

    // Help overlay
    if app.show_help {
        render_help(frame, app);
    }
}

fn highlight_search_in_line<'a>(line: &str, line_idx: usize, app: &App) -> Line<'a> {
    if app.log_search_query.is_empty() {
        return Line::from(line.to_string());
    }

    let query_lower = app.log_search_query.to_lowercase();
    let line_lower = line.to_lowercase();

    if !line_lower.contains(&query_lower) {
        return Line::from(line.to_string());
    }

    // Determine if this line is the current match
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

    while pos < line.len() {
        if let Some(match_start) = line_lower[pos..].find(&query_lower) {
            let abs_start = pos + match_start;
            if abs_start > pos {
                spans.push(Span::raw(line[pos..abs_start].to_string()));
            }
            spans.push(Span::styled(
                line[abs_start..abs_start + query_len].to_string(),
                highlight_style,
            ));
            pos = abs_start + query_len;
        } else {
            spans.push(Span::raw(line[pos..].to_string()));
            break;
        }
    }

    Line::from(spans)
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
        Line::from("  t             Open unit type picker"),
        Line::from("  Esc           Clear search/filter"),
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
