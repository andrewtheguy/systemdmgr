//! Interactive connection menu shown when systemdmgr is started without a
//! connection flag: pick the local machine, or an SSH destination (either an
//! alias from `~/.ssh/config` or a destination typed in full).

use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
    prelude::CrosstermBackend,
};

use crate::service::split_ssh_args;

/// What the user picked in the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Local,
    /// ssh CLI arguments in `[options] destination` form, as `--ssh` takes them.
    Ssh(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Local vs. SSH.
    Choice,
    /// Which SSH destination.
    Ssh,
}

pub const CHOICE_LOCAL: usize = 0;
pub const CHOICE_SSH: usize = 1;

pub struct Menu {
    pub step: Step,
    pub choice: usize,
    /// Free-form destination; also filters the `~/.ssh/config` host list.
    pub input: String,
    hosts: Vec<String>,
    /// Index into `filtered_hosts`; `None` means the typed destination is used.
    pub selected_host: Option<usize>,
    pub error: Option<String>,
}

impl Menu {
    pub fn new(hosts: Vec<String>, error: Option<String>) -> Self {
        Menu {
            step: Step::Choice,
            choice: CHOICE_LOCAL,
            input: String::new(),
            hosts,
            selected_host: None,
            error,
        }
    }

    /// Config hosts matching what has been typed so far.
    pub fn filtered_hosts(&self) -> Vec<&str> {
        let query = self.input.trim().to_lowercase();
        self.hosts
            .iter()
            .filter(|host| query.is_empty() || host.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    pub fn next(&mut self) {
        match self.step {
            Step::Choice => self.choice = CHOICE_SSH,
            Step::Ssh => {
                let count = self.filtered_hosts().len();
                if count == 0 {
                    return;
                }
                self.selected_host = Some(match self.selected_host {
                    None => 0,
                    Some(i) => (i + 1).min(count - 1),
                });
            }
        }
    }

    pub fn previous(&mut self) {
        match self.step {
            Step::Choice => self.choice = CHOICE_LOCAL,
            // Moving above the first host returns focus to the typed destination.
            Step::Ssh => {
                self.selected_host = match self.selected_host {
                    Some(0) | None => None,
                    Some(i) => Some(i - 1),
                }
            }
        }
    }

    pub fn type_char(&mut self, c: char) {
        self.input.push(c);
        self.selected_host = None;
        self.error = None;
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.selected_host = None;
        self.error = None;
    }

    /// Esc: leave the SSH step, or quit from the first step (returns `true`).
    pub fn back(&mut self) -> bool {
        match self.step {
            Step::Choice => true,
            Step::Ssh => {
                self.step = Step::Choice;
                self.error = None;
                false
            }
        }
    }

    /// Enter. `None` means the menu stays open — either it advanced a step or
    /// it set `error`.
    pub fn submit(&mut self) -> Option<Connection> {
        match self.step {
            Step::Choice => {
                if self.choice == CHOICE_LOCAL {
                    return Some(Connection::Local);
                }
                self.step = Step::Ssh;
                self.error = None;
                None
            }
            Step::Ssh => {
                if let Some(index) = self.selected_host {
                    let host = self.filtered_hosts().get(index).map(|h| h.to_string());
                    if let Some(host) = host {
                        return Some(Connection::Ssh(vec![host]));
                    }
                }
                let args: Vec<String> =
                    self.input.split_whitespace().map(String::from).collect();
                if args.is_empty() {
                    self.error =
                        Some("Type an SSH destination, or pick a host with ↑/↓.".to_string());
                    return None;
                }
                match split_ssh_args(&args) {
                    Ok(_) => Some(Connection::Ssh(args)),
                    Err(e) => {
                        self.error = Some(e);
                        None
                    }
                }
            }
        }
    }
}

/// Concrete `Host` aliases in an OpenSSH client config. Only the names are
/// read, so they can be offered as choices; ssh itself interprets the rest of
/// the file. Wildcard and negated patterns are skipped, as they name no host.
pub fn parse_ssh_config_hosts(content: &str) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line
            .split_once(char::is_whitespace)
            .filter(|(keyword, _)| keyword.eq_ignore_ascii_case("Host"))
            .map(|(_, rest)| rest)
        else {
            continue;
        };
        // Extra names on one Host line are alternative spellings of the same
        // entry; the first concrete one is enough to connect with.
        if let Some(name) = rest
            .split_whitespace()
            .find(|name| !name.contains(['*', '?', '!']))
            && !hosts.iter().any(|h| h == name)
        {
            hosts.push(name.to_string());
        }
    }
    hosts
}

fn ssh_config_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".ssh").join("config"))
}

/// Host aliases from `~/.ssh/config`. A missing or unreadable config just
/// means no suggestions; the destination can always be typed in full.
fn load_ssh_config_hosts() -> Vec<String> {
    let Some(path) = ssh_config_path() else {
        return Vec::new();
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => parse_ssh_config_hosts(&content),
        Err(_) => Vec::new(),
    }
}

