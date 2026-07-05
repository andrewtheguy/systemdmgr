use chrono::TimeZone;
use ratatui::style::{Color, Modifier, Style};

/// Muted foreground color for inactive/dimmed states (visible on DarkGray highlight)
pub const COLOR_MUTED: Color = Color::Rgb(100, 100, 100);
use serde::Deserialize;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String>;
}

pub const MIN_SYSTEMD_VERSION: u32 = 246;

pub struct LocalRunner;

impl CommandRunner for LocalRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
        let output = Command::new(program)
            .stdin(Stdio::null())
            .args(args)
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", program, e))?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

pub fn validate_systemctl_version(runner: &dyn CommandRunner) -> Result<u32, String> {
    let output = runner.run("systemctl", &["--version"])
        .map_err(|e| format!("systemctl was not found on PATH or could not be executed: {}", e))?;
    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        if detail.is_empty() {
            return Err("systemctl --version failed".to_string());
        }
        return Err(format!("systemctl --version failed: {}", detail));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = parse_systemd_version(&stdout)
        .ok_or_else(|| "could not parse systemd version from systemctl --version".to_string())?;
    if version < MIN_SYSTEMD_VERSION {
        return Err(format!(
            "systemd {} is too old; systemdmgr requires systemd {} or newer",
            version, MIN_SYSTEMD_VERSION
        ));
    }
    Ok(version)
}

fn parse_systemd_version(output: &str) -> Option<u32> {
    let first_line = output.lines().next()?.trim();
    let rest = first_line.strip_prefix("systemd ")?;
    let raw_version = rest.split_whitespace().next()?;
    let digits: String = raw_version
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Runs commands on a remote host by invoking the system OpenSSH client.
///
/// The CLI arguments after `--ssh` use ssh's own `[options] destination`
/// syntax. A ControlMaster connection is established interactively on connect
/// (so ssh can prompt for passwords, key passphrases, verification codes, and
/// host key verification on the real terminal) and every subsequent command
/// multiplexes over that master socket in batch mode.
///
/// The master is kept as a direct child process running `cat` on the remote
/// host, with its stdin tied to a pipe this process holds. If this process
/// dies without running `Drop` (SIGKILL, crash), the pipe closes, `cat` sees
/// EOF, and the master shuts itself down — no orphaned ssh process.
pub struct SshRunner {
    ssh_options: Vec<String>,
    destination: String,
    control_dir: std::path::PathBuf,
    master: std::process::Child,
}

/// ssh option letters that take a value, from the ssh(1) usage string.
const SSH_OPTS_WITH_ARG: &str = "BbcDEeFIiJLlmOoPpQRSWw";

const SSH_USAGE: &str = "usage: --ssh [ssh-options] destination";

/// Splits ssh CLI arguments into (options, destination).
///
/// There is exactly one accepted form: all ssh options first, the destination
/// as the final argument. A `--` separator is rejected (systemdmgr inserts
/// its own before the destination when invoking ssh), as are arguments after
/// the destination — in plain ssh those would be a remote command, which
/// systemdmgr always supplies itself.
pub fn split_ssh_args(args: &[String]) -> Result<(Vec<String>, String), String> {
    let mut options: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            return Err(format!("'--' is not supported; {}", SSH_USAGE));
        }
        if !arg.starts_with('-') || arg == "-" {
            // The destination must be the final argument.
            if i + 1 != args.len() {
                return Err(format!(
                    "unexpected arguments after destination '{}': '{}'; {}",
                    arg,
                    args[i + 1..].join(" "),
                    SSH_USAGE
                ));
            }
            return Ok((options, arg.clone()));
        }
        options.push(arg.clone());
        // Walk a cluster of short flags; a value-taking letter consumes the
        // rest of the token, or the following argument if the token ends.
        let mut letters = arg[1..].chars();
        while let Some(letter) = letters.next() {
            if SSH_OPTS_WITH_ARG.contains(letter) {
                if letters.as_str().is_empty() {
                    i += 1;
                    let value = args.get(i).ok_or_else(|| {
                        format!("ssh option -{} is missing its value", letter)
                    })?;
                    options.push(value.clone());
                }
                break;
            }
        }
        i += 1;
    }
    Err(format!("no SSH destination given; {}", SSH_USAGE))
}

/// The destination within ssh-style `[options] destination` arguments, for
/// display purposes.
pub fn ssh_destination(args: &[String]) -> Option<String> {
    split_ssh_args(args).ok().map(|(_, destination)| destination)
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg.bytes().all(|b| b.is_ascii_alphanumeric() || b"@._+:/-=".contains(&b)) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn join_remote_command(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Unix socket paths are limited to ~104 bytes (macOS); leave margin for the
/// socket name within the control directory.
const MAX_CONTROL_SOCKET_PATH: usize = 90;

/// The control socket inside the per-process control directory. The directory
/// is private to this process and one process talks to exactly one
/// destination, so a short fixed name is unique — ssh's `%C` hash (40 bytes)
/// would overflow the socket path limit under macOS's long temp dirs.
fn control_socket_path(control_dir: &std::path::Path) -> std::path::PathBuf {
    control_dir.join("ctl")
}

fn control_dir_candidate(base: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = base.join(format!("systemdmgr-ssh-{}", std::process::id()));
    (control_socket_path(&dir).as_os_str().len() <= MAX_CONTROL_SOCKET_PATH).then_some(dir)
}

fn create_control_dir() -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    // Fall back to /tmp when the system temp dir would make the socket path
    // exceed the OS limit (e.g. an unusually long $TMPDIR).
    let dir = [std::env::temp_dir(), std::path::PathBuf::from("/tmp")]
        .iter()
        .find_map(|base| control_dir_candidate(base))
        .ok_or_else(|| {
            "no temp directory short enough for the SSH control socket; set TMPDIR to a shorter path".to_string()
        })?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| format!("Failed to create SSH control directory {}: {}", dir.display(), e))?;
    Ok(dir)
}

/// Multiplexing options prepended to every ssh invocation. `batch_mode`
/// disables prompts and log noise for command execution inside the TUI; the
/// initial master connection runs without it so ssh can interact with the
/// terminal. These come before the user's arguments, and ssh gives the first
/// occurrence of an option precedence, so they cannot be accidentally
/// overridden.
fn multiplex_args(control_dir: &std::path::Path, batch_mode: bool) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        format!("ControlPath={}", control_socket_path(control_dir).display()),
        "-o".to_string(),
        "ControlMaster=auto".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=60".to_string(),
    ];
    if batch_mode {
        args.push("-o".to_string());
        args.push("BatchMode=yes".to_string());
        args.push("-o".to_string());
        args.push("LogLevel=ERROR".to_string());
    }
    args
}

impl SshRunner {
    /// `ssh_cli_args` uses ssh's own syntax: `[options] destination`,
    /// e.g. `["deploy@myserver"]` or `["-p", "2222", "-i", "key", "myserver"]`.
    /// Options may also follow the destination.
    pub fn connect(ssh_cli_args: Vec<String>) -> Result<Self, String> {
        let (ssh_options, destination) = split_ssh_args(&ssh_cli_args)?;
        let control_dir = create_control_dir()?;

        // Remote `cat` blocks on our pipe forever; the master stays up
        // exactly as long as this process is alive.
        let master = Command::new("ssh")
            .args(multiplex_args(&control_dir, false))
            .args(&ssh_options)
            .arg("--")
            .arg(&destination)
            .arg("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to run ssh (is the OpenSSH client installed?): {}", e))?;

