use ratatui::widgets::ListState;

use crate::service::{fetch_logs, fetch_services, SystemdService};

pub struct App {
    pub services: Vec<SystemdService>,
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
        };
        app.load_services();
        app
    }

    pub fn load_services(&mut self) {
        match fetch_services() {
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

    pub fn cycle_status_filter(&mut self) {
        self.status_filter = match self.status_filter.as_deref() {
            None => Some("running".to_string()),
            Some("running") => Some("exited".to_string()),
            Some("exited") => Some("failed".to_string()),
            Some("failed") => Some("dead".to_string()),
            Some("dead") => None,
            _ => None,
        };
        self.update_filter();
    }

    pub fn clear_status_filter(&mut self) {
        self.status_filter = None;
        self.update_filter();
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

    pub fn selected_service(&self) -> Option<&SystemdService> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .map(|&i| &self.services[i])
    }

    pub fn load_logs_for_selected(&mut self) {
        let current_service = self.selected_service().map(|s| s.unit.clone());

        if current_service != self.last_selected_service {
            self.last_selected_service = current_service.clone();
            self.logs_scroll = 0;

            if let Some(unit) = current_service {
                match fetch_logs(&unit, 1000) {
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
}
