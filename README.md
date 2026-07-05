# systemdmgr

A terminal UI for managing and browsing systemd units.

![Rust](https://img.shields.io/badge/rust-stable-orange)

> [!WARNING]
> This project is a work in progress and features are still incomplete and subject to change.

## Features

- Browse systemd units (services, sockets, timers, paths, targets) with status indicators
- Search units by name or description
- Filter by status, file state, and unit type via picker dialogs
- View unit details, properties, and read-only unit file content
- Perform unit actions (start, stop, restart, enable, disable, reload, daemon-reload)
- View focused per-unit or system-wide logs with search, priority filter, and time range filter
- Live tail mode with pause/resume for real-time log monitoring
- Toggle between user and system units
- Remote management via SSH (authenticate once, persistent connection)
- Mouse support (click to select, scroll to navigate)

## Installation

### Quick Install (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | bash
```

To install a prerelease version:

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | bash -s -- --prerelease
```

On Linux the installer uses sudo to place the binary in `/usr/local/bin`.

### macOS (SSH remote management only)

macOS does not have systemd, but you can use systemdmgr as an SSH client to manage remote Linux servers:

```bash
curl -fsSL https://raw.githubusercontent.com/andrewtheguy/systemdmgr/main/install.sh | bash
```

The binary is installed to `~/.local/bin` (no sudo required). Then connect to a remote host:

```bash
systemdmgr --ssh user@server
```

### From Source

```bash
cargo install --path .
```

## Usage

```bash
systemdmgr
```

### Remote Management

Manage systemd units on a remote server over SSH:

```bash
systemdmgr --ssh user@server
systemdmgr --ssh user@server --ssh-identity-file ~/.ssh/deploy_key
```

Connectivity is delegated to the system OpenSSH client: authentication (agent, keys, password, OTP/MFA), host key verification, `~/.ssh/config` (including `ProxyJump` and `Match`), and passphrase prompts all work exactly as they do for plain `ssh`. A single multiplexed connection (ControlMaster) is reused for all commands. `--user` mode works over SSH (requires `loginctl enable-linger` on the remote).

See [docs/ssh.md](docs/ssh.md) for full details on host resolution, authentication, and troubleshooting.

### Version

```bash
systemdmgr version
systemdmgr --version
```

## Keyboard Shortcuts

Press `?` in the app to see context-sensitive help.

### Unit List

| Key | Action |
|-----|--------|
| `Down` | Move down |
| `Up` | Move up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Page up / down |
| `/` | Search units |
| `s` | Status filter picker |
| `f` | File state filter picker |
| `t` | Unit type picker |
| `i` / `Enter` | Open unit details |
| `v` | View unit file |
| `x` | Action picker (start/stop/restart/etc.) |
| `R` | Daemon reload |
| `l` | Open logs |
| `L` | Open system-wide logs |
| `p` | Log priority filter |
| `T` | Log time range filter |
| `r` | Refresh units |
| `u` | Toggle user/system units |
| `Esc` | Clear search or quit |
| `q` | Quit |
| `?` | Toggle help |

### Logs View

| Key | Action |
|-----|--------|
| `Down` | Scroll down |
| `Up` | Scroll up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Page scroll |
| `Ctrl+u` / `Ctrl+d` | Half page scroll |
| `/` | Search logs |
| `n` / `N` | Next / previous match |
| `p` | Priority filter |
| `t` | Time range filter |
| `x` | Action picker |
| `f` | Pause/resume live tail |
| `l` | Exit logs |
| `L` | Toggle system-wide logs |
| `Enter` | Open selected unit from paused system-wide logs |
| `Esc` | Clear search / exit logs |
| `?` | Toggle help |

### Unit File View

| Key | Action |
|-----|--------|
| `Down` / `Up` | Scroll down / up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` / `PgDn` | Page scroll |
| `Ctrl+u` / `Ctrl+d` | Half page scroll |
| `/` | Search unit file |
| `n` / `N` | Next / previous match |
| `v` / `Esc` / `q` | Close unit file |
| `?` | Toggle help |

## Documentation

- [Specification](docs/spec.md) — architecture, features, and UI details
- [SSH Remote Management](docs/ssh.md) — host resolution, authentication, and troubleshooting
- [Roadmap](docs/roadmap.md) — planned features and future considerations

## Requirements

- Rust 1.85+ (2024 edition)
- **Linux:** systemd 246+ with `systemctl` on `PATH` (local host, or remote host when using `--ssh`)
- **macOS:** SSH remote management only (`--ssh` flag required; the remote host must have systemd)
- OpenSSH client (`ssh` on `PATH`) for remote management via `--ssh`

## License

MIT
