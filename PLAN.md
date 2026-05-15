# uhubctl-mqtt

A Rust daemon that exposes USB hub per-port power control to Home Assistant via MQTT.

## Goals

- Reliably discover USB hubs that support per-port power switching (PPPS)
- Expose each hub port as an HA MQTT switch
- Show connected device name/VID/PID/max power in switch attributes
- Report root hub (host controller) ports as read-only binary sensors
- Fall back to OS-supplied USB IDs database for human-readable names
- Support multiple machines via `--node-id` flag (defaults to hostname)
- React to hub hotplug — auto-register new hubs, clean up removed ones
- Single static binary, no runtime dependencies beyond `libusb`

## Architecture

```
┌─────────────────────────────────────────┐
│          uhubctl-mqtt daemon            │
│                                         │
│  Timer      ──→  HubScanner            │
│  (poll)           (rusb)               │
│                     │                  │
│                     ▼                  │
│  Diff engine ──→  broadcast channel    │
│                     │                  │
│                     ▼                  │
│  MQTT bridge  ──→  HA discovery        │
│  (paho-mqtt)       State updates       │
│                     │                  │
│                     ▼                  │
│              MQTT broker               │
│                     │                  │
│                     ▼                  │
│              Home Assistant            │
└─────────────────────────────────────────┘
```

## Features

- **Hub scanning** — enumerates USB devices, detects hubs with per-port power switching
- **USB2/USB3 duality** — matches dual virtual hubs via container ID (USB 3.0 spec §11.2)
- **Port control** — `GET_STATUS` / `SET_FEATURE` / `CLEAR_FEATURE` control transfers via `rusb`
- **Raspberry Pi workarounds** — RPi 4B, 5 special handling (ported from uhubctl)
- **Human-readable names** — USB descriptor strings with fallback to `/usr/share/usb.ids` DB
- **Root hub reporting** — host controller ports shown as read-only binary sensors in HA
- **Max power export** — each connected device's declared `bMaxPower` published as attribute
- **Multi-machine support** — `--node-id` flag isolates MQTT topics per machine
- **HA MQTT Discovery** — auto-registers `switch` (controllable) and `binary_sensor` (root) entities
- **Poll-based monitoring** — 10s default, configurable via `--interval`
- **Automatic reconnection** — MQTT reconnect with full resync
- **Single static binary** — no Python, no `pip`, no dependency hell
- **CLI-only configuration** — no config file needed

## CLI

```bash
uhubctl-mqtt \
  --mqtt-url mqtt://192.168.1.10:1883 \
  --mqtt-username hubctl \
  --mqtt-password secret \
  --node-id server-room \
  --interval 10 \
  --log-level info
```

Without `--node-id`, defaults to the machine's hostname.

## MQTT Topics

| Purpose | Topic | Payload |
|---------|-------|---------|
| Global availability | `uhubctl/{node_id}/status` | `online` / `offline` |
| Per-hub availability | `uhubctl/{node_id}/{hub}/status` | `online` / `offline` |
| Hub sensor state | `uhubctl/{node_id}/{hub}/hub/state` | stable_id |
| Hub sensor attributes | `uhubctl/{node_id}/{hub}/hub/attributes` | JSON |
| Port switch/binary_sensor state | `uhubctl/{node_id}/{hub}/port/{port}/state` | `ON` / `OFF` |
| Port switch command | `uhubctl/{node_id}/{hub}/port/{port}/set` | `ON` / `OFF` |
| Port attributes | `uhubctl/{node_id}/{hub}/port/{port}/attributes` | JSON |
| Root port connected state | `uhubctl/{node_id}/{hub}/port/{port}/connected` | `ON` / `OFF` |

Discovery config topics: `homeassistant/{component}/{node_id}/{object_id}/config`

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `rusb` | 0.9 | libusb bindings |
| `paho-mqtt` | 0.13 | MQTT client |
| `tokio` | 1 | Async runtime |
| `clap` | 4.6 | CLI argument parsing |
| `serde` + `serde_json` | 1 | JSON serialization for HA discovery |
| `log` + `env_logger` | 0.4/0.11 | Structured logging |
| `hostname` | 0.4 | Default node ID from system hostname |
