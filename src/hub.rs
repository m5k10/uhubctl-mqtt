use log::debug;
use rusb::UsbContext;
use std::collections::HashMap;
use std::time::Duration;

const USB_CTRL_TIMEOUT: Duration = Duration::from_secs(5);
const LIBUSB_DT_HUB: u8 = 0x29;
const LIBUSB_DT_SUPERSPEED_HUB: u8 = 0x2a;

const HUB_CHAR_LPSM: u8 = 0x03;
pub const HUB_CHAR_INDV_PORT_LPSM: u8 = 0x01;

const LIBUSB_CLASS_HUB: u8 = 0x09;

#[derive(Clone, Debug, Default)]
pub struct DescriptorStrings {
    pub vendor: String,
    pub product: String,
    pub serial: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct HubInfo {
    pub device: Option<rusb::Device<rusb::Context>>,
    pub location: String,
    pub vendor: String,
    pub bus: u8,
    pub super_speed: bool,
    pub nports: u8,
    pub lpsm: u8,
    pub container_id: String,
    pub port_numbers: Vec<u8>,
    pub ds: DescriptorStrings,
    pub applicable: bool,
    pub is_root_hub: bool,
    pub dual_location: Option<String>,
    pub stable_id: String,
}

fn read_container_id(
    handle: &rusb::DeviceHandle<rusb::Context>,
    vid: u16,
    pid: u16,
) -> Result<String, String> {
    let rt = rusb::request_type(
        rusb::Direction::In,
        rusb::RequestType::Standard,
        rusb::Recipient::Device,
    );

    let mut header = [0u8; 5];
    let n = handle
        .read_control(rt, 0x06, 0x0F00, 0, &mut header, USB_CTRL_TIMEOUT)
        .map_err(|e| format!("BOS header: {}", e))?;
    if n < 5 {
        return Err("BOS header too short".to_string());
    }

    let total_len = u16::from_le_bytes([header[2], header[3]]) as usize;
    let mut buf = vec![0u8; total_len];
    buf[..5].copy_from_slice(&header);
    if total_len > 5 {
        handle
            .read_control(rt, 0x06, 0x0F00, 0, &mut buf[5..], USB_CTRL_TIMEOUT)
            .map_err(|e| format!("BOS body: {}", e))?;
    }

    let num_caps = header[4] as usize;
    let mut offset = 5;

    for i in 0..num_caps {
        if offset + 3 >= buf.len() {
            debug!("  BOS cap[{}]: truncated at offset {}", i, offset);
            break;
        }
        let cap_len = buf[offset] as usize;
        let cap_type = buf[offset + 2];

        if cap_type == 0x04 && cap_len >= 20 && offset + 20 <= buf.len() {
            let id_start = offset + 4;
            let mut hex = String::with_capacity(32);
            for i in 0..16 {
                hex.push_str(&format!("{:02x}", buf[id_start + i]));
            }
            debug!("  {:04x}:{:04x} BOS container ID found", vid, pid);
            return Ok(hex);
        }

        offset += cap_len;
    }

    Err(format!(
        "No container ID capability in BOS ({} caps)",
        num_caps
    ))
}

fn read_hub_descriptor(
    handle: &rusb::DeviceHandle<rusb::Context>,
    super_speed: bool,
) -> Result<(u8, u8), String> {
    let desc_type = if super_speed {
        LIBUSB_DT_SUPERSPEED_HUB
    } else {
        LIBUSB_DT_HUB
    };

    let rt = rusb::request_type(
        rusb::Direction::In,
        rusb::RequestType::Class,
        rusb::Recipient::Device,
    );
    let mut buf = [0u8; 12];

    let len = handle
        .read_control(
            rt,
            0x06,
            (desc_type as u16) << 8,
            0,
            &mut buf,
            USB_CTRL_TIMEOUT,
        )
        .map_err(|e| format!("Hub desc: {}", e))?;

    if len < 7 {
        return Err(format!("Hub desc too short: {}", len));
    }

    let nports = buf[2];
    let lpsm = buf[3] & HUB_CHAR_LPSM;
    debug!(
        "  Hub desc type=0x{:02x} len={} nports={} lpsm={}",
        desc_type, len, nports, lpsm
    );
    Ok((nports, lpsm))
}

fn get_device_description(device: &rusb::Device<rusb::Context>) -> DescriptorStrings {
    let desc = match device.device_descriptor() {
        Ok(d) => d,
        Err(_) => return DescriptorStrings::default(),
    };

    let mut ds = DescriptorStrings::default();
    if let Ok(handle) = device.open() {
        let _ = handle.set_auto_detach_kernel_driver(true);
        if desc.manufacturer_string_index().is_some()
            && let Ok(s) = handle.read_manufacturer_string_ascii(&desc)
        {
            ds.vendor = s.trim().to_string();
        }
        if desc.product_string_index().is_some()
            && let Ok(s) = handle.read_product_string_ascii(&desc)
        {
            ds.product = s.trim().to_string();
        }
        if desc.serial_number_string_index().is_some()
            && let Ok(s) = handle.read_serial_number_string_ascii(&desc)
        {
            ds.serial = s.trim().to_string();
        }

        if ds.serial.is_empty() {
            for idx in 1u8..=5 {
                if let Ok(s) = handle.read_string_descriptor_ascii(idx) {
                    let s = s.trim().to_string();
                    if !s.is_empty() && s != ds.vendor && s != ds.product {
                        ds.serial = s;
                        break;
                    }
                }
            }
        }

        ds.description = format!(
            "{:04x}:{:04x}{}{}{}{}{}{}",
            desc.vendor_id(),
            desc.product_id(),
            if ds.vendor.is_empty() { "" } else { " " },
            ds.vendor,
            if ds.product.is_empty() { "" } else { " " },
            ds.product,
            if ds.serial.is_empty() { "" } else { " " },
            ds.serial,
        );

        if desc.class_code() == LIBUSB_CLASS_HUB {
            let ver = desc.usb_version();
            let bcd_usb = (ver.0 as u16) << 8 | (ver.1 as u16);
            let ss = ver.0 >= 3;
            if let Ok((nports, lpsm)) = read_hub_descriptor(&handle, ss) {
                let lpsm_str = match lpsm {
                    1 => "ppps",
                    0 => "ganged",
                    _ => "nops",
                };
                ds.description.push_str(&format!(
                    ", USB {:x}.{:02x}, {} ports, {}",
                    bcd_usb >> 8,
                    bcd_usb & 0xff,
                    nports,
                    lpsm_str
                ));
            }
        }
    }
    ds
}

fn build_location(bus: u8, port_numbers: &[u8]) -> String {
    if port_numbers.is_empty() {
        return bus.to_string();
    }
    let mut s = bus.to_string();
    for (i, p) in port_numbers.iter().enumerate() {
        if i == 0 {
            s.push('-');
        } else {
            s.push('.');
        }
        s.push_str(&p.to_string());
    }
    s
}

fn match_dual_hubs(hubs: &mut [HubInfo]) {
    let n = hubs.len();
    let locations: Vec<String> = hubs.iter().map(|h| h.location.clone()).collect();

    for i in 0..n {
        if !hubs[i].applicable || hubs[i].container_id.is_empty() {
            continue;
        }

        let mut best_score = -1i32;
        let mut best_match = None;

        for j in 0..n {
            if i == j {
                continue;
            }
            if hubs[i].super_speed == hubs[j].super_speed {
                continue;
            }
            if hubs[j].container_id.is_empty() || hubs[i].container_id != hubs[j].container_id {
                continue;
            }
            if hubs[i].nports != hubs[j].nports && hubs[i].nports as u16 + hubs[j].nports as u16 > 3
            {
                continue;
            }

            let mut score = 1i32;
            let p1 = &hubs[i].port_numbers;
            let p2 = &hubs[j].port_numbers;

            if !p1.is_empty() && p1.len() == p2.len() && p1.len() >= 2 && p1[1..] == p2[1..] {
                score = 2;
            }
            if p1 == p2 {
                score = 4;
            }

            if score > best_score {
                best_score = score;
                best_match = Some(j);
            }
        }

        if let Some(idx) = best_match {
            hubs[i].dual_location = Some(locations[idx].clone());
            if !hubs[idx].applicable {
                hubs[idx].dual_location = Some(locations[i].clone());
            }
        }
    }
}

fn is_hub_device(device: &rusb::Device<rusb::Context>, desc: &rusb::DeviceDescriptor) -> bool {
    let vid = desc.vendor_id();
    let pid = desc.product_id();
    if desc.class_code() == LIBUSB_CLASS_HUB {
        return true;
    }
    if desc.class_code() != 0x00 {
        debug!(
            "  {:04x}:{:04x} bDeviceClass=0x{:02x} -> skip (not hub, not per-interface)",
            vid,
            pid,
            desc.class_code()
        );
        return false;
    }
    if let Ok(config) = device.config_descriptor(0) {
        for interface in config.interfaces() {
            for alt in interface.descriptors() {
                if alt.class_code() == LIBUSB_CLASS_HUB {
                    debug!(
                        "  {:04x}:{:04x} bDeviceClass=0x00, iface {} bInterfaceClass=0x{:02x} -> hub (interface)",
                        vid,
                        pid,
                        alt.interface_number(),
                        alt.class_code()
                    );
                    return true;
                }
            }
        }
    }
    debug!(
        "  {:04x}:{:04x} bDeviceClass=0x00, no hub interface found -> skip",
        vid, pid
    );
    false
}

pub fn scan_hubs(context: &rusb::Context) -> Result<Vec<HubInfo>, String> {
    let devices = context
        .devices()
        .map_err(|e| format!("Cannot enumerate USB: {}", e))?;

    let device_count = devices.iter().count();
    debug!("scan_hubs: {} total USB devices found", device_count);

    let mut hubs: Vec<HubInfo> = Vec::new();

    for device in devices.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        if !is_hub_device(&device, &desc) {
            continue;
        }

        let handle = match device.open() {
            Ok(h) => h,
            Err(e) => {
                debug!(
                    "  {:04x}:{:04x} cannot open device: {}. Try running with sudo or install udev rules",
                    desc.vendor_id(),
                    desc.product_id(),
                    e
                );
                continue;
            }
        };
        let _ = handle.set_auto_detach_kernel_driver(true);

        let ver = desc.usb_version();
        let super_speed = ver.0 >= 3;

        let (nports, lpsm) = match read_hub_descriptor(&handle, super_speed) {
            Ok(v) => v,
            Err(e) => {
                debug!(
                    "  {:04x}:{:04x} read_hub_descriptor failed: {}",
                    desc.vendor_id(),
                    desc.product_id(),
                    e
                );
                drop(handle);
                continue;
            }
        };
        debug!(
            "  {:04x}:{:04x} nports={} lpsm={} super_speed={}",
            desc.vendor_id(),
            desc.product_id(),
            nports,
            lpsm,
            super_speed
        );

        let vid = desc.vendor_id();
        let pid = desc.product_id();
        let container_id = match read_container_id(&handle, vid, pid) {
            Ok(id) => id,
            Err(e) => {
                debug!("  {:04x}:{:04x} container ID unavailable: {}", vid, pid, e);
                String::new()
            }
        };
        drop(handle);

        let bus = device.bus_number();
        let port_numbers = device.port_numbers().unwrap_or_default();
        let location = build_location(bus, &port_numbers);
        let vendor = format!("{:04x}:{:04x}", vid, pid);
        let applicable = lpsm == HUB_CHAR_INDV_PORT_LPSM;
        let is_root_hub = port_numbers.is_empty();

        hubs.push(HubInfo {
            device: Some(device.clone()),
            location,
            vendor: vendor.clone(),
            bus,
            super_speed,
            nports,
            lpsm,
            container_id,
            port_numbers,
            ds: DescriptorStrings::default(),
            applicable,
            is_root_hub,
            dual_location: None,
            stable_id: String::new(),
        });

        if (applicable || is_root_hub)
            && let Some(hub) = hubs.last_mut()
        {
            hub.ds = get_device_description(&device);
        }
    }

