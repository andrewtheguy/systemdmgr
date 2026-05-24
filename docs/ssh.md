# SSH Remote Management

systemdmgr can manage systemd units on a remote server over SSH.

## Quick Start

```bash
systemdmgr --ssh user@server
```

The connection authenticates and enters the TUI. All `systemctl` and `journalctl` commands run transparently over the SSH session.
The remote host must have systemd 246+ and `systemctl` on `PATH`; this is validated with `systemctl --version` after authentication.

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

Use `--ssh-identity-file` to specify one or more private key files from the CLI:

```bash
systemdmgr --ssh deploy@myserver --ssh-identity-file ~/.ssh/deploy_key
```

When this flag is present, the specified key files override `IdentityFile` entries from `~/.ssh/config` and are tried before SSH agent identities.
Passphrase-protected private key files are not supported when loaded directly from disk. Add encrypted keys to `ssh-agent` instead.

### `~/.ssh/config` Support

systemdmgr reads `~/.ssh/config` and resolves the following directives:

| Directive | Description |
|-----------|-------------|
| `Host` | Pattern matching with exact names and `*`/`?` globs |
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

1. **None** -- the initial auth-method discovery request can authenticate servers that explicitly allow `none`.
2. **CLI key files** -- if `--ssh-identity-file` is set, those paths are tried before the agent.
3. **SSH agent** (`ssh-agent`) -- all loaded identities are tried.
4. **Config/default key files** -- if the agent fails and no CLI key file was specified, key files are tried:
   - If `IdentityFile` is set in `~/.ssh/config`, those paths are used.
   - Otherwise, the default keys are tried: `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`, `~/.ssh/id_ecdsa`.
5. **Hostbased** -- if the server offers hostbased authentication, readable local host keys under `/etc/ssh/ssh_host_*_key` are tried.
6. **Keyboard-interactive** -- if the server requests interactive prompts, systemdmgr displays them before entering the TUI. This supports OTP, MFA, and PAM challenge flows, and honors whether each response should be echoed.
7. **Password** -- if the server offers plain SSH password authentication, systemdmgr prompts up to three times with hidden input.

Authentication prompts are shown before systemdmgr enters the TUI.

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
| "SSH authentication failed: no supported authentication methods succeeded" | Ensure `ssh-agent` is running with keys loaded, key files exist at `~/.ssh/id_ed25519` (or similar), or the server offers keyboard-interactive/password authentication. Check `IdentityFile` in `~/.ssh/config`. |
| Encrypted private key file fails | Add the key to `ssh-agent`; direct key-file authentication does not prompt for private key passphrases. |
| "Failed to connect to host:22" | Verify the host is reachable and sshd is running. Check `HostName` and `Port` in `~/.ssh/config`. |
| "SSH handshake failed" | The remote server may not support the key exchange algorithms available in libssh2. |
| User units not visible | Run `loginctl enable-linger <username>` on the remote server. |
