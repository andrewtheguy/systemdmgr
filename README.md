# systemdmgr

A terminal UI for viewing and browsing systemd services.

![Rust](https://img.shields.io/badge/rust-stable-orange)

> [!WARNING]
> This project is a work in progress and is currently a proof of concept. Features is still incomplete and subject to change.

## Features

- Browse all systemd services with status indicators
- Search services by name or description
- Filter by status (running/exited/failed/dead)
- View service logs in a side panel
- Vim-style keyboard navigation

## Installation

### Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | bash
```

To install a prerelease version:

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | bash -s -- --prerelease
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

Press `?` in the app to see all shortcuts.

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |

### Search & Filter

| Key | Action |
|-----|--------|
| `/` | Start search |
| `s` | Cycle status filter (running → exited → failed → dead) |
| `S` | Clear status filter |
| `Esc` | Clear search/filter |

### Logs Panel

| Key | Action |
|-----|--------|
| `l` | Toggle logs panel |
| `PgUp` / `PgDn` | Scroll logs |
| `Ctrl+u` / `Ctrl+d` | Scroll logs half page |

### Other

| Key | Action |
|-----|--------|
| `r` | Refresh services |
| `?` | Show help |
| `q` / `Esc` | Quit |

## Requirements

- Linux with systemd
- Rust 1.85+ (2024 edition)

## License

MIT