/// Runs the menu on the alternate screen. `Ok(None)` means the user quit.
/// `error` is shown as a banner, so a failed connection attempt can send the
/// user straight back here with the reason in view.
pub fn choose_connection(error: Option<String>) -> io::Result<Option<Connection>> {
    let mut menu = Menu::new(load_ssh_config_hosts(), error);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = run_menu(&mut terminal, &mut menu);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_menu(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    menu: &mut Menu,
) -> io::Result<Option<Connection>> {
    loop {
        terminal.draw(|frame| render(frame, menu))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc => {
                if menu.back() {
                    return Ok(None);
                }
            }
            KeyCode::Enter => {
                if let Some(connection) = menu.submit() {
                    return Ok(Some(connection));
                }
            }
            KeyCode::Down => menu.next(),
            KeyCode::Up => menu.previous(),
            KeyCode::Backspace if menu.step == Step::Ssh => menu.backspace(),
            // On the first step the list is the only thing to drive, so the
            // usual TUI keys apply; on the SSH step every character is input.
            KeyCode::Char('q') if menu.step == Step::Choice => return Ok(None),
            KeyCode::Char('j') if menu.step == Step::Choice => menu.next(),
            KeyCode::Char('k') if menu.step == Step::Choice => menu.previous(),
            KeyCode::Char(c) if menu.step == Step::Ssh => menu.type_char(c),
            _ => {}
        }
    }
}

fn render(frame: &mut Frame, menu: &mut Menu) {
    let area = centered_rect(64, 18, frame.area());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" systemdmgr {} ", env!("CARGO_PKG_VERSION")))
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let error_height = if menu.error.is_some() { 4 } else { 0 };
    let [error_area, body] =
        Layout::vertical([Constraint::Length(error_height), Constraint::Min(0)]).areas(inner);

    if let Some(error) = &menu.error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true }),
            error_area,
        );
    }

    match menu.step {
        Step::Choice => render_choice(frame, menu, body),
        Step::Ssh => render_ssh(frame, menu, body),
    }
}

fn render_choice(frame: &mut Frame, menu: &Menu, area: Rect) {
    let [heading, list_area, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new("Where do you want to manage services?")
            .style(Style::default().add_modifier(Modifier::BOLD)),
        heading,
    );

    let entries = [
        ("Local", "this machine"),
        ("SSH", "a remote host over ssh"),
    ];
    let items: Vec<ListItem> = entries
        .iter()
        .map(|(label, detail)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{label:<6}"), Style::default().fg(Color::White)),
                Span::styled(*detail, Style::default().fg(crate::service::COLOR_MUTED)),
            ]))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(menu.choice));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        list_area,
        &mut state,
    );

    frame.render_widget(footer_hint("↑/↓ select · Enter confirm · q quit"), footer);
}

fn render_ssh(frame: &mut Frame, menu: &Menu, area: Rect) {
    let [input_area, hosts_title, list_area, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    let typing = menu.selected_host.is_none();
    // The project renders text cursors as a trailing underscore.
    let input_text = if typing {
        format!("{}_", menu.input)
    } else {
        menu.input.clone()
    };
    frame.render_widget(
        Paragraph::new(input_text)
            .style(Style::default().fg(if typing { Color::Yellow } else { crate::service::COLOR_MUTED }))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Destination  (user@host, or ssh options then destination)"),
            ),
        input_area,
    );

    let hosts = menu.filtered_hosts();
    let title = if hosts.is_empty() {
        "No matching hosts in ~/.ssh/config".to_string()
    } else {
        format!("Hosts from ~/.ssh/config ({})", hosts.len())
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(crate::service::COLOR_MUTED)),
        hosts_title,
    );

    let items: Vec<ListItem> = hosts.iter().map(|host| ListItem::new(*host)).collect();
    let mut state = ListState::default().with_selected(menu.selected_host);
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        list_area,
        &mut state,
    );

    frame.render_widget(
        footer_hint("↑/↓ pick host · Enter connect · Esc back"),
        footer,
    );
}

