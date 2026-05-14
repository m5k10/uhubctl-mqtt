use crate::hub::HubInfo;
use serde::Serialize;

#[derive(Serialize)]
pub struct AvailabilityTopic {
    #[serde(rename = "t")]
    pub topic: String,
    #[serde(rename = "pl")]
    pub payload_available: String,
    #[serde(rename = "pl_not_avail")]
    pub payload_not_available: String,
}

#[derive(Serialize)]
pub struct MqttDiscoverySwitch {
    pub name: String,
    #[serde(rename = "uniq_id")]
    pub uniq_id: String,
    #[serde(rename = "cmd_t")]
    pub command_topic: String,
    #[serde(rename = "stat_t")]
    pub state_topic: String,
    #[serde(rename = "json_attr_t")]
    pub attributes_topic: String,
    #[serde(rename = "avty")]
    pub availability: Vec<AvailabilityTopic>,
    #[serde(rename = "pl_on")]
    pub payload_on: String,
    #[serde(rename = "pl_off")]
    pub payload_off: String,
    #[serde(rename = "stat_on")]
    pub state_on: String,
    #[serde(rename = "stat_off")]
    pub state_off: String,
    #[serde(rename = "dev")]
    pub device: MqttDevice,
}

#[derive(Serialize)]
pub struct MqttDevice {
    #[serde(rename = "ids")]
    pub identifiers: Vec<String>,
    pub name: String,
    #[serde(rename = "mdl")]
    pub model: String,
    #[serde(rename = "mf")]
    pub manufacturer: String,
}

impl MqttDevice {
    pub fn new(
        node_id: &str,
        topic_prefix: &str,
        safe_loc: &str,
        hub_location: &str,
        hub_vendor: &str,
        vendor_name: &str,
        product_name: &str,
    ) -> Self {
        MqttDevice {
            identifiers: vec![format!("{}_{}", topic_prefix, safe_loc)],
            name: format!("USB Hub {} ({})", hub_location, node_id),
            model: if !product_name.is_empty() {
                product_name.to_string()
            } else {
                format!("USB Hub ({})", hub_vendor)
            },
            manufacturer: if !vendor_name.is_empty() {
                vendor_name.to_string()
            } else {
                "uhubctl-mqtt".to_string()
            },
        }
    }
}

fn safe_location(loc: &str) -> String {
    loc.replace(['.', '-'], "_")
}

fn availability_topics(global_avail_topic: &str, per_hub_avail: &str) -> Vec<AvailabilityTopic> {
    vec![
        AvailabilityTopic {
            topic: global_avail_topic.to_string(),
            payload_available: "online".to_string(),
            payload_not_available: "offline".to_string(),
        },
        AvailabilityTopic {
            topic: per_hub_avail.to_string(),
            payload_available: "online".to_string(),
            payload_not_available: "offline".to_string(),
        },
    ]
}

#[derive(Serialize)]
pub struct PortAttributes {
    pub hub_location: String,
    pub port_number: u8,
    pub connected: bool,
    pub powered: bool,
    pub enabled: bool,
    pub suspended: bool,
    pub overcurrent: bool,
    pub speed: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub link_state: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub connected_vid_pid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub connected_vendor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub connected_product: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub connected_serial: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub connected_description: String,
    pub connected_max_power_ma: Option<u16>,
}

#[derive(Serialize)]
pub struct MqttHubSensor {
    pub name: String,
    #[serde(rename = "uniq_id")]
    pub unique_id: String,
    #[serde(rename = "stat_t")]
    pub state_topic: String,
    #[serde(rename = "json_attr_t")]
    pub attributes_topic: String,
    #[serde(rename = "avty")]
    pub availability: Vec<AvailabilityTopic>,
    #[serde(rename = "dev")]
    pub device: MqttDevice,
}

#[derive(Serialize)]
pub struct HubAttributes {
    pub vid_pid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub product: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub serial: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub location: String,
    pub stable_id: String,
    pub bus: u8,
    pub super_speed: bool,
    pub nports: u8,
    pub lpsm: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub container_id: String,
    pub applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dual_location: Option<String>,
}

impl HubAttributes {
    pub fn from_hub(hub: &HubInfo) -> Self {
        let lpsm_str = match hub.lpsm {
            1 => "ppps",
            0 => "ganged",
            _ => "nops",
        };
        HubAttributes {
            vid_pid: hub.vendor.clone(),
            vendor: hub.ds.vendor.clone(),
            product: hub.ds.product.clone(),
            serial: hub.ds.serial.clone(),
            description: hub.ds.description.clone(),
            location: hub.location.clone(),
            stable_id: hub.stable_id.clone(),
            bus: hub.bus,
            super_speed: hub.super_speed,
            nports: hub.nports,
            lpsm: lpsm_str.to_string(),
            container_id: hub.container_id.clone(),
            applicable: hub.applicable,
            dual_location: hub.dual_location.clone(),
        }
    }
}

impl MqttHubSensor {
    pub fn new(
        hub: &HubInfo,
        node_id: &str,
        global_avail_topic: &str,
        topic_prefix: &str,
        _discovery_prefix: &str,
    ) -> Self {
        let safe_loc = safe_location(&hub.location);
        let per_hub_avail = format!("{}/{}/status", topic_prefix, hub.location);
        MqttHubSensor {
            name: format!("USB Hub {}", hub.location),
            unique_id: format!("{}_{}_hub", topic_prefix, safe_loc),
            state_topic: format!("{}/{}/hub/state", topic_prefix, hub.location),
            attributes_topic: format!("{}/{}/hub/attributes", topic_prefix, hub.location),
            availability: availability_topics(global_avail_topic, &per_hub_avail),
            device: MqttDevice::new(
                node_id,
                topic_prefix,
                &safe_loc,
                &hub.location,
                &hub.vendor,
                &hub.ds.vendor,
                &hub.ds.product,
            ),
        }
    }

