# SSH Remote Management

systemdmgr can manage systemd units on a remote server over SSH.

## Quick Start

```bash
systemdmgr --ssh user@server
```

The connection authenticates and enters the TUI. All `systemctl` and `journalctl` commands run transparently over SSH.
The remote host must have systemd 246+ and `systemctl` on `PATH`; this is validated with `systemctl --version` after authentication.

## Connection

systemdmgr delegates all SSH connectivity to the system OpenSSH client (the `ssh` binary on `PATH`). It does not bundle an SSH library, which means everything your `ssh` command supports works unchanged: `~/.ssh/config` (including `Match`, `Include`, `ProxyJump`, `CanonicalizeHostname`), `ssh-agent` and agent forwarding, FIDO2/`sk-` keys, host certificates, `known_hosts` handling, and Kerberos/GSSAPI.

Connection lifecycle:

- On startup, systemdmgr opens an interactive SSH **ControlMaster** connection. Because this first connection runs on your real terminal, ssh itself handles host key verification and any authentication prompts (password, key passphrase, OTP/MFA).
- Every subsequent command multiplexes over the master socket (`ControlPath` under a private, per-process directory in the system temp dir with `0700` permissions), so there is no per-command handshake.
- Commands inside the TUI run with `BatchMode=yes`, so they fail fast instead of prompting if the master connection is ever lost. If the master dies and your setup authenticates non-interactively (agent or unencrypted key), each command still works by performing its own handshake.
- Keepalives are sent every 60 seconds (`ServerAliveInterval=60`).
- The master is not a detached daemon: it is a child process running `cat` on the remote host with its stdin tied to a pipe systemdmgr holds. If systemdmgr dies for any reason — including `SIGKILL` — the pipe closes, `cat` sees EOF, and the master stops itself within moments. On normal exit (including panics) the master is closed immediately (`ssh -O exit`) and the control directory removed.

## Command-Line Arguments

Everything after `--ssh` is forwarded to the ssh client verbatim, using ssh's own `[options] destination` syntax — the same arguments you would give a plain `ssh` command:

```bash
systemdmgr --ssh myserver
systemdmgr --ssh deploy@myserver
systemdmgr --ssh -p 2222 -i ~/.ssh/deploy_key deploy@myserver
systemdmgr --ssh -J bastion deploy@myserver
systemdmgr --ssh -- deploy@myserver
```

The destination can be anything your SSH setup resolves: a hostname, an IP address, a `Host` alias from `~/.ssh/config`, or a `ssh://user@host:port` URI. Options resolve exactly as they would for a plain `ssh` invocation.

Example `~/.ssh/config`:

```
Host prod
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/deploy_key
    ProxyJump bastion
```

```bash
systemdmgr --ssh prod
```

The multiplexing options systemdmgr adds (`ControlPath`, `ControlMaster`, `ServerAliveInterval`, and `BatchMode` for in-TUI commands) are placed before your arguments, and ssh gives the first occurrence of an option precedence — so they cannot be accidentally overridden.

## Authentication

Authentication is performed entirely by the OpenSSH client, following its normal method order and your client configuration (`PreferredAuthentications`, `IdentitiesOnly`, etc.). Passwords, key passphrases, and keyboard-interactive challenges such as OTP/MFA are prompted by ssh itself before systemdmgr enters the TUI. Passphrase-protected keys work, both via prompt and via `ssh-agent`.

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
    |-- ssh (system OpenSSH client, ControlMaster connection)
    |     |
    |     |-- ssh -o ControlPath=... -o BatchMode=yes host "systemctl --no-ask-password list-units ..."
    |     |-- ssh -o ControlPath=... -o BatchMode=yes host "journalctl -n 1000 --output=json ..."
    |     |-- ...
    |
    |-- CommandRunner trait
          |-- LocalRunner  (std::process::Command, used without --ssh)
          |-- SshRunner    (ssh subprocess per command, multiplexed over the master socket)
```

All command execution goes through the `CommandRunner` trait. `LocalRunner` runs commands as local processes; `SshRunner` runs each command as an `ssh` subprocess that reuses the master connection. The rest of the application is unaware of which is in use.

An ssh exit status of 255 signals a transport or authentication failure and is reported as a connection error; any other exit status is the remote command's own.

## Shell Escaping

The remote command is passed to `ssh` as a single string that the remote shell evaluates. Arguments containing spaces or special characters are POSIX shell-quoted (single-quote wrapping with `'\''` escape for embedded single quotes).

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `Failed to run ssh (is the OpenSSH client installed?)` | Install the OpenSSH client (`openssh-client` on Debian/Ubuntu; preinstalled on macOS). |
| Authentication or host key errors on connect | These come directly from ssh. Verify `ssh <same-destination>` works in a plain terminal first — if it does, systemdmgr will too. |
| `SSH error: ...` while inside the TUI | The master connection dropped and could not be re-established non-interactively (`BatchMode=yes`). Quit and reconnect. |
| User units not visible | Run `loginctl enable-linger <username>` on the remote server. |
