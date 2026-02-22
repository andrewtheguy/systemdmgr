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