fn footer_hint(text: &str) -> Paragraph<'_> {
    Paragraph::new(text).style(Style::default().fg(crate::service::COLOR_MUTED))
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<String> {
        ["web1", "web2", "db"].iter().map(|s| s.to_string()).collect()
    }

    fn ssh_menu() -> Menu {
        let mut menu = Menu::new(hosts(), None);
        menu.step = Step::Ssh;
        menu
    }

    #[test]
    fn test_parse_ssh_config_hosts_basic() {
        let config = "Host web1\n  HostName 10.0.0.1\n\nHost db\n  User root\n";
        assert_eq!(parse_ssh_config_hosts(config), vec!["web1", "db"]);
    }

    #[test]
    fn test_parse_ssh_config_hosts_skips_wildcards_and_comments() {
        let config = "# comment\nHost *\n  User root\nHost !bad web1\nHost real\n";
        assert_eq!(parse_ssh_config_hosts(config), vec!["web1", "real"]);
    }

    #[test]
    fn test_parse_ssh_config_hosts_first_name_wins_and_dedupes() {
        let config = "Host web1 web1.example.com\nHost web1\n";
        assert_eq!(parse_ssh_config_hosts(config), vec!["web1"]);
    }

    #[test]
    fn test_parse_ssh_config_hosts_keyword_is_case_insensitive() {
        assert_eq!(parse_ssh_config_hosts("host web1\nHOST db\n"), vec!["web1", "db"]);
    }

    #[test]
    fn test_parse_ssh_config_hosts_ignores_other_keywords() {
        assert_eq!(parse_ssh_config_hosts("HostName web1\nHostKeyAlias x\n"), Vec::<String>::new());
    }

    #[test]
    fn test_choice_navigation_clamps() {
        let mut menu = Menu::new(hosts(), None);
        assert_eq!(menu.choice, CHOICE_LOCAL);
        menu.previous();
        assert_eq!(menu.choice, CHOICE_LOCAL);
        menu.next();
        menu.next();
        assert_eq!(menu.choice, CHOICE_SSH);
    }

    #[test]
    fn test_submit_local() {
        let mut menu = Menu::new(hosts(), None);
        assert_eq!(menu.submit(), Some(Connection::Local));
    }

    #[test]
    fn test_submit_ssh_choice_opens_ssh_step() {
        let mut menu = Menu::new(hosts(), None);
        menu.next();
        assert_eq!(menu.submit(), None);
        assert_eq!(menu.step, Step::Ssh);
    }

    #[test]
    fn test_submit_selected_host() {
        let mut menu = ssh_menu();
        menu.next();
        menu.next();
        assert_eq!(menu.submit(), Some(Connection::Ssh(vec!["web2".to_string()])));
    }

    #[test]
    fn test_submit_typed_destination() {
        let mut menu = ssh_menu();
        for c in "root@example.com".chars() {
            menu.type_char(c);
        }
        assert_eq!(
            menu.submit(),
            Some(Connection::Ssh(vec!["root@example.com".to_string()]))
        );
    }

    #[test]
    fn test_submit_typed_destination_with_options() {
        let mut menu = ssh_menu();
        for c in "-p 2222 root@example.com".chars() {
            menu.type_char(c);
        }
        assert_eq!(
            menu.submit(),
            Some(Connection::Ssh(
                ["-p", "2222", "root@example.com"].iter().map(|s| s.to_string()).collect()
            ))
        );
    }

    #[test]
    fn test_submit_empty_destination_sets_error() {
        let mut menu = ssh_menu();
        assert_eq!(menu.submit(), None);
        assert!(menu.error.is_some());
    }

    #[test]
    fn test_submit_invalid_ssh_args_sets_error() {
        let mut menu = ssh_menu();
        for c in "host extra".chars() {
            menu.type_char(c);
        }
        assert_eq!(menu.submit(), None);
        assert!(menu.error.unwrap().contains("unexpected arguments"));
    }

    #[test]
    fn test_typing_filters_hosts_and_clears_selection() {
        let mut menu = ssh_menu();
        menu.next();
        assert_eq!(menu.selected_host, Some(0));
        menu.type_char('w');
        assert_eq!(menu.selected_host, None);
        assert_eq!(menu.filtered_hosts(), vec!["web1", "web2"]);
    }

    #[test]
    fn test_filter_is_case_insensitive() {
        let mut menu = ssh_menu();
        menu.type_char('D');
        assert_eq!(menu.filtered_hosts(), vec!["db"]);
    }

    #[test]
    fn test_backspace_restores_full_host_list() {
        let mut menu = ssh_menu();
        menu.type_char('d');
        menu.backspace();
        assert_eq!(menu.filtered_hosts(), vec!["web1", "web2", "db"]);
        assert_eq!(menu.input, "");
    }

    #[test]
    fn test_host_navigation_clamps_at_end() {
        let mut menu = ssh_menu();
        for _ in 0..5 {
            menu.next();
        }
        assert_eq!(menu.selected_host, Some(2));
    }

    #[test]
    fn test_up_from_first_host_returns_to_input() {
        let mut menu = ssh_menu();
        menu.next();
        menu.previous();
        assert_eq!(menu.selected_host, None);
        menu.previous();
        assert_eq!(menu.selected_host, None);
    }

    #[test]
    fn test_no_hosts_keeps_typed_destination_focused() {
        let mut menu = Menu::new(Vec::new(), None);
        menu.step = Step::Ssh;
        menu.next();
        assert_eq!(menu.selected_host, None);
    }

    #[test]
    fn test_back_from_ssh_step_returns_to_choice() {
        let mut menu = ssh_menu();
        menu.error = Some("boom".to_string());
        assert!(!menu.back());
        assert_eq!(menu.step, Step::Choice);
        assert!(menu.error.is_none());
    }

    #[test]
    fn test_back_from_choice_step_quits() {
        let mut menu = Menu::new(hosts(), None);
        assert!(menu.back());
    }

    #[test]
    fn test_filtered_selection_submits_filtered_host() {
        let mut menu = ssh_menu();
        menu.type_char('d');
        assert_eq!(menu.filtered_hosts(), vec!["db"]);
        menu.next();
        assert_eq!(menu.submit(), Some(Connection::Ssh(vec!["db".to_string()])));
    }
}
