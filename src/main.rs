mod control;
mod ha;
mod hub;
mod mqtt;
mod usb_ids;

use clap::Parser;
use log::{LevelFilter, error, info, warn};
use std::collections::HashMap;
use std::process;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

fn default_node_id() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

use control::PortStatusInfo;
use hub::{DescriptorStrings, HubInfo};
use mqtt::{HubEvent, MainCmd};
use usb_ids::UsbIds;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "USB hub per-port power control via MQTT for Home Assistant"
)]
struct Args {
    #[arg(
        short = 'm',
        long,
        help = "MQTT broker URL (e.g. mqtt://localhost:1883)"
    )]
    mqtt_url: String,

    #[arg(short = 'u', long, help = "MQTT username")]
    mqtt_username: Option<String>,

    #[arg(short = 'p', long, help = "MQTT password")]
    mqtt_password: Option<String>,

    #[arg(
        short = 'i',
        long,
        default_value_t = 10,
        help = "Poll interval in seconds"
    )]
    interval: u16,

    #[arg(short = 'l', long, default_value_t = LevelFilter::Info, help = "Log level [off, error, warn, info, debug, trace]")]
    log_level: LevelFilter,

    #[arg(
        short = 'f',
        long,
        help = "Force power control on hubs without per-port switching"
    )]
    force: bool,

    #[arg(
        short = 'e',
        long,
        help = "Require exact hub match (vendor+product in addition to location)"
    )]
    exact: bool,

    #[arg(
        long,
        default_value_t = 60,
        help = "Max seconds to wait for MQTT reconnect before exiting"
    )]
    reconnect_timeout: u64,

    #[arg(
        long,
        default_value_t = default_node_id(),
        help = "Unique node identifier (defaults to hostname)"
    )]
    node_id: String,

    #[arg(
        long,
        help = "Path to USB IDs database (can be specified multiple times; default: /usr/share/usb.ids, /usr/share/hwdata/usb.ids)"
    )]
    usb_ids_path: Vec<String>,
}

#[derive(Clone, Debug)]
struct TrackedHub {
    location: String,
    vendor: String,
    nports: u8,
    super_speed: bool,
    is_root_hub: bool,
    dual_location: Option<String>,
    port_status: Vec<PortStatusInfo>,
    stable_id: String,
    ds: DescriptorStrings,
}

impl TrackedHub {
    fn from_hub_info(h: &HubInfo, status: Vec<PortStatusInfo>) -> Self {
        TrackedHub {
            location: h.location.clone(),
            vendor: h.vendor.clone(),
            nports: h.nports,
            super_speed: h.super_speed,
            is_root_hub: h.is_root_hub,
            dual_location: h.dual_location.clone(),
            port_status: status,
            stable_id: h.stable_id.clone(),
            ds: h.ds.clone(),
        }
    }
}

fn hubs_to_map(
    context: &rusb::Context,
    hubs: &[HubInfo],
    usb_ids: Option<&UsbIds>,
) -> HashMap<String, TrackedHub> {
    let device_tree = control::build_device_tree(context, usb_ids);
    let mut map = HashMap::new();
    for h in hub::discovery_hubs(hubs) {
        let status = if let Some(ref dev) = h.device {
            control::read_all_port_status(
                dev,
                h.nports,
                h.super_speed,
                h.bus,
                &h.port_numbers,
                &device_tree,
            )
        } else {
            vec![PortStatusInfo::default(); h.nports as usize]
        };
        map.insert(h.location.clone(), TrackedHub::from_hub_info(h, status));
    }
    map
}

fn synthetic_hub_added(hub: &TrackedHub) -> HubEvent {
    HubEvent::HubAdded(Box::new(HubInfo {
        device: None,
        location: hub.location.clone(),
        vendor: hub.vendor.clone(),
        bus: 0,
        super_speed: hub.super_speed,
        nports: hub.nports,
        lpsm: 1,
        container_id: String::new(),
        port_numbers: Vec::new(),
        ds: hub.ds.clone(),
        applicable: true,
        is_root_hub: hub.is_root_hub,
        dual_location: hub.dual_location.clone(),
        stable_id: hub.stable_id.clone(),
    }))
}

fn format_port_status(loc: &str, port: u8, cur: &PortStatusInfo) -> String {
    let conn = if cur.connected {
        match &cur.connected_device {
            Some(d) => format!("connected, {}, [{}]", cur.speed, d.description),
            None => "connected".to_string(),
        }
    } else {
        "not connected".to_string()
    };
    let power = if cur.powered { "powered" } else { "power off" };
    format!("Hub {} port {}: {}, {}", loc, port, conn, power)
}

