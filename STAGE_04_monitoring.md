# Stage 4: Hotplug Monitoring & Change Detection

**Status**: Implemented in `src/main.rs`

## Goal

Poll USB, detect changes, emit events.

## Implementation

### Event Types

```rust
enum HubEvent {
    HubAdded(HubInfo),
    HubRemoved(String),          // location
    PortPowerChanged { hub_location, port, powered },
}
```

### Poll Loop

```rust
let mut scan_interval = tokio::time::interval(Duration::from_secs(interval));
loop {
    tokio::select! {
        _ = scan_interval.tick() => {
            let hubs = scan_hubs(&context)?;
            diff_hubs(&last_map, &hubs, &tx_event);
            last_map = hubs;
        }
        Some(cmd) = rx_cmd.recv() => {
            // handle MQTT command → USB control
        }
        _ = rx_resync.recv() => {
            // MQTT reconnected: force full re-publish
        }
        _ = tokio::signal::ctrl_c() => {
            // graceful shutdown
        }
    }
}
```

### Diff Algorithm

- **Hub added**: in current scan but not in previous
- **Hub removed**: in previous scan but not in current
- **Port power changed**: detected after command execution

### Resync on Reconnect

When MQTT reconnects, a resync signal triggers a full scan and all hubs are re-published as `HubAdded`. This ensures HA entities are re-created after broker restart.

## Data Structures

```rust
struct TrackedHub {
    location: String,
    vendor: String,
    nports: u8,
    super_speed: bool,
    dual_location: Option<String>,
}
```

The `TrackedHub` is a lightweight representation (no USB device handle) used for diffing.

## Acceptance Criteria

- New hub plugged in → MQTT discovery published within 1 poll cycle
- Hub unplugged → device removed from HA
- Power toggle → state update within 100ms
- MQTT reconnect → all hubs re-published
