# systemdmgr

A terminal UI for managing and browsing systemd units.

![Rust](https://img.shields.io/badge/rust-stable-orange)

> [!WARNING]
> This project is a work in progress and features are still incomplete and subject to change.

## Features

- Browse systemd units (services, sockets, timers, paths, targets) with status indicators
- Search units by name or description
- Filter by status, file state, and unit type via picker dialogs
- View unit details and properties
- Perform unit actions (start, stop, restart, enable, disable, reload, daemon-reload)
- View unit logs in a side panel with search, priority filter, and time range filter
- Live tail mode for real-time log monitoring
- Toggle between user and system units
- Remote management via SSH (authenticate once, persistent connection)
- Mouse support (click to select, scroll to navigate)

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

### Remote Management

Manage systemd units on a remote server over SSH:

```bash
systemdmgr --ssh user@server
systemdmgr --ssh user@server --ssh-identity-file ~/.ssh/deploy_key
```

Authenticates via none, SSH agent, unencrypted key files, hostbased auth, keyboard-interactive prompts such as OTP/MFA, or password prompts, then reuses a single connection for all commands. Supports `~/.ssh/config` Host aliases, custom ports, and identity files. Add passphrase-protected keys to `ssh-agent`; direct key-file authentication does not prompt for private key passphrases. `--user` mode works over SSH (requires `loginctl enable-linger` on the remote).

See [docs/ssh.md](docs/ssh.md) for full details on host resolution, authentication, and troubleshooting.

### Version

```bash
systemdmgr version
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
| `f` | Toggle live tail |
| `l` | Exit logs |
| `Esc` | Clear search / exit logs |
| `?` | Toggle help |

## Documentation

- [Specification](docs/spec.md) — architecture, features, and UI details
- [SSH Remote Management](docs/ssh.md) — host resolution, authentication, and troubleshooting
- [Roadmap](docs/roadmap.md) — planned features and future considerations

## Requirements

- Rust 1.85+ (2024 edition)
- systemd 246+ with `systemctl` on `PATH` (local host, or remote host when using `--ssh`)
- OpenSSL/libssl (for remote management via `--ssh`)

## License

MIT
