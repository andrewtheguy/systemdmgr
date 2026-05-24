# SSH Remote Management

systemdmgr can manage systemd units on a remote server over SSH.

## Quick Start

```bash
systemdmgr --ssh user@server
```

The connection authenticates and enters the TUI. All `systemctl` and `journalctl` commands run transparently over the SSH session.

## Connection

systemdmgr uses the [ssh2](https://crates.io/crates/ssh2) crate (libssh2 bindings) for SSH connectivity. There is no dependency on the system `ssh` binary.

- A single TCP connection is opened and reused for all commands
- Keepalive packets are sent every 60 seconds (`ServerAliveInterval` equivalent)
- The session is cleaned up automatically on exit (including panics)

## Host Resolution

The `--ssh` argument accepts these forms:

| Form | Example |
|------|---------|
| `host` | `systemdmgr --ssh myserver` |
| `user@host` | `systemdmgr --ssh deploy@myserver` |
| `user@host:port` | `systemdmgr --ssh deploy@myserver:2222` |

### `~/.ssh/config` Support

systemdmgr reads `~/.ssh/config` and resolves the following directives:

| Directive | Description |
|-----------|-------------|
| `Host` | Pattern matching (exact match and `*` wildcard) |
| `HostName` | Resolved hostname to connect to |
| `Port` | SSH port (overridden by `:port` in the CLI argument) |
| `User` | Login username (overridden by `user@` in the CLI argument) |
| `IdentityFile` | Path to private key file (`~` expansion supported) |

Example `~/.ssh/config`:

```
Host prod
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/deploy_key
```

```bash
systemdmgr --ssh prod
```

Directives not listed above are ignored. CLI arguments (`user@`, `:port`) take precedence over config file values.

## Authentication

Authentication is attempted in this order:

1. **SSH agent** (`ssh-agent`) -- tried first. Works with any loaded key.
2. **Key files** -- if the agent fails, key files are tried:
   - If `IdentityFile` is set in `~/.ssh/config`, those paths are used.
   - Otherwise, the default keys are tried: `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`, `~/.ssh/id_ecdsa`.

Password authentication is not currently supported. Ensure you have either an SSH agent running or key files accessible.

## User Mode

User-scoped units (`systemctl --user`) work over SSH:

```bash
systemdmgr --ssh user@server
# then press 'u' to toggle to user mode
```

The remote user must have lingering enabled for user units to be accessible:

```bash
# On the remote server:
loginctl enable-linger <username>
```

## Architecture

```
systemdmgr (local)
    |
    |-- ssh2::Session (TCP connection to remote)
    |     |
    |     |-- channel.exec("systemctl --no-ask-password list-units ...")
    |     |-- channel.exec("journalctl -n 1000 --output=json ...")
    |     |-- ...
    |
    |-- CommandRunner trait
          |-- LocalRunner  (std::process::Command, used without --ssh)
          |-- SshRunner    (ssh2::Session, used with --ssh)
```

All command execution goes through the `CommandRunner` trait. `LocalRunner` runs commands as local processes; `SshRunner` runs them over the SSH session. The rest of the application is unaware of which is in use.

Commands are serialized through a `Mutex<ssh2::Session>` since `ssh2::Session` is not `Sync`. This matches the application's usage pattern (one command at a time).

## Shell Escaping

When running commands over SSH, arguments are combined into a single command string for `channel.exec()`. Arguments containing spaces or special characters are POSIX shell-quoted (single-quote wrapping with `'\\''` escape for embedded single quotes).

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "SSH authentication failed: no suitable key found" | Ensure `ssh-agent` is running with keys loaded, or that key files exist at `~/.ssh/id_ed25519` (or similar). Check `IdentityFile` in `~/.ssh/config`. |
| "Failed to connect to host:22" | Verify the host is reachable and sshd is running. Check `HostName` and `Port` in `~/.ssh/config`. |
| "SSH handshake failed" | The remote server may not support the key exchange algorithms available in libssh2. |
| User units not visible | Run `loginctl enable-linger <username>` on the remote server. |
