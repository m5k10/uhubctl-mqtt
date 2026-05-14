# Stage 1: USB Hub Scanning

**Status**: Implemented in `src/hub.rs`

## Goal

Port uhubctl's hub discovery logic to Rust + `rusb`.

## Implementation

### Enumerate all USB devices

```rust
let context = rusb::Context::new()?;
let devices = context.devices()?;
for device in devices.iter() {
    let desc = device.device_descriptor()?;
    if desc.class_code() == LIBUSB_CLASS_HUB {
        // process as hub
    }
}
```

### Read hub descriptor

- Open device → `read_control()` with `GET_DESCRIPTOR` (0x06), descriptor type `LIBUSB_DT_HUB` (0x29) or `LIBUSB_DT_SUPERSPEED_HUB` (0x2a)
- Parse: `bNbrPorts` (offset 2), `wHubCharacteristics` (offset 3, LPSM bits)
- **Actionable** if LPSM == `HUB_CHAR_INDV_PORT_LPSM` (0x01)

### Build location string

- `bus_number()` + `port_numbers()` → format: `"1-2.3"`

### Read BOS descriptor for container ID

- Raw `read_control()` with `GET_DESCRIPTOR`, descriptor type 0x0F (BOS)
- Parse capabilities: find type 0x04 (Container ID), extract 16-byte UUID
- Used for USB2/USB3 dual hub matching

### Device description strings

- `read_manufacturer_string_ascii()`, `read_product_string_ascii()`, `read_serial_number_string_ascii()`
- Build composite description: `"VID:PID vendor product serial, USB x.yz, N ports, ppps"`

### USB2/USB3 Duality Matching

Port the scoring algorithm from `usb_find_hubs()` in uhubctl.c:

1. For each applicable hub with non-empty container_id, find its dual (USB2↔USB3)
2. Match by same container_id + compatible port counts
3. Score candidates by port_path similarity (same path = higher score)
4. Mark dual as discovered; primary gets `dual_location` field

## Data Structures

```rust
struct HubInfo {
    device: Option<Device<Context>>,
    location: String,       // "1-2.3"
    vendor: String,         // "VID:PID"
    bus: u8,
    super_speed: bool,
    nports: u8,
    lpsm: u8,
    container_id: String,   // 32-char hex UUID
    port_numbers: Vec<u8>,
    ds: DescriptorStrings,
    applicable: bool,
    dual_location: Option<String>,
}

struct DescriptorStrings {
    vendor: String,
    product: String,
    serial: String,
    description: String,
}
```

## Acceptance Criteria

- `scan_hubs()` returns all PPPS hubs connected to the system
- Duality matching produces same pairs as `uhubctl`
- Location strings match `uhubctl -l` format
- Device descriptions match `uhubctl` output
- Graceful handling of USB permission errors
