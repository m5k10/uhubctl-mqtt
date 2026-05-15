# AGENTS.md - Coding Guidelines for uhubctl-mqtt

This project follows the same conventions as onewire-bridge-cli. See that
project's AGENTS.md for full details. The sections below document conventions
specific to this project.

## Build, Lint, and Test Commands

### Building
```bash
cargo build
cargo build --release
```

### Running
```bash
cargo run -- --mqtt-url mqtt://localhost:1883 --mqtt-username user --mqtt-password pass
cargo run -- --mqtt-url mqtt://localhost:1883 --node-id my-pc  # explicit node ID
```

### Testing
```bash
cargo test
cargo test -- --nocapture
```

### Code Quality
```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo fmt && cargo clippy && cargo test
```

## Code Style Guidelines

- Module structure: hub.rs, control.rs, ha.rs, mqtt.rs, usb_ids.rs, main.rs
- Use `log` + `env_logger` for logging
- CLI via `clap` derive
- MQTT via `paho-mqtt` `AsyncClient`
- USB via `rusb`
- No `#![allow(dead_code)]` — delete unused code or add targeted `#[expect(dead_code)]`
- No shadowing of loop variables. Rename inner variables (e.g. `i` → `j`).

### Module Responsibilities

| Module | Owns | Depends On |
|--------|------|------------|
| `main.rs` | CLI args, tokio orchestration, diff engine, scan loop | all modules |
| `hub.rs` | USB hub enumeration, `HubInfo`, `DescriptorStrings`, USB descriptor helpers | `rusb` |
| `control.rs` | Port power control, `PortStatusInfo`, `ConnectedDeviceInfo` | `hub.rs`, `usb_ids.rs` |
| `ha.rs` | HA MQTT discovery structs and serialization (no business logic) | `hub.rs` |
| `mqtt.rs` | MQTT client lifecycle, event processing, topic publishing | `ha.rs`, `control.rs` |
| `usb_ids.rs` | USB ID database parser, vendor/product lookup | standalone |

### Error Handling

- Use `Result<(), String>` throughout (not custom error types).
- Error messages should describe the operation that failed, not just propagate the inner error.

### Established Refactoring Patterns

These patterns have been extracted from repeated code and should be used in all new code:

| Pattern | Location | Purpose |
|---------|----------|---------|
| `publish_str(cli, topic, payload)` | `mqtt.rs` | Publish a plain string payload to an MQTT topic |
| `publish_json(cli, topic, &payload)` | `mqtt.rs` | Serialize and publish any `Serialize` type to MQTT |
| `safe_location(loc)` | `ha.rs` | Sanitize a hub location for use in MQTT topic identifiers |
| `availability_topics(global, per_hub)` | `ha.rs` | Build the standard 2-element availability vector for HA discovery |
| `MqttDevice::new(node_id, ...)` | `ha.rs` | Construct an HA device metadata struct consistently |
| `read_device_strings(handle, desc)` | `hub.rs` | Read vendor/product/serial strings from a USB device handle |
| `lpsm_str(lpsm)` | `hub.rs` | Map LPSM byte to human-readable string |

### Code Conventions

- **Publish helpers** — always use `publish_str` / `publish_json`. Never construct `mqtt::Message` directly outside these helpers.
- **HA struct construction** — always use `MqttDevice::new`, `safe_location`, and `availability_topics` helpers rather than inline construction.
- **USB descriptor reading** — always use `read_device_strings(handle, desc)` from `hub.rs` rather than repeating the open → read strings pattern.
- **Default on data structs** — derive `Default` on serialization structs that have optional/fill-later fields, enabling `..Default::default()`.
- **Pure mappings** — extract `match`-based string mappings into `pub fn xxx_str()` helpers (e.g. `lpsm_str()`).
- **`if let` over `match`** — prefer `if let` for single-arm `Option` destructuring.
- **USB control transfers** — keep USB control transfer helpers private within `control.rs`.

## Dependencies

- **tokio 1**: Async runtime
- **rusb 0.9**: libusb bindings
- **paho-mqtt 0.13**: MQTT client
- **clap 4.6**: CLI argument parsing
- **serde 1** + **serde_json 1**: JSON serialization
- **log 0.4** + **env_logger 0.11**: Logging
- **hostname 0.4**: Default node ID from system hostname
