use log::{error, info, warn};
use paho_mqtt as mqtt;
use serde::Serialize;
use std::process;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Duration, Instant};

use crate::control::PortStatusInfo;
use crate::ha::{
    HubAttributes, MqttDiscoverySwitch, MqttHubSensor, MqttPortBinarySensor, PortAttributes,
};
use crate::hub::HubInfo;

const DISCOVERY_PREFIX: &str = "homeassistant";

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

fn create_conn_opts(
    username: &Option<String>,
    password: &Option<String>,
    avail_topic: &str,
) -> mqtt::ConnectOptions {
    let will = mqtt::Message::new(avail_topic, "offline", mqtt::QoS::AtLeastOnce);
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

async fn create_client(url: &str, node_id: &str) -> Result<mqtt::AsyncClient, String> {
    let client_id = format!("uhubctl-mqtt-{}", node_id);
    let opts = mqtt::CreateOptionsBuilder::new()
        .server_uri(url)
        .client_id(client_id)
        .finalize();

    mqtt::AsyncClient::new(opts).map_err(|e| format!("Cannot create MQTT client: {}", e))
}

fn parse_command(topic: &str, payload: &str) -> Option<MainCmd> {
    let parts: Vec<&str> = topic.split('/').collect();
    // uhubctl/<node_id>/<hub_location>/port/<port>/set
    if parts.len() < 6 {
        return None;
    }
    if parts[0] != "uhubctl" {
        return None;
    }
    if parts[parts.len() - 3] != "port" || parts[parts.len() - 1] != "set" {
        return None;
    }

    let hub_location = parts[parts.len() - 4].to_string();
    let port: u8 = parts[parts.len() - 2].parse().ok()?;
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

async fn publish_str(cli: &mqtt::AsyncClient, topic: &str, payload: &str) -> Result<(), String> {
    let msg = mqtt::Message::new(topic, payload, mqtt::QoS::AtLeastOnce);
    cli.publish(msg)
        .await
        .map_err(|e| format!("Publish failed: {}", e))
}

async fn publish_json<T: Serialize>(
    cli: &mqtt::AsyncClient,
    topic: &str,
    payload: &T,
) -> Result<(), String> {
    let json = serde_json::to_string(payload).map_err(|e| format!("JSON: {}", e))?;
    publish_str(cli, topic, &json).await
}

async fn publish_discovery(
    cli: &mqtt::AsyncClient,
    hub: &HubInfo,
    node_id: &str,
    topic_prefix: &str,
    avail_topic: &str,
) -> Result<(), String> {
    for port in 1..=hub.nports {
        if hub.is_root_hub {
            let config = MqttPortBinarySensor::new(
                &hub.location,
                &hub.vendor,
                &hub.ds.vendor,
                &hub.ds.product,
                port,
                node_id,
                avail_topic,
                topic_prefix,
                DISCOVERY_PREFIX,
            );
            let topic =
                MqttPortBinarySensor::config_topic(DISCOVERY_PREFIX, node_id, &hub.location, port);
            publish_json(cli, &topic, &config).await?;
        } else {
            let config = MqttDiscoverySwitch::new(
                &hub.location,
                &hub.vendor,
                &hub.ds.vendor,
                &hub.ds.product,
                port,
                node_id,
                avail_topic,
                topic_prefix,
                DISCOVERY_PREFIX,
            );
            let topic =
                MqttDiscoverySwitch::config_topic(DISCOVERY_PREFIX, node_id, &hub.location, port);
            publish_json(cli, &topic, &config).await?;
        }
    }
    Ok(())
}

async fn publish_state(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    hub_location: &str,
    port: u8,
    powered: bool,
) -> Result<(), String> {
    let state = if powered { "ON" } else { "OFF" };
    let topic = MqttDiscoverySwitch::state_topic(topic_prefix, hub_location, port);
    publish_str(cli, &topic, state).await
}

async fn publish_connected_state(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    hub_location: &str,
    port: u8,
    connected: bool,
) -> Result<(), String> {
    let state = if connected { "ON" } else { "OFF" };
    let topic = MqttPortBinarySensor::state_topic(topic_prefix, hub_location, port);
    publish_str(cli, &topic, state).await
}

async fn publish_attributes(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    hub_location: &str,
    port: u8,
    status: &PortStatusInfo,
) -> Result<(), String> {
    let mut attrs = PortAttributes {
        hub_location: hub_location.to_string(),
        port_number: port,
        connected: status.connected,
        powered: status.powered,
        enabled: status.enabled,
        suspended: status.suspended,
        overcurrent: status.overcurrent,
        speed: status.speed.clone(),
        link_state: status.link_state.clone(),
        ..Default::default()
    };
    if let Some(d) = &status.connected_device {
        attrs.connected_vid_pid = d.vid_pid.clone();
        attrs.connected_vendor = d.vendor.clone();
        attrs.connected_product = d.product.clone();
        attrs.connected_serial = d.serial.clone();
        attrs.connected_description = d.description.clone();
        attrs.connected_max_power_ma = d.max_power_ma;
    }
    let topic = MqttDiscoverySwitch::attributes_topic(topic_prefix, hub_location, port);
    publish_json(cli, &topic, &attrs).await
}

async fn publish_hub_discovery(
    cli: &mqtt::AsyncClient,
    hub: &HubInfo,
    node_id: &str,
    topic_prefix: &str,
    avail_topic: &str,
) -> Result<(), String> {
    let config = MqttHubSensor::new(hub, node_id, avail_topic, topic_prefix, DISCOVERY_PREFIX);
    let topic = MqttHubSensor::config_topic(DISCOVERY_PREFIX, node_id, &hub.location);
    publish_json(cli, &topic, &config).await
}

async fn publish_hub_state(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    hub: &HubInfo,
) -> Result<(), String> {
    let topic = MqttHubSensor::state_topic(topic_prefix, &hub.location);
    publish_str(cli, &topic, &hub.stable_id).await
}

async fn publish_hub_attributes(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    hub: &HubInfo,
) -> Result<(), String> {
    let attrs = HubAttributes::from_hub(hub);
    let topic = MqttHubSensor::attributes_topic(topic_prefix, &hub.location);
    publish_json(cli, &topic, &attrs).await
}

async fn publish_hub_sensor(
    cli: &mqtt::AsyncClient,
    hub: &HubInfo,
    node_id: &str,
    topic_prefix: &str,
    avail_topic: &str,
) -> Result<(), String> {
    publish_hub_discovery(cli, hub, node_id, topic_prefix, avail_topic).await?;
    publish_hub_state(cli, topic_prefix, hub).await?;
    publish_hub_attributes(cli, topic_prefix, hub).await
}

async fn publish_hub_availability(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    location: &str,
    online: bool,
) -> Result<(), String> {
    let payload = if online { "online" } else { "offline" };
    let topic = format!("{}/{}/status", topic_prefix, location);
    publish_str(cli, &topic, payload).await
}

async fn publish_global_availability(
    cli: &mqtt::AsyncClient,
    avail_topic: &str,
    online: bool,
) -> Result<(), String> {
    let payload = if online { "online" } else { "offline" };
    publish_str(cli, avail_topic, payload).await
}

async fn publish_birth(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    avail_topic: &str,
    known_hubs: &[String],
) -> Result<(), String> {
    publish_global_availability(cli, avail_topic, true).await?;
    for loc in known_hubs {
        publish_hub_availability(cli, topic_prefix, loc, true).await?;
    }
    Ok(())
}

async fn publish_all_offline(
    cli: &mqtt::AsyncClient,
    topic_prefix: &str,
    avail_topic: &str,
    known_hubs: &[String],
) -> Result<(), String> {
    publish_global_availability(cli, avail_topic, false).await?;
    for loc in known_hubs {
        publish_hub_availability(cli, topic_prefix, loc, false).await?;
    }
    Ok(())
}

async fn subscribe_commands(cli: &mqtt::AsyncClient, topic_prefix: &str) -> Result<(), String> {
    let pattern = MqttDiscoverySwitch::command_topic_pattern(topic_prefix);
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
    known_root_hubs: &mut Vec<String>,
    node_id: &str,
    topic_prefix: &str,
    avail_topic: &str,
) -> Result<(), String> {
    match event {
        HubEvent::HubAdded(hub) => {
            info!(
                "Hub added: {} ({} ports) stable_id={}",
                hub.location, hub.nports, hub.stable_id
            );
            publish_hub_sensor(cli, &hub, node_id, topic_prefix, avail_topic).await?;
            publish_hub_availability(cli, topic_prefix, &hub.location, true).await?;
            publish_discovery(cli, &hub, node_id, topic_prefix, avail_topic).await?;
            if hub.is_root_hub {
                known_root_hubs.push(hub.location.clone());
            }
            known_hubs.push(hub.location);
        }
        HubEvent::HubRemoved(location) => {
            info!("Hub removed: {}", location);
            publish_hub_availability(cli, topic_prefix, &location, false).await?;
            known_hubs.retain(|l| l != &location);
            known_root_hubs.retain(|l| l != &location);
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
            publish_state(cli, topic_prefix, &hub_location, port, powered).await?;
        }
        HubEvent::PortStatusChanged {
            hub_location,
            port,
            status,
        } => {
            publish_attributes(cli, topic_prefix, &hub_location, port, &status).await?;
            if known_root_hubs.contains(&hub_location) {
                publish_connected_state(cli, topic_prefix, &hub_location, port, status.connected)
                    .await?;
            }
        }
        HubEvent::Shutdown => {
            info!("Shutdown signal received, publishing offline...");
            publish_all_offline(cli, topic_prefix, avail_topic, known_hubs).await?;
        }
    }
    Ok(())
}

/// Run a single MQTT session.
async fn run_session(
    url: &str,
    username: &Option<String>,
    password: &Option<String>,
    node_id: &str,
    mut rx_events: broadcast::Receiver<HubEvent>,
    cmd_tx: mpsc::UnboundedSender<MainCmd>,
) -> Result<(), String> {
    let cli = create_client(url, node_id).await?;
    let topic_prefix = format!("uhubctl/{}", node_id);
    let avail_topic = format!("{}/status", topic_prefix);
    let conn_opts = create_conn_opts(username, password, &avail_topic);

    cli.connect(conn_opts)
        .await
        .map_err(|e| format!("Connect failed: {}", e))?;
    info!("Connected to MQTT");

    subscribe_commands(&cli, &topic_prefix).await?;
    setup_command_callback(&cli, cmd_tx);

    let mut known_hubs: Vec<String> = Vec::new();
    let mut known_root_hubs: Vec<String> = Vec::new();
    publish_birth(&cli, &topic_prefix, &avail_topic, &known_hubs).await?;

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            event = rx_events.recv() => {
                match event {
                    Ok(event) => {
                        if matches!(event, HubEvent::Shutdown) {
                            if let Err(e) = process_event(&cli, event, &mut known_hubs, &mut known_root_hubs, node_id, &topic_prefix, &avail_topic).await {
                                error!("Shutdown publish error: {}", e);
                            }
                            break;
                        }
                        if let Err(e) = process_event(&cli, event, &mut known_hubs, &mut known_root_hubs, node_id, &topic_prefix, &avail_topic).await {
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
#[allow(clippy::too_many_arguments)]
pub async fn mqtt_loop(
    url: String,
    username: Option<String>,
    password: Option<String>,
    node_id: String,
    tx_event: broadcast::Sender<HubEvent>,
    cmd_tx: mpsc::UnboundedSender<MainCmd>,
    resync_tx: mpsc::UnboundedSender<()>,
    reconnect_timeout: Duration,
) {
    let mut first_connect = true;

    loop {
        info!("Starting MQTT session...");
        let rx = tx_event.subscribe();
        let res = run_session(&url, &username, &password, &node_id, rx, cmd_tx.clone()).await;

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
                    match run_session(&url, &username, &password, &node_id, rx, cmd_tx.clone())
                        .await
                    {
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
