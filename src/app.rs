use ratatui::widgets::ListState;

use crate::service::{fetch_logs, fetch_units, SystemdUnit, UnitType, UNIT_TYPES};

pub struct App {
    pub services: Vec<SystemdUnit>,
    pub list_state: ListState,
    pub should_quit: bool,
    pub error: Option<String>,
    pub search_query: String,
    pub search_mode: bool,
    pub filtered_indices: Vec<usize>,
    pub logs: Vec<String>,
    pub logs_scroll: usize,
    pub last_selected_service: Option<String>,
    pub status_filter: Option<String>,
    pub show_logs: bool,
    pub show_help: bool,
    pub show_status_picker: bool,
    pub status_picker_state: ListState,
    pub log_search_query: String,
    pub log_search_mode: bool,
    pub log_search_matches: Vec<usize>,
    pub log_search_match_index: Option<usize>,
    pub user_mode: bool,
    pub unit_type: UnitType,
    pub show_type_picker: bool,
    pub type_picker_state: ListState,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            services: Vec::new(),
            list_state: ListState::default(),
            should_quit: false,
            error: None,
            search_query: String::new(),
            search_mode: false,
            filtered_indices: Vec::new(),
            logs: Vec::new(),
            logs_scroll: 0,
            last_selected_service: None,
            status_filter: None,
            show_logs: false,
            show_help: false,
            show_status_picker: false,
            status_picker_state: ListState::default(),
            log_search_query: String::new(),
            log_search_mode: false,
            log_search_matches: Vec::new(),
            log_search_match_index: None,
            user_mode: false,
            unit_type: UnitType::Service,
            show_type_picker: false,
            type_picker_state: ListState::default(),
        };
        app.load_services();
        app
    }

    pub fn load_services(&mut self) {
        match fetch_units(self.unit_type, self.user_mode) {
            Ok(services) => {
                self.services = services;
                self.error = None;
                self.update_filter();
                if !self.filtered_indices.is_empty() && self.list_state.selected().is_none() {
                    self.list_state.select(Some(0));
                }
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }

    pub fn update_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .services
            .iter()
            .enumerate()
            .filter(|(_, service)| {
                // Text search filter
                let matches_search = self.search_query.is_empty()
                    || service.unit.to_lowercase().contains(&query)
                    || service.description.to_lowercase().contains(&query);

                // Status filter
                let matches_status = self.status_filter.is_none()
                    || self.status_filter.as_ref() == Some(&service.sub);

                matches_search && matches_status
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection if current selection is out of bounds
        if let Some(selected) = self.list_state.selected() {
            if selected >= self.filtered_indices.len() {
                if self.filtered_indices.is_empty() {
                    self.list_state.select(None);
                } else {
                    self.list_state.select(Some(0));
                }
            }
        } else if !self.filtered_indices.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.update_filter();
    }

    pub fn open_status_picker(&mut self) {
        self.show_status_picker = true;
        let options = self.unit_type.status_options();
        let index = match &self.status_filter {
            None => 0, // "All"
            Some(s) => options
                .iter()
                .position(|&opt| opt == s)
                .unwrap_or(0),
        };
        self.status_picker_state.select(Some(index));
    }

    pub fn close_status_picker(&mut self) {
        self.show_status_picker = false;
    }

    pub fn status_picker_next(&mut self) {
        let len = self.unit_type.status_options().len();
        let i = self.status_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % len;
        self.status_picker_state.select(Some(next));
    }

    pub fn status_picker_previous(&mut self) {
        let len = self.unit_type.status_options().len();
        let i = self.status_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 { len - 1 } else { i - 1 };
        self.status_picker_state.select(Some(prev));
    }

    pub fn status_picker_confirm(&mut self) {
        let options = self.unit_type.status_options();
        if let Some(i) = self.status_picker_state.selected() {
            if i == 0 {
                self.status_filter = None;
            } else {
                self.status_filter = Some(options[i].to_string());
            }
            self.update_filter();
        }
        self.show_status_picker = false;
    }

    pub fn open_type_picker(&mut self) {
        self.show_type_picker = true;
        let index = UNIT_TYPES
            .iter()
            .position(|&t| t == self.unit_type)
            .unwrap_or(0);
        self.type_picker_state.select(Some(index));
    }

    pub fn close_type_picker(&mut self) {
        self.show_type_picker = false;
    }

    pub fn type_picker_next(&mut self) {
        let i = self.type_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % UNIT_TYPES.len();
        self.type_picker_state.select(Some(next));
    }

    pub fn type_picker_previous(&mut self) {
        let i = self.type_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 {
            UNIT_TYPES.len() - 1
        } else {
            i - 1
        };
        self.type_picker_state.select(Some(prev));
    }

    pub fn type_picker_confirm(&mut self) {
        if let Some(i) = self.type_picker_state.selected() {
            let new_type = UNIT_TYPES[i];
            if new_type != self.unit_type {
                self.unit_type = new_type;
                self.status_filter = None;
                self.search_query.clear();
                self.last_selected_service = None;
                self.logs.clear();
                self.clear_log_search();
                self.load_services();
            }
        }
        self.show_type_picker = false;
    }

    pub fn next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_indices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_indices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn go_to_top(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn go_to_bottom(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.list_state.select(Some(self.filtered_indices.len() - 1));
        }
    }

    pub fn selected_unit(&self) -> Option<&SystemdUnit> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .map(|&i| &self.services[i])
    }

    pub fn load_logs_for_selected(&mut self) {
        let current_service = self.selected_unit().map(|s| s.unit.clone());

        if current_service != self.last_selected_service {
            self.last_selected_service = current_service.clone();
            self.logs_scroll = 0;
            self.clear_log_search();

            if let Some(unit) = current_service {
                match fetch_logs(&unit, 1000, self.user_mode) {
                    Ok(logs) => {
                        self.logs = logs;
                        // Auto-scroll to bottom (most recent logs)
                        if !self.logs.is_empty() {
                            self.logs_scroll = self.logs.len().saturating_sub(1);
                        }
                    }
                    Err(e) => {
                        self.logs = vec![format!("Error fetching logs: {}", e)];
                    }
                }
            } else {
                self.logs.clear();
            }
        }
    }

    pub fn scroll_logs_up(&mut self, amount: usize) {
        self.logs_scroll = self.logs_scroll.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: usize, visible_lines: usize) {
        if !self.logs.is_empty() {
            let max_scroll = self.logs.len().saturating_sub(visible_lines);
            self.logs_scroll = (self.logs_scroll + amount).min(max_scroll);
        }
    }

    pub fn toggle_logs(&mut self) {
        self.show_logs = !self.show_logs;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn page_up(&mut self, page_size: usize) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let new_index = current.saturating_sub(page_size);
        self.list_state.select(Some(new_index));
    }

    pub fn page_down(&mut self, page_size: usize) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let max_index = self.filtered_indices.len().saturating_sub(1);
        let new_index = (current + page_size).min(max_index);
        self.list_state.select(Some(new_index));
    }

    pub fn update_log_search(&mut self) {
        self.log_search_matches.clear();
        self.log_search_match_index = None;

        if self.log_search_query.is_empty() {
            return;
        }

        let query = self.log_search_query.to_lowercase();
        for (i, line) in self.logs.iter().enumerate() {
            if line.to_lowercase().contains(&query) {
                self.log_search_matches.push(i);
            }
        }

        // Auto-scroll to first match
        if !self.log_search_matches.is_empty() {
            self.log_search_match_index = Some(0);
            self.logs_scroll = self.log_search_matches[0];
        }
    }

    pub fn clear_log_search(&mut self) {
        self.log_search_query.clear();
        self.log_search_matches.clear();
        self.log_search_match_index = None;
    }

    pub fn next_log_match(&mut self, visible_lines: usize) {
        if self.log_search_matches.is_empty() {
            return;
        }
        let next = match self.log_search_match_index {
            Some(i) => (i + 1) % self.log_search_matches.len(),
            None => 0,
        };
        self.log_search_match_index = Some(next);
        let line_idx = self.log_search_matches[next];
        // Scroll so match is visible
        if line_idx < self.logs_scroll || line_idx >= self.logs_scroll + visible_lines {
            self.logs_scroll = line_idx;
        }
    }

    pub fn prev_log_match(&mut self, visible_lines: usize) {
        if self.log_search_matches.is_empty() {
            return;
        }
        let prev = match self.log_search_match_index {
            Some(0) => self.log_search_matches.len() - 1,
            Some(i) => i - 1,
            None => self.log_search_matches.len() - 1,
        };
        self.log_search_match_index = Some(prev);
        let line_idx = self.log_search_matches[prev];
        if line_idx < self.logs_scroll || line_idx >= self.logs_scroll + visible_lines {
            self.logs_scroll = line_idx;
        }
    }

    pub fn logs_go_to_top(&mut self) {
        self.logs_scroll = 0;
    }

    pub fn logs_go_to_bottom(&mut self, visible_lines: usize) {
        if !self.logs.is_empty() {
            self.logs_scroll = self.logs.len().saturating_sub(visible_lines);
        }
    }

    pub fn toggle_user_mode(&mut self) {
        self.user_mode = !self.user_mode;
        self.last_selected_service = None;
        self.logs.clear();
        self.clear_log_search();
        self.load_services();
    }
}