        let mut runner = SshRunner {
            ssh_options,
            destination,
            control_dir,
            master,
        };
        // On failure this drops `runner`, which tears down the master and
        // removes the control directory.
        runner.wait_until_ready()?;
        Ok(runner)
    }

    /// Waits for the master to finish authenticating and publish its control
    /// socket. Authentication may block indefinitely on user prompts, so the
    /// only exit conditions are the socket answering or the master dying.
    fn wait_until_ready(&mut self) -> Result<(), String> {
        loop {
            if let Ok(Some(status)) = self.master.try_wait() {
                return Err(format!(
                    "ssh could not connect to {} ({})",
                    self.destination, status
                ));
            }
            let check = Command::new("ssh")
                .args(multiplex_args(&self.control_dir, true))
                .args(&self.ssh_options)
                .args(["-O", "check", "--"])
                .arg(&self.destination)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Ok(status) = check
                && status.success()
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
}

impl Drop for SshRunner {
    fn drop(&mut self) {
        // Closing the watchdog pipe alone would stop the master; the explicit
        // -O exit and kill just make teardown immediate and reap the child.
        drop(self.master.stdin.take());
        let _ = Command::new("ssh")
            .args(multiplex_args(&self.control_dir, true))
            .args(&self.ssh_options)
            .args(["-O", "exit", "--"])
            .arg(&self.destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.master.kill();
        let _ = self.master.wait();
        let _ = std::fs::remove_dir_all(&self.control_dir);
    }
}

impl CommandRunner for SshRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
        let output = Command::new("ssh")
            .stdin(Stdio::null())
            .args(multiplex_args(&self.control_dir, true))
            .args(&self.ssh_options)
            .arg("--")
            .arg(&self.destination)
            .arg(join_remote_command(program, args))
            .output()
            .map_err(|e| format!("Failed to run ssh: {}", e))?;

        // ssh exits 255 on transport/auth errors; anything else is the
        // remote command's own exit status.
        if output.status.code() == Some(255) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("SSH error: {}", stderr.trim()));
        }

        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn run_systemctl(runner: &dyn CommandRunner, extra_args: &[&str]) -> Result<CommandOutput, String> {
    let mut args = vec!["--no-ask-password"];
    args.extend_from_slice(extra_args);
    runner.run("systemctl", &args)
}

fn run_journalctl(runner: &dyn CommandRunner, args: &[&str]) -> Result<CommandOutput, String> {
    runner.run("journalctl", args)
}

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
    /// Styles parsed from ANSI SGR escape sequences embedded in the raw
    /// message, as byte ranges over the cleaned `message`. Empty when the
    /// message contained no escape sequences.
    pub message_styles: Vec<(std::ops::Range<usize>, Style)>,
    pub boot_id: Option<String>,
    pub invocation_id: Option<String>,
    pub cursor: Option<String>,
    pub unit: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitAction {
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
    DaemonReload,
}

impl UnitAction {
    pub fn label(&self) -> &'static str {
        match self {
            UnitAction::Start => "Start",
            UnitAction::Stop => "Stop",
            UnitAction::Restart => "Restart",
            UnitAction::Reload => "Reload",
            UnitAction::Enable => "Enable",
            UnitAction::Disable => "Disable",
            UnitAction::DaemonReload => "Daemon Reload",
        }
    }

    pub fn shortcut(&self) -> char {
        match self {
            UnitAction::Start => 's',
            UnitAction::Stop => 't',
            UnitAction::Restart => 'r',
            UnitAction::Reload => 'l',
            UnitAction::Enable => 'e',
            UnitAction::Disable => 'd',
            UnitAction::DaemonReload => 'D',
        }
    }

    pub fn systemctl_verb(&self) -> &'static str {
        match self {
            UnitAction::Start => "start",
            UnitAction::Stop => "stop",
            UnitAction::Restart => "restart",
            UnitAction::Reload => "reload",
            UnitAction::Enable => "enable",
            UnitAction::Disable => "disable",
            UnitAction::DaemonReload => "daemon-reload",
        }
    }

    pub fn progress_label(&self) -> &'static str {
        match self {
            UnitAction::Start => "Starting...",
            UnitAction::Stop => "Stopping...",
            UnitAction::Restart => "Restarting...",
            UnitAction::Reload => "Reloading...",
            UnitAction::Enable => "Enabling...",
            UnitAction::Disable => "Disabling...",
            UnitAction::DaemonReload => "Reloading daemon...",
        }
    }

    pub fn confirmation_message(&self, unit_name: &str) -> String {
        match self {
            UnitAction::DaemonReload => "Reload systemd daemon configuration?".to_string(),
            _ => format!("{} {}?", self.label(), unit_name),
        }
    }

    pub fn available_actions(sub_state: &str, file_state: Option<&str>) -> Vec<UnitAction> {
        let mut actions = Vec::new();

        match sub_state {
            "running" | "active" | "listening" | "waiting" => {
                actions.push(UnitAction::Stop);
                actions.push(UnitAction::Restart);
                actions.push(UnitAction::Reload);
            }
            "dead" | "failed" | "inactive" | "exited" => {
                actions.push(UnitAction::Start);
            }
            _ => {
                actions.push(UnitAction::Start);
                actions.push(UnitAction::Stop);
            }
        }

        match file_state {
            Some("enabled") => actions.push(UnitAction::Disable),
            Some("disabled") => actions.push(UnitAction::Enable),
            _ => {}
        }

        actions.push(UnitAction::DaemonReload);
        actions
    }
}

