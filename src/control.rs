use crate::hub::{HubInfo, find_hub_by_location, scan_hubs};
use crate::usb_ids::UsbIds;
use rusb::UsbContext;
use serde::Serialize;
use std::time::Duration;

const USB_CTRL_TIMEOUT: Duration = Duration::from_secs(5);
const USB_PORT_FEAT_POWER: u16 = 8;

pub const USB_PORT_STAT_POWER: u16 = 0x0100;
pub const USB_SS_PORT_STAT_POWER: u16 = 0x0200;
pub const USB_PORT_STAT_CONNECTION: u16 = 0x0001;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ConnectedDeviceInfo {
    pub vid_pid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub product: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub serial: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub max_power_ma: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PortStatusInfo {
    pub connected: bool,
    pub powered: bool,
    pub enabled: bool,
    pub suspended: bool,
    pub overcurrent: bool,
    pub speed: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub link_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_device: Option<ConnectedDeviceInfo>,
}

fn speed_string(w_status: u16, super_speed: bool) -> String {
    if super_speed {
        let spd = (w_status >> 10) & 0x07;
        match spd {
            0 => "5 Gbps".to_string(),
            1 => "10 Gbps".to_string(),
            2 => "20 Gbps".to_string(),
            3 => "40 Gbps".to_string(),
            _ => "unknown".to_string(),
        }
    } else {
        if w_status & 0x0200 != 0 {
            "1.5 Mbps".to_string()
        } else if w_status & 0x0400 != 0 {
            "480 Mbps".to_string()
        } else if w_status & 0x0002 != 0 {
            "12 Mbps".to_string()
        } else {
            "not negotiated".to_string()
        }
    }
}

fn link_state_string(w_status: u16) -> String {
    let ls = (w_status >> 5) & 0x0f;
    match ls {
        0 => "U0".to_string(),
        1 => "U1".to_string(),
        2 => "U2".to_string(),
        3 => "U3".to_string(),
        4 => "SS.Disabled".to_string(),
        5 => "Rx.Detect".to_string(),
        6 => "SS.Inactive".to_string(),
        7 => "Polling".to_string(),
        8 => "Recovery".to_string(),
        9 => "Hot Reset".to_string(),
        10 => "Compliance".to_string(),
        11 => "Loopback".to_string(),
        _ => String::new(),
    }
}

pub fn decode_port_status(w_status: u16, super_speed: bool) -> PortStatusInfo {
    let power_mask = if super_speed {
        USB_SS_PORT_STAT_POWER
    } else {
        USB_PORT_STAT_POWER
    };
    PortStatusInfo {
        connected: (w_status & USB_PORT_STAT_CONNECTION) != 0,
        powered: (w_status & power_mask) != 0,
        enabled: (w_status & 0x0002) != 0,
        suspended: (w_status & 0x0004) != 0,
        overcurrent: (w_status & 0x0008) != 0,
        speed: speed_string(w_status, super_speed),
        link_state: if super_speed {
            link_state_string(w_status)
        } else {
            String::new()
        },
        connected_device: None,
    }
}

/// Read raw port status word.
pub fn get_port_status_raw(
    handle: &rusb::DeviceHandle<rusb::Context>,
    port: u8,
) -> Result<u16, String> {
    let rt = rusb::request_type(
        rusb::Direction::In,
        rusb::RequestType::Class,
        rusb::Recipient::Other,
    );
    let mut buf = [0u8; 4];
    let len = handle
        .read_control(rt, 0x00, 0, port as u16, &mut buf, USB_CTRL_TIMEOUT)
        .map_err(|e| format!("Port status: {}", e))?;
    if len < 2 {
        return Err(format!("Short status: {}", len));
    }
    Ok(u16::from_le_bytes([buf[0], buf[1]]))
}

/// Read port power status. Returns true if powered on.
pub fn get_port_status(
    handle: &rusb::DeviceHandle<rusb::Context>,
    port: u8,
    super_speed: bool,
) -> Result<bool, String> {
    let w_status = get_port_status_raw(handle, port)?;
    let power_mask = if super_speed {
        USB_SS_PORT_STAT_POWER
    } else {
        USB_PORT_STAT_POWER
    };
    Ok((w_status & power_mask) != 0)
}

fn read_connected_device_info(
    device: &rusb::Device<rusb::Context>,
    usb_ids: Option<&UsbIds>,
) -> ConnectedDeviceInfo {
    let desc = match device.device_descriptor() {
        Ok(d) => d,
        Err(_) => return ConnectedDeviceInfo::default(),
    };
    let vid = desc.vendor_id();
    let pid = desc.product_id();
    let mut info = ConnectedDeviceInfo {
        vid_pid: format!("{:04x}:{:04x}", vid, pid),
        ..Default::default()
    };
    if let Ok(handle) = device.open() {
        let _ = handle.set_auto_detach_kernel_driver(true);
        if desc.manufacturer_string_index().is_some()
            && let Ok(s) = handle.read_manufacturer_string_ascii(&desc)
        {
            info.vendor = s.trim().to_string();
        }
        if desc.product_string_index().is_some()
            && let Ok(s) = handle.read_product_string_ascii(&desc)
        {
            info.product = s.trim().to_string();
        }
        if desc.serial_number_string_index().is_some()
            && let Ok(s) = handle.read_serial_number_string_ascii(&desc)
        {
            info.serial = s.trim().to_string();
        }
    }

    if info.vendor.is_empty()
        && let Some(ids) = usb_ids
        && let Some(name) = ids.lookup_vendor(vid)
    {
        info.vendor = name.to_string();
    }
    if info.product.is_empty()
        && let Some(ids) = usb_ids
        && let Some(name) = ids.lookup_product(vid, pid)
    {
        info.product = name.to_string();
    }

    info.max_power_ma = device.config_descriptor(0).ok().map(|cd| cd.max_power());

    info.description = format!(
        "{}{}{}{}",
        info.vid_pid,
        if info.vendor.is_empty() {
            String::new()
        } else {
            format!(" {}", info.vendor)
        },
        if info.product.is_empty() {
            String::new()
        } else {
            format!(" {}", info.product)
        },
        if info.serial.is_empty() {
            String::new()
        } else {
            format!(" [{}]", info.serial)
        },
    );
    info
}

pub fn build_device_tree(
    context: &rusb::Context,
    usb_ids: Option<&UsbIds>,
) -> Vec<(u8, Vec<u8>, ConnectedDeviceInfo)> {
    let Ok(devices) = context.devices() else {
        return vec![];
    };
    let mut tree = Vec::new();
    for device in devices.iter() {
        let Ok(pn) = device.port_numbers() else {
            continue;
        };
        if pn.is_empty() {
            continue; // skip root hubs
        }
        let bus = device.bus_number();
        let info = read_connected_device_info(&device, usb_ids);
        tree.push((bus, pn, info));
    }
    tree
}

fn find_connected_device<'a>(
    bus: u8,
    hub_pn: &[u8],
    port: u8,
    tree: &'a [(u8, Vec<u8>, ConnectedDeviceInfo)],
) -> Option<&'a ConnectedDeviceInfo> {
    tree.iter()
        .find(|(b, pn, _)| {
            *b == bus
                && pn.len() == hub_pn.len() + 1
                && pn[..hub_pn.len()] == hub_pn[..]
                && pn[hub_pn.len()] == port
        })
        .map(|(_, _, info)| info)
}

