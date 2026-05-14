use log::{error, info, warn};
use paho_mqtt as mqtt;
use std::process;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Duration, Instant};

use crate::control::PortStatusInfo;
use crate::ha::{HubAttributes, MqttDiscoverySwitch, MqttHubSensor, PortAttributes};
use crate::hub::HubInfo;

const DISCOVERY_PREFIX: &str = "homeassistant";
const TOPIC_PREFIX: &str = "uhubctl";
const AVAIL_TOPIC: &str = "uhubctl/status";

#[derive(Debug)]
pub enum MainCmd {
    SetPortPower {
        hub_location: String,
        port: u8,
        on: bool,
    },
}

#[derive(Clone, Debug)]
pub enum HubEvent {
    HubAdded(Box<HubInfo>),
    HubRemoved(String),
    PortPowerChanged {
        hub_location: String,
        port: u8,
        powered: bool,
    },
    PortStatusChanged {
        hub_location: String,
        port: u8,
        status: PortStatusInfo,
    },
    Shutdown,
}

fn create_conn_opts(username: &Option<String>, password: &Option<String>) -> mqtt::ConnectOptions {
    let will = mqtt::Message::new(AVAIL_TOPIC, "offline", mqtt::QoS::AtLeastOnce);
    let mut builder = mqtt::ConnectOptionsBuilder::new();
    builder
        .keep_alive_interval(Duration::from_secs(30))
        .will_message(will)
        .clean_session(true);

    if let Some(u) = username {
        builder.user_name(u.clone());
    }
    if let Some(p) = password {
        builder.password(p.clone());
    }

    builder.finalize()
}

async fn create_client(url: &str) -> Result<mqtt::AsyncClient, String> {
    let opts = mqtt::CreateOptionsBuilder::new()
        .server_uri(url)
        .client_id("uhubctl-mqtt")
        .finalize();

    mqtt::AsyncClient::new(opts).map_err(|e| format!("Cannot create MQTT client: {}", e))
}

fn parse_command(topic: &str, payload: &str) -> Option<MainCmd> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() != 5 {
        return None;
    }
    if parts[0] != TOPIC_PREFIX || parts[2] != "port" || parts[4] != "set" {
        return None;
    }

    let hub_location = parts[1].to_string();
    let port: u8 = parts[3].parse().ok()?;
    let on = match payload {
        "ON" => true,
        "OFF" => false,
        _ => return None,
    };

    Some(MainCmd::SetPortPower {
        hub_location,
        port,
        on,
    })
}

async fn publish_discovery(cli: &mqtt::AsyncClient, hub: &HubInfo) -> Result<(), String> {
    for port in 1..=hub.nports {
        let config = MqttDiscoverySwitch::new(
            &hub.location,
            &hub.vendor,
            &hub.ds.vendor,
            &hub.ds.product,
            port,
            AVAIL_TOPIC,
            TOPIC_PREFIX,
            DISCOVERY_PREFIX,
        );
        let json = serde_json::to_string(&config).map_err(|e| format!("Discovery JSON: {}", e))?;
        let topic =
            MqttDiscoverySwitch::config_topic(DISCOVERY_PREFIX, TOPIC_PREFIX, &hub.location, port);
        let msg = mqtt::Message::new(topic, json, mqtt::QoS::AtLeastOnce);
        cli.publish(msg)
            .await
            .map_err(|e| format!("Discovery publish: {}", e))?;
    }
    Ok(())
}

async fn publish_state(
    cli: &mqtt::AsyncClient,
    hub_location: &str,
    port: u8,
    powered: bool,
) -> Result<(), String> {
    let state = if powered { "ON" } else { "OFF" };
    let topic = MqttDiscoverySwitch::state_topic(TOPIC_PREFIX, hub_location, port);
    let msg = mqtt::Message::new(topic, state, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("State publish: {}", e))?;
    Ok(())
}

