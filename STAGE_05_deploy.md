# Stage 5: CLI, Error Recovery, Deployment

**Status**: Implemented in `src/main.rs`

## CLI

```rust
struct Args {
    #[arg(short = 'm', long)]
    mqtt_url: String,

    #[arg(short = 'u', long)]
    mqtt_username: Option<String>,

    #[arg(short = 'p', long)]
    mqtt_password: Option<String>,

    #[arg(short = 'i', long, default_value_t = 10)]
    interval: u16,                    // USB poll interval in seconds

    #[arg(short = 'l', long, default_value_t = LevelFilter::Info)]
    log_level: LevelFilter,           // trace, debug, info, warn, error

    #[arg(short = 'f', long)]
    force: bool,                      // force operation on unsupported hubs

    #[arg(short = 'e', long)]
    exact: bool,                      // exact location (disable USB3 duality)
}
```

## Error Recovery

| Scenario | Behavior |
|----------|----------|
| USB permission denied | Logs error, exits. Points to udev setup. |
| MQTT broker down | Reconnects with 5s backoff. Re-publishes all hubs on reconnect. |
| USB transient error | Logs warning, hub skipped in current scan cycle. |
| Ctrl+C | Graceful shutdown via `tokio::signal::ctrl_c()`. |

## udev Rules

Create `/etc/udev/rules.d/52-usb-hubctl.rules`:

```
SUBSYSTEM=="usb", DRIVER=="hub|usb", MODE="0666"
SUBSYSTEM=="usb", DRIVER=="hub|usb", \
  RUN="/bin/sh -c \"chmod -f 666 $sys$devpath/*port*/disable || true\""
```

Then reload:

```bash
sudo udevadm trigger --attr-match=subsystem=usb
```

## Build

```bash
# Debug build
cargo build

# Release build (optimized, single binary)
cargo build --release

# Binary location
./target/release/uhubctl-mqtt
```

## Cross-compilation

```bash
# For Raspberry Pi (aarch64)
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu

# For Raspberry Pi Zero (armv7)
rustup target add armv7-unknown-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## Installation

```bash
# Copy binary to system
sudo cp target/release/uhubctl-mqtt /usr/local/bin/

# Create systemd service (optional)
cat <<EOF | sudo tee /etc/systemd/system/uhubctl-mqtt.service
[Unit]
Description=uhubctl MQTT Bridge
After=network.target

[Service]
ExecStart=/usr/local/bin/uhubctl-mqtt \
  --mqtt-url mqtt://localhost:1883 \
  --mqtt-username hubctl \
  --mqtt-password secret
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now uhubctl-mqtt
```
