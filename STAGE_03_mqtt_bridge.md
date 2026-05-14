# Stage 3: MQTT Bridge to Home Assistant

**Status**: Implemented in `src/ha.rs` + `src/mqtt.rs`

## Goal

HA MQTT discovery + state publishing + command subscription.

## Implementation

### HA Discovery Structs (`src/ha.rs`)

Uses `#[derive(Serialize)]` with `#[serde(rename = "...")]` for short MQTT field names:

```rust
struct MqttDiscoverySwitch {
    name, uniq_id,
    cmd_t, stat_t, json_attr_t, avty_t,
    pl_avail, pl_not_avail, pl_on, pl_off,
    stat_on, stat_off,
    dev: { ids, name, mdl, mf },
}
```

### MQTT Topics

| Purpose | Topic | Payload |
|---------|-------|---------|
| Discovery config | `homeassistant/switch/uhubctl/{loc}_p{port}/config` | JSON |
| State | `uhubctl/{loc}/port/{port}/state` | `ON` / `OFF` |
| Command | `uhubctl/{loc}/port/{port}/set` | `ON` / `OFF` |
| Availability | `uhubctl/status` | `online` / `offline` |

### MQTT Client (`src/mqtt.rs`)

Uses `paho-mqtt` `AsyncClient`:

- **Connect** with last-will (LWT) set to `uhubctl/status` → `offline`
- **Birth message**: publishes `online` on connect
- **Discovery**: for each `HubAdded` event, publishes config for all ports
- **State**: for each `PortPowerChanged` event, publishes `ON`/`OFF`
- **Commands**: subscribes to `uhubctl/+/port/+/set` wildcard, forwards `MainCmd` via channel

### Reconnection

`mqtt_loop()` wraps `run_session()` in a reconnect loop:
- On disconnect, waits 5s and retries
- On reconnect, sends `resync` signal to trigger full hub re-publish

### Command Parsing

```rust
fn parse_command(topic: &str, payload: &str) -> Option<MainCmd> {
    // topic = "uhubctl/1-2.3/port/2/set"
    // payload = "ON" or "OFF"
    // Returns: MainCmd::SetPortPower { hub_location: "1-2.3", port: 2, on: true/false }
}
```

## Acceptance Criteria

- HA auto-discovers switches via MQTT discovery
- Toggling switch in HA turns USB port power on/off
- State updates reflect actual port power state
- Device goes offline in HA when daemon disconnects (LWT)
- Reconnection: daemon reconnects and re-publishes all entities
