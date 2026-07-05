# systemdmgr Specification

## Overview

systemdmgr is a terminal UI (TUI) for browsing, inspecting, and managing systemd units. It provides access to unit listings, focused per-unit and system-wide logs, detailed properties, read-only unit file content, and basic unit management actions (start/stop/restart/reload, enable/disable, daemon-reload).

**Tech stack:**
- Language: Rust
- TUI framework: [ratatui](https://ratatui.rs/)
- Terminal backend: [crossterm](https://docs.rs/crossterm/)
- Data source: `systemctl` and `journalctl` CLI commands (JSON output where available)
- Minimum systemd version: 246

## Architecture

```
src/
  main.rs      — entry point, terminal setup, event loop, mouse handling
  app.rs       — application state (App struct), navigation, filtering, picker logic
  service.rs   — data types (SystemdUnit, LogEntry, UnitProperties), CLI fetching, parsing
  ui.rs        — rendering (layout, widgets, modals, color helpers)
```

**Data flow:** `systemctl`/`journalctl` CLI → JSON parsing → `App` state → ratatui rendering

### Remote Management (SSH)

- Enabled via `--ssh user@server` CLI flag
- Delegates connectivity to the system OpenSSH client (`ssh` on `PATH`) — no bundled SSH library
- An interactive ControlMaster connection is opened on startup; each command runs as an `ssh` subprocess multiplexed over the master socket (`BatchMode=yes`)
- Full `~/.ssh/config` semantics, authentication methods (agent, passphrase-protected keys, password, OTP/MFA), host key handling, and jump hosts — all handled by ssh itself
- Supports `--ssh-identity-file`, forwarded to ssh as `-i`
- Remote target must have systemd 246+ with `systemctl` on `PATH`
- Both system and user (`--user`) mode supported over SSH
- Header displays remote host (e.g., `"SystemD Services [System] on user@server"`)
- Master connection closed (`ssh -O exit`) via `Drop` on normal exit
- See [ssh.md](ssh.md) for full details

## UI Layout

```
┌──────────────────────────────────────────────┐
│  Header (title / search bar / filter)        │  3 rows
├──────────────────────────────────────────────┤
│  Primary pane: unit list, logs, or unit file  │  Dynamic
├──────────────────────────────────────────────┤
│  Footer (context-sensitive keybindings)      │  3 rows
└──────────────────────────────────────────────┘
```

- Header shows: app title with scope label, active search query, active filter summary with match count, status messages, or the current focused view
- The middle area shows one focused view at a time: unit list, logs, or unit file content
- Logs and unit file views replace the unit list until closed
- Footer keybindings change based on current mode (unit list, search, logs, log search, unit file, unit file search)

## Features

### Unit Browsing

**Supported unit types** (5):

| Type | systemctl flag | Extra data source |
|------|---------------|-------------------|
| Service | `--type=service` | — |
| Timer | `--type=timer` | `list-timers` (next trigger time) |
| Socket | `--type=socket` | `list-sockets` (listen address) |
| Target | `--type=target` | — |
| Path | `--type=path` | — |

- Units fetched via `systemctl list-units --type=<type> --all --no-pager --output=json`
- Type picker popup opened with `t` key to switch between types
- Switching type clears all filters, search, logs, and property cache
- Timer units show next trigger time as relative duration (e.g., "2h 30m")
- Socket units show listen address
- File state badges displayed per unit (fetched via `systemctl list-unit-files --output=json`):
  - Green: enabled
  - Yellow: disabled
  - Dark gray: static
  - Red: masked
  - Cyan: indirect

### System/User Scope

- Toggle between system and user unit scope via `u` key
- System mode: `systemctl` (default) / per-unit logs via `journalctl -u`
- User mode: `systemctl --user` / per-unit logs via `journalctl --user-unit`
- Header displays `[System]` or `[User]`
- Switching scope clears: logs, log search, priority filter, time range, property cache, file state filter

### Filtering & Search

**Text search** (`/` key):
- Case-insensitive search across unit name and description
- Results update live as you type
- Filtered count shown in header

**Status filter** (`s` key):
- Popup picker with status options that vary by unit type:
  - Service: All, running, exited, failed, dead
  - Timer: All, waiting, running, elapsed
  - Socket: All, listening, running, failed
  - Target: All, active, inactive
  - Path: All, waiting, running, failed

**File state filter** (`f` key):
- Popup picker: All, enabled, disabled, static, masked, indirect

**Combined filtering:**
- All three filters (search, status, file state) can be active simultaneously
- Match count displayed in header
- `Esc` clears the text search when one is active; status and file state filters are reset by choosing `All` in their pickers

### Status Colors

| Status | Color |
|--------|-------|
| running | Green |
| listening | Green |
| active | Green |
| exited | Yellow |
| elapsed | Yellow |
| dead | Dark gray |
| stopped | Dark gray |
| inactive | Dark gray |
| failed | Red |
| waiting | Cyan |
| other | White |

### Log Viewing

- Toggled with `l` key for the selected unit; opens a focused full-screen logs view
- `L` opens system-wide logs with no unit filter
- Fetches last 1000 log entries via `journalctl --output=json`
- Auto-scrolls to most recent entry on load
- Per-unit logs load for the selected unit when the logs view opens; logs reload when filters are marked dirty
- Live tail is enabled by default and refreshes from the last journal cursor every 500ms when not paused; `f` pauses/resumes live tail
- When paused, arrows move a selected log entry. In system-wide logs, `Enter` opens that entry's unit if it is present in the current unit list.

**Structured log display** — each line shows:
1. Timestamp (local time, format: `Mon DD HH:MM:SS`)
2. Priority label in brackets (e.g., `[err]`)
3. Identifier/PID (e.g., `(sshd/1234):`)
4. Message text

**Byte-array messages:** journalctl sometimes returns `MESSAGE` as a byte array instead of a string — handled via UTF-8 lossy conversion.

**Boundaries:** boot ID changes render a boot separator; per-unit invocation ID changes render a restart separator.

**Priority filter** (`p` key):
- Popup picker: All + 8 levels (emerg, alert, crit, err, warning, notice, info, debug)
- Passes `-p <level>` to journalctl

**Time range filter** (`t` key in logs, `T` key from the unit list):
- Popup picker: All, Last 15 minutes, Last 1 hour, Last 24 hours, Last 7 days, Today
- Passes `--since <value>` to journalctl

**Severity color coding:**

| Priority | Color | Bold |
|----------|-------|------|
| 0-2 (emerg/alert/crit) | Red | Yes |
| 3 (err) | Red | No |
| 4 (warning) | Yellow | No |
| 5 (notice) | Cyan | No |
| 6 (info) | White | No |
| 7 (debug) | Dark gray | No |

**Log search** (`/` in log focus mode):
- Case-insensitive search within log message text
- Match highlighting: current match = yellow bg/black fg, other matches = dark gray bg/yellow fg
- `n`/`N` to navigate next/previous match (wraps around)
- Auto-scrolls to keep current match visible

### Unit Details Modal

- Opened with `i` or `Enter`, closed with `Esc`/`i`/`Enter`
- Scrollable (arrows, g/G, PgUp/PgDn)
- Scroll position indicator in title: `[1-20/35]`
- Centered at 70% width, 80% height of terminal

**Data source:** `systemctl show <unit> --no-pager` (key=value output parsed into `UnitProperties`)

**Sections:**

| Section | Fields | Visibility |
|---------|--------|------------|
| General | Name, Status, Enabled State (color-coded), Load State, Description, Active State, Active Since, Unit File path | Always |
| Timer | Schedule, Next Trigger, Last Trigger, Result, Persistent, Accuracy, Random Delay | `.timer` units when data is available |
| Socket | Listen, Accept, Accepted, Connected, Triggers | `.socket` units when data is available |
| Path | Watch, Triggers | `.path` units when data is available |
| Process | Main PID, Start Timestamp | Only when PID > 0 |
| Resources | Memory (formatted), CPU Time (formatted) | Only when data available |
| Dependencies | Requires, Wants, After, Before, Conflicts, TriggeredBy, Triggers | Only when any present |

**Formatting helpers:**
- `format_bytes()`: 0 → "0 B", 1024 → "1.0 KB", 1048576 → "1.0 MB", etc.
- `format_cpu_time()`: nanoseconds → "0.500s" or "1.5min"

**Caching:** Properties cached per unit name per session. Cache cleared on refresh, scope switch, or type switch.

### Unit File Viewer

- Opened with `v` from the unit list
- Fetches read-only unit content via `systemctl [--user] cat <unit> --no-pager`
- Replaces the unit list with a focused full-screen unit file view until closed
- Searchable with `/`; matches are highlighted and navigable with `n`/`N`
- Navigation keys: arrows, `g`/`G`, `Home`/`End`, `PgUp`/`PgDn`, `Ctrl+u`/`Ctrl+d`
- Closed with `v`, `Esc`, or `q`

### Unit Actions

- Opened with `x` key — shows action picker popup with context-sensitive actions
- Available actions depend on current unit state:
  - Running/active/listening/waiting: Stop, Restart, Reload
  - Dead/failed/inactive/exited: Start
  - Unknown states: Start, Stop
- Enable/Disable shown based on file state (enabled → Disable, disabled → Enable; static/masked/indirect → neither)
- Daemon Reload always available
- `R` key provides direct daemon-reload shortcut (skips action picker)
- All actions require confirmation via `[Y]/[N/Esc]` dialog before execution
- Executes via `systemctl [--user] <verb> [unit_name]`
- On success: status message shown in header (green), unit list refreshed
- On failure: error message shown, unit list refreshed
- Status message clears on next key press

**Action picker colors:**

| Action | Color |
|--------|-------|
| Start | Green |
| Stop | Red |
| Restart | Yellow |
| Reload | Cyan |
| Enable | Green |
| Disable | Yellow |
| Daemon Reload | Magenta |

### Input

**Keybindings:**

| Key | Action |
|-----|--------|
| `Down` | Move down / scroll |
| `Up` | Move up / scroll |
| `g`/`Home` | Go to top |
| `G`/`End` | Go to bottom |
| `PgUp`/`PgDn` | Page up/down |
| `Ctrl+u`/`Ctrl+d` | Half-page scroll (logs or unit file) |
| `/` | Start search in the current view |
| `n`/`N` | Next/prev search match (logs or unit file) |
| `s` | Status filter picker |
| `f` | File state filter picker (unit list) / pause-resume live tail (logs) |
| `t` | Unit type picker (unit list) / time range filter picker (logs) |
| `p` | Priority filter picker |
| `T` | Time range filter picker (unit list) |
| `i`/`Enter` | Open unit details from the unit list |
| `Enter` | Open selected unit from paused system-wide logs |
| `v` | Open/close unit file view |
| `x` | Open unit action picker |
| `R` | Daemon reload (direct confirm) |
| `l` | Open/close selected unit logs |
| `L` | Toggle system-wide logs |
| `u` | Toggle user/system scope |
| `r` | Refresh units |
| `?` | Toggle help overlay |
| `q`/`Esc` | Quit, clear active search, or exit focused view depending on context |

**Mouse support:**
- Left click to select unit in list
- Scroll wheel to navigate the unit list or scroll logs
- In logs, left click pauses live tail and selects a log entry; re-clicking a selected system-wide log entry navigates to its unit when available

**Modal overlays** block all other input until closed — status picker, type picker, priority picker, time picker, file state picker, action picker, confirmation dialog, details modal, help overlay. Logs and unit file content are focused views with their own keymaps, not overlays.

## Feature Matrix

| Feature | Cockpit | systemdmgr |
|---------|---------|-------------|
| **Listing & Browsing** | | |
| List system services | Yes | Yes |
| List user services | Yes | Yes |
| List timer units | Yes | Yes |
| List socket units | Yes | Yes |
| List target units | Yes | Yes |
| List path units | Yes | Yes |
| **Status & Filtering** | | |
| Color-coded status display | Yes | Yes |
| Filter by runtime state | Yes | Yes |
| Filter by unit file state | Yes | Yes |
| Search by name/description | Yes | Yes |
| **Log Viewing** | | |
| View unit logs | Yes | Yes |
| View system-wide logs | Yes | Yes |
| Search within logs | Yes | Yes |
| Filter by log severity/priority | Yes | Yes |
| Filter by time range | Yes | Yes |
| Structured log metadata | Yes | Yes |
| **Unit Management** | | |
| Start / Stop / Restart / Reload | Yes | Yes |
| Enable / Disable | Yes | Yes |
| Daemon reload | Yes | Yes |
| **Remote Management** | | |
| SSH remote management | No (web-based) | Yes |
| **Unit Details** | | |
| Unit file path display | Yes | Yes |
| Unit file content display | Yes | Yes |
| Unit dependencies | Yes | Yes |
| Auto-start / enabled state | Yes | Yes |
| Runtime properties (PID, memory, CPU) | Yes | Yes |
