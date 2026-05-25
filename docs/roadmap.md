# systemdmgr Roadmap

## Out of Scope

These features are excluded from systemdmgr's scope.

### Excluded Mutating Operations

- Mask, unmask services
- Create new timer or service units
- Edit unit files (viewing unit files is supported)
- PolicyKit / privilege escalation

### System Administration

- Web-based remote access
- Hostname, timezone, or NTP configuration
- Certificate or user account management

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