pub fn execute_unit_action(action: UnitAction, unit_name: &str, user_mode: bool, runner: &dyn CommandRunner) -> Result<String, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.push(action.systemctl_verb());
    if action != UnitAction::DaemonReload {
        args.push(unit_name);
    }

    let output = run_systemctl(runner, &args)?;

    if output.success {
        Ok(format!("{} succeeded for {}", action.label(), unit_name))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("{} failed: {}", action.label(), stderr.trim()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct UnitProperties {
    pub fragment_path: String,
    pub unit_file_state: String,
    pub active_state: String,
    pub active_enter_timestamp: String,
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
    pub timers_calendar: Vec<String>,
    pub timers_monotonic: Vec<String>,
    pub last_trigger_usec: String,
    pub result: String,
    pub next_elapse_realtime: String,
    pub persistent: String,
    pub accuracy_usec: String,
    pub randomized_delay_usec: String,
    // Path properties
    pub paths: String,
    // Socket properties
    pub listen: String,
    pub accept: String,
    pub n_connections: String,
    pub n_accepted: String,
}

impl SystemdUnit {
    pub fn status_display(&self) -> &str {
        &self.sub
    }

    pub fn status_color(&self) -> Color {
        match self.sub.as_str() {
            "running" => Color::Green,
            "exited" => Color::Yellow,
            "dead" | "stopped" => COLOR_MUTED,
            "failed" => Color::Red,
            "waiting" => Color::Cyan,
            "listening" => Color::Green,
            "active" => Color::Green,
            "inactive" => COLOR_MUTED,
            "elapsed" => Color::Yellow,
            _ => Color::White,
        }
    }
}

pub fn fetch_log_entries(
    unit_name: Option<&str>,
    lines: usize,
    user_mode: bool,
    priority: Option<u8>,
    time_range: TimeRange,
    runner: &dyn CommandRunner,
) -> Result<Vec<LogEntry>, String> {
    let lines_str = lines.to_string();
    let mut args = vec!["-n", &lines_str, "--no-pager", "--output=json"];
    if let Some(name) = unit_name {
        let unit_flag = if user_mode { "--user-unit" } else { "-u" };
        args.insert(0, name);
        args.insert(0, unit_flag);
    }

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

    let output = run_journalctl(runner, &args)?;

    let entries = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(parse_journal_json_line)
        .collect();

    Ok(entries)
}

pub fn fetch_log_entries_after_cursor(
    unit_name: Option<&str>,
    cursor: &str,
    user_mode: bool,
    priority: Option<u8>,
    time_range: TimeRange,
    runner: &dyn CommandRunner,
) -> Result<Vec<LogEntry>, String> {
    let after_cursor = format!("--after-cursor={}", cursor);
    let mut args = vec![&*after_cursor, "--no-pager", "--output=json"];
    if let Some(name) = unit_name {
        let unit_flag = if user_mode { "--user-unit" } else { "-u" };
        args.insert(0, name);
        args.insert(0, unit_flag);
    }

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

    let output = run_journalctl(runner, &args)?;

    let entries = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(parse_journal_json_line)
        .collect();

    Ok(entries)
}

/// Parses ANSI SGR escape sequences out of a log message, returning the
/// visible text plus the style for each byte range that had one. Non-SGR
/// escape sequences (cursor movement, OSC titles, ...) are stripped.
fn parse_ansi_message(raw: &str) -> (String, Vec<(std::ops::Range<usize>, Style)>) {
    if !raw.contains('\x1b') {
        return (raw.to_string(), Vec::new());
    }

    fn push_run(
        styles: &mut Vec<(std::ops::Range<usize>, Style)>,
        start: usize,
        end: usize,
        style: Style,
    ) {
        if end > start && style != Style::default() {
            styles.push((start..end, style));
        }
    }

    let mut clean = String::with_capacity(raw.len());
    let mut styles = Vec::new();
    let mut current = Style::default();
    let mut run_start = 0;

    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            clean.push(ch);
            continue;
        }
        match chars.peek() {
            // CSI: ESC [ <params> <final byte in @..~>
            Some('[') => {
                chars.next();
                let mut params = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        final_byte = Some(c);
                        break;
                    }
                    params.push(c);
                }
                if final_byte == Some('m') {
                    push_run(&mut styles, run_start, clean.len(), current);
                    run_start = clean.len();
                    current = apply_sgr(current, &params);
                }
            }
            // OSC: ESC ] ... terminated by BEL or ESC \
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Two-character escape (ESC c, ESC ( B, ...): drop the next char
            _ => {
                chars.next();
            }
        }
    }
    push_run(&mut styles, run_start, clean.len(), current);
    (clean, styles)
}

fn apply_sgr(mut style: Style, params: &str) -> Style {
    // Empty parameters (ESC[m) mean reset, hence the unwrap_or(0).
    let mut codes = params
        .split([';', ':'])
        .map(|p| p.parse::<u16>().unwrap_or(0));
    while let Some(code) = codes.next() {
        match code {
            0 => style = Style::default(),
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            5 | 6 => style = style.add_modifier(Modifier::SLOW_BLINK),
            7 => style = style.add_modifier(Modifier::REVERSED),
            8 => style = style.add_modifier(Modifier::HIDDEN),
            9 => style = style.add_modifier(Modifier::CROSSED_OUT),
            21 | 22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            25 => style = style.remove_modifier(Modifier::SLOW_BLINK),
            27 => style = style.remove_modifier(Modifier::REVERSED),
            28 => style = style.remove_modifier(Modifier::HIDDEN),
            29 => style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => style = style.fg(ansi_standard_color(code - 30)),
            38 => {
                if let Some(color) = ansi_extended_color(&mut codes) {
                    style = style.fg(color);
                }
            }
            39 => style.fg = None,
            40..=47 => style = style.bg(ansi_standard_color(code - 40)),
            48 => {
                if let Some(color) = ansi_extended_color(&mut codes) {
                    style = style.bg(color);
                }
            }
            49 => style.bg = None,
            90..=97 => style = style.fg(ansi_bright_color(code - 90)),
            100..=107 => style = style.bg(ansi_bright_color(code - 100)),
            _ => {}
        }
    }
    style
}

fn ansi_standard_color(n: u16) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::Gray,
    }
}

fn ansi_bright_color(n: u16) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

fn ansi_extended_color(codes: &mut impl Iterator<Item = u16>) -> Option<Color> {
    match codes.next()? {
        5 => Some(Color::Indexed(codes.next()? as u8)),
        2 => {
            let (r, g, b) = (codes.next()?, codes.next()?, codes.next()?);
            Some(Color::Rgb(r as u8, g as u8, b as u8))
        }
        _ => None,
    }
}

