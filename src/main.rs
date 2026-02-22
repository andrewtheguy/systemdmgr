mod app;
mod service;
mod ui;

use std::io::{self, stdout};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, Terminal};

use app::App;

fn main() -> io::Result<()> {
    // Setup terminal with mouse capture
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
            // Help can be toggled from anywhere (except modals)
            if key.code == KeyCode::Char('?')
                && !app.show_status_picker && !app.show_type_picker
                && !app.show_priority_picker && !app.show_time_picker
                && !app.show_details && !app.show_file_state_picker
            {
                app.toggle_help();
                continue;
            }

            // Close help with Esc or any key if help is shown
            if app.show_help {
                app.show_help = false;
                continue;
            }

            // Status picker modal
            if app.show_status_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('s') => app.close_status_picker(),
                    KeyCode::Down | KeyCode::Char('j') => app.status_picker_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.status_picker_previous(),
                    KeyCode::Enter => app.status_picker_confirm(),
                    _ => {}
                }
                continue;
            }

            // Type picker modal
            if app.show_type_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('t') => app.close_type_picker(),
                    KeyCode::Down | KeyCode::Char('j') => app.type_picker_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.type_picker_previous(),
                    KeyCode::Enter => app.type_picker_confirm(),
                    _ => {}
                }
                continue;
            }

            // Priority picker modal
            if app.show_priority_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('p') => app.close_priority_picker(),
                    KeyCode::Down | KeyCode::Char('j') => app.priority_picker_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.priority_picker_previous(),
                    KeyCode::Enter => app.priority_picker_confirm(),
                    _ => {}
                }
                continue;
            }

            // Time range picker modal
            if app.show_time_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('T') => app.close_time_picker(),
                    KeyCode::Down | KeyCode::Char('j') => app.time_picker_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.time_picker_previous(),
                    KeyCode::Enter => app.time_picker_confirm(),
                    _ => {}
                }
                continue;
            }

            // File state picker modal
            if app.show_file_state_picker {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('f') => app.close_file_state_picker(),
                    KeyCode::Down | KeyCode::Char('j') => app.file_state_picker_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.file_state_picker_previous(),
                    KeyCode::Enter => app.file_state_picker_confirm(),
                    _ => {}
                }
                continue;
            }

            // Details modal
            if app.show_details {
                let visible = ui::get_details_visible_lines(&terminal.get_frame());
                let content_height = app.detail_content_height;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') | KeyCode::Enter => app.close_details(),
                    KeyCode::Down | KeyCode::Char('j') => app.detail_scroll_down(1, content_height, visible),
                    KeyCode::Up | KeyCode::Char('k') => app.detail_scroll_up(1),
                    KeyCode::Char('g') | KeyCode::Home => { app.detail_scroll = 0; }
                    KeyCode::Char('G') | KeyCode::End => app.detail_scroll_down(usize::MAX, content_height, visible),
                    KeyCode::PageDown => app.detail_scroll_down(10, content_height, visible),
                    KeyCode::PageUp => app.detail_scroll_up(10),
                    _ => {}
                }
                continue;
            }

            // Calculate visible lines for scrolling
            let visible_lines = ui::get_logs_visible_lines(&terminal.get_frame(), app.show_logs);
            let visible_services = ui::get_services_visible_lines(&terminal.get_frame(), app.show_logs);

            if app.search_mode {
                // Branch 1: Service search mode (only reachable when show_logs=false)
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        app.search_mode = false;
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                        app.update_filter();
                    }
                    KeyCode::Down => {
                        app.next();
                    }
                    KeyCode::Up => {
                        app.previous();
                    }
                    KeyCode::PageUp => {
                        app.page_up(visible_services);
                    }
                    KeyCode::PageDown => {
                        app.page_down(visible_services);
                    }
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.update_filter();
                    }
                    _ => {}
                }
            } else if app.log_search_mode {
                // Branch 2: Log search typing mode
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        app.log_search_mode = false;
                    }
                    KeyCode::Backspace => {
                        app.log_search_query.pop();
                        app.update_log_search();
                    }
                    KeyCode::PageUp => {
                        app.scroll_logs_up(visible_lines);
                    }
                    KeyCode::PageDown => {
                        app.scroll_logs_down(visible_lines, visible_lines);
                    }
                    KeyCode::Char(c) => {
                        app.log_search_query.push(c);
                        app.update_log_search();
                    }
                    _ => {}
                }
            } else if app.show_logs {
                // Branch 3: Log focus normal mode
                match key.code {
                    KeyCode::Char('q') => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('l') => {
                        app.clear_log_search();
                        app.toggle_logs();
                    }
                    KeyCode::Esc => {
                        if !app.log_search_query.is_empty() {
                            app.clear_log_search();
                        } else {
                            app.show_logs = false;
                        }
                    }
                    KeyCode::Char('/') => {
                        app.log_search_mode = true;
                    }
                    KeyCode::Char('n') => {
                        app.next_log_match(visible_lines);
                    }
                    KeyCode::Char('N') => {
                        app.prev_log_match(visible_lines);
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.scroll_logs_down(1, visible_lines);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.scroll_logs_up(1);
                    }
                    KeyCode::Char('g') | KeyCode::Home => {
                        app.logs_go_to_top();
                    }
                    KeyCode::Char('G') | KeyCode::End => {
                        app.logs_go_to_bottom(visible_lines);
                    }
                    KeyCode::PageUp => {
                        app.scroll_logs_up(visible_lines);
                    }
                    KeyCode::PageDown => {
                        app.scroll_logs_down(visible_lines, visible_lines);
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.scroll_logs_up(visible_lines / 2);
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.scroll_logs_down(visible_lines / 2, visible_lines);
                    }
                    KeyCode::Char('u') => {
                        app.toggle_user_mode();
                    }
                    KeyCode::Char('t') => {
                        app.open_type_picker();
                    }
                    KeyCode::Char('p') => {
                        app.open_priority_picker();
                    }
                    KeyCode::Char('T') => {
                        app.open_time_picker();
                    }
                    KeyCode::Char('i') | KeyCode::Enter => {
                        app.open_details();
                    }
                    KeyCode::Char('f') => {
                        app.open_file_state_picker();
                    }
                    _ => {}
                }
            } else {
                // Branch 4: Service normal mode
                match key.code {
                    KeyCode::Char('q') => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('l') => {
                        app.toggle_logs();
                    }
                    KeyCode::Esc => {
                        if !app.search_query.is_empty() {
                            app.clear_search();
                        }
                    }
                    KeyCode::Char('/') => {
                        app.search_mode = true;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.next();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.previous();
                    }
                    KeyCode::Char('g') | KeyCode::Home => {
                        app.go_to_top();
                    }
                    KeyCode::Char('G') | KeyCode::End => {
                        app.go_to_bottom();
                    }
                    KeyCode::Char('r') => {
                        app.load_services();
                    }
                    KeyCode::Char('u') => {
                        app.toggle_user_mode();
                    }
                    KeyCode::Char('s') => {
                        app.open_status_picker();
                    }
                    KeyCode::Char('t') => {
                        app.open_type_picker();
                    }
                    KeyCode::Char('p') => {
                        app.open_priority_picker();
                    }
                    KeyCode::Char('T') => {
                        app.open_time_picker();
                    }
                    KeyCode::Char('i') | KeyCode::Enter => {
                        app.open_details();
                    }
                    KeyCode::Char('f') => {
                        app.open_file_state_picker();
                    }
                    KeyCode::PageUp => {
                        app.page_up(visible_services);
                    }
                    KeyCode::PageDown => {
                        app.page_down(visible_services);
                    }
                    _ => {}
                }
            }
            }
            Event::Mouse(mouse) => {
                let size = terminal.size()?;
                let frame_rect = Rect::new(0, 0, size.width, size.height);
                handle_mouse_event(&mut app, mouse, frame_rect);
            }
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn handle_mouse_event(app: &mut App, mouse: MouseEvent, frame_size: Rect) {
    // Don't handle mouse events when help or modal is shown
    if app.show_help || app.show_status_picker || app.show_type_picker
        || app.show_priority_picker || app.show_time_picker
        || app.show_details || app.show_file_state_picker
    {
        return;
    }

    let regions = ui::get_layout_regions(frame_size, app.show_logs);

    if app.show_logs {
        // Log mode: all scroll events go to logs, clicks are ignored
        if let Some(logs) = regions.logs_panel {
            let visible = logs.height.saturating_sub(2) as usize;
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    app.scroll_logs_up(3);
                }
                MouseEventKind::ScrollDown => {
                    app.scroll_logs_down(3, visible);
                }
                _ => {}
            }
        }
    } else {
        // Service mode: existing behavior
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse_in_rect(mouse, regions.services_list) {
                    let y_in_list = mouse.row.saturating_sub(regions.services_list.y + 1);
                    let clicked_index = app.list_state.offset() + y_in_list as usize;
                    if clicked_index < app.filtered_indices.len() {
                        app.list_state.select(Some(clicked_index));
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if mouse_in_rect(mouse, regions.services_list) {
                    app.previous();
                }
            }
            MouseEventKind::ScrollDown => {
                if mouse_in_rect(mouse, regions.services_list) {
                    app.next();
                }
            }
            _ => {}
        }
    }
}

