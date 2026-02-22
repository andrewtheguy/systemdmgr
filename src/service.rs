use chrono::TimeZone;
use ratatui::style::Color;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitType {
    Service,
    Timer,
    Socket,
    Target,
    Path,
}

impl UnitType {
    pub fn label(&self) -> &'static str {
        match self {
            UnitType::Service => "Services",
            UnitType::Timer => "Timers",
            UnitType::Socket => "Sockets",
            UnitType::Target => "Targets",
            UnitType::Path => "Paths",
        }
    }

    pub fn systemctl_type(&self) -> &'static str {
        match self {
            UnitType::Service => "service",
            UnitType::Timer => "timer",
            UnitType::Socket => "socket",
            UnitType::Target => "target",
            UnitType::Path => "path",
        }
    }

    pub fn status_options(&self) -> &'static [&'static str] {
        match self {
            UnitType::Service => &["All", "running", "exited", "failed", "dead"],
            UnitType::Timer => &["All", "waiting", "running", "elapsed"],
            UnitType::Socket => &["All", "listening", "running", "failed"],
            UnitType::Target => &["All", "active", "inactive"],
            UnitType::Path => &["All", "waiting", "running", "failed"],
        }
    }
}

pub const UNIT_TYPES: [UnitType; 5] = [
    UnitType::Service,
    UnitType::Timer,
    UnitType::Socket,
    UnitType::Target,
    UnitType::Path,
];

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Option<i64>,
    pub priority: Option<u8>,
    pub pid: Option<String>,
    pub identifier: Option<String>,
    pub message: String,
}

pub const PRIORITY_LABELS: [&str; 8] = [
    "emerg", "alert", "crit", "err", "warning", "notice", "info", "debug",
];

pub fn priority_label(p: u8) -> &'static str {
    PRIORITY_LABELS.get(p as usize).unwrap_or(&"unknown")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    All,
    FifteenMinutes,
    OneHour,
    OneDay,
    SevenDays,
    Today,
}

impl TimeRange {
    pub fn label(&self) -> &'static str {
        match self {
            TimeRange::All => "All",
            TimeRange::FifteenMinutes => "Last 15 minutes",
            TimeRange::OneHour => "Last 1 hour",
            TimeRange::OneDay => "Last 24 hours",
            TimeRange::SevenDays => "Last 7 days",
            TimeRange::Today => "Today",
        }
    }

    pub fn journalctl_since(&self) -> Option<&'static str> {
        match self {
            TimeRange::All => None,
            TimeRange::FifteenMinutes => Some("15 min ago"),
            TimeRange::OneHour => Some("1 hour ago"),
            TimeRange::OneDay => Some("1 day ago"),
            TimeRange::SevenDays => Some("7 days ago"),
            TimeRange::Today => Some("today"),
        }
    }
}

pub const TIME_RANGES: [TimeRange; 6] = [
    TimeRange::All,
    TimeRange::FifteenMinutes,
    TimeRange::OneHour,
    TimeRange::OneDay,
    TimeRange::SevenDays,
    TimeRange::Today,
];

#[derive(Debug, Clone, Deserialize)]
pub struct SystemdUnit {
    pub unit: String,
    #[allow(dead_code)]
    pub load: String,
    #[allow(dead_code)]
    pub active: String,
    pub sub: String,
    pub description: String,
    #[serde(skip)]
    pub detail: Option<String>,
    #[serde(skip)]
    pub file_state: Option<String>,
}

pub const FILE_STATE_OPTIONS: &[&str] = &["All", "enabled", "disabled", "static", "masked", "indirect"];

#[derive(Debug, Clone, Default)]
pub struct UnitProperties {
    pub fragment_path: String,
    pub unit_file_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
    pub description: String,
    pub main_pid: u32,
    pub exec_main_start_timestamp: String,
    pub memory_current: Option<u64>,
    pub cpu_usage_nsec: Option<u64>,
    pub requires: Vec<String>,
    pub wants: Vec<String>,
    pub after: Vec<String>,
    pub before: Vec<String>,
    pub conflicts: Vec<String>,
    pub triggered_by: Vec<String>,
    pub triggers: Vec<String>,
}

impl SystemdUnit {
    pub fn status_display(&self) -> &str {
        &self.sub
    }

