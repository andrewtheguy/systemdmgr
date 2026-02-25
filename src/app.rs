use std::collections::HashMap;
use std::sync::mpsc;

use ratatui::widgets::ListState;

use crate::service::{
    execute_unit_action, fetch_log_entries, fetch_log_entries_after_cursor,
    fetch_unit_file_content, fetch_unit_properties, fetch_units, LogEntry, SystemdUnit, TimeRange,
    UnitAction, UnitProperties, UnitType, FILE_STATE_OPTIONS, TIME_RANGES, UNIT_TYPES,
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
    pub cached_entry_heights: Vec<usize>,
    pub cached_entry_heights_width: usize,
    pub cached_entry_heights_query: String,
    pub cached_entry_heights_dirty: bool,
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
    // Unit actions
    pub show_action_picker: bool,
    pub action_picker_state: ListState,
    pub available_actions: Vec<UnitAction>,
    pub show_confirm: bool,
    pub confirm_action: Option<UnitAction>,
    pub confirm_unit_name: Option<String>,
    pub action_in_progress: bool,
    pub action_result: Option<Result<String, String>>,
    pub action_receiver: Option<mpsc::Receiver<Result<String, String>>>,
    pub refresh_receiver: Option<mpsc::Receiver<Vec<SystemdUnit>>>,
    pub status_message: Option<String>,
    pub live_tail: bool,
    pub last_refreshed: Option<chrono::DateTime<chrono::Local>>,
    // Unit file viewer
    pub show_unit_file: bool,
    pub unit_file_content: Vec<String>,
    pub unit_file_scroll: usize,
    pub unit_file_unit_name: Option<String>,
    pub unit_file_search_query: String,
    pub unit_file_search_mode: bool,
    pub unit_file_search_matches: Vec<usize>,
    pub unit_file_search_match_index: Option<usize>,
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
            cached_entry_heights: Vec::new(),
            cached_entry_heights_width: 0,
            cached_entry_heights_query: String::new(),
            cached_entry_heights_dirty: true,
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
            show_action_picker: false,
            action_picker_state: ListState::default(),
            available_actions: Vec::new(),
            show_confirm: false,
            confirm_action: None,
            confirm_unit_name: None,
            action_in_progress: false,
            action_result: None,
            action_receiver: None,
            refresh_receiver: None,
            status_message: None,
            live_tail: false,
            last_refreshed: None,
            show_unit_file: false,
            unit_file_content: Vec::new(),
            unit_file_scroll: 0,
            unit_file_unit_name: None,
            unit_file_search_query: String::new(),
            unit_file_search_mode: false,
            unit_file_search_matches: Vec::new(),
            unit_file_search_match_index: None,
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
                self.last_refreshed = Some(chrono::Local::now());
                self.update_filter();
                if !self.filtered_indices.is_empty() && self.list_state.selected().is_none() {
                    self.list_state.select(Some(0));
                }
            }
            Err(e) => {
                if self.user_mode {
                    self.error = Some(e);
                } else {
                    self.error = Some(format!("{} (press 'u' to switch to user mode)", e));
                }
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
        let max_index = self.filtered_indices.len() - 1;
        let i = match self.list_state.selected() {
            Some(i) => i.min(max_index).saturating_add(1).min(max_index),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let max_index = self.filtered_indices.len() - 1;
        let i = match self.list_state.selected() {
            Some(i) => i.min(max_index).saturating_sub(1),
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
            self.invalidate_log_entry_heights_cache();
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
                            self.logs_scroll = usize::MAX;
                        }
                    }
                    Err(e) => {
                        self.logs = vec![LogEntry {
                            timestamp: None,
                            priority: None,
                            pid: None,
                            identifier: None,
                            message: format!("Error fetching logs: {}", e),
                            boot_id: None,
                            invocation_id: None,
                            cursor: None,
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

    pub fn invalidate_log_entry_heights_cache(&mut self) {
        self.cached_entry_heights_dirty = true;
    }

    pub fn scroll_logs_up(&mut self, amount: usize) {
        self.live_tail = false;
        self.logs_scroll = self.logs_scroll.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: usize) {
        if !self.logs.is_empty() {
            let max_scroll = self.logs.len().saturating_sub(1);
            self.logs_scroll = self.logs_scroll.saturating_add(amount).min(max_scroll);
        }
    }

    pub fn toggle_logs(&mut self) {
        self.show_logs = !self.show_logs;
        if self.show_logs {
            self.live_tail = true;
        } else {
            self.live_tail = false;
            self.last_selected_service = None;
        }
    }

    pub fn toggle_live_tail(&mut self) {
        self.live_tail = !self.live_tail;
        if self.live_tail {
            self.logs_go_to_bottom();
        }
    }

    pub fn refresh_logs(&mut self) {
        let unit = match self.last_selected_service.as_ref() {
            Some(u) => u.clone(),
            None => return,
        };
        let cursor = match self.logs.last().and_then(|e| e.cursor.as_ref()) {
            Some(c) => c.clone(),
            None => return,
        };
        if let Ok(new_entries) = fetch_log_entries_after_cursor(
            &unit,
            &cursor,
            self.user_mode,
            self.log_priority_filter,
            self.log_time_range,
        )
            && !new_entries.is_empty()
        {
            self.logs.extend(new_entries);
            self.invalidate_log_entry_heights_cache();
            self.logs_scroll = usize::MAX;
        }
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
        self.invalidate_log_entry_heights_cache();
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
        self.invalidate_log_entry_heights_cache();
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

    pub fn logs_go_to_bottom(&mut self) {
        if !self.logs.is_empty() {
            // Sentinel value resolved by UI once panel dimensions are known.
            self.logs_scroll = usize::MAX;
        }
    }

    pub fn toggle_user_mode(&mut self) {
        self.user_mode = !self.user_mode;
        self.last_selected_service = None;
        self.logs.clear();
        self.invalidate_log_entry_heights_cache();
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

    // Unit action picker methods

    pub fn open_action_picker(&mut self) {
        if let Some(unit) = self.selected_unit() {
            let sub = unit.sub.clone();
            let file_state = unit.file_state.clone();
            self.available_actions =
                UnitAction::available_actions(&sub, file_state.as_deref());
            if !self.available_actions.is_empty() {
                self.action_picker_state.select(Some(0));
                self.show_action_picker = true;
            }
        }
    }

    pub fn close_action_picker(&mut self) {
        self.show_action_picker = false;
    }

    pub fn action_picker_next(&mut self) {
        let len = self.available_actions.len();
        if len == 0 {
            return;
        }
        let i = self.action_picker_state.selected().unwrap_or(0);
        let next = (i + 1) % len;
        self.action_picker_state.select(Some(next));
    }

    pub fn action_picker_previous(&mut self) {
        let len = self.available_actions.len();
        if len == 0 {
            return;
        }
        let i = self.action_picker_state.selected().unwrap_or(0);
        let prev = if i == 0 { len - 1 } else { i - 1 };
        self.action_picker_state.select(Some(prev));
    }

    pub fn action_picker_confirm(&mut self) {
        if let Some(i) = self.action_picker_state.selected()
            && let Some(&action) = self.available_actions.get(i)
        {
            let unit_name = self
                .selected_unit()
                .map(|u| u.unit.clone())
                .unwrap_or_default();
            self.confirm_action = Some(action);
            self.confirm_unit_name = Some(unit_name);
            self.show_action_picker = false;
            self.show_confirm = true;
        }
    }

    pub fn confirm_yes(&mut self) {
        if let (Some(action), Some(unit_name)) = (self.confirm_action, &self.confirm_unit_name)
        {
            let unit_name = unit_name.clone();
            let user_mode = self.user_mode;
            let unit_type = self.unit_type;
            let (action_tx, action_rx) = mpsc::channel();
            let (refresh_tx, refresh_rx) = mpsc::channel();
            self.action_in_progress = true;
            self.action_receiver = Some(action_rx);
            self.refresh_receiver = Some(refresh_rx);
            std::thread::spawn(move || {
                let result = execute_unit_action(action, &unit_name, user_mode);
                let _ = action_tx.send(result);
                if let Ok(units) = fetch_units(unit_type, user_mode) {
                    let _ = refresh_tx.send(units);
                }
            });
        }
    }

    pub fn check_action_progress(&mut self) {
        if let Some(ref rx) = self.action_receiver
            && let Ok(result) = rx.try_recv()
        {
            self.action_in_progress = false;
            self.action_result = Some(result);
            self.action_receiver = None;
            if self.show_logs {
                self.mark_logs_dirty();
            }
        }
        if let Some(ref rx) = self.refresh_receiver
            && let Ok(units) = rx.try_recv()
        {
            self.refresh_receiver = None;
            self.properties_cache.clear();
            self.services = units;
            self.last_refreshed = Some(chrono::Local::now());
            self.update_filter();
        }
    }

    pub fn confirm_no(&mut self) {
        self.show_confirm = false;
        self.confirm_action = None;
        self.confirm_unit_name = None;
        self.action_in_progress = false;
        self.action_result = None;
        self.action_receiver = None;
        self.refresh_receiver = None;
    }

    pub fn dismiss_action_result(&mut self) {
        self.show_confirm = false;
        self.confirm_action = None;
        self.confirm_unit_name = None;
        self.action_in_progress = false;
        self.action_result = None;
        self.action_receiver = None;
        self.refresh_receiver = None;
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    // Unit file viewer methods

    pub fn open_unit_file(&mut self) {
        if let Some(unit) = self.selected_unit() {
            let name = unit.unit.clone();
            match fetch_unit_file_content(&name, self.user_mode) {
                Ok(lines) => {
                    self.unit_file_content = lines;
                }
                Err(e) => {
                    self.unit_file_content = vec![format!("Error: {}", e)];
                }
            }
            self.unit_file_unit_name = Some(name);
            self.unit_file_scroll = 0;
            self.unit_file_search_query.clear();
            self.unit_file_search_matches.clear();
            self.unit_file_search_match_index = None;
            self.unit_file_search_mode = false;
            self.show_unit_file = true;
        }
    }

    pub fn close_unit_file(&mut self) {
        self.show_unit_file = false;
        self.unit_file_content.clear();
        self.unit_file_scroll = 0;
        self.unit_file_unit_name = None;
        self.unit_file_search_query.clear();
        self.unit_file_search_matches.clear();
        self.unit_file_search_match_index = None;
        self.unit_file_search_mode = false;
    }

    pub fn scroll_unit_file_up(&mut self, amount: usize) {
        self.unit_file_scroll = self.unit_file_scroll.saturating_sub(amount);
    }

    pub fn scroll_unit_file_down(&mut self, amount: usize) {
        if !self.unit_file_content.is_empty() {
            let max_scroll = self.unit_file_content.len().saturating_sub(1);
            self.unit_file_scroll = self.unit_file_scroll.saturating_add(amount).min(max_scroll);
        }
    }

    pub fn unit_file_go_to_top(&mut self) {
        self.unit_file_scroll = 0;
    }

    pub fn unit_file_go_to_bottom(&mut self) {
        if !self.unit_file_content.is_empty() {
            self.unit_file_scroll = self.unit_file_content.len().saturating_sub(1);
        }
    }

    pub fn update_unit_file_search(&mut self) {
        self.unit_file_search_matches.clear();
        self.unit_file_search_match_index = None;

        if self.unit_file_search_query.is_empty() {
            return;
        }

        let query = self.unit_file_search_query.to_lowercase();
        for (i, line) in self.unit_file_content.iter().enumerate() {
            if line.to_lowercase().contains(&query) {
                self.unit_file_search_matches.push(i);
            }
        }

        if !self.unit_file_search_matches.is_empty() {
            self.unit_file_search_match_index = Some(0);
            self.unit_file_scroll = self.unit_file_search_matches[0];
        }
    }

    pub fn clear_unit_file_search(&mut self) {
        self.unit_file_search_query.clear();
        self.unit_file_search_matches.clear();
        self.unit_file_search_match_index = None;
    }

    pub fn next_unit_file_match(&mut self, visible_lines: usize) {
        if self.unit_file_search_matches.is_empty() {
            return;
        }
        let next = match self.unit_file_search_match_index {
            Some(i) => (i + 1) % self.unit_file_search_matches.len(),
            None => 0,
        };
        self.unit_file_search_match_index = Some(next);
        let line_idx = self.unit_file_search_matches[next];
        if line_idx < self.unit_file_scroll
            || line_idx >= self.unit_file_scroll + visible_lines
        {
            self.unit_file_scroll = line_idx;
        }
    }

    pub fn prev_unit_file_match(&mut self, visible_lines: usize) {
        if self.unit_file_search_matches.is_empty() {
            return;
        }
        let prev = match self.unit_file_search_match_index {
            Some(0) => self.unit_file_search_matches.len() - 1,
            Some(i) => i - 1,
            None => self.unit_file_search_matches.len() - 1,
        };
        self.unit_file_search_match_index = Some(prev);
        let line_idx = self.unit_file_search_matches[prev];
        if line_idx < self.unit_file_scroll
            || line_idx >= self.unit_file_scroll + visible_lines
        {
            self.unit_file_scroll = line_idx;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{LogEntry, SystemdUnit, UnitAction, UnitProperties, UnitType, TimeRange};

    fn make_unit(name: &str, sub: &str, desc: &str, file_state: Option<&str>) -> SystemdUnit {
        SystemdUnit {
            unit: name.into(),
            load: "loaded".into(),
            active: "active".into(),
            sub: sub.into(),
            description: desc.into(),
            detail: None,
            file_state: file_state.map(|s| s.into()),
        }
    }

    fn make_log(message: &str) -> LogEntry {
        LogEntry {
            timestamp: None,
            priority: None,
            pid: None,
            identifier: None,
            message: message.into(),
            boot_id: None,
            invocation_id: None,
            cursor: None,
        }
    }

    fn test_app_with_services(services: Vec<SystemdUnit>) -> App {
        let len = services.len();
        let mut app = App {
            services,
            list_state: ListState::default(),
            should_quit: false,
            error: None,
            search_query: String::new(),
            search_mode: false,
            filtered_indices: (0..len).collect(),
            logs: Vec::new(),
            cached_entry_heights: Vec::new(),
            cached_entry_heights_width: 0,
            cached_entry_heights_query: String::new(),
            cached_entry_heights_dirty: true,
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
            show_action_picker: false,
            action_picker_state: ListState::default(),
            available_actions: Vec::new(),
            show_confirm: false,
            confirm_action: None,
            confirm_unit_name: None,
            action_in_progress: false,
            action_result: None,
            action_receiver: None,
            refresh_receiver: None,
            status_message: None,
            live_tail: false,
            last_refreshed: None,
            show_unit_file: false,
            unit_file_content: Vec::new(),
            unit_file_scroll: 0,
            unit_file_unit_name: None,
            unit_file_search_query: String::new(),
            unit_file_search_mode: false,
            unit_file_search_matches: Vec::new(),
            unit_file_search_match_index: None,
        };
        if !app.filtered_indices.is_empty() {
            app.list_state.select(Some(0));
        }
        app
    }

    fn test_app_empty() -> App {
        test_app_with_services(Vec::new())
    }

    fn test_app_with_subs(subs: &[&str]) -> App {
        let services: Vec<SystemdUnit> = subs
            .iter()
            .enumerate()
            .map(|(i, sub)| make_unit(&format!("unit{}.service", i), sub, &format!("Unit {}", i), None))
            .collect();
        test_app_with_services(services)
    }

    // Phase 1 — Navigation: next

    #[test]
    fn test_next_moves_down() {
        let mut app = test_app_with_subs(&["running", "exited", "dead"]);
        assert_eq!(app.list_state.selected(), Some(0));
        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_next_clamps_at_end() {
        let mut app = test_app_with_subs(&["running", "exited"]);
        app.list_state.select(Some(1));
        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_next_no_op_on_empty() {
        let mut app = test_app_empty();
        app.next();
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_next_selects_0_from_none() {
        let mut app = test_app_with_subs(&["running"]);
        app.list_state.select(None);
        app.next();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    // Phase 1 — Navigation: previous

    #[test]
    fn test_previous_moves_up() {
        let mut app = test_app_with_subs(&["running", "exited", "dead"]);
        app.list_state.select(Some(2));
        app.previous();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_previous_clamps_at_top() {
        let mut app = test_app_with_subs(&["running", "exited"]);
        app.list_state.select(Some(0));
        app.previous();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_previous_no_op_on_empty() {
        let mut app = test_app_empty();
        app.previous();
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_previous_selects_0_from_none() {
        let mut app = test_app_with_subs(&["running"]);
        app.list_state.select(None);
        app.previous();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    // Phase 1 — Navigation: go_to_top / go_to_bottom

    #[test]
    fn test_go_to_top() {
        let mut app = test_app_with_subs(&["a", "b", "c"]);
        app.list_state.select(Some(2));
        app.go_to_top();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_go_to_top_empty() {
        let mut app = test_app_empty();
        app.go_to_top();
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_go_to_bottom() {
        let mut app = test_app_with_subs(&["a", "b", "c"]);
        app.go_to_bottom();
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn test_go_to_bottom_empty() {
        let mut app = test_app_empty();
        app.go_to_bottom();
        assert_eq!(app.list_state.selected(), None);
    }

    // Phase 1 — Navigation: page_up / page_down

    #[test]
    fn test_page_down() {
        let mut app = test_app_with_subs(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
        app.page_down(3);
        assert_eq!(app.list_state.selected(), Some(3));
    }

    #[test]
    fn test_page_down_clamps() {
        let mut app = test_app_with_subs(&["a", "b", "c"]);
        app.page_down(10);
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn test_page_up() {
        let mut app = test_app_with_subs(&["a", "b", "c", "d", "e"]);
        app.list_state.select(Some(4));
        app.page_up(3);
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_page_up_clamps() {
        let mut app = test_app_with_subs(&["a", "b", "c"]);
        app.list_state.select(Some(1));
        app.page_up(10);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_page_up_empty() {
        let mut app = test_app_empty();
        app.page_up(5);
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_page_down_empty() {
        let mut app = test_app_empty();
        app.page_down(5);
        assert_eq!(app.list_state.selected(), None);
    }

    // Phase 1 — Selection & filtering

    #[test]
    fn test_selected_unit_returns_correct_unit() {
        let app = test_app_with_subs(&["running", "exited", "dead"]);
        let unit = app.selected_unit().unwrap();
        assert_eq!(unit.sub, "running");
    }

    #[test]
    fn test_selected_unit_index_1() {
        let mut app = test_app_with_subs(&["running", "exited", "dead"]);
        app.list_state.select(Some(1));
        let unit = app.selected_unit().unwrap();
        assert_eq!(unit.sub, "exited");
    }

    #[test]
    fn test_selected_unit_with_filter() {
        let mut app = test_app_with_services(vec![
            make_unit("a.service", "running", "A", None),
            make_unit("b.service", "dead", "B", None),
            make_unit("c.service", "running", "C", None),
        ]);
        app.status_filter = Some("running".into());
        app.update_filter();
        // filtered_indices should be [0, 2]
        assert_eq!(app.filtered_indices, vec![0, 2]);
        // Select the second filtered item
        app.list_state.select(Some(1));
        let unit = app.selected_unit().unwrap();
        assert_eq!(unit.unit, "c.service");
    }

    #[test]
    fn test_selected_unit_none_empty() {
        let app = test_app_empty();
        assert!(app.selected_unit().is_none());
    }

    #[test]
    fn test_selected_unit_none_no_selection() {
        let mut app = test_app_with_subs(&["running"]);
        app.list_state.select(None);
        assert!(app.selected_unit().is_none());
    }

    #[test]
    fn test_update_filter_no_filters() {
        let mut app = test_app_with_subs(&["running", "exited", "dead"]);
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_update_filter_search_by_unit_name() {
        let mut app = test_app_with_services(vec![
            make_unit("ssh.service", "running", "SSH", None),
            make_unit("sshd.service", "running", "SSH Daemon", None),
            make_unit("nginx.service", "running", "Nginx", None),
        ]);
        app.search_query = "ssh".into();
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0, 1]);
    }

    #[test]
    fn test_update_filter_search_by_description() {
        let mut app = test_app_with_services(vec![
            make_unit("pg.service", "running", "PostgreSQL database", None),
            make_unit("nginx.service", "running", "Nginx web", None),
        ]);
        app.search_query = "database".into();
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0]);
    }

    #[test]
    fn test_update_filter_search_case_insensitive() {
        let mut app = test_app_with_services(vec![
            make_unit("SSH.service", "running", "desc", None),
        ]);
        app.search_query = "ssh".into();
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0]);
    }

    #[test]
    fn test_update_filter_status_filter() {
        let mut app = test_app_with_subs(&["running", "dead", "running"]);
        app.status_filter = Some("running".into());
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0, 2]);
    }

    #[test]
    fn test_update_filter_combined_search_and_status() {
        let mut app = test_app_with_services(vec![
            make_unit("ssh.service", "running", "SSH", None),
            make_unit("sshd.service", "dead", "SSH Daemon", None),
            make_unit("nginx.service", "running", "Nginx", None),
        ]);
        app.search_query = "ssh".into();
        app.status_filter = Some("running".into());
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0]);
    }

    #[test]
    fn test_update_filter_no_matches_clears_selection() {
        let mut app = test_app_with_subs(&["running"]);
        app.search_query = "nonexistent".into();
        app.update_filter();
        assert!(app.filtered_indices.is_empty());
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn test_update_filter_resets_out_of_bounds_selection() {
        let mut app = test_app_with_subs(&["running", "dead", "exited"]);
        app.list_state.select(Some(2));
        app.status_filter = Some("running".into());
        app.update_filter();
        // Only 1 match, selection 2 is out of bounds → reset to 0
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_clear_search() {
        let mut app = test_app_with_subs(&["running", "dead"]);
        app.search_query = "nonexistent".into();
        app.update_filter();
        assert!(app.filtered_indices.is_empty());
        app.clear_search();
        assert!(app.search_query.is_empty());
        assert_eq!(app.filtered_indices, vec![0, 1]);
    }

    // Phase 4 — File state filtering

    #[test]
    fn test_update_filter_file_state() {
        let mut app = test_app_with_services(vec![
            make_unit("a.service", "running", "A", Some("enabled")),
            make_unit("b.service", "running", "B", Some("disabled")),
            make_unit("c.service", "running", "C", Some("enabled")),
        ]);
        app.file_state_filter = Some("enabled".into());
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0, 2]);
    }

    #[test]
    fn test_update_filter_combined_search_status_file_state() {
        let mut app = test_app_with_services(vec![
            make_unit("ssh.service", "running", "SSH", Some("enabled")),
            make_unit("sshd.service", "running", "SSH Daemon", Some("disabled")),
            make_unit("nginx.service", "dead", "Nginx", Some("enabled")),
            make_unit("pg.service", "running", "PostgreSQL", Some("enabled")),
        ]);
        app.search_query = "ssh".into();
        app.status_filter = Some("running".into());
        app.file_state_filter = Some("enabled".into());
        app.update_filter();
        assert_eq!(app.filtered_indices, vec![0]);
    }

    // Phase 1 — Status picker

    #[test]
    fn test_status_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_status_picker();
        assert_eq!(app.status_picker_state.selected(), Some(0));
        app.status_picker_next();
        assert_eq!(app.status_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_status_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_status_picker();
        app.status_picker_state.select(Some(2));
        app.status_picker_previous();
        assert_eq!(app.status_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_status_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_status_picker();
        let last = app.unit_type.status_options().len() - 1;
        app.status_picker_state.select(Some(last));
        app.status_picker_next();
        assert_eq!(app.status_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_status_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_status_picker();
        app.status_picker_state.select(Some(0));
        app.status_picker_previous();
        let last = app.unit_type.status_options().len() - 1;
        assert_eq!(app.status_picker_state.selected(), Some(last));
    }

    #[test]
    fn test_status_picker_confirm_all() {
        let mut app = test_app_with_subs(&["running", "dead"]);
        app.status_filter = Some("running".into());
        app.open_status_picker();
        app.status_picker_state.select(Some(0)); // "All"
        app.status_picker_confirm();
        assert_eq!(app.status_filter, None);
        assert!(!app.show_status_picker);
    }

    #[test]
    fn test_status_picker_confirm_running() {
        let mut app = test_app_with_subs(&["running", "dead"]);
        app.open_status_picker();
        app.status_picker_state.select(Some(1)); // "running"
        app.status_picker_confirm();
        assert_eq!(app.status_filter, Some("running".into()));
        assert_eq!(app.filtered_indices, vec![0]);
        assert!(!app.show_status_picker);
    }

    #[test]
    fn test_open_status_picker_preselects_all() {
        let mut app = test_app_with_subs(&["running"]);
        app.status_filter = None;
        app.open_status_picker();
        assert_eq!(app.status_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_open_status_picker_preselects_current() {
        let mut app = test_app_with_subs(&["running"]);
        app.status_filter = Some("running".into());
        app.open_status_picker();
        // "running" is index 1 for Service type
        assert_eq!(app.status_picker_state.selected(), Some(1));
    }

    // Phase 2 — Type picker

    #[test]
    fn test_type_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_type_picker();
        assert_eq!(app.type_picker_state.selected(), Some(0));
        app.type_picker_next();
        assert_eq!(app.type_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_type_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_type_picker();
        app.type_picker_state.select(Some(2));
        app.type_picker_previous();
        assert_eq!(app.type_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_type_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_type_picker();
        app.type_picker_state.select(Some(UNIT_TYPES.len() - 1));
        app.type_picker_next();
        assert_eq!(app.type_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_type_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_type_picker();
        app.type_picker_state.select(Some(0));
        app.type_picker_previous();
        assert_eq!(
            app.type_picker_state.selected(),
            Some(UNIT_TYPES.len() - 1)
        );
    }

    #[test]
    fn test_type_picker_confirm_same_type_no_change() {
        let mut app = test_app_with_subs(&["running", "dead"]);
        let original_services_len = app.services.len();
        app.open_type_picker();
        // Service is index 0, which is already the current type
        app.type_picker_state.select(Some(0));
        app.type_picker_confirm();
        // Services should not be reloaded (same type)
        assert_eq!(app.services.len(), original_services_len);
        assert!(!app.show_type_picker);
    }

    // Phase 4 — File state picker

    #[test]
    fn test_file_state_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_file_state_picker();
        assert_eq!(app.file_state_picker_state.selected(), Some(0));
        app.file_state_picker_next();
        assert_eq!(app.file_state_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_file_state_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_file_state_picker();
        app.file_state_picker_state.select(Some(3));
        app.file_state_picker_previous();
        assert_eq!(app.file_state_picker_state.selected(), Some(2));
    }

    #[test]
    fn test_file_state_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_file_state_picker();
        app.file_state_picker_state
            .select(Some(FILE_STATE_OPTIONS.len() - 1));
        app.file_state_picker_next();
        assert_eq!(app.file_state_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_file_state_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_file_state_picker();
        app.file_state_picker_state.select(Some(0));
        app.file_state_picker_previous();
        assert_eq!(
            app.file_state_picker_state.selected(),
            Some(FILE_STATE_OPTIONS.len() - 1)
        );
    }

    #[test]
    fn test_file_state_picker_confirm_all() {
        let mut app = test_app_with_services(vec![
            make_unit("a.service", "running", "A", Some("enabled")),
            make_unit("b.service", "running", "B", Some("disabled")),
        ]);
        app.file_state_filter = Some("enabled".into());
        app.open_file_state_picker();
        app.file_state_picker_state.select(Some(0)); // "All"
        app.file_state_picker_confirm();
        assert_eq!(app.file_state_filter, None);
        assert!(!app.show_file_state_picker);
    }

    #[test]
    fn test_file_state_picker_confirm_enabled() {
        let mut app = test_app_with_services(vec![
            make_unit("a.service", "running", "A", Some("enabled")),
            make_unit("b.service", "running", "B", Some("disabled")),
        ]);
        app.open_file_state_picker();
        app.file_state_picker_state.select(Some(1)); // "enabled"
        app.file_state_picker_confirm();
        assert_eq!(app.file_state_filter, Some("enabled".into()));
        assert_eq!(app.filtered_indices, vec![0]);
        assert!(!app.show_file_state_picker);
    }

    #[test]
    fn test_open_file_state_picker_preselects_all() {
        let mut app = test_app_with_subs(&["running"]);
        app.file_state_filter = None;
        app.open_file_state_picker();
        assert_eq!(app.file_state_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_open_file_state_picker_preselects_current() {
        let mut app = test_app_with_subs(&["running"]);
        app.file_state_filter = Some("disabled".into());
        app.open_file_state_picker();
        // "disabled" is index 2 in FILE_STATE_OPTIONS
        assert_eq!(app.file_state_picker_state.selected(), Some(2));
    }

    // Phase 3 — Priority picker

    #[test]
    fn test_priority_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_priority_picker();
        assert_eq!(app.priority_picker_state.selected(), Some(0));
        app.priority_picker_next();
        assert_eq!(app.priority_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_priority_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_priority_picker();
        app.priority_picker_state.select(Some(3));
        app.priority_picker_previous();
        assert_eq!(app.priority_picker_state.selected(), Some(2));
    }

    #[test]
    fn test_priority_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_priority_picker();
        app.priority_picker_state.select(Some(8)); // last item (0-8 = 9 items)
        app.priority_picker_next();
        assert_eq!(app.priority_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_priority_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_priority_picker();
        app.priority_picker_state.select(Some(0));
        app.priority_picker_previous();
        assert_eq!(app.priority_picker_state.selected(), Some(8));
    }

    #[test]
    fn test_priority_picker_confirm_all() {
        let mut app = test_app_with_subs(&["running"]);
        app.log_priority_filter = Some(3);
        app.open_priority_picker();
        app.priority_picker_state.select(Some(0)); // "All"
        app.priority_picker_confirm();
        assert_eq!(app.log_priority_filter, None);
        assert!(!app.show_priority_picker);
    }

    #[test]
    fn test_priority_picker_confirm_err() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_priority_picker();
        app.priority_picker_state.select(Some(4)); // err (index-1 = 3)
        app.priority_picker_confirm();
        assert_eq!(app.log_priority_filter, Some(3));
        assert!(app.log_filters_dirty);
        assert!(!app.show_priority_picker);
    }

    #[test]
    fn test_open_priority_picker_preselects_current() {
        let mut app = test_app_with_subs(&["running"]);
        app.log_priority_filter = Some(5);
        app.open_priority_picker();
        assert_eq!(app.priority_picker_state.selected(), Some(6)); // 5 + 1
    }

    // Phase 3 — Time picker

    #[test]
    fn test_time_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_time_picker();
        assert_eq!(app.time_picker_state.selected(), Some(0));
        app.time_picker_next();
        assert_eq!(app.time_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_time_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_time_picker();
        app.time_picker_state.select(Some(3));
        app.time_picker_previous();
        assert_eq!(app.time_picker_state.selected(), Some(2));
    }

    #[test]
    fn test_time_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_time_picker();
        app.time_picker_state.select(Some(TIME_RANGES.len() - 1));
        app.time_picker_next();
        assert_eq!(app.time_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_time_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_time_picker();
        app.time_picker_state.select(Some(0));
        app.time_picker_previous();
        assert_eq!(
            app.time_picker_state.selected(),
            Some(TIME_RANGES.len() - 1)
        );
    }

    #[test]
    fn test_time_picker_confirm() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_time_picker();
        app.time_picker_state.select(Some(2)); // OneHour
        app.time_picker_confirm();
        assert_eq!(app.log_time_range, TimeRange::OneHour);
        assert!(app.log_filters_dirty);
        assert!(!app.show_time_picker);
    }

    #[test]
    fn test_open_time_picker_preselects_current() {
        let mut app = test_app_with_subs(&["running"]);
        app.log_time_range = TimeRange::SevenDays;
        app.open_time_picker();
        assert_eq!(app.time_picker_state.selected(), Some(4)); // SevenDays is index 4
    }

    // Phase 1 — Toggles

    #[test]
    fn test_toggle_logs() {
        let mut app = test_app_with_subs(&["running"]);
        assert!(!app.show_logs);
        assert!(!app.live_tail);
        app.toggle_logs();
        assert!(app.show_logs);
        assert!(app.live_tail);
        app.toggle_logs();
        assert!(!app.show_logs);
        assert!(!app.live_tail);
    }

    #[test]
    fn test_toggle_help() {
        let mut app = test_app_with_subs(&["running"]);
        assert!(!app.show_help);
        app.toggle_help();
        assert!(app.show_help);
        app.toggle_help();
        assert!(!app.show_help);
    }

    #[test]
    fn test_toggle_live_tail() {
        let mut app = test_app_with_subs(&["running"]);
        assert!(!app.live_tail);
        app.toggle_live_tail();
        assert!(app.live_tail);
        app.toggle_live_tail();
        assert!(!app.live_tail);
    }

    #[test]
    fn test_toggle_live_tail_enabling_goes_to_bottom() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("a"), make_log("b"), make_log("c")];
        app.logs_scroll = 1;

        app.toggle_live_tail();

        assert!(app.live_tail);
        assert_eq!(app.logs_scroll, usize::MAX);
    }

    #[test]
    fn test_toggle_logs_disables_live_tail() {
        let mut app = test_app_with_subs(&["running"]);
        app.show_logs = true;
        app.live_tail = true;
        app.toggle_logs(); // turns off logs
        assert!(!app.show_logs);
        assert!(!app.live_tail);
    }

    // Phase 1 — User mode

    #[test]
    fn test_toggle_user_mode_flips_flag() {
        let mut app = test_app_with_subs(&["running"]);
        assert!(!app.user_mode);
        app.toggle_user_mode();
        assert!(app.user_mode);
    }

    #[test]
    fn test_toggle_user_mode_clears_state() {
        let mut app = test_app_with_subs(&["running"]);
        app.last_selected_service = Some("test".into());
        app.logs = vec![make_log("log1")];
        app.log_search_query = "search".into();
        app.log_priority_filter = Some(3);
        app.log_time_range = TimeRange::OneHour;
        app.properties_cache
            .insert("test".into(), UnitProperties::default());
        app.file_state_filter = Some("enabled".into());

        app.toggle_user_mode();

        assert_eq!(app.last_selected_service, None);
        assert!(app.logs.is_empty());
        assert!(app.log_search_query.is_empty());
        assert_eq!(app.log_priority_filter, None);
        assert_eq!(app.log_time_range, TimeRange::All);
        assert!(app.properties_cache.is_empty());
        assert_eq!(app.file_state_filter, None);
    }

    // Phase 3 — Log scrolling

    #[test]
    fn test_scroll_logs_up() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("a"), make_log("b"), make_log("c")];
        app.logs_scroll = 2;
        app.scroll_logs_up(1);
        assert_eq!(app.logs_scroll, 1);
    }

    #[test]
    fn test_scroll_logs_up_clamps_at_zero() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("a")];
        app.logs_scroll = 0;
        app.scroll_logs_up(5);
        assert_eq!(app.logs_scroll, 0);
    }

    #[test]
    fn test_scroll_logs_up_disables_live_tail() {
        let mut app = test_app_with_subs(&["running"]);
        app.live_tail = true;
        app.scroll_logs_up(1);
        assert!(!app.live_tail);
    }

    #[test]
    fn test_scroll_logs_down() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![
            make_log("a"),
            make_log("b"),
            make_log("c"),
            make_log("d"),
            make_log("e"),
        ];
        app.logs_scroll = 0;
        app.scroll_logs_down(1);
        assert_eq!(app.logs_scroll, 1);
    }

    #[test]
    fn test_scroll_logs_down_clamps_at_max() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("a"), make_log("b"), make_log("c")];
        app.logs_scroll = 0;
        app.scroll_logs_down(100);
        assert_eq!(app.logs_scroll, 2);
    }

    #[test]
    fn test_scroll_logs_down_empty() {
        let mut app = test_app_with_subs(&["running"]);
        app.scroll_logs_down(1);
        assert_eq!(app.logs_scroll, 0);
    }

    #[test]
    fn test_logs_go_to_top() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("a"), make_log("b")];
        app.logs_scroll = 5;
        app.logs_go_to_top();
        assert_eq!(app.logs_scroll, 0);
    }

    #[test]
    fn test_logs_go_to_bottom() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![
            make_log("a"),
            make_log("b"),
            make_log("c"),
            make_log("d"),
            make_log("e"),
        ];
        app.logs_scroll = 0;
        app.logs_go_to_bottom();
        assert_eq!(app.logs_scroll, usize::MAX);
    }

    // Phase 4 — Detail scrolling

    #[test]
    fn test_detail_scroll_up() {
        let mut app = test_app_with_subs(&["running"]);
        app.detail_scroll = 5;
        app.detail_scroll_up(2);
        assert_eq!(app.detail_scroll, 3);
    }

    #[test]
    fn test_detail_scroll_up_clamps() {
        let mut app = test_app_with_subs(&["running"]);
        app.detail_scroll = 2;
        app.detail_scroll_up(10);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_detail_scroll_down() {
        let mut app = test_app_with_subs(&["running"]);
        app.detail_scroll = 0;
        app.detail_scroll_down(3, 20, 10); // content=20, visible=10
        assert_eq!(app.detail_scroll, 3);
    }

    #[test]
    fn test_detail_scroll_down_clamps_at_max() {
        let mut app = test_app_with_subs(&["running"]);
        app.detail_scroll = 0;
        app.detail_scroll_down(100, 20, 10); // max = 20 - 10 = 10
        assert_eq!(app.detail_scroll, 10);
    }

    #[test]
    fn test_detail_scroll_down_no_scroll_when_content_fits() {
        let mut app = test_app_with_subs(&["running"]);
        app.detail_scroll = 0;
        app.detail_scroll_down(5, 10, 20); // content=10 < visible=20
        assert_eq!(app.detail_scroll, 0);
    }

    // Phase 4 — Details modal

    #[test]
    fn test_close_details() {
        let mut app = test_app_with_subs(&["running"]);
        app.show_details = true;
        app.detail_properties = Some(UnitProperties::default());
        app.detail_unit_name = Some("test.service".into());
        app.detail_scroll = 5;

        app.close_details();

        assert!(!app.show_details);
        assert!(app.detail_properties.is_none());
        assert!(app.detail_unit_name.is_none());
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_open_details_uses_cache() {
        let mut app = test_app_with_services(vec![
            make_unit("test.service", "running", "Test", None),
        ]);
        let mut props = UnitProperties::default();
        props.description = "Cached description".into();
        app.properties_cache
            .insert("test.service".into(), props);

        app.open_details();

        assert!(app.show_details);
        assert_eq!(app.detail_unit_name.as_deref(), Some("test.service"));
        assert_eq!(
            app.detail_properties.as_ref().unwrap().description,
            "Cached description"
        );
    }

    // Phase 3 — Log search

    #[test]
    fn test_update_log_search_finds_matches() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![
            make_log("hello world"),
            make_log("goodbye"),
            make_log("hello again"),
        ];
        app.log_search_query = "hello".into();
        app.update_log_search();
        assert_eq!(app.log_search_matches, vec![0, 2]);
        assert_eq!(app.log_search_match_index, Some(0));
        assert_eq!(app.logs_scroll, 0); // scrolled to first match
    }

    #[test]
    fn test_update_log_search_case_insensitive() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("Hello World"), make_log("no match")];
        app.log_search_query = "hello".into();
        app.update_log_search();
        assert_eq!(app.log_search_matches, vec![0]);
    }

    #[test]
    fn test_update_log_search_no_matches() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("hello")];
        app.log_search_query = "xyz".into();
        app.update_log_search();
        assert!(app.log_search_matches.is_empty());
        assert_eq!(app.log_search_match_index, None);
    }

    #[test]
    fn test_update_log_search_empty_query() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("hello")];
        app.log_search_query = "".into();
        app.update_log_search();
        assert!(app.log_search_matches.is_empty());
        assert_eq!(app.log_search_match_index, None);
    }

    #[test]
    fn test_next_log_match() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![
            make_log("match1"),
            make_log("no"),
            make_log("match2"),
        ];
        app.log_search_query = "match".into();
        app.update_log_search();
        assert_eq!(app.log_search_match_index, Some(0));
        app.next_log_match(10);
        assert_eq!(app.log_search_match_index, Some(1));
    }

    #[test]
    fn test_prev_log_match() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![
            make_log("match1"),
            make_log("no"),
            make_log("match2"),
        ];
        app.log_search_query = "match".into();
        app.update_log_search();
        app.log_search_match_index = Some(1);
        app.prev_log_match(10);
        assert_eq!(app.log_search_match_index, Some(0));
    }

    #[test]
    fn test_next_log_match_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("match1"), make_log("match2")];
        app.log_search_query = "match".into();
        app.update_log_search();
        app.log_search_match_index = Some(1); // last match
        app.next_log_match(10);
        assert_eq!(app.log_search_match_index, Some(0));
    }

    #[test]
    fn test_prev_log_match_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = vec![make_log("match1"), make_log("match2")];
        app.log_search_query = "match".into();
        app.update_log_search();
        app.log_search_match_index = Some(0); // first match
        app.prev_log_match(10);
        assert_eq!(app.log_search_match_index, Some(1));
    }

    #[test]
    fn test_next_log_match_empty() {
        let mut app = test_app_with_subs(&["running"]);
        app.next_log_match(10);
        assert_eq!(app.log_search_match_index, None);
    }

    #[test]
    fn test_prev_log_match_empty() {
        let mut app = test_app_with_subs(&["running"]);
        app.prev_log_match(10);
        assert_eq!(app.log_search_match_index, None);
    }

    #[test]
    fn test_next_log_match_scrolls_when_out_of_view() {
        let mut app = test_app_with_subs(&["running"]);
        app.logs = (0..20)
            .map(|i| {
                if i == 0 || i == 15 {
                    make_log("match")
                } else {
                    make_log("no")
                }
            })
            .collect();
        app.log_search_query = "match".into();
        app.update_log_search();
        // matches at 0 and 15
        assert_eq!(app.log_search_matches, vec![0, 15]);
        app.logs_scroll = 0;
        app.next_log_match(5); // visible = 5, match at 15 is out of view
        assert_eq!(app.log_search_match_index, Some(1));
        assert_eq!(app.logs_scroll, 15);
    }

    #[test]
    fn test_clear_log_search() {
        let mut app = test_app_with_subs(&["running"]);
        app.log_search_query = "test".into();
        app.log_search_matches = vec![0, 1];
        app.log_search_match_index = Some(0);

        app.clear_log_search();

        assert!(app.log_search_query.is_empty());
        assert!(app.log_search_matches.is_empty());
        assert_eq!(app.log_search_match_index, None);
    }

    #[test]
    fn test_mark_logs_dirty() {
        let mut app = test_app_with_subs(&["running"]);
        assert!(!app.log_filters_dirty);
        app.mark_logs_dirty();
        assert!(app.log_filters_dirty);
    }

    // Unit action picker

    #[test]
    fn test_open_action_picker_running() {
        let mut app = test_app_with_services(vec![
            make_unit("test.service", "running", "Test", Some("enabled")),
        ]);
        app.open_action_picker();
        assert!(app.show_action_picker);
        assert!(!app.available_actions.is_empty());
        assert_eq!(app.action_picker_state.selected(), Some(0));
        assert!(app.available_actions.contains(&UnitAction::Stop));
        assert!(app.available_actions.contains(&UnitAction::Restart));
        assert!(app.available_actions.contains(&UnitAction::Disable));
    }

    #[test]
    fn test_open_action_picker_dead() {
        let mut app = test_app_with_services(vec![
            make_unit("test.service", "dead", "Test", Some("disabled")),
        ]);
        app.open_action_picker();
        assert!(app.show_action_picker);
        assert!(app.available_actions.contains(&UnitAction::Start));
        assert!(app.available_actions.contains(&UnitAction::Enable));
    }

    #[test]
    fn test_open_action_picker_no_selection() {
        let mut app = test_app_empty();
        app.open_action_picker();
        assert!(!app.show_action_picker);
    }

    #[test]
    fn test_close_action_picker() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_action_picker();
        assert!(app.show_action_picker);
        app.close_action_picker();
        assert!(!app.show_action_picker);
    }

    #[test]
    fn test_action_picker_next() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_action_picker();
        assert_eq!(app.action_picker_state.selected(), Some(0));
        app.action_picker_next();
        assert_eq!(app.action_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_action_picker_next_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_action_picker();
        let last = app.available_actions.len() - 1;
        app.action_picker_state.select(Some(last));
        app.action_picker_next();
        assert_eq!(app.action_picker_state.selected(), Some(0));
    }

    #[test]
    fn test_action_picker_previous() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_action_picker();
        app.action_picker_state.select(Some(2));
        app.action_picker_previous();
        assert_eq!(app.action_picker_state.selected(), Some(1));
    }

    #[test]
    fn test_action_picker_previous_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.open_action_picker();
        app.action_picker_state.select(Some(0));
        app.action_picker_previous();
        assert_eq!(
            app.action_picker_state.selected(),
            Some(app.available_actions.len() - 1)
        );
    }

    #[test]
    fn test_action_picker_confirm_opens_confirm() {
        let mut app = test_app_with_services(vec![
            make_unit("test.service", "running", "Test", None),
        ]);
        app.open_action_picker();
        app.action_picker_confirm();
        assert!(!app.show_action_picker);
        assert!(app.show_confirm);
        assert!(app.confirm_action.is_some());
        assert_eq!(app.confirm_unit_name.as_deref(), Some("test.service"));
    }

    #[test]
    fn test_confirm_no_clears_state() {
        let mut app = test_app_with_subs(&["running"]);
        app.show_confirm = true;
        app.confirm_action = Some(UnitAction::Stop);
        app.confirm_unit_name = Some("test.service".into());
        app.confirm_no();
        assert!(!app.show_confirm);
        assert!(app.confirm_action.is_none());
        assert!(app.confirm_unit_name.is_none());
        assert!(app.action_result.is_none());
    }

    #[test]
    fn test_dismiss_action_result() {
        let mut app = test_app_with_subs(&["running"]);
        app.show_confirm = true;
        app.confirm_action = Some(UnitAction::Stop);
        app.confirm_unit_name = Some("test.service".into());
        app.action_result = Some(Ok("Done".into()));
        app.dismiss_action_result();
        assert!(!app.show_confirm);
        assert!(app.confirm_action.is_none());
        assert!(app.confirm_unit_name.is_none());
        assert!(app.action_result.is_none());
    }

    #[test]
    fn test_clear_status_message() {
        let mut app = test_app_with_subs(&["running"]);
        app.status_message = Some("Success".into());
        app.clear_status_message();
        assert!(app.status_message.is_none());
    }

    // Unit file viewer

    #[test]
    fn test_close_unit_file_resets_state() {
        let mut app = test_app_with_subs(&["running"]);
        app.show_unit_file = true;
        app.unit_file_content = vec!["line1".into(), "line2".into()];
        app.unit_file_scroll = 5;
        app.unit_file_unit_name = Some("test.service".into());
        app.unit_file_search_query = "search".into();
        app.unit_file_search_matches = vec![0, 1];
        app.unit_file_search_match_index = Some(0);
        app.unit_file_search_mode = true;

        app.close_unit_file();

        assert!(!app.show_unit_file);
        assert!(app.unit_file_content.is_empty());
        assert_eq!(app.unit_file_scroll, 0);
        assert!(app.unit_file_unit_name.is_none());
        assert!(app.unit_file_search_query.is_empty());
        assert!(app.unit_file_search_matches.is_empty());
        assert_eq!(app.unit_file_search_match_index, None);
        assert!(!app.unit_file_search_mode);
    }

    #[test]
    fn test_scroll_unit_file_up() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into(), "b".into(), "c".into()];
        app.unit_file_scroll = 2;
        app.scroll_unit_file_up(1);
        assert_eq!(app.unit_file_scroll, 1);
    }

    #[test]
    fn test_scroll_unit_file_up_clamps_at_zero() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into()];
        app.unit_file_scroll = 0;
        app.scroll_unit_file_up(5);
        assert_eq!(app.unit_file_scroll, 0);
    }

    #[test]
    fn test_scroll_unit_file_down() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        app.unit_file_scroll = 0;
        app.scroll_unit_file_down(1);
        assert_eq!(app.unit_file_scroll, 1);
    }

    #[test]
    fn test_scroll_unit_file_down_clamps_at_max() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into(), "b".into(), "c".into()];
        app.unit_file_scroll = 0;
        app.scroll_unit_file_down(100);
        assert_eq!(app.unit_file_scroll, 2);
    }

    #[test]
    fn test_unit_file_go_to_top() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into(), "b".into()];
        app.unit_file_scroll = 5;
        app.unit_file_go_to_top();
        assert_eq!(app.unit_file_scroll, 0);
    }

    #[test]
    fn test_unit_file_go_to_bottom() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        app.unit_file_scroll = 0;
        app.unit_file_go_to_bottom();
        assert_eq!(app.unit_file_scroll, 4);
    }

    #[test]
    fn test_update_unit_file_search_finds_matches() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec![
            "ExecStart=/usr/bin/foo".into(),
            "Restart=always".into(),
            "ExecStop=/usr/bin/bar".into(),
        ];
        app.unit_file_search_query = "Exec".into();
        app.update_unit_file_search();
        assert_eq!(app.unit_file_search_matches, vec![0, 2]);
        assert_eq!(app.unit_file_search_match_index, Some(0));
        assert_eq!(app.unit_file_scroll, 0);
    }

    #[test]
    fn test_update_unit_file_search_no_matches() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["ExecStart=/usr/bin/foo".into()];
        app.unit_file_search_query = "xyz".into();
        app.update_unit_file_search();
        assert!(app.unit_file_search_matches.is_empty());
        assert_eq!(app.unit_file_search_match_index, None);
    }

    #[test]
    fn test_clear_unit_file_search() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_search_query = "test".into();
        app.unit_file_search_matches = vec![0, 1];
        app.unit_file_search_match_index = Some(0);

        app.clear_unit_file_search();

        assert!(app.unit_file_search_query.is_empty());
        assert!(app.unit_file_search_matches.is_empty());
        assert_eq!(app.unit_file_search_match_index, None);
    }

    #[test]
    fn test_next_unit_file_match() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["match1".into(), "no".into(), "match2".into()];
        app.unit_file_search_query = "match".into();
        app.update_unit_file_search();
        assert_eq!(app.unit_file_search_match_index, Some(0));
        app.next_unit_file_match(10);
        assert_eq!(app.unit_file_search_match_index, Some(1));
    }

    #[test]
    fn test_next_unit_file_match_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["match1".into(), "match2".into()];
        app.unit_file_search_query = "match".into();
        app.update_unit_file_search();
        app.unit_file_search_match_index = Some(1);
        app.next_unit_file_match(10);
        assert_eq!(app.unit_file_search_match_index, Some(0));
    }

    #[test]
    fn test_prev_unit_file_match() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["match1".into(), "no".into(), "match2".into()];
        app.unit_file_search_query = "match".into();
        app.update_unit_file_search();
        app.unit_file_search_match_index = Some(1);
        app.prev_unit_file_match(10);
        assert_eq!(app.unit_file_search_match_index, Some(0));
    }

    #[test]
    fn test_prev_unit_file_match_wraps() {
        let mut app = test_app_with_subs(&["running"]);
        app.unit_file_content = vec!["match1".into(), "match2".into()];
        app.unit_file_search_query = "match".into();
        app.update_unit_file_search();
        app.unit_file_search_match_index = Some(0);
        app.prev_unit_file_match(10);
        assert_eq!(app.unit_file_search_match_index, Some(1));
    }
}
