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
| List timer units | Yes | No | Phase 2 |
| List socket units | Yes | No | Phase 2 |
| List target units | Yes | No | Phase 2 |
| List path units | Yes | No | Phase 2 |
| **Status & Filtering** | | | |
| Color-coded status display | Yes | Yes | Done |
| Filter by runtime state (running/exited/failed/dead) | Yes | Yes | Done |
| Filter by unit file state (enabled/disabled/static/masked) | Yes | No | Phase 4 |
| Search by name/description | Yes | Yes | Done |
| **Log Viewing** | | | |
| View unit logs | Yes | Yes | Done |
| Search within logs | Yes | Yes | Done |
| Filter by log severity/priority | Yes | No | Phase 3 |
| Filter by time range | Yes | No | Phase 3 |
| Structured log metadata (PID, priority, timestamp) | Yes | No | Phase 3 |
| Real-time log streaming (follow) | Yes | No | Phase 5 |
| **Unit Details** | | | |
| Unit file path display | Yes | No | Phase 4 |
| Unit dependencies (Requires/Wants/After/Before/Conflicts) | Yes | No | Phase 4 |
| Auto-start / enabled state | Yes | No | Phase 4 |
| Runtime properties (PID, memory, CPU) | Yes | No | Phase 4 |

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

### Phase 2: Additional Unit Types

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

### Phase 3: Enhanced Log Viewing

**Goal:** Improve log viewing with severity filtering, time-range filtering, and structured metadata.

**Features:**
- Filter by severity/priority level
- Filter by time range
- Display structured log metadata (timestamp, PID, priority)
- Color-code log lines by severity

**Implementation details:**

**Severity/priority filtering:**

journalctl supports priority filtering with `-p <level>`:
- `0` emerg, `1` alert, `2` crit, `3` err, `4` warning, `5` notice, `6` info, `7` debug
- `-p err` shows err and above (emerg, alert, crit, err)

Add a `log_priority_filter: Option<u8>` field to `App`. Pass to journalctl:
```
journalctl -u <unit> -p <level> -n <lines> --no-pager
```

UI: Add a priority picker popup (reuse `render_status_picker` pattern from `src/ui.rs:371-410`), bound to `p` key. Options: All, emerg, alert, crit, err, warning, notice, info, debug.

**Time-range filtering:**

journalctl supports `--since` and `--until` with timestamps:
```
journalctl -u <unit> --since "2024-01-01 00:00:00" --until "2024-01-02 00:00:00"
```

Predefined presets are simpler to implement in a TUI than free-form date input:
- Last 15 minutes: `--since "15 min ago"`
- Last 1 hour: `--since "1 hour ago"`
- Last 24 hours: `--since "1 day ago"`
- Last 7 days: `--since "7 days ago"`
- Today: `--since today`
- All time: (no flag)

Add a time range picker popup, bound to a key (e.g., `T`).

**Structured log output:**

Switch from plain text to JSON output for richer metadata:
```
journalctl -u <unit> -n <lines> --no-pager --output=json
```

Each JSON line contains fields like:
- `MESSAGE` — the log message
- `PRIORITY` — severity level (0-7)
- `_PID` — process ID
- `__REALTIME_TIMESTAMP` — microsecond timestamp
- `SYSLOG_IDENTIFIER` — program name

Parse into a `LogEntry` struct:
```rust
pub struct LogEntry {
    pub message: String,
    pub priority: u8,
    pub pid: Option<String>,
    pub timestamp: u64,       // microseconds since epoch
    pub identifier: Option<String>,
}
```

**Color-coding by severity** — extend the log rendering in `src/ui.rs:191-200`:
- emerg/alert/crit: Red, bold
- err: Red
- warning: Yellow
- notice: Cyan
- info: White (default)
- debug: DarkGray

---

### Phase 4: Unit Details & Metadata

**Goal:** Show detailed read-only information about a selected unit, including its file path, dependencies, enabled state, and runtime properties.

**Features:**
- Display unit file path
- Show unit dependencies (Requires, Wants, After, Before, Conflicts)
- Show unit file state (enabled, disabled, static, masked)
- Show runtime properties (PID, memory, CPU time)
- Filter units by unit file state (enabled/disabled/static/masked)
- Dependency tree view