    pub fn status_color(&self) -> Color {
        match self.sub.as_str() {
            "running" => Color::Green,
            "exited" => Color::Yellow,
            "dead" | "stopped" => Color::DarkGray,
            "failed" => Color::Red,
            "waiting" => Color::Cyan,
            "listening" => Color::Green,
            "active" => Color::Green,
            "inactive" => Color::DarkGray,
            "elapsed" => Color::Yellow,
            _ => Color::White,
        }
    }
}

pub fn fetch_log_entries(
    unit_name: &str,
    lines: usize,
    user_mode: bool,
    priority: Option<u8>,
    time_range: TimeRange,
) -> Result<Vec<LogEntry>, String> {
    let unit_flag = if user_mode { "--user-unit" } else { "-u" };
    let lines_str = lines.to_string();
    let mut args = vec![unit_flag, unit_name, "-n", &lines_str, "--no-pager", "--output=json"];

    let priority_str;
    if let Some(p) = priority {
        priority_str = p.to_string();
        args.push("-p");
        args.push(&priority_str);
    }

    let since_value;
    if let Some(since) = time_range.journalctl_since() {
        since_value = since.to_string();
        args.push("--since");
        args.push(&since_value);
    }

    let output = Command::new("journalctl")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute journalctl: {}", e))?;

    let entries = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(parse_journal_json_line)
        .collect();

    Ok(entries)
}

fn parse_journal_json_line(line: &str) -> LogEntry {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        return LogEntry {
            timestamp: None,
            priority: None,
            pid: None,
            identifier: None,
            message: line.to_string(),
        };
    };

    let message = match &val["MESSAGE"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            let bytes: Vec<u8> = arr
                .iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect();
            String::from_utf8_lossy(&bytes).to_string()
        }
        _ => line.to_string(),
    };

    let priority = val["PRIORITY"]
        .as_str()
        .and_then(|s| s.parse::<u8>().ok());

    let timestamp = val["__REALTIME_TIMESTAMP"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok());

    let pid = val["_PID"].as_str().map(|s| s.to_string());

    let identifier = val["SYSLOG_IDENTIFIER"].as_str().map(|s| s.to_string());

    LogEntry {
        timestamp,
        priority,
        pid,
        identifier,
        message,
    }
}

pub fn format_log_timestamp(timestamp_us: i64) -> String {
    let secs = timestamp_us / 1_000_000;
    let nsecs = ((timestamp_us % 1_000_000) * 1000) as u32;
    match chrono::Local.timestamp_opt(secs, nsecs) {
        chrono::LocalResult::Single(dt) => dt.format("%b %d %H:%M:%S").to_string(),
        _ => String::new(),
    }
}

pub fn fetch_units(unit_type: UnitType, user_mode: bool) -> Result<Vec<SystemdUnit>, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    let type_arg = format!("--type={}", unit_type.systemctl_type());
    args.extend(["list-units", &type_arg, "--all", "--no-pager", "--output=json"]);
    let output = Command::new("systemctl")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute systemctl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "systemctl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut units: Vec<SystemdUnit> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    match unit_type {
        UnitType::Timer => merge_timer_details(&mut units, user_mode),
        UnitType::Socket => merge_socket_details(&mut units, user_mode),
        _ => {}
    }

    merge_file_states(&mut units, unit_type, user_mode);

    Ok(units)
}

#[derive(Deserialize)]
struct TimerEntry {
    unit: String,
    next: u64,
}

fn merge_timer_details(units: &mut [SystemdUnit], user_mode: bool) {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["list-timers", "--all", "--no-pager", "--output=json"]);

    let Ok(output) = Command::new("systemctl").args(&args).output() else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let Ok(entries) = serde_json::from_slice::<Vec<TimerEntry>>(&output.stdout) else {
        return;
    };

    let map: HashMap<&str, &TimerEntry> = entries.iter().map(|e| (e.unit.as_str(), e)).collect();

    for unit in units.iter_mut() {
        if let Some(entry) = map.get(unit.unit.as_str()) {
            unit.detail = Some(if entry.next == 0 {
                "next: n/a".to_string()
            } else {
                format!("next: {}", format_relative_time(entry.next))
            });
        }
    }
}

