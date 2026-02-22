# Cockpit Feature Parity Roadmap (Read-Only)

This document outlines the steps needed for systemdview to reach read-only feature parity with [Cockpit's](https://cockpit-project.org/) systemd service management interface. The scope is limited to **viewing, browsing, and inspecting** systemd units — write operations (start/stop, enable/disable, unit creation) are excluded.

## Current State

systemdview (v0.0.1-alpha) is a terminal UI for browsing systemd services, built with Rust using [ratatui](https://ratatui.rs/) for rendering and [crossterm](https://docs.rs/crossterm/) for terminal control.

### What's Implemented

**Service discovery and display:**
- Lists all systemd services by shelling out to `systemctl`:
  ```
  systemctl list-units --type=service --all --no-pager --output=json
  ```
- Parses JSON output into `SystemdService` structs (`src/service.rs:6-14`) with fields: `unit`, `load`, `active`, `sub`, `description`
- Color-coded status based on the `sub` field (`src/service.rs:22-29`): green=running, yellow=exited, dark gray=dead/stopped, red=failed

**Filtering and search:**
- Status filtering via popup picker with options: All, running, exited, failed, dead (`src/app.rs:5`)
- Case-insensitive search across unit name and description (`src/app.rs:72-105`)
- Combined search + status filter with match count display

**Log viewing:**
- Fetches last N log lines via `journalctl -u <unit> -n <lines> --no-pager` (`src/service.rs:32-42`)
- Scrollable log panel (right 60% of screen when visible)
- In-log text search with match highlighting and next/prev navigation (`src/ui.rs:258-305`)
- Auto-scroll to most recent logs on load

**UI layout** (`src/ui.rs:43-256`):
```
┌─────────────────────────────────────────┐
│  Header (title / search bar / filter)   │  3 rows
├───────────────────┬─────────────────────┤
│  Service List     │  Logs Panel         │  Dynamic
│  (full or 40%)    │  (60%, optional)    │
├───────────────────┴─────────────────────┤
│  Footer (context-sensitive keybindings) │  3 rows
└─────────────────────────────────────────┘
```

**Modals:**
- Help overlay (`src/ui.rs:307-369`) — centered 50%x70% popup with keybinding reference
- Status picker overlay (`src/ui.rs:371-410`) — fixed-size centered popup for status filter selection

**Input:**
- Vim-style keybindings (j/k, g/G, /, n/N, Ctrl+u/d)
- Mouse support (click-to-select, scroll wheel)
- Manual refresh (`r` key)

### What's Missing (Read-Only Features)

- Only service unit type (no timers, sockets, targets, paths)
- No real-time log streaming (static fetch only)
- No log severity or time-range filtering
- No unit detail inspection (file path, dependencies, properties, runtime metrics)
- No filter by unit file state (enabled/disabled/static/masked)

## Feature Comparison (Read-Only Only)

| Feature | Cockpit | systemdview | Status |
|---------|---------|-------------|--------|
| **Listing & Browsing** | | | |
| List system services | Yes | Yes | Done |
| List user services | Yes | Yes | Done |
| List timer units | Yes | Yes | Done |
| List socket units | Yes | Yes | Done |
| List target units | Yes | Yes | Done |
| List path units | Yes | Yes | Done |
| **Status & Filtering** | | | |
| Color-coded status display | Yes | Yes | Done |
| Filter by runtime state (running/exited/failed/dead) | Yes | Yes | Done |
| Filter by unit file state (enabled/disabled/static/masked) | Yes | Yes | Done |
| Search by name/description | Yes | Yes | Done |
| **Log Viewing** | | | |
| View unit logs | Yes | Yes | Done |
| Search within logs | Yes | Yes | Done |
| Filter by log severity/priority | Yes | Yes | Done |
| Filter by time range | Yes | Yes | Done |
| Structured log metadata (PID, priority, timestamp) | Yes | Yes | Done |
| Real-time log streaming (follow) | Yes | No | Phase 5 |
| **Unit Details** | | | |
| Unit file path display | Yes | Yes | Done |
| Unit dependencies (Requires/Wants/After/Before/Conflicts) | Yes | Yes | Done |
| Auto-start / enabled state | Yes | Yes | Done |
| Runtime properties (PID, memory, CPU) | Yes | Yes | Done |

## Roadmap

### Phase 1: User/System Unit Toggle — Done

**Goal:** Allow switching between system-level and user-level systemd units.

**Features:**
- Toggle between system and user unit scope via `u` keybinding
- Display current scope (`[System]` / `[User]`) in the header
- Fetch user-level logs correctly using `journalctl --user-unit`
- Persist scope across refreshes within a session

**What was implemented:**
- Added `user_mode: bool` field to `App` struct (`src/app.rs`)
- `fetch_services(user_mode)` passes `--user` to systemctl when in user mode (`src/service.rs`)
- `fetch_logs(unit, lines, user_mode)` uses `--user-unit` instead of `-u` for journalctl (`src/service.rs`)
- `toggle_user_mode()` method resets log cache and reloads services (`src/app.rs`)
- `u` keybinding works in both service normal mode and log focus mode (`src/main.rs`)
- Header shows `SystemD Services [System]` or `SystemD Services [User]` (`src/ui.rs`)
- Footer hints and help overlay updated with `u: User/System`

---

### Phase 2: Additional Unit Types — Done

**Goal:** Extend beyond services to browse timers, sockets, targets, and paths — matching Cockpit's tabbed unit type interface.

**Features:**
- View timer units with next/last trigger times
- View socket units with listening addresses
- View target units
- View path units
- Tab bar or type picker to switch between unit types

**Implementation details:**

**Data model changes** (`src/service.rs`):

Rename `SystemdService` to `SystemdUnit` and add type-specific optional fields:
```rust
pub enum UnitType { Service, Timer, Socket, Target, Path }

pub struct SystemdUnit {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
    pub unit_type: UnitType,
    // Timer-specific (from systemctl list-timers --output=json)
    pub next_trigger: Option<String>,   // NEXT field
    pub last_trigger: Option<String>,   // LAST field
    // Socket-specific (from systemctl list-sockets --output=json)
    pub listen: Option<String>,         // LISTEN field
}
```

**Fetching by type** — add a `fetch_units(unit_type, scope)` function:
- Services: `systemctl list-units --type=service --all --no-pager --output=json`
- Timers: `systemctl list-timers --all --no-pager --output=json` (returns different JSON fields: `next`, `left`, `last`, `passed`, `unit`, `activates`)
- Sockets: `systemctl list-sockets --all --no-pager --output=json` (returns: `listen`, `type`, `unit`, `activates`)
- Targets: `systemctl list-units --type=target --all --no-pager --output=json`
- Paths: `systemctl list-units --type=path --all --no-pager --output=json`

Note: `list-timers` and `list-sockets` return different JSON schemas than `list-units`. Each needs its own deserialization struct or the fields must be `Option<String>`.

**UI changes:**

Add a type selector — two approaches:

*Option A: Tab bar* — render a horizontal tab bar at the top:
```
 Services | Timers | Sockets | Targets | Paths
```
Use `Tab`/`Shift+Tab` or `1-5` number keys to switch. Requires adding a row to the vertical layout in `src/ui.rs:49-54`.

*Option B: Type picker popup* — reuse the existing `render_status_picker` pattern (`src/ui.rs:371-410`). Bind to a key (e.g., `t`) to open a popup with unit type options. This is the simpler approach and consistent with the existing status filter picker.

**Display customization per type:**
- Timers: show `[next_trigger] unit_name` instead of `[status] unit_name`
- Sockets: show `[listen_address] unit_name`
- Targets/Paths: show `[status] unit_name` (same as services)

Adapt `App.update_filter()` (`src/app.rs:72-105`) — the status filter options differ by type:
- Services: running, exited, failed, dead
- Timers: waiting, running, elapsed
- Sockets: listening, running, failed
- Targets: active, inactive
- Paths: waiting, running, failed

---

### Phase 3: Enhanced Log Viewing — Done

**Goal:** Improve log viewing with severity filtering, time-range filtering, and structured metadata.

**Features:**
- Filter by severity/priority level
- Filter by time range
- Display structured log metadata (timestamp, PID, priority)
- Color-code log lines by severity

**What was implemented:**
- Switched journalctl from plain text to `--output=json` for structured log entries (`src/service.rs`)
- Added `LogEntry` struct with `timestamp`, `priority`, `pid`, `identifier`, `message` fields
- Added `fetch_log_entries()` replacing `fetch_logs()`, passing `-p` and `--since` flags to journalctl
- Added `parse_journal_json_line()` to parse JSON lines (handles string and byte-array MESSAGE variants)
- Added `TimeRange` enum (All, 15min, 1h, 24h, 7d, Today) with `journalctl_since()` method
- Added priority picker popup (`p` key) with All + 8 severity levels (emerg through debug)
- Added time range picker popup (`T` key) with 6 preset time ranges
- Log lines color-coded by severity: red+bold for emerg/alert/crit, red for err, yellow for warning, cyan for notice, white for info, gray for debug
- Each log line shows: timestamp (local time), priority label, identifier/PID, and message
- Active filters shown in log panel title (e.g. `[p:err] [t:Last 1 hour]`)
- Log search (`/`) searches within message text with severity-colored highlighting
- Filters reset when switching user/system mode or unit type
- Added `chrono` dependency for timestamp formatting

---

### Phase 4: Unit Details & Metadata — Done

**Goal:** Show detailed read-only information about a selected unit, including its file path, dependencies, enabled state, and runtime properties.

**Features:**
- Display unit file path
- Show unit dependencies (Requires, Wants, After, Before, Conflicts, TriggeredBy, Triggers)
- Show unit file state (enabled, disabled, static, masked)
- Show runtime properties (PID, memory, CPU time)
- Filter units by unit file state (enabled/disabled/static/masked/indirect)

**What was implemented:**
- Added `file_state: Option<String>` field to `SystemdUnit`, populated via batch `systemctl list-unit-files --output=json` (`src/service.rs`)
- Added `UnitProperties` struct with all unit detail fields (path, states, PID, timestamps, memory, CPU, dependencies)
- Added `fetch_unit_properties()` parsing `systemctl show <unit>` key=value output into `UnitProperties` (`src/service.rs`)
- Added `fetch_unit_file_states()` and `merge_file_states()` to batch-fetch and merge file states into unit list
- Added `format_bytes()` and `format_cpu_time()` formatting helpers for human-readable resource display
- Added scrollable details modal (`i`/`Enter` to open, `Esc`/`i`/`Enter` to close) with sections: General, Process, Resources, Dependencies (`src/ui.rs`)
- Details modal uses `centered_rect(70, 80)` with scroll indicator `[1-20/35]` in title
- Process section hidden when PID=0, Resources section hidden when no memory/CPU data
- Dependencies displayed compact (comma-joined) when short, expanded (one per line) when long
- Added file state filter picker (`f` key) with options: All, enabled, disabled, static, masked, indirect (`src/ui.rs`)
- File state badges shown in unit list with color coding: green=enabled, yellow=disabled, gray=static, red=masked, cyan=indirect
- Properties cached per session (cleared on refresh, mode switch, or type switch)
- Updated footer and help overlay with `i`, `f` keybindings

---

### Phase 5: Real-Time Log Streaming

**Goal:** Add live log streaming so users can watch logs update in real time, like `journalctl -f`.

**Features:**
- Real-time log streaming (follow mode, like `journalctl -f`)
- Toggle follow mode on/off

**Implementation details:**

Currently `fetch_logs()` runs a one-shot `journalctl` command and collects all output. For real-time streaming:

- Spawn `journalctl -f -u <unit> --no-pager` as a child process using `std::process::Command::spawn()` instead of `.output()`
- Read stdout line-by-line from a background thread (or using non-blocking I/O)
- Append new lines to `app.logs` and auto-scroll if the user is at the bottom
- Add a `follow_mode: bool` field to `App` — toggled with `f` key
- When follow mode is off, fall back to the current one-shot fetch behavior
- Kill the child process when switching units or exiting follow mode

---

## Out of Scope

These features are **excluded** from this read-only roadmap:

**Write/mutating operations:**
- Start, stop, restart, reload services
- Enable, disable, mask, unmask services
- Create new timer or service units
- Edit unit files
- `systemctl daemon-reload`
- PolicyKit / privilege escalation

**Non-unit-management features:**
- Web-based remote access (Cockpit is a web server; systemdview is a local TUI)
- Hostname configuration (`hostnamed`)
- Time/timezone management (`timedated`)
- NTP server management
- Multi-host management
- Certificate management
- User account management

## Future Considerations

**D-Bus integration:** The [`zbus`](https://crates.io/crates/zbus) crate (pure Rust, async) can replace CLI subprocess calls for read-only operations. Benefits:
- Lower latency than spawning `systemctl` processes
- Real-time state change notifications via `PropertiesChanged` D-Bus signals (subscribe to `org.freedesktop.systemd1.Manager` on the system bus)
- Structured data without JSON parsing
- For user units: connect to the session bus instead of the system bus

This is optional since all read-only features can be implemented with CLI commands, but D-Bus becomes more valuable as the feature set grows (especially for real-time streaming and property watching).