fn parse_journal_json_line(line: &str) -> LogEntry {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        return LogEntry {
            timestamp: None,
            priority: None,
            pid: None,
            identifier: None,
            message: line.to_string(),
            message_styles: Vec::new(),
            boot_id: None,
            invocation_id: None,
            cursor: None,
            unit: None,
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
    let (message, message_styles) = parse_ansi_message(&message);

    let priority = val["PRIORITY"]
        .as_str()
        .and_then(|s| s.parse::<u8>().ok());

    let timestamp = val["__REALTIME_TIMESTAMP"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok());

    let pid = val["_PID"].as_str().map(|s| s.to_string());

    let identifier = val["SYSLOG_IDENTIFIER"].as_str().map(|s| s.to_string());

    let boot_id = val["_BOOT_ID"].as_str().map(|s| s.to_string());

    let invocation_id = val["_SYSTEMD_INVOCATION_ID"].as_str().map(|s| s.to_string());

    let cursor = val["__CURSOR"].as_str().map(|s| s.to_string());

    let unit = val["_SYSTEMD_UNIT"].as_str().map(|s| s.to_string());

    LogEntry {
        timestamp,
        priority,
        pid,
        identifier,
        message,
        message_styles,
        boot_id,
        invocation_id,
        cursor,
        unit,
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

pub fn fetch_units(unit_type: UnitType, user_mode: bool, runner: &dyn CommandRunner) -> Result<Vec<SystemdUnit>, String> {
    // The unit list, detail entries, and file states come from independent
    // systemctl calls; fetch them concurrently so a remote runner (SSH) pays
    // one network round trip instead of three.
    let (units, timer_entries, socket_entries, file_states) = std::thread::scope(|s| {
        let timers = (unit_type == UnitType::Timer)
            .then(|| s.spawn(|| fetch_timer_entries(user_mode, runner)));
        let sockets = (unit_type == UnitType::Socket)
            .then(|| s.spawn(|| fetch_socket_entries(user_mode, runner)));
        let file_states = s.spawn(|| fetch_unit_file_states(unit_type, user_mode, runner));
        let units = fetch_unit_list(unit_type, user_mode, runner);
        (
            units,
            timers.map_or_else(Vec::new, |h| h.join().unwrap_or_default()),
            sockets.map_or_else(Vec::new, |h| h.join().unwrap_or_default()),
            file_states.join().unwrap_or_default(),
        )
    });

    let mut units = units?;
    apply_timer_details(&mut units, &timer_entries);
    apply_socket_details(&mut units, &socket_entries);
    apply_file_states(&mut units, &file_states);
    Ok(units)
}

fn fetch_unit_list(unit_type: UnitType, user_mode: bool, runner: &dyn CommandRunner) -> Result<Vec<SystemdUnit>, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    let type_arg = format!("--type={}", unit_type.systemctl_type());
    args.extend(["list-units", &type_arg, "--all", "--no-pager", "--output=json"]);
    let output = run_systemctl(runner, &args)?;

    if !output.success {
        return Err(format!(
            "systemctl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse JSON: {}", e))
}

#[derive(Deserialize)]
struct TimerEntry {
    unit: String,
    next: u64,
}

fn fetch_timer_entries(user_mode: bool, runner: &dyn CommandRunner) -> Vec<TimerEntry> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["list-timers", "--all", "--no-pager", "--output=json"]);

    let Ok(output) = run_systemctl(runner, &args) else {
        return Vec::new();
    };
    if !output.success {
        return Vec::new();
    }
    serde_json::from_slice(&output.stdout).unwrap_or_default()
}

fn apply_timer_details(units: &mut [SystemdUnit], entries: &[TimerEntry]) {
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

pub fn format_relative_time(target_us: u64) -> String {
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

fn fetch_socket_entries(user_mode: bool, runner: &dyn CommandRunner) -> Vec<SocketEntry> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["list-sockets", "--all", "--no-pager", "--output=json"]);

    let Ok(output) = run_systemctl(runner, &args) else {
        return Vec::new();
    };
    if !output.success {
        return Vec::new();
    }
    serde_json::from_slice(&output.stdout).unwrap_or_default()
}

fn apply_socket_details(units: &mut [SystemdUnit], entries: &[SocketEntry]) {
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

fn fetch_unit_file_states(unit_type: UnitType, user_mode: bool, runner: &dyn CommandRunner) -> HashMap<String, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    let type_arg = format!("--type={}", unit_type.systemctl_type());
    args.extend(["list-unit-files", &type_arg, "--no-pager", "--output=json"]);

    let Ok(output) = run_systemctl(runner, &args) else {
        return HashMap::new();
    };
    if !output.success {
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

fn apply_file_states(units: &mut [SystemdUnit], states: &HashMap<String, String>) {
    for unit in units.iter_mut() {
        if let Some(state) = states.get(&unit.unit) {
            unit.file_state = Some(state.clone());
        }
    }
}

fn parse_timer_specs(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split('}')
        .filter_map(|chunk| {
            let chunk = chunk.trim().trim_start_matches('{').trim();
            if chunk.is_empty() {
                return None;
            }
            let before_semi = chunk.split(';').next().unwrap_or("").trim();
            if before_semi.is_empty() {
                None
            } else {
                Some(before_semi.to_string())
            }
        })
        .collect()
}

pub fn fetch_unit_properties(unit_name: &str, user_mode: bool, runner: &dyn CommandRunner) -> UnitProperties {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["show", unit_name, "--no-pager"]);

    let Ok(output) = run_systemctl(runner, &args) else {
        return UnitProperties::default();
    };
    if !output.success {
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
        active_enter_timestamp: get("ActiveEnterTimestamp"),
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
        timers_calendar: parse_timer_specs(&get("TimersCalendar")),
        timers_monotonic: parse_timer_specs(&get("TimersMonotonic")),
        last_trigger_usec: get("LastTriggerUSec"),
        result: get("Result"),
        next_elapse_realtime: get("NextElapseUSecRealtime"),
        persistent: get("Persistent"),
        accuracy_usec: get("AccuracyUSec"),
        randomized_delay_usec: get("RandomizedDelayUSec"),
        paths: get("Paths"),
        listen: get("Listen"),
        accept: get("Accept"),
        n_connections: get("NConnections"),
        n_accepted: get("NAccepted"),
    }
}

pub fn fetch_unit_file_content(unit: &str, user_mode: bool, runner: &dyn CommandRunner) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    if user_mode {
        args.push("--user");
    }
    args.extend(["cat", unit, "--no-pager"]);

    let output = run_systemctl(runner, &args)?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("systemctl cat failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|l| l.to_string()).collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_unit(sub: &str) -> SystemdUnit {
        SystemdUnit {
            unit: "test.service".into(),
            load: "loaded".into(),
            active: "active".into(),
            sub: sub.into(),
            description: "Test".into(),
            detail: None,
            file_state: None,
        }
    }

    #[test]
    fn test_parse_systemd_version() {
        let output = "systemd 257 (257.13-1~deb13u1)\n+PAM +OPENSSL\n";
        assert_eq!(parse_systemd_version(output), Some(257));
    }

    #[test]
    fn test_parse_systemd_version_with_prefixed_number() {
        assert_eq!(parse_systemd_version("systemd v246\n"), Some(246));
    }

    #[test]
    fn test_parse_systemd_version_invalid() {
        assert_eq!(parse_systemd_version("not systemd\n"), None);
    }

    // shell_quote / join_remote_command

    #[test]
    fn test_shell_quote_plain() {
        assert_eq!(shell_quote("systemctl"), "systemctl");
        assert_eq!(shell_quote("--type=service"), "--type=service");
    }

    #[test]
    fn test_shell_quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn test_shell_quote_spaces() {
        assert_eq!(shell_quote("15 min ago"), "'15 min ago'");
    }

    #[test]
    fn test_shell_quote_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_join_remote_command() {
        assert_eq!(
            join_remote_command("journalctl", &["--since", "15 min ago"]),
            "journalctl --since '15 min ago'"
        );
    }

    // SshRunner argument construction

    #[test]
    fn test_multiplex_args_batch_mode() {
        let args = multiplex_args(std::path::Path::new("/nonexistent/cdir"), true);
        assert!(args.contains(&"ControlPath=/nonexistent/cdir/ctl".to_string()));
        assert!(args.contains(&"ControlMaster=auto".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        // The master's lifetime is tied to the watchdog child, never to a
        // detached daemon that could outlive this process.
        assert!(!args.iter().any(|a| a.starts_with("ControlPersist")));
    }

    #[test]
    fn test_control_dir_candidate_short_base_accepted() {
        assert!(control_dir_candidate(std::path::Path::new("/tmp")).is_some());
    }

    #[test]
    fn test_control_dir_candidate_macos_style_base_accepted() {
        // Typical macOS temp dir (~49 bytes) must fit; the old %C-based path
        // exceeded the 104-byte Unix socket limit here.
        let base = "/var/folders/46/28sdmn0s021cm_l4c_tkx0h80000gn/T";
        let dir = control_dir_candidate(std::path::Path::new(base)).unwrap();
        assert!(control_socket_path(&dir).as_os_str().len() <= MAX_CONTROL_SOCKET_PATH);
    }

    #[test]
    fn test_control_dir_candidate_overlong_base_rejected() {
        let base = format!("/{}", "x".repeat(MAX_CONTROL_SOCKET_PATH));
        assert!(control_dir_candidate(std::path::Path::new(&base)).is_none());
    }

    #[test]
    fn test_multiplex_args_interactive_has_no_batch_mode() {
        let args = multiplex_args(std::path::Path::new("/nonexistent/ctl"), false);
        assert!(!args.contains(&"BatchMode=yes".to_string()));
        assert!(!args.contains(&"LogLevel=ERROR".to_string()));
    }

    #[test]
    fn test_connect_rejects_empty_args() {
        assert!(SshRunner::connect(Vec::new()).is_err());
    }

    // split_ssh_args

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_split_ssh_args_destination_only() {
        assert_eq!(
            split_ssh_args(&strings(&["root@host"])),
            Ok((Vec::new(), "root@host".to_string()))
        );
    }

    #[test]
    fn test_split_ssh_args_options_before_destination() {
        assert_eq!(
            split_ssh_args(&strings(&["-p", "2222", "-i", "key", "root@host"])),
            Ok((strings(&["-p", "2222", "-i", "key"]), "root@host".to_string()))
        );
    }

    #[test]
    fn test_split_ssh_args_rejects_options_after_destination() {
        // In plain ssh, trailing arguments are the remote command — which
        // systemdmgr supplies itself; only one form is accepted.
        let err = split_ssh_args(&strings(&["root@host", "-i", "key"])).unwrap_err();
        assert!(err.contains("unexpected arguments after destination 'root@host'"));
        assert!(err.contains("usage: --ssh [ssh-options] destination"));
    }

    #[test]
    fn test_split_ssh_args_rejects_double_dash() {
        let err = split_ssh_args(&strings(&["--", "root@host"])).unwrap_err();
        assert!(err.contains("'--' is not supported"));
        assert!(split_ssh_args(&strings(&["-i", "key", "--", "root@host"])).is_err());
    }

    #[test]
    fn test_split_ssh_args_attached_option_value() {
        // -p2222: the value is attached to the flag, no extra argument.
        assert_eq!(
            split_ssh_args(&strings(&["-p2222", "host"])),
            Ok((strings(&["-p2222"]), "host".to_string()))
        );
    }

    #[test]
    fn test_split_ssh_args_flag_cluster() {
        // -4A is two no-value flags; host is the destination.
        assert_eq!(
            split_ssh_args(&strings(&["-4A", "host"])),
            Ok((strings(&["-4A"]), "host".to_string()))
        );
    }

    #[test]
    fn test_split_ssh_args_cluster_ending_in_value_flag() {
        // -vp 2222: -v is a flag, -p consumes the next argument.
        assert_eq!(
            split_ssh_args(&strings(&["-vp", "2222", "host"])),
            Ok((strings(&["-vp", "2222"]), "host".to_string()))
        );
    }

    #[test]
    fn test_split_ssh_args_missing_option_value() {
        assert!(split_ssh_args(&strings(&["-i"])).is_err());
    }

    #[test]
    fn test_split_ssh_args_no_destination() {
        assert!(split_ssh_args(&strings(&["-p", "2222"])).is_err());
        assert!(split_ssh_args(&[]).is_err());
    }

    #[test]
    fn test_ssh_destination_helper() {
        assert_eq!(
            ssh_destination(&strings(&["-i", "key", "root@host"])),
            Some("root@host".to_string())
        );
        assert_eq!(ssh_destination(&strings(&["-v"])), None);
    }

    // Phase 2 — UnitType::label

    #[test]
    fn test_unit_type_label_service() {
        assert_eq!(UnitType::Service.label(), "Services");
    }

    #[test]
    fn test_unit_type_label_timer() {
        assert_eq!(UnitType::Timer.label(), "Timers");
    }

    #[test]
    fn test_unit_type_label_socket() {
        assert_eq!(UnitType::Socket.label(), "Sockets");
    }

    #[test]
    fn test_unit_type_label_target() {
        assert_eq!(UnitType::Target.label(), "Targets");
    }

    #[test]
    fn test_unit_type_label_path() {
        assert_eq!(UnitType::Path.label(), "Paths");
    }

    // Phase 2 — UnitType::systemctl_type

    #[test]
    fn test_unit_type_systemctl_type_service() {
        assert_eq!(UnitType::Service.systemctl_type(), "service");
    }

    #[test]
    fn test_unit_type_systemctl_type_timer() {
        assert_eq!(UnitType::Timer.systemctl_type(), "timer");
    }

    #[test]
    fn test_unit_type_systemctl_type_socket() {
        assert_eq!(UnitType::Socket.systemctl_type(), "socket");
    }

    #[test]
    fn test_unit_type_systemctl_type_target() {
        assert_eq!(UnitType::Target.systemctl_type(), "target");
    }

    #[test]
    fn test_unit_type_systemctl_type_path() {
        assert_eq!(UnitType::Path.systemctl_type(), "path");
    }

    // Phase 2 — status_options

    #[test]
    fn test_status_options_service() {
        assert_eq!(
            UnitType::Service.status_options(),
            &["All", "running", "exited", "failed", "dead"]
        );
    }

    #[test]
    fn test_status_options_timer() {
        assert_eq!(
            UnitType::Timer.status_options(),
            &["All", "waiting", "running", "elapsed"]
        );
    }

    #[test]
    fn test_status_options_socket() {
        assert_eq!(
            UnitType::Socket.status_options(),
            &["All", "listening", "running", "failed"]
        );
    }

    #[test]
    fn test_status_options_target() {
        assert_eq!(
            UnitType::Target.status_options(),
            &["All", "active", "inactive"]
        );
    }

    #[test]
    fn test_status_options_path() {
        assert_eq!(
            UnitType::Path.status_options(),
            &["All", "waiting", "running", "failed"]
        );
    }

    #[test]
    fn test_status_options_all_start_with_all() {
        for ut in &UNIT_TYPES {
            assert_eq!(
                ut.status_options()[0],
                "All",
                "{:?} status_options should start with All",
                ut
            );
        }
    }

    #[test]
    fn test_unit_types_count() {
        assert_eq!(UNIT_TYPES.len(), 5);
    }

    // Phase 1 — SystemdUnit methods

    #[test]
    fn test_status_display_returns_sub() {
        let unit = make_unit("running");
        assert_eq!(unit.status_display(), "running");
    }

    #[test]
    fn test_status_color_running() {
        assert_eq!(make_unit("running").status_color(), Color::Green);
    }

    #[test]
    fn test_status_color_exited() {
        assert_eq!(make_unit("exited").status_color(), Color::Yellow);
    }

    #[test]
    fn test_status_color_dead() {
        assert_eq!(make_unit("dead").status_color(), COLOR_MUTED);
    }

    #[test]
    fn test_status_color_stopped() {
        assert_eq!(make_unit("stopped").status_color(), COLOR_MUTED);
    }

    #[test]
    fn test_status_color_failed() {
        assert_eq!(make_unit("failed").status_color(), Color::Red);
    }

    #[test]
    fn test_status_color_waiting() {
        assert_eq!(make_unit("waiting").status_color(), Color::Cyan);
    }

    #[test]
    fn test_status_color_listening() {
        assert_eq!(make_unit("listening").status_color(), Color::Green);
    }

    #[test]
    fn test_status_color_active() {
        assert_eq!(make_unit("active").status_color(), Color::Green);
    }

    #[test]
    fn test_status_color_inactive() {
        assert_eq!(make_unit("inactive").status_color(), COLOR_MUTED);
    }

    #[test]
    fn test_status_color_elapsed() {
        assert_eq!(make_unit("elapsed").status_color(), Color::Yellow);
    }

    #[test]
    fn test_status_color_unknown() {
        assert_eq!(make_unit("something_else").status_color(), Color::White);
    }

    // Phase 3 — priority_label

    #[test]
    fn test_priority_label_0() {
        assert_eq!(priority_label(0), "emerg");
    }

    #[test]
    fn test_priority_label_1() {
        assert_eq!(priority_label(1), "alert");
    }

    #[test]
    fn test_priority_label_2() {
        assert_eq!(priority_label(2), "crit");
    }

    #[test]
    fn test_priority_label_3() {
        assert_eq!(priority_label(3), "err");
    }

    #[test]
    fn test_priority_label_4() {
        assert_eq!(priority_label(4), "warning");
    }

    #[test]
    fn test_priority_label_5() {
        assert_eq!(priority_label(5), "notice");
    }

    #[test]
    fn test_priority_label_6() {
        assert_eq!(priority_label(6), "info");
    }

    #[test]
    fn test_priority_label_7() {
        assert_eq!(priority_label(7), "debug");
    }

    #[test]
    fn test_priority_label_8() {
        assert_eq!(priority_label(8), "unknown");
    }

    #[test]
    fn test_priority_label_255() {
        assert_eq!(priority_label(255), "unknown");
    }

    #[test]
    fn test_priority_labels_count() {
        assert_eq!(PRIORITY_LABELS.len(), 8);
    }

    // Phase 3 — TimeRange

    #[test]
    fn test_time_range_label_all() {
        assert_eq!(TimeRange::All.label(), "All");
    }

    #[test]
    fn test_time_range_label_fifteen_minutes() {
        assert_eq!(TimeRange::FifteenMinutes.label(), "Last 15 minutes");
    }

    #[test]
    fn test_time_range_label_one_hour() {
        assert_eq!(TimeRange::OneHour.label(), "Last 1 hour");
    }

    #[test]
    fn test_time_range_label_one_day() {
        assert_eq!(TimeRange::OneDay.label(), "Last 24 hours");
    }

    #[test]
    fn test_time_range_label_seven_days() {
        assert_eq!(TimeRange::SevenDays.label(), "Last 7 days");
    }

    #[test]
    fn test_time_range_label_today() {
        assert_eq!(TimeRange::Today.label(), "Today");
    }

    #[test]
    fn test_time_range_since_all() {
        assert_eq!(TimeRange::All.journalctl_since(), None);
    }

    #[test]
    fn test_time_range_since_fifteen_minutes() {
        assert_eq!(
            TimeRange::FifteenMinutes.journalctl_since(),
            Some("15 min ago")
        );
    }

    #[test]
    fn test_time_range_since_one_hour() {
        assert_eq!(TimeRange::OneHour.journalctl_since(), Some("1 hour ago"));
    }

    #[test]
    fn test_time_range_since_one_day() {
        assert_eq!(TimeRange::OneDay.journalctl_since(), Some("1 day ago"));
    }

    #[test]
    fn test_time_range_since_seven_days() {
        assert_eq!(
            TimeRange::SevenDays.journalctl_since(),
            Some("7 days ago")
        );
    }

    #[test]
    fn test_time_range_since_today() {
        assert_eq!(TimeRange::Today.journalctl_since(), Some("today"));
    }

    #[test]
    fn test_time_ranges_count() {
        assert_eq!(TIME_RANGES.len(), 6);
    }

    // Phase 3 — parse_journal_json_line

    #[test]
    fn test_parse_complete() {
        let line = r#"{"MESSAGE":"hello world","PRIORITY":"3","__REALTIME_TIMESTAMP":"1700000000000000","_PID":"1234","SYSLOG_IDENTIFIER":"myapp"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, "hello world");
        assert_eq!(entry.priority, Some(3));
        assert_eq!(entry.timestamp, Some(1700000000000000));
        assert_eq!(entry.pid.as_deref(), Some("1234"));
        assert_eq!(entry.identifier.as_deref(), Some("myapp"));
    }

    #[test]
    fn test_parse_missing_optional_fields() {
        let line = r#"{"MESSAGE":"only message"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, "only message");
        assert_eq!(entry.priority, None);
        assert_eq!(entry.timestamp, None);
        assert_eq!(entry.pid, None);
        assert_eq!(entry.identifier, None);
    }

    #[test]
    fn test_parse_byte_array_message() {
        let line = r#"{"MESSAGE":[104,101,108,108,111]}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, "hello");
    }

    // parse_ansi_message

    #[test]
    fn test_ansi_no_escapes_passthrough() {
        let (clean, styles) = parse_ansi_message("plain text");
        assert_eq!(clean, "plain text");
        assert!(styles.is_empty());
    }

    #[test]
    fn test_ansi_tracing_style_line() {
        // The shape emitted by Rust tracing subscribers (e.g. garage).
        let raw = "\x1b[2m2026-07-05\x1b[0m \x1b[32m INFO\x1b[0m rest";
        let (clean, styles) = parse_ansi_message(raw);
        assert_eq!(clean, "2026-07-05  INFO rest");
        assert_eq!(styles.len(), 2);
        assert_eq!(styles[0].0, 0..10);
        assert_eq!(styles[0].1, Style::default().add_modifier(Modifier::DIM));
        assert_eq!(styles[1].0, 11..16);
        assert_eq!(styles[1].1, Style::default().fg(Color::Green));
        assert_eq!(&clean[styles[1].0.clone()], " INFO");
    }

    #[test]
    fn test_ansi_bold_color_combined() {
        let (clean, styles) = parse_ansi_message("\x1b[1;31mX\x1b[0m");
        assert_eq!(clean, "X");
        assert_eq!(
            styles,
            vec![(0..1, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))]
        );
    }

    #[test]
    fn test_ansi_empty_params_resets() {
        let (clean, styles) = parse_ansi_message("\x1b[31ma\x1b[mb");
        assert_eq!(clean, "ab");
        assert_eq!(styles, vec![(0..1, Style::default().fg(Color::Red))]);
    }

    #[test]
    fn test_ansi_indexed_and_rgb_colors() {
        let (_, styles) = parse_ansi_message("\x1b[38;5;196ma\x1b[0m\x1b[38;2;1;2;3mb\x1b[0m");
        assert_eq!(styles[0].1, Style::default().fg(Color::Indexed(196)));
        assert_eq!(styles[1].1, Style::default().fg(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn test_ansi_bright_and_background_colors() {
        let (_, styles) = parse_ansi_message("\x1b[91;44ma\x1b[0m");
        assert_eq!(
            styles,
            vec![(0..1, Style::default().fg(Color::LightRed).bg(Color::Blue))]
        );
    }

    #[test]
    fn test_ansi_non_sgr_csi_stripped() {
        let (clean, styles) = parse_ansi_message("a\x1b[2Kb\x1b[1;5Hc");
        assert_eq!(clean, "abc");
        assert!(styles.is_empty());
    }

    #[test]
    fn test_ansi_osc_stripped() {
        let (clean, styles) =
            parse_ansi_message("a\x1b]0;window title\x07b\x1b]8;;http://x\x1b\\c");
        assert_eq!(clean, "abc");
        assert!(styles.is_empty());
    }

    #[test]
    fn test_ansi_malformed_trailing_escape() {
        assert_eq!(parse_ansi_message("a\x1b").0, "a");
        assert_eq!(parse_ansi_message("a\x1b[").0, "a");
        assert_eq!(parse_ansi_message("a\x1b[3").0, "a");
    }

    #[test]
    fn test_ansi_multibyte_text_ranges() {
        let (clean, styles) = parse_ansi_message("\x1b[32m\u{7e7c}\u{7e8c}\x1b[0m ok");
        assert_eq!(clean, "\u{7e7c}\u{7e8c} ok");
        assert_eq!(styles, vec![(0..6, Style::default().fg(Color::Green))]);
    }

    #[test]
    fn test_parse_byte_array_message_with_ansi() {
        // journald delivers messages containing control bytes as byte arrays;
        // this is ESC [ 3 2 m h i ESC [ 0 m
        let line = r#"{"MESSAGE":[27,91,51,50,109,104,105,27,91,48,109]}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, "hi");
        assert_eq!(
            entry.message_styles,
            vec![(0..2, Style::default().fg(Color::Green))]
        );
    }

    #[test]
    fn test_parse_non_string_non_array_message() {
        let line = r#"{"MESSAGE":42}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, line);
    }

    #[test]
    fn test_parse_invalid_json() {
        let line = "not json at all";
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.message, "not json at all");
        assert_eq!(entry.priority, None);
        assert_eq!(entry.timestamp, None);
        assert_eq!(entry.pid, None);
        assert_eq!(entry.identifier, None);
    }

    #[test]
    fn test_parse_empty_string() {
        let entry = parse_journal_json_line("");
        assert_eq!(entry.message, "");
    }

    #[test]
    fn test_parse_priority_non_numeric() {
        let line = r#"{"MESSAGE":"test","PRIORITY":"abc"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.priority, None);
    }

    #[test]
    fn test_parse_timestamp_non_numeric() {
        let line = r#"{"MESSAGE":"test","__REALTIME_TIMESTAMP":"not_a_number"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.timestamp, None);
    }

    #[test]
    fn test_parse_boot_id() {
        let line = r#"{"MESSAGE":"hello","_BOOT_ID":"abcdef123456"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.boot_id.as_deref(), Some("abcdef123456"));
    }

    #[test]
    fn test_parse_boot_id_missing() {
        let line = r#"{"MESSAGE":"hello"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.boot_id, None);
    }

    #[test]
    fn test_parse_invocation_id_present() {
        let line = r#"{"MESSAGE":"hello","_SYSTEMD_INVOCATION_ID":"0123456789abcdef"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.invocation_id.as_deref(), Some("0123456789abcdef"));
    }

    #[test]
    fn test_parse_invocation_id_missing() {
        let line = r#"{"MESSAGE":"hello"}"#;
        let entry = parse_journal_json_line(line);
        assert_eq!(entry.invocation_id, None);
    }

    // Phase 3 — format_log_timestamp

    #[test]
    fn test_format_log_timestamp_valid() {
        let ts = 1700000000000000_i64; // 2023-11-14
        let result = format_log_timestamp(ts);
        assert!(!result.is_empty());
        // Format is "Mon DD HH:MM:SS" → 15 chars
        assert_eq!(result.len(), 15);
    }

    #[test]
    fn test_format_log_timestamp_zero() {
        let result = format_log_timestamp(0);
        assert!(!result.is_empty());
    }

    // Phase 4 — format_bytes

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_500() {
        assert_eq!(format_bytes(500), "500 B");
    }

    #[test]
    fn test_format_bytes_1024() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn test_format_bytes_1536() {
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_format_bytes_1mb() {
        assert_eq!(format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_format_bytes_1gb() {
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    // Phase 4 — format_cpu_time

    #[test]
    fn test_format_cpu_time_zero() {
        assert_eq!(format_cpu_time(0), "0.000s");
    }

    #[test]
    fn test_format_cpu_time_500ms() {
        assert_eq!(format_cpu_time(500_000_000), "0.500s");
    }

    #[test]
    fn test_format_cpu_time_60s() {
        assert_eq!(format_cpu_time(60_000_000_000), "1.0min");
    }

    #[test]
    fn test_format_cpu_time_90s() {
        assert_eq!(format_cpu_time(90_000_000_000), "1.5min");
    }

    // UnitAction — label

    #[test]
    fn test_unit_action_label_start() {
        assert_eq!(UnitAction::Start.label(), "Start");
    }

    #[test]
    fn test_unit_action_label_stop() {
        assert_eq!(UnitAction::Stop.label(), "Stop");
    }

    #[test]
    fn test_unit_action_label_restart() {
        assert_eq!(UnitAction::Restart.label(), "Restart");
    }

    #[test]
    fn test_unit_action_label_reload() {
        assert_eq!(UnitAction::Reload.label(), "Reload");
    }

    #[test]
    fn test_unit_action_label_enable() {
        assert_eq!(UnitAction::Enable.label(), "Enable");
    }

    #[test]
    fn test_unit_action_label_disable() {
        assert_eq!(UnitAction::Disable.label(), "Disable");
    }

    #[test]
    fn test_unit_action_label_daemon_reload() {
        assert_eq!(UnitAction::DaemonReload.label(), "Daemon Reload");
    }

    // UnitAction — shortcut

    #[test]
    fn test_unit_action_shortcut_start() {
        assert_eq!(UnitAction::Start.shortcut(), 's');
    }

    #[test]
    fn test_unit_action_shortcut_stop() {
        assert_eq!(UnitAction::Stop.shortcut(), 't');
    }

    #[test]
    fn test_unit_action_shortcut_restart() {
        assert_eq!(UnitAction::Restart.shortcut(), 'r');
    }

    #[test]
    fn test_unit_action_shortcut_reload() {
        assert_eq!(UnitAction::Reload.shortcut(), 'l');
    }

    #[test]
    fn test_unit_action_shortcut_enable() {
        assert_eq!(UnitAction::Enable.shortcut(), 'e');
    }

    #[test]
    fn test_unit_action_shortcut_disable() {
        assert_eq!(UnitAction::Disable.shortcut(), 'd');
    }

    #[test]
    fn test_unit_action_shortcut_daemon_reload() {
        assert_eq!(UnitAction::DaemonReload.shortcut(), 'D');
    }

    #[test]
    fn test_unit_action_shortcuts_unique() {
        let actions = [
            UnitAction::Start,
            UnitAction::Stop,
            UnitAction::Restart,
            UnitAction::Reload,
            UnitAction::Enable,
            UnitAction::Disable,
            UnitAction::DaemonReload,
        ];
        let shortcuts: HashSet<char> = actions.iter().map(UnitAction::shortcut).collect();
        assert_eq!(shortcuts.len(), actions.len());
    }

    // UnitAction — systemctl_verb

    #[test]
    fn test_unit_action_verb_start() {
        assert_eq!(UnitAction::Start.systemctl_verb(), "start");
    }

    #[test]
    fn test_unit_action_verb_stop() {
        assert_eq!(UnitAction::Stop.systemctl_verb(), "stop");
    }

    #[test]
    fn test_unit_action_verb_restart() {
        assert_eq!(UnitAction::Restart.systemctl_verb(), "restart");
    }

    #[test]
    fn test_unit_action_verb_reload() {
        assert_eq!(UnitAction::Reload.systemctl_verb(), "reload");
    }

    #[test]
    fn test_unit_action_verb_enable() {
        assert_eq!(UnitAction::Enable.systemctl_verb(), "enable");
    }

    #[test]
    fn test_unit_action_verb_disable() {
        assert_eq!(UnitAction::Disable.systemctl_verb(), "disable");
    }

    #[test]
    fn test_unit_action_verb_daemon_reload() {
        assert_eq!(UnitAction::DaemonReload.systemctl_verb(), "daemon-reload");
    }

    // UnitAction — confirmation_message

    #[test]
    fn test_unit_action_confirm_msg_start() {
        assert_eq!(
            UnitAction::Start.confirmation_message("foo.service"),
            "Start foo.service?"
        );
    }

    #[test]
    fn test_unit_action_confirm_msg_daemon_reload() {
        assert_eq!(
            UnitAction::DaemonReload.confirmation_message("foo.service"),
            "Reload systemd daemon configuration?"
        );
    }

    // UnitAction — available_actions

    #[test]
    fn test_available_actions_running() {
        let actions = UnitAction::available_actions("running", None);
        assert!(actions.contains(&UnitAction::Stop));
        assert!(actions.contains(&UnitAction::Restart));
        assert!(actions.contains(&UnitAction::Reload));
        assert!(!actions.contains(&UnitAction::Start));
        assert!(actions.contains(&UnitAction::DaemonReload));
    }

    #[test]
    fn test_available_actions_dead() {
        let actions = UnitAction::available_actions("dead", None);
        assert!(actions.contains(&UnitAction::Start));
        assert!(!actions.contains(&UnitAction::Stop));
        assert!(actions.contains(&UnitAction::DaemonReload));
    }

    #[test]
    fn test_available_actions_failed() {
        let actions = UnitAction::available_actions("failed", None);
        assert!(actions.contains(&UnitAction::Start));
        assert!(!actions.contains(&UnitAction::Stop));
    }

    #[test]
    fn test_available_actions_unknown_sub_state() {
        let actions = UnitAction::available_actions("something-unknown", None);
        assert!(actions.contains(&UnitAction::Start));
        assert!(actions.contains(&UnitAction::Stop));
        assert!(actions.contains(&UnitAction::DaemonReload));
    }

    #[test]
    fn test_available_actions_enabled_file_state() {
        let actions = UnitAction::available_actions("running", Some("enabled"));
        assert!(actions.contains(&UnitAction::Disable));
        assert!(!actions.contains(&UnitAction::Enable));
    }

    #[test]
    fn test_available_actions_disabled_file_state() {
        let actions = UnitAction::available_actions("dead", Some("disabled"));
        assert!(actions.contains(&UnitAction::Enable));
        assert!(!actions.contains(&UnitAction::Disable));
    }

    #[test]
    fn test_available_actions_static_file_state() {
        let actions = UnitAction::available_actions("running", Some("static"));
        assert!(!actions.contains(&UnitAction::Enable));
        assert!(!actions.contains(&UnitAction::Disable));
    }

    #[test]
    fn test_available_actions_listening() {
        let actions = UnitAction::available_actions("listening", None);
        assert!(actions.contains(&UnitAction::Stop));
        assert!(actions.contains(&UnitAction::Restart));
    }

    #[test]
    fn test_available_actions_waiting() {
        let actions = UnitAction::available_actions("waiting", None);
        assert!(actions.contains(&UnitAction::Stop));
        assert!(actions.contains(&UnitAction::Restart));
    }

    #[test]
    fn test_available_actions_exited() {
        let actions = UnitAction::available_actions("exited", None);
        assert!(actions.contains(&UnitAction::Start));
        assert!(!actions.contains(&UnitAction::Stop));
    }

    #[test]
    fn test_available_actions_always_has_daemon_reload() {
        for sub in &["running", "dead", "failed", "unknown", "listening"] {
            let actions = UnitAction::available_actions(sub, None);
            assert!(
                actions.contains(&UnitAction::DaemonReload),
                "DaemonReload missing for sub_state={}",
                sub
            );
        }
    }

    // Phase 4 — FILE_STATE_OPTIONS

    #[test]
    fn test_file_state_options_contents() {
        assert_eq!(
            FILE_STATE_OPTIONS,
            &["All", "enabled", "disabled", "static", "masked", "indirect"]
        );
    }

    // Phase 4 — UnitProperties::default

    #[test]
    fn test_unit_properties_default() {
        let props = UnitProperties::default();
        assert_eq!(props.fragment_path, "");
        assert_eq!(props.unit_file_state, "");
        assert_eq!(props.active_state, "");
        assert_eq!(props.active_enter_timestamp, "");
        assert_eq!(props.sub_state, "");
        assert_eq!(props.load_state, "");
        assert_eq!(props.description, "");
        assert_eq!(props.main_pid, 0);
        assert_eq!(props.exec_main_start_timestamp, "");
        assert_eq!(props.memory_current, None);
        assert_eq!(props.cpu_usage_nsec, None);
        assert!(props.requires.is_empty());
        assert!(props.wants.is_empty());
        assert!(props.after.is_empty());
        assert!(props.before.is_empty());
        assert!(props.conflicts.is_empty());
        assert!(props.triggered_by.is_empty());
        assert!(props.triggers.is_empty());
        assert!(props.timers_calendar.is_empty());
        assert!(props.timers_monotonic.is_empty());
        assert_eq!(props.last_trigger_usec, "");
        assert_eq!(props.result, "");
        assert_eq!(props.next_elapse_realtime, "");
        assert_eq!(props.persistent, "");
        assert_eq!(props.accuracy_usec, "");
        assert_eq!(props.randomized_delay_usec, "");
        assert_eq!(props.paths, "");
        assert_eq!(props.listen, "");
        assert_eq!(props.accept, "");
        assert_eq!(props.n_connections, "");
        assert_eq!(props.n_accepted, "");
    }

    // parse_timer_specs

    #[test]
    fn test_parse_timer_specs_single_calendar() {
        let input = "{ OnCalendar=*-*-* 06:00:00 ; next_elapse=Sun 2026-02-22 06:00:00 UTC }";
        let result = parse_timer_specs(input);
        assert_eq!(result, vec!["OnCalendar=*-*-* 06:00:00"]);
    }

    #[test]
    fn test_parse_timer_specs_multiple() {
        let input = "{ OnCalendar=*-*-* 06:00:00 ; next_elapse=Sun 2026-02-22 06:00:00 UTC }{ OnCalendar=*-*-* 18:00:00 ; next_elapse=Sun 2026-02-22 18:00:00 UTC }";
        let result = parse_timer_specs(input);
        assert_eq!(result, vec!["OnCalendar=*-*-* 06:00:00", "OnCalendar=*-*-* 18:00:00"]);
    }

    #[test]
    fn test_parse_timer_specs_monotonic() {
        let input = "{ OnBootSec=15min ; next_elapse=n/a }";
        let result = parse_timer_specs(input);
        assert_eq!(result, vec!["OnBootSec=15min"]);
    }

    #[test]
    fn test_parse_timer_specs_empty() {
        let result = parse_timer_specs("");
        assert!(result.is_empty());
    }
}