fn format_relative_time(target_us: u64) -> String {
    let now_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    if target_us <= now_us {
        return "elapsed".to_string();
    }

    let diff_secs = (target_us - now_us) / 1_000_000;

    let days = diff_secs / 86400;
    let hours = (diff_secs % 86400) / 3600;
    let minutes = (diff_secs % 3600) / 60;
    let seconds = diff_secs % 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

#[derive(Deserialize)]
struct SocketEntry {
    unit: String,
    listen: String,
}

fn merge_socket_details(units: &mut [SystemdUnit], user_mode: bool) {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["list-sockets", "--all", "--no-pager", "--output=json"]);

    let Ok(output) = Command::new("systemctl").args(&args).output() else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let Ok(entries) = serde_json::from_slice::<Vec<SocketEntry>>(&output.stdout) else {
        return;
    };

    let map: HashMap<&str, &SocketEntry> = entries.iter().map(|e| (e.unit.as_str(), e)).collect();

    for unit in units.iter_mut() {
        if let Some(entry) = map.get(unit.unit.as_str()) {
            unit.detail = Some(entry.listen.clone());
        }
    }
}

#[derive(Deserialize)]
struct UnitFileEntry {
    unit_file: String,
    state: String,
}

fn fetch_unit_file_states(unit_type: UnitType, user_mode: bool) -> HashMap<String, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    let type_arg = format!("--type={}", unit_type.systemctl_type());
    args.extend(["list-unit-files", &type_arg, "--no-pager", "--output=json"]);

    let Ok(output) = Command::new("systemctl").args(&args).output() else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }

    let Ok(entries) = serde_json::from_slice::<Vec<UnitFileEntry>>(&output.stdout) else {
        return HashMap::new();
    };

    entries
        .into_iter()
        .map(|e| {
            // unit_file may be a full path like /usr/lib/systemd/system/foo.service
            // Extract just the filename for matching
            let name = e
                .unit_file
                .rsplit('/')
                .next()
                .unwrap_or(&e.unit_file)
                .to_string();
            (name, e.state)
        })
        .collect()
}

fn merge_file_states(units: &mut [SystemdUnit], unit_type: UnitType, user_mode: bool) {
    let states = fetch_unit_file_states(unit_type, user_mode);
    for unit in units.iter_mut() {
        if let Some(state) = states.get(&unit.unit) {
            unit.file_state = Some(state.clone());
        }
    }
}

pub fn fetch_unit_properties(unit_name: &str, user_mode: bool) -> UnitProperties {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["show", unit_name, "--no-pager"]);

    let Ok(output) = Command::new("systemctl").args(&args).output() else {
        return UnitProperties::default();
    };
    if !output.status.success() {
        return UnitProperties::default();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: HashMap<&str, &str> = stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();

    let get = |key: &str| -> String {
        map.get(key).unwrap_or(&"").to_string()
    };

    let parse_optional_u64 = |key: &str| -> Option<u64> {
        let val = map.get(key).unwrap_or(&"");
        if val.is_empty() || *val == "[not set]" || *val == "infinity" {
            None
        } else {
            val.parse::<u64>().ok()
        }
    };

    let split_deps = |key: &str| -> Vec<String> {
        let val = map.get(key).unwrap_or(&"");
        if val.is_empty() {
            Vec::new()
        } else {
            val.split_whitespace().map(|s| s.to_string()).collect()
        }
    };

    UnitProperties {
        fragment_path: get("FragmentPath"),
        unit_file_state: get("UnitFileState"),
        active_state: get("ActiveState"),
        sub_state: get("SubState"),
        load_state: get("LoadState"),
        description: get("Description"),
        main_pid: map
            .get("MainPID")
            .unwrap_or(&"0")
            .parse::<u32>()
            .unwrap_or(0),
        exec_main_start_timestamp: get("ExecMainStartTimestamp"),
        memory_current: parse_optional_u64("MemoryCurrent"),
        cpu_usage_nsec: parse_optional_u64("CPUUsageNSec"),
        requires: split_deps("Requires"),
        wants: split_deps("Wants"),
        after: split_deps("After"),
        before: split_deps("Before"),
        conflicts: split_deps("Conflicts"),
        triggered_by: split_deps("TriggeredBy"),
        triggers: split_deps("Triggers"),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_cpu_time(nsec: u64) -> String {
    let secs = nsec as f64 / 1_000_000_000.0;
    if secs >= 60.0 {
        format!("{:.1}min", secs / 60.0)
    } else {
        format!("{:.3}s", secs)
    }
}
