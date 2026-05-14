# Stage 2: USB Port Control

**Status**: Implemented in `src/control.rs`

## Goal

Read per-port power status and set power on/off via libusb control transfers.

## Implementation

### Port Status Read

Control transfer: `GET_STATUS` (0x00), `LIBUSB_RECIPIENT_OTHER` (0x03)

```rust
let rt = rusb::request_type(Direction::Out, RequestType::Class, Recipient::Other);
let mut buf = [0u8; 4];
handle.read_control(rt, 0x00, 0, port, &mut buf, timeout)?;
let w_status = u16::from_le_bytes([buf[0], buf[1]]);
```

Parse `wStatus` bitfield:
- USB 2.0: `USB_PORT_STAT_POWER` = 0x0100
- USB 3.0: `USB_SS_PORT_STAT_POWER` = 0x0200

### Set Port Power

Control transfer: `SET_FEATURE` (0x03) for ON, `CLEAR_FEATURE` (0x01) for OFF
Feature selector: `USB_PORT_FEAT_POWER` = 8

```rust
handle.write_control(rt, 0x03, 8, port, &[], timeout)?; // ON
handle.write_control(rt, 0x01, 8, port, &[], timeout)?; // OFF
```

### Dual Hub Handling

When controlling power, also control the same port on the dual hub (if any):

```rust
set_single_port_power(primary, port, on)?;
if let Some(ref dual_loc) = primary.dual_location {
    if let Some(dual) = find_hub_by_location(&hubs, dual_loc) {
        set_single_port_power(dual, port, on)?;
    }
}
```

### Functions

- `get_port_status(handle, port, super_speed) → Result<bool>` — read power state
- `set_port_power(handle, port, on) → Result<()>` — set power
- `control_port_power(context, hub_location, port, on) → Result<()>` — high-level: scan, find, control both hubs

## Acceptance Criteria

- `control_port_power()` turns a port on/off
- Dual hubs: both USB2 and USB3 virtual ports are controlled
- Error handling: invalid port numbers, device disappeared, permission denied