async fn publish_attributes(
    cli: &mqtt::AsyncClient,
    hub_location: &str,
    port: u8,
    status: &PortStatusInfo,
) -> Result<(), String> {
    let (
        connected_vid_pid,
        connected_vendor,
        connected_product,
        connected_serial,
        connected_description,
        connected_max_power_ma,
    ) = match &status.connected_device {
        Some(d) => (
            d.vid_pid.clone(),
            d.vendor.clone(),
            d.product.clone(),
            d.serial.clone(),
            d.description.clone(),
            d.max_power_ma,
        ),
        None => Default::default(),
    };
    let attrs = PortAttributes {
        hub_location: hub_location.to_string(),
        port_number: port,
        connected: status.connected,
        powered: status.powered,
        enabled: status.enabled,
        suspended: status.suspended,
        overcurrent: status.overcurrent,
        speed: status.speed.clone(),
        link_state: status.link_state.clone(),
        connected_vid_pid,
        connected_vendor,
        connected_product,
        connected_serial,
        connected_description,
        connected_max_power_ma,
    };
    let topic = MqttDiscoverySwitch::attributes_topic(TOPIC_PREFIX, hub_location, port);
    let json = serde_json::to_string(&attrs).map_err(|e| format!("Attrs JSON: {}", e))?;
    let msg = mqtt::Message::new(topic, json, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Attrs publish: {}", e))
}

async fn publish_hub_discovery(cli: &mqtt::AsyncClient, hub: &HubInfo) -> Result<(), String> {
    let config = MqttHubSensor::new(hub, AVAIL_TOPIC, TOPIC_PREFIX, DISCOVERY_PREFIX);
    let json = serde_json::to_string(&config).map_err(|e| format!("Hub sensor JSON: {}", e))?;
    let topic = MqttHubSensor::config_topic(DISCOVERY_PREFIX, TOPIC_PREFIX, &hub.location);
    let msg = mqtt::Message::new(topic, json, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Hub sensor publish: {}", e))
}

async fn publish_hub_state(cli: &mqtt::AsyncClient, hub: &HubInfo) -> Result<(), String> {
    let topic = MqttHubSensor::state_topic(TOPIC_PREFIX, &hub.location);
    let msg = mqtt::Message::new(topic, hub.stable_id.as_str(), mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Hub state publish: {}", e))
}

async fn publish_hub_attributes(cli: &mqtt::AsyncClient, hub: &HubInfo) -> Result<(), String> {
    let attrs = HubAttributes::from_hub(hub);
    let topic = MqttHubSensor::attributes_topic(TOPIC_PREFIX, &hub.location);
    let json = serde_json::to_string(&attrs).map_err(|e| format!("Hub attrs JSON: {}", e))?;
    let msg = mqtt::Message::new(topic, json, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Hub attrs publish: {}", e))
}

async fn publish_hub_sensor(cli: &mqtt::AsyncClient, hub: &HubInfo) -> Result<(), String> {
    publish_hub_discovery(cli, hub).await?;
    publish_hub_state(cli, hub).await?;
    publish_hub_attributes(cli, hub).await
}

async fn publish_hub_availability(
    cli: &mqtt::AsyncClient,
    location: &str,
    online: bool,
) -> Result<(), String> {
    let payload = if online { "online" } else { "offline" };
    let topic = format!("{}/{}/status", TOPIC_PREFIX, location);
    let msg = mqtt::Message::new(topic, payload, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Hub avail publish: {}", e))
}

async fn publish_global_availability(cli: &mqtt::AsyncClient, online: bool) -> Result<(), String> {
    let payload = if online { "online" } else { "offline" };
    let msg = mqtt::Message::new(AVAIL_TOPIC, payload, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Global avail publish: {}", e))
}

async fn publish_birth(cli: &mqtt::AsyncClient, known_hubs: &[String]) -> Result<(), String> {
    publish_global_availability(cli, true).await?;
    for loc in known_hubs {
        publish_hub_availability(cli, loc, true).await?;
    }
    Ok(())
}

async fn publish_all_offline(cli: &mqtt::AsyncClient, known_hubs: &[String]) -> Result<(), String> {
    publish_global_availability(cli, false).await?;
    for loc in known_hubs {
        publish_hub_availability(cli, loc, false).await?;
    }
    Ok(())
}

async fn subscribe_commands(cli: &mqtt::AsyncClient) -> Result<(), String> {
    let pattern = MqttDiscoverySwitch::command_topic_pattern(TOPIC_PREFIX);
    cli.subscribe(&pattern, mqtt::QoS::AtLeastOnce)
        .await
        .map_err(|e| format!("Subscribe: {}", e))?;
    info!("Subscribed to {}", pattern);
    Ok(())
}

fn setup_command_callback(cli: &mqtt::AsyncClient, cmd_tx: mpsc::UnboundedSender<MainCmd>) {
    cli.set_message_callback(
        move |_cli: &mqtt::AsyncClient, msg: Option<mqtt::Message>| {
            if let Some(msg) = msg {
                let topic = msg.topic().to_string();
                let payload = msg.payload_str().to_string();
                if let Some(cmd) = parse_command(&topic, &payload) {
                    info!("Command: {:?}", cmd);
                    let _ = cmd_tx.send(cmd);
                }
            }
        },
    );
}

async fn process_event(
    cli: &mqtt::AsyncClient,
    event: HubEvent,
    known_hubs: &mut Vec<String>,
) -> Result<(), String> {
    match event {
        HubEvent::HubAdded(hub) => {
            info!(
                "Hub added: {} ({} ports) stable_id={}",
                hub.location, hub.nports, hub.stable_id
            );
            publish_hub_sensor(cli, &hub).await?;
            publish_hub_availability(cli, &hub.location, true).await?;
            publish_discovery(cli, &hub).await?;
            known_hubs.push(hub.location);
        }
        HubEvent::HubRemoved(location) => {
            info!("Hub removed: {}", location);
            publish_hub_availability(cli, &location, false).await?;
            known_hubs.retain(|l| l != &location);
        }
        HubEvent::PortPowerChanged {
            hub_location,
            port,
            powered,
        } => {
            info!(
                "Port {} on {} is now {}",
                port,
                hub_location,
                if powered { "ON" } else { "OFF" }
            );
            publish_state(cli, &hub_location, port, powered).await?;
        }
        HubEvent::PortStatusChanged {
            hub_location,
            port,
            status,
        } => {
            publish_attributes(cli, &hub_location, port, &status).await?;
        }
        HubEvent::Shutdown => {
            info!("Shutdown signal received, publishing offline...");
            publish_all_offline(cli, known_hubs).await?;
        }
    }
    Ok(())
}

/// Run a single MQTT session.
async fn run_session(
    url: &str,
    username: &Option<String>,
    password: &Option<String>,
    mut rx_events: broadcast::Receiver<HubEvent>,
    cmd_tx: mpsc::UnboundedSender<MainCmd>,
) -> Result<(), String> {
    let cli = create_client(url).await?;
    let conn_opts = create_conn_opts(username, password);

    cli.connect(conn_opts)
        .await
        .map_err(|e| format!("Connect failed: {}", e))?;
    info!("Connected to MQTT");

    subscribe_commands(&cli).await?;
    setup_command_callback(&cli, cmd_tx);

    let mut known_hubs: Vec<String> = Vec::new();
    publish_birth(&cli, &known_hubs).await?;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            event = rx_events.recv() => {
                match event {
                    Ok(event) => {
                        if matches!(event, HubEvent::Shutdown) {
                            if let Err(e) = process_event(&cli, event, &mut known_hubs).await {
                                error!("Shutdown publish error: {}", e);
                            }
                            break;
                        }
                        if let Err(e) = process_event(&cli, event, &mut known_hubs).await {
                            error!("Event error: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Missed {} events, will resync", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Event channel closed");
                        break;
                    }
                }
            }
            _ = heartbeat.tick() => {
                if !cli.is_connected() {
                    return Err("MQTT connection lost".to_string());
                }
            }
        }
    }

    Ok(())
}

/// MQTT main loop with reconnection handling.
pub async fn mqtt_loop(
    url: String,
    username: Option<String>,
    password: Option<String>,
    tx_event: broadcast::Sender<HubEvent>,
    cmd_tx: mpsc::UnboundedSender<MainCmd>,
    resync_tx: mpsc::UnboundedSender<()>,
    reconnect_timeout: Duration,
) {
    let mut first_connect = true;

    loop {
        info!("Starting MQTT session...");
        let rx = tx_event.subscribe();
        let res = run_session(&url, &username, &password, rx, cmd_tx.clone()).await;

        match res {
            Ok(()) => {
                info!("MQTT session ended normally");
                break;
            }
            Err(e) => {
                if first_connect {
                    error!("Initial MQTT connection failed: {}. Exiting.", e);
                    process::exit(1);
                }

                if e == "MQTT connection lost" {
                    warn!("MQTT connection lost. Will attempt reconnect...");
                } else {
                    warn!("MQTT error: {}. Reconnecting in 5s...", e);
                }

                // Try reconnecting with timeout
                let deadline = Instant::now() + reconnect_timeout;
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let _ = resync_tx.send(());

                    let rx = tx_event.subscribe();
                    match run_session(&url, &username, &password, rx, cmd_tx.clone()).await {
                        Ok(()) => {
                            info!("Reconnected successfully");
                            first_connect = false;
                            break;
                        }
                        Err(e2) => {
                            if Instant::now() >= deadline {
                                error!("Failed to reconnect within timeout. Exiting.");
                                process::exit(1);
                            }
                            warn!("Reconnect attempt failed: {}. Retrying...", e2);
                        }
                    }
                }
            }
        }
    }
}
