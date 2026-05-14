# uhubctl-mqtt

A Rust daemon that exposes USB hub per-port power control to Home Assistant via MQTT.

## Goals

- Reliably discover USB hubs that support per-port power switching (PPPS)
- Expose each hub port as an HA MQTT switch
- Show connected device name/VID/PID in switch attributes
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
- **HA MQTT Discovery** — auto-registers `switch` entities for each port
- **Poll-based monitoring** — 10s default, configurable via `--interval`
- **Automatic reconnection** — MQTT reconnect with full resync
- **Single static binary** — no Python, no `pip`, no dependency hell
- **CLI-only configuration** — no config file needed

## CLI

```
uhubctl-mqtt \
  --mqtt-url mqtt://192.168.1.10:1883 \
  --mqtt-username hubctl \
  --mqtt-password secret \
  --interval 10 \
  --log-level info
```

## Implementation Stages

| Stage | File | Description | Est. lines |
|-------|------|-------------|------------|
| 1 | [STAGE_01_hub_scan.md](STAGE_01_hub_scan.md) | USB hub enumeration, descriptor parsing, duality matching | ~300 |
| 2 | [STAGE_02_port_control.md](STAGE_02_port_control.md) | Port status read, set power on/off via rusb | ~150 |
| 3 | [STAGE_03_mqtt_bridge.md](STAGE_03_mqtt_bridge.md) | HA MQTT discovery config, state publishing, command subscription | ~250 |
| 4 | [STAGE_04_monitoring.md](STAGE_04_monitoring.md) | Poll loop, diff engine, hotplug events, resync | ~150 |
| 5 | [STAGE_05_deploy.md](STAGE_05_deploy.md) | CLI args, error recovery, udev rules, cross-compilation | ~100 |

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `rusb` | 0.9 | libusb bindings |
| `paho-mqtt` | 0.13 | MQTT client |
| `tokio` | 1 | Async runtime |
| `clap` | 4.6 | CLI argument parsing |
| `serde` + `serde_json` | 1 | JSON serialization for HA discovery |
| `log` + `env_logger` | 0.4/0.11 | Structured logging |
