mod app;
mod service;
mod ui;

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use app::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();

    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            // Help can be toggled from anywhere
            if key.code == KeyCode::Char('?') {
                app.toggle_help();
                continue;
            }

            // Close help with Esc or any key if help is shown
            if app.show_help {
                app.show_help = false;
                continue;
            }

            // Calculate visible lines for log scrolling
            let visible_lines = ui::get_logs_visible_lines(&terminal.get_frame(), app.show_logs);

            if app.search_mode {
                // Handle search mode input
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
                        app.scroll_logs_up(visible_lines);
                    }
                    KeyCode::PageDown => {
                        app.scroll_logs_down(visible_lines, visible_lines);
                    }
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.update_filter();
                    }
                    _ => {}
                }
            } else {
                // Normal mode input
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
                        } else {
                            app.should_quit = true;
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
                    KeyCode::Char('s') => {
                        app.cycle_status_filter();
                    }
                    KeyCode::Char('S') => {
                        app.clear_status_filter();
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
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}