fn mouse_in_rect(mouse: MouseEvent, rect: Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x + rect.width
        && mouse.row >= rect.y
        && mouse.row < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton};

    fn make_mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn test_mouse_in_rect_inside() {
        let rect = Rect::new(10, 10, 20, 15);
        assert!(mouse_in_rect(make_mouse(15, 15), rect));
    }

    #[test]
    fn test_mouse_in_rect_top_left_corner() {
        let rect = Rect::new(10, 10, 20, 15);
        assert!(mouse_in_rect(make_mouse(10, 10), rect));
    }

    #[test]
    fn test_mouse_in_rect_bottom_right_exclusive() {
        let rect = Rect::new(10, 10, 20, 15);
        // x=30, y=25 is outside (exclusive boundary)
        assert!(!mouse_in_rect(make_mouse(30, 25), rect));
    }

    #[test]
    fn test_mouse_in_rect_just_inside_bottom_right() {
        let rect = Rect::new(10, 10, 20, 15);
        // x=29, y=24 is the last inside position
        assert!(mouse_in_rect(make_mouse(29, 24), rect));
    }

    #[test]
    fn test_mouse_in_rect_outside_left() {
        let rect = Rect::new(10, 10, 20, 15);
        assert!(!mouse_in_rect(make_mouse(9, 15), rect));
    }

    #[test]
    fn test_mouse_in_rect_outside_above() {
        let rect = Rect::new(10, 10, 20, 15);
        assert!(!mouse_in_rect(make_mouse(15, 9), rect));
    }

    #[test]
    fn test_mouse_in_rect_zero_rect() {
        let rect = Rect::new(5, 5, 0, 0);
        assert!(!mouse_in_rect(make_mouse(5, 5), rect));
    }
}
