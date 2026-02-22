use std::collections::HashMap;

use ratatui::widgets::ListState;

use crate::service::{
    fetch_log_entries, fetch_unit_properties, fetch_units, LogEntry, SystemdUnit, TimeRange,
    UnitProperties, UnitType, FILE_STATE_OPTIONS, UNIT_TYPES, TIME_RANGES,
};

pub struct App {
    pub services: Vec<SystemdUnit>,
    pub list_state: ListState,
    pub should_quit: bool,
    pub error: Option<String>,
    pub search_query: String,
    pub search_mode: bool,
    pub filtered_indices: Vec<usize>,
    pub logs: Vec<LogEntry>,
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
    pub log_priority_filter: Option<u8>,
    pub log_time_range: TimeRange,
    pub log_filters_dirty: bool,
    pub show_priority_picker: bool,
    pub priority_picker_state: ListState,
    pub show_time_picker: bool,
    pub time_picker_state: ListState,
    // Details modal
    pub show_details: bool,
    pub detail_scroll: usize,
    pub detail_properties: Option<UnitProperties>,
    pub detail_unit_name: Option<String>,
    pub detail_content_height: usize,
    pub properties_cache: HashMap<String, UnitProperties>,
    // File state filter
    pub file_state_filter: Option<String>,
    pub show_file_state_picker: bool,
    pub file_state_picker_state: ListState,
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
            log_priority_filter: None,
            log_time_range: TimeRange::All,
            log_filters_dirty: false,
            show_priority_picker: false,
            priority_picker_state: ListState::default(),
            show_time_picker: false,
            time_picker_state: ListState::default(),
            show_details: false,
            detail_scroll: 0,
            detail_properties: None,
            detail_unit_name: None,
            detail_content_height: 0,
            properties_cache: HashMap::new(),
            file_state_filter: None,
            show_file_state_picker: false,
            file_state_picker_state: ListState::default(),
        };
        app.load_services();
        app
    }

    pub fn load_services(&mut self) {
        self.properties_cache.clear();
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

                // File state filter
                let matches_file_state = self.file_state_filter.is_none()
                    || service.file_state.as_ref() == self.file_state_filter.as_ref();

                matches_search && matches_status && matches_file_state
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
                self.file_state_filter = None;
                self.search_query.clear();
                self.last_selected_service = None;
                self.logs.clear();
                self.clear_log_search();
                self.log_priority_filter = None;
                self.log_time_range = TimeRange::All;
                self.properties_cache.clear();
                self.load_services();
            }
        }
        self.show_type_picker = false;
    }

    pub fn open_priority_picker(&mut self) {
        self.show_priority_picker = true;
        let index = match self.log_priority_filter {
            None => 0,
            Some(p) => (p as usize) + 1,
        };
        self.priority_picker_state.select(Some(index));
    }

    pub fn close_priority_picker(&mut self) {
        self.show_priority_picker = false;
    }

    pub fn priority_picker_next(&mut self) {
        let len = 9; // All + 8 priority levels
        let i = self.priority_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % len;
        self.priority_picker_state.select(Some(next));
    }

    pub fn priority_picker_previous(&mut self) {
        let len = 9;
        let i = self.priority_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 { len - 1 } else { i - 1 };
        self.priority_picker_state.select(Some(prev));
    }

    pub fn priority_picker_confirm(&mut self) {
        if let Some(i) = self.priority_picker_state.selected() {
            if i == 0 {
                self.log_priority_filter = None;
            } else {
                self.log_priority_filter = Some((i - 1) as u8);
            }
            self.mark_logs_dirty();
        }
        self.show_priority_picker = false;
    }

    pub fn open_time_picker(&mut self) {
        self.show_time_picker = true;
        let index = TIME_RANGES
            .iter()
            .position(|&t| t == self.log_time_range)
            .unwrap_or(0);
        self.time_picker_state.select(Some(index));
    }

    pub fn close_time_picker(&mut self) {
        self.show_time_picker = false;
    }

    pub fn time_picker_next(&mut self) {
        let len = TIME_RANGES.len();
        let i = self.time_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % len;
        self.time_picker_state.select(Some(next));
    }

    pub fn time_picker_previous(&mut self) {
        let len = TIME_RANGES.len();
        let i = self.time_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 { len - 1 } else { i - 1 };
        self.time_picker_state.select(Some(prev));
    }

    pub fn time_picker_confirm(&mut self) {
        if let Some(i) = self.time_picker_state.selected() {
            self.log_time_range = TIME_RANGES[i];
            self.mark_logs_dirty();
        }
        self.show_time_picker = false;
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

        if current_service != self.last_selected_service || self.log_filters_dirty {
            self.last_selected_service = current_service.clone();
            self.log_filters_dirty = false;
            self.logs_scroll = 0;
            self.clear_log_search();

            if let Some(unit) = current_service {
                match fetch_log_entries(
                    &unit,
                    1000,
                    self.user_mode,
                    self.log_priority_filter,
                    self.log_time_range,
                ) {
                    Ok(logs) => {
                        self.logs = logs;
                        if !self.logs.is_empty() {
                            self.logs_scroll = self.logs.len().saturating_sub(1);
                        }
                    }
                    Err(e) => {
                        self.logs = vec![LogEntry {
                            timestamp: None,
                            priority: None,
                            pid: None,
                            identifier: None,
                            message: format!("Error fetching logs: {}", e),
                        }];
                    }
                }
            } else {
                self.logs.clear();
            }
        }
    }

    pub fn mark_logs_dirty(&mut self) {
        self.log_filters_dirty = true;
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
        for (i, entry) in self.logs.iter().enumerate() {
            if entry.message.to_lowercase().contains(&query) {
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
        self.log_priority_filter = None;
        self.log_time_range = TimeRange::All;
        self.properties_cache.clear();
        self.file_state_filter = None;
        self.load_services();
    }

    // Details modal methods

    pub fn open_details(&mut self) {
        if let Some(unit) = self.selected_unit() {
            let name = unit.unit.clone();
            let props = if let Some(cached) = self.properties_cache.get(&name) {
                cached.clone()
            } else {
                let props = fetch_unit_properties(&name, self.user_mode);
                self.properties_cache.insert(name.clone(), props.clone());
                props
            };
            self.detail_unit_name = Some(name);
            self.detail_properties = Some(props);
            self.detail_scroll = 0;
            self.show_details = true;
        }
    }

    pub fn close_details(&mut self) {
        self.show_details = false;
        self.detail_properties = None;
        self.detail_unit_name = None;
        self.detail_scroll = 0;
    }

    pub fn detail_scroll_up(&mut self, amount: usize) {
        self.detail_scroll = self.detail_scroll.saturating_sub(amount);
    }

    pub fn detail_scroll_down(&mut self, amount: usize, content_height: usize, visible_height: usize) {
        if content_height > visible_height {
            let max_scroll = content_height.saturating_sub(visible_height);
            self.detail_scroll = (self.detail_scroll.saturating_add(amount)).min(max_scroll);
        }
    }

    // File state picker methods

    pub fn open_file_state_picker(&mut self) {
        self.show_file_state_picker = true;
        let index = match &self.file_state_filter {
            None => 0,
            Some(s) => FILE_STATE_OPTIONS
                .iter()
                .position(|&opt| opt == s)
                .unwrap_or(0),
        };
        self.file_state_picker_state.select(Some(index));
    }

    pub fn close_file_state_picker(&mut self) {
        self.show_file_state_picker = false;
    }

    pub fn file_state_picker_next(&mut self) {
        let len = FILE_STATE_OPTIONS.len();
        let i = self.file_state_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % len;
        self.file_state_picker_state.select(Some(next));
    }

    pub fn file_state_picker_previous(&mut self) {
        let len = FILE_STATE_OPTIONS.len();
        let i = self.file_state_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 { len - 1 } else { i - 1 };
        self.file_state_picker_state.select(Some(prev));
    }

    pub fn file_state_picker_confirm(&mut self) {
        if let Some(i) = self.file_state_picker_state.selected() {
            if i == 0 {
                self.file_state_filter = None;
            } else {
                self.file_state_filter = Some(FILE_STATE_OPTIONS[i].to_string());
            }
            self.update_filter();
        }
        self.show_file_state_picker = false;
    }
}
