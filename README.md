# systemdmgr

A terminal UI for managing and browsing systemd units.

![Rust](https://img.shields.io/badge/rust-stable-orange)

> [!WARNING]
> This project is a work in progress and is currently a proof of concept. Features are still incomplete and subject to change.

## Features

- Browse systemd units (services, sockets, timers, paths, targets) with status indicators
- Search units by name or description
- Filter by status, file state, and unit type via picker dialogs
- View unit details and properties
- Perform unit actions (start, stop, restart, enable, disable, reload, daemon-reload)
- View unit logs in a side panel with search, priority filter, and time range filter
- Live tail mode for real-time log monitoring
- Toggle between user and system units
- Mouse support (click to select, scroll to navigate)
- Vim-style keyboard navigation

## Installation

### Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | sudo bash
```

To install a prerelease version:

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | sudo bash -s -- --prerelease
```

### From Source

```bash
cargo install --path .
```

## Usage

```bash
systemdmgr
```

## Keyboard Shortcuts

Press `?` in the app to see context-sensitive help.

### Unit List

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Page up / down |
| `/` | Search units |
| `s` | Status filter picker |
| `f` | File state filter picker |
| `t` | Unit type picker |
| `i` / `Enter` | Open unit details |
| `x` | Action picker (start/stop/restart/etc.) |
| `R` | Daemon reload |
| `l` | Open logs |
| `r` | Refresh units |
| `u` | Toggle user/system units |
| `Esc` | Clear search or quit |
| `q` | Quit |
| `?` | Toggle help |

### Logs Panel

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Page scroll |
| `Ctrl+u` / `Ctrl+d` | Half page scroll |
| `/` | Search logs |
| `n` / `N` | Next / previous match |
| `p` | Priority filter |
| `t` | Time range filter |
| `f` | Toggle live tail |
| `l` | Exit logs |
| `Esc` | Clear search / exit logs |
| `?` | Toggle help |

## Requirements

- Linux with systemd
- Rust 1.85+ (2024 edition)

## License

MIT