**Implementation details:**

**Fetching unit properties:**

Use `systemctl show <unit>` to retrieve all properties as key-value pairs:
```
systemctl show nginx.service --no-pager
```

This outputs lines like `Key=Value`. Parse into a `HashMap<String, String>`.

Key properties to extract and display:

| Property | Description | Example |
|----------|-------------|---------|
| `FragmentPath` | Unit file location | `/lib/systemd/system/nginx.service` |
| `UnitFileState` | Enabled state | `enabled`, `disabled`, `static`, `masked` |
| `ActiveState` | Current active state | `active`, `inactive`, `failed` |
| `SubState` | Detailed sub-state | `running`, `dead`, `exited` |
| `Description` | Unit description | `A high performance web server` |
| `MainPID` | Main process ID | `1234` |
| `MemoryCurrent` | Current memory usage (bytes) | `12345678` |
| `CPUUsageNSec` | CPU time (nanoseconds) | `987654321` |
| `Requires` | Hard dependencies | `sysinit.target system.slice` |
| `Wants` | Soft dependencies | `network-online.target` |
| `After` | Ordering (start after) | `network.target remote-fs.target` |
| `Before` | Ordering (start before) | `multi-user.target` |
| `Conflicts` | Conflicting units | `shutdown.target` |
| `TriggeredBy` | Units that trigger this one | `nginx.socket` |
| `Triggers` | Units this one triggers | (for timers/paths) |
| `LoadState` | Load state | `loaded`, `not-found`, `error` |

Add a `fetch_unit_properties(unit: &str, scope: UnitScope) -> HashMap<String, String>` function to `src/service.rs`.

For user units: `systemctl --user show <unit> --no-pager`.

**Details panel UI:**

Two approaches:

*Option A: Details modal* — reuse the `render_help` centered popup pattern (`src/ui.rs:307-369`). Toggle with `i` or `Enter`. Shows a scrollable list of properties grouped by section:

```
┌─ Unit Details: nginx.service ──────────────┐
│                                             │
│ General                                     │
│   File:    /lib/systemd/system/nginx.service│
│   State:   enabled                          │
│   Active:  active (running)                 │
│   PID:     1234                             │
│                                             │
│ Dependencies                                │
│   Requires: sysinit.target system.slice     │
│   Wants:    network-online.target           │
│   After:    network.target remote-fs.target │
│   Before:   multi-user.target               │
│   Conflicts: shutdown.target                │
│                                             │
│ Resources                                   │
│   Memory:  11.8 MB                          │
│   CPU:     0.98s                            │
└─────────────────────────────────────────────┘
```

*Option B: Details panel* — add a third panel in the layout. This is more complex as it changes the layout structure. The modal approach (Option A) is simpler and consistent with existing patterns.

**Unit file state filter:**

Extend `STATUS_OPTIONS` (`src/app.rs:5`) to include unit file state filtering. This could be a separate filter dimension (in addition to runtime status):

Add a `file_state_filter: Option<String>` field to `App` with options: All, enabled, disabled, static, masked.

This requires fetching `UnitFileState` for each unit. Two approaches:
1. **Batch query:** `systemctl list-unit-files --type=service --output=json` returns unit name + state. Cross-reference with the unit list.
2. **Per-unit query:** fetch properties for each unit on demand (slower, but avoids a separate command).

The batch approach is more efficient:
```
systemctl list-unit-files --type=service --no-pager --output=json
```
Returns JSON with `unit_file` and `state` fields. Merge this into the `SystemdUnit` struct as `file_state: Option<String>`.

**Dependency tree:**

For a simple dependency tree view, recursively query dependencies:
1. Get `Requires` and `Wants` from `systemctl show <unit>`
2. For each dependency, fetch its state
3. Display as an indented tree:

```
nginx.service (running)
├── sysinit.target (active)
├── system.slice (active)
└── network-online.target (active)
    └── network.target (active)
```

Limit depth to avoid circular dependencies (systemd allows cycles via `After`/`Before` ordering). Use a visited set to prevent infinite loops.

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