fn emit_hub_diffs(
    prev: &HashMap<String, TrackedHub>,
    curr: &HashMap<String, TrackedHub>,
    tx: &broadcast::Sender<HubEvent>,
) {
    for (loc, hub) in curr.iter() {
        if let Some(prev_hub) = prev.get(loc) {
            // Existing hub — check for port status changes
            if hub.port_status != prev_hub.port_status {
                for (port_idx, (cur, prv)) in hub
                    .port_status
                    .iter()
                    .zip(prev_hub.port_status.iter())
                    .enumerate()
                {
                    if cur != prv {
                        let port = (port_idx + 1) as u8;
                        info!("{}", format_port_status(loc, port, cur));
                        if cur.powered != prv.powered {
                            let _ = tx.send(HubEvent::PortPowerChanged {
                                hub_location: loc.clone(),
                                port,
                                powered: cur.powered,
                            });
                        }
                        let _ = tx.send(HubEvent::PortStatusChanged {
                            hub_location: loc.clone(),
                            port,
                            status: cur.clone(),
                        });
                    }
                }
            }
        } else {
            // New hub
            let _ = tx.send(synthetic_hub_added(hub));
            for (port_idx, status) in hub.port_status.iter().enumerate() {
                let port = (port_idx + 1) as u8;
                let _ = tx.send(HubEvent::PortStatusChanged {
                    hub_location: loc.clone(),
                    port,
                    status: status.clone(),
                });
            }
        }
    }
    for loc in prev.keys() {
        if !curr.contains_key(loc) {
            let _ = tx.send(HubEvent::HubRemoved(loc.clone()));
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    env_logger::builder().filter_level(args.log_level).init();

    info!("uhubctl-mqtt starting");

    let (tx_event, _) = broadcast::channel::<HubEvent>(100);
    let (tx_cmd, mut rx_cmd) = mpsc::unbounded_channel::<MainCmd>();
    let (tx_resync, mut rx_resync) = mpsc::unbounded_channel::<()>();

    let reconnect_timeout = Duration::from_secs(args.reconnect_timeout);

    // Initialize USB context (before MQTT — scan first)
    let context = match rusb::Context::new() {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("USB init failed: {}", e);
            process::exit(1);
        }
    };

    let ids_paths = if args.usb_ids_path.is_empty() {
        None
    } else {
        Some(
            args.usb_ids_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
    };
    let usb_ids = UsbIds::load(ids_paths.as_deref());

    let mut scan_interval = tokio::time::interval(Duration::from_secs(args.interval as u64));
    let mut last_map: HashMap<String, TrackedHub> = HashMap::new();

    // Initial scan
    match hub::scan_hubs(&context) {
        Ok(hubs) => {
            let discovery = hub::discovery_hubs(&hubs);
            info!("Found {} hub(s)", discovery.len());
            let controllable = discovery.iter().any(|h| h.applicable);
            if !controllable && discovery.iter().any(|h| !h.is_root_hub) {
                warn!(
                    "No hubs with per-port power switching found. If you expected hubs, check USB permissions — try running with sudo or install udev rules."
                );
            }
            for h in discovery {
                info!(
                    "  Hub {}: {} ports stable_id={} [{}]",
                    h.location, h.nports, h.stable_id, h.ds.description
                );
            }
            let curr = hubs_to_map(&context, &hubs, usb_ids.as_ref());
            for hub in curr.values() {
                for (port_idx, status) in hub.port_status.iter().enumerate() {
                    info!(
                        "  {}",
                        format_port_status(&hub.location, (port_idx + 1) as u8, status)
                    );
                }
            }
            emit_hub_diffs(&last_map, &curr, &tx_event);
            last_map = curr;
        }
        Err(e) => warn!("Initial scan failed: {}", e),
    }

    // Start MQTT handler after initial scan
    let mqtt_handle = tokio::spawn({
        let txe = tx_event.clone();
        let txc = tx_cmd.clone();
        let txr = tx_resync.clone();
        let url = args.mqtt_url.clone();
        let user = args.mqtt_username.clone();
        let pass = args.mqtt_password.clone();
        let node_id = args.node_id.clone();
        async move {
            mqtt::mqtt_loop(url, user, pass, node_id, txe, txc, txr, reconnect_timeout).await;
        }
    });

    // Signal an initial resync to MQTT (sends HubAdded for current hubs)
    let _ = tx_resync.send(());

    loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down...");
                let _ = tx_event.send(HubEvent::Shutdown);
                // Wait for MQTT to publish offline messages
                if tokio::time::timeout(Duration::from_secs(3), mqtt_handle).await.is_err() {
                    warn!("MQTT task did not finish in time");
                }
                break;
            }
            _ = scan_interval.tick() => {
                match hub::scan_hubs(&context) {
                    Ok(hubs) => {
                        let curr = hubs_to_map(&context, &hubs, usb_ids.as_ref());
                        emit_hub_diffs(&last_map, &curr, &tx_event);
                        last_map = curr;
                    }
                    Err(e) => warn!("Scan error: {}", e),
                }
            }
            _ = rx_resync.recv() => {
                info!("Resync triggered, re-publishing all hubs...");
                match hub::scan_hubs(&context) {
                    Ok(hubs) => {
                        let curr = hubs_to_map(&context, &hubs, usb_ids.as_ref());
                        for hub in curr.values() {
                            let _ = tx_event.send(synthetic_hub_added(hub));
                            for (port_idx, status) in hub.port_status.iter().enumerate() {
                                let _ = tx_event.send(HubEvent::PortStatusChanged {
                                    hub_location: hub.location.clone(),
                                    port: (port_idx + 1) as u8,
                                    status: status.clone(),
                                });
                            }
                        }
                        last_map = curr;
                    }
                    Err(e) => warn!("Resync scan failed: {}", e),
                }
            }
            Some(cmd) = rx_cmd.recv() => {
                match cmd {
                    MainCmd::SetPortPower { hub_location, port, on } => {
                        info!("Set port {} on hub {} to {}", port, hub_location, if on { "ON" } else { "OFF" });
                        match control::control_port_power(&context, &hub_location, port, on) {
                            Ok(()) => {
                                let _ = tx_event.send(HubEvent::PortPowerChanged {
                                    hub_location,
                                    port,
                                    powered: on,
                                });
                            }
                            Err(e) => error!("Port control failed: {}", e),
                        }
                    }
                }
            }
        }
    }

    info!("Goodbye");
}