    pub fn config_topic(discovery_prefix: &str, node_id: &str, hub_location: &str) -> String {
        let safe_loc = safe_location(hub_location);
        format!(
            "{}/sensor/{}/{}_hub/config",
            discovery_prefix, node_id, safe_loc
        )
    }

    pub fn state_topic(topic_prefix: &str, hub_location: &str) -> String {
        format!("{}/{}/hub/state", topic_prefix, hub_location)
    }

    pub fn attributes_topic(topic_prefix: &str, hub_location: &str) -> String {
        format!("{}/{}/hub/attributes", topic_prefix, hub_location)
    }
}
/// Read-only port binary sensor for root hub ports (no power control).
#[derive(Serialize)]
pub struct MqttPortBinarySensor {
    pub name: String,
    #[serde(rename = "uniq_id")]
    pub unique_id: String,
    #[serde(rename = "stat_t")]
    pub state_topic: String,
    #[serde(rename = "json_attr_t")]
    pub attributes_topic: String,
    #[serde(rename = "avty")]
    pub availability: Vec<AvailabilityTopic>,
    #[serde(rename = "dev")]
    pub device: MqttDevice,
    #[serde(rename = "dev_cla")]
    pub device_class: String,
    #[serde(rename = "pl_on")]
    pub payload_on: String,
    #[serde(rename = "pl_off")]
    pub payload_off: String,
}

impl MqttPortBinarySensor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hub_location: &str,
        hub_vendor: &str,
        vendor_name: &str,
        product_name: &str,
        port: u8,
        node_id: &str,
        global_avail_topic: &str,
        topic_prefix: &str,
        _discovery_prefix: &str,
    ) -> Self {
        let safe_loc = safe_location(hub_location);
        let id = format!("{}_{}_p{}_sensor", topic_prefix, &safe_loc, port);
        let per_hub_avail = format!("{}/{}/status", topic_prefix, hub_location);

        MqttPortBinarySensor {
            name: format!("USB Hub {} Port {}", hub_location, port),
            unique_id: id,
            state_topic: format!("{}/{}/port/{}/connected", topic_prefix, hub_location, port),
            attributes_topic: format!("{}/{}/port/{}/attributes", topic_prefix, hub_location, port),
            availability: availability_topics(global_avail_topic, &per_hub_avail),
            device: MqttDevice::new(
                node_id,
                topic_prefix,
                &safe_loc,
                hub_location,
                hub_vendor,
                vendor_name,
                product_name,
            ),
            device_class: "connectivity".to_string(),
            payload_on: "ON".to_string(),
            payload_off: "OFF".to_string(),
        }
    }

    pub fn config_topic(
        discovery_prefix: &str,
        node_id: &str,
        hub_location: &str,
        port: u8,
    ) -> String {
        let safe_loc = safe_location(hub_location);
        format!(
            "{}/binary_sensor/{}/p{}_{}/config",
            discovery_prefix, node_id, safe_loc, port
        )
    }

    pub fn state_topic(topic_prefix: &str, hub_location: &str, port: u8) -> String {
        format!("{}/{}/port/{}/connected", topic_prefix, hub_location, port)
    }

    #[allow(dead_code)]
    pub fn attributes_topic(topic_prefix: &str, hub_location: &str, port: u8) -> String {
        format!("{}/{}/port/{}/attributes", topic_prefix, hub_location, port)
    }
}

impl MqttDiscoverySwitch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hub_location: &str,
        hub_vendor: &str,
        vendor_name: &str,
        product_name: &str,
        port: u8,
        node_id: &str,
        global_avail_topic: &str,
        topic_prefix: &str,
        _discovery_prefix: &str,
    ) -> Self {
        let safe_loc = safe_location(hub_location);
        let id = format!("{}_{}_p{}", topic_prefix, &safe_loc, port);
        let base_topic = format!("{}/{}/port/{}", topic_prefix, hub_location, port);
        let per_hub_avail = format!("{}/{}/status", topic_prefix, hub_location);

        MqttDiscoverySwitch {
            name: format!("USB Hub {} Port {}", hub_location, port),
            uniq_id: id,
            command_topic: format!("{}/set", base_topic),
            state_topic: format!("{}/state", base_topic),
            attributes_topic: format!("{}/attributes", base_topic),
            availability: availability_topics(global_avail_topic, &per_hub_avail),
            payload_on: "ON".to_string(),
            payload_off: "OFF".to_string(),
            state_on: "ON".to_string(),
            state_off: "OFF".to_string(),
            device: MqttDevice::new(
                node_id,
                topic_prefix,
                &safe_loc,
                hub_location,
                hub_vendor,
                vendor_name,
                product_name,
            ),
        }
    }

    pub fn config_topic(
        discovery_prefix: &str,
        node_id: &str,
        hub_location: &str,
        port: u8,
    ) -> String {
        let safe_loc = safe_location(hub_location);
        format!(
            "{}/switch/{}/p{}_{}/config",
            discovery_prefix, node_id, safe_loc, port
        )
    }

    pub fn state_topic(topic_prefix: &str, hub_location: &str, port: u8) -> String {
        format!("{}/{}/port/{}/state", topic_prefix, hub_location, port)
    }

    pub fn attributes_topic(topic_prefix: &str, hub_location: &str, port: u8) -> String {
        format!("{}/{}/port/{}/attributes", topic_prefix, hub_location, port)
    }

    pub fn command_topic_pattern(topic_prefix: &str) -> String {
        format!("{}/+/port/+/set", topic_prefix)
    }
}
