# AGENTS.md - Coding Guidelines for uhubctl-mqtt

This project follows the same conventions as onewire-bridge-cli. See that
project's AGENTS.md for full details.

## Build, Lint, and Test Commands

### Building
```bash
cargo build
cargo build --release
```

### Running
```bash
cargo run -- --mqtt-url mqtt://localhost:1883 --mqtt-username user --mqtt-password pass
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

- Follow onewire-bridge-cli conventions (see AGENTS.md in that project)
- Module structure: hub.rs, control.rs, ha.rs, mqtt.rs, monitor.rs, main.rs
- Use `log` + `env_logger` for logging
- CLI via `clap` derive
- MQTT via `paho-mqtt` `AsyncClient`
- USB via `rusb`

## Dependencies

- **tokio 1**: Async runtime
- **rusb 0.9**: libusb bindings
- **paho-mqtt 0.13**: MQTT client
- **clap 4.6**: CLI argument parsing
- **serde 1** + **serde_json 1**: JSON serialization
- **log 0.4** + **env_logger 0.11**: Logging
