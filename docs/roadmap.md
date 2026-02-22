# systemdmgr Roadmap

## Planned Features

### Real-Time Log Streaming

**Goal:** Add live log streaming so users can watch logs update in real time, like `journalctl -f`.

**Features:**
- Real-time log streaming (follow mode)
- Toggle follow mode on/off
- Auto-scroll when user is at the bottom of logs
- Respect active priority and time range filters

**Implementation approach:**
- Spawn `journalctl -f -u <unit> --no-pager --output=json` as a child process using `std::process::Command::spawn()` instead of `.output()`
- Read stdout line-by-line from a background thread (or using non-blocking I/O)
- Parse each line with the existing `parse_journal_json_line()` function
- Append new entries to `app.logs` and auto-scroll if the user is at the bottom
- Add a `follow_mode: bool` field to `App`
- When follow mode is off, fall back to the current one-shot fetch behavior
- Kill the child process when switching units or exiting follow mode

---

## Out of Scope

These features are excluded from systemdmgr's scope.

### Write/Mutating Operations

- Start, stop, restart, reload services
- Enable, disable, mask, unmask services
- Create new timer or service units
- Edit unit files
- `systemctl daemon-reload`
- PolicyKit / privilege escalation

### Non-Unit-Management Features

- Web-based remote access (Cockpit is a web server; systemdmgr is a local TUI)
- Hostname configuration (`hostnamed`)
- Time/timezone management (`timedated`)
- NTP server management
- Multi-host management
- Certificate management
- User account management

---

## Future Considerations

### D-Bus Integration

The [`zbus`](https://crates.io/crates/zbus) crate (pure Rust, async) could replace CLI subprocess calls for read-only operations.

**Benefits:**
- Lower latency than spawning `systemctl` processes
- Real-time state change notifications via `PropertiesChanged` D-Bus signals (subscribe to `org.freedesktop.systemd1.Manager` on the system bus)
- Structured data without JSON parsing
- For user units: connect to the session bus instead of the system bus

This is optional since all read-only features can be implemented with CLI commands, but D-Bus becomes more valuable as the feature set grows (especially for real-time streaming and property watching).