    match_dual_hubs(&mut hubs);

    let dual_locs: Vec<String> = hubs
        .iter()
        .filter(|h| h.dual_location.is_some())
        .map(|h| h.dual_location.as_ref().unwrap().clone())
        .collect();

    let raw_count = hubs.len();
    hubs.retain(|h| h.applicable || dual_locs.contains(&h.location) || h.is_root_hub);
    debug!(
        "scan_hubs: {} raw hubs, {} after retain",
        raw_count,
        hubs.len()
    );

    let mut descriptions: Vec<(usize, DescriptorStrings)> = Vec::new();
    for i in 0..hubs.len() {
        if !hubs[i].ds.description.is_empty() {
            continue;
        }
        for j in 0..hubs.len() {
            if hubs[j].applicable
                && hubs[j]
                    .dual_location
                    .as_ref()
                    .is_some_and(|l| l == &hubs[i].location)
            {
                descriptions.push((i, hubs[j].ds.clone()));
                break;
            }
        }
    }
    for (idx, ds) in descriptions {
        hubs[idx].ds = ds;
    }

    let mut fallback_seq = 1;
    for hub in &mut hubs {
        if !hub.ds.serial.is_empty() {
            hub.stable_id = format!("serial-{}", hub.ds.serial);
        } else if !hub.container_id.is_empty() {
            hub.stable_id = format!("cid-{}", hub.container_id);
        } else {
            hub.stable_id = format!("{}-{}", hub.vendor.replace(':', "_"), fallback_seq);
            fallback_seq += 1;
        }
    }

    Ok(hubs)
}

pub fn discovery_hubs(hubs: &[HubInfo]) -> Vec<&HubInfo> {
    hubs.iter()
        .filter(|h| h.applicable || h.dual_location.is_some() || h.is_root_hub)
        .filter(|h| !(h.dual_location.is_some() && h.super_speed))
        .collect()
}

pub fn build_location_map(hubs: &[HubInfo]) -> HashMap<String, &HubInfo> {
    hubs.iter().map(|h| (h.location.clone(), h)).collect()
}

pub fn find_hub_by_location<'a>(hubs: &'a [HubInfo], location: &str) -> Option<&'a HubInfo> {
    hubs.iter().find(|h| h.location == location)
}