/// Read decoded port status for all ports on a hub, enriched with connected device info.
pub fn read_all_port_status(
    device: &rusb::Device<rusb::Context>,
    nports: u8,
    super_speed: bool,
    hub_bus: u8,
    hub_port_numbers: &[u8],
    device_tree: &[(u8, Vec<u8>, ConnectedDeviceInfo)],
) -> Vec<PortStatusInfo> {
    let handle = match device.open() {
        Ok(h) => h,
        Err(_) => return vec![PortStatusInfo::default(); nports as usize],
    };
    let _ = handle.set_auto_detach_kernel_driver(true);
    let mut info = Vec::with_capacity(nports as usize);
    for port in 1..=nports {
        let mut st = match get_port_status_raw(&handle, port) {
            Ok(w_status) => decode_port_status(w_status, super_speed),
            Err(_) => PortStatusInfo::default(),
        };
        if st.connected {
            st.connected_device =
                find_connected_device(hub_bus, hub_port_numbers, port, device_tree).cloned();
        }
        info.push(st);
    }
    info
}

/// Turn port power on or off.
pub fn set_port_power(
    handle: &rusb::DeviceHandle<rusb::Context>,
    port: u8,
    on: bool,
) -> Result<(), String> {
    let rt = rusb::request_type(
        rusb::Direction::Out,
        rusb::RequestType::Class,
        rusb::Recipient::Other,
    );
    let request = if on { 0x03u8 } else { 0x01u8 };

    handle
        .write_control(
            rt,
            request,
            USB_PORT_FEAT_POWER,
            port as u16,
            &[],
            USB_CTRL_TIMEOUT,
        )
        .map_err(|e| format!("Port power: {}", e))?;

    Ok(())
}

/// Control port power on a hub by location, including its dual partner.
pub fn control_port_power(
    context: &rusb::Context,
    hub_location: &str,
    port: u8,
    on: bool,
) -> Result<(), String> {
    let hubs = scan_hubs(context)?;

    let primary = find_hub_by_location(&hubs, hub_location)
        .ok_or_else(|| format!("Hub not found: {}", hub_location))?;

    if port < 1 || port > primary.nports {
        return Err(format!("Port {} out of range (1-{})", port, primary.nports));
    }

    set_single_port_power(primary, port, on)?;

    if let Some(ref dual_loc) = primary.dual_location
        && let Some(dual) = find_hub_by_location(&hubs, dual_loc)
    {
        let _ = set_single_port_power(dual, port, on);
    }

    Ok(())
}

fn set_single_port_power(hub: &HubInfo, port: u8, on: bool) -> Result<(), String> {
    let dev = hub
        .device
        .as_ref()
        .ok_or_else(|| format!("No device for {}", hub.location))?;

    let handle = dev
        .open()
        .map_err(|e| format!("Cannot open {}: {}", hub.location, e))?;
    let _ = handle.set_auto_detach_kernel_driver(true);

    set_port_power(&handle, port, on)
}
