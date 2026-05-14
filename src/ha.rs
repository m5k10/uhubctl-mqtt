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
}

impl MqttDiscoverySwitch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hub_location: &str,
        hub_vendor: &str,
        vendor_name: &str,
        product_name: &str,
        port: u8,
        global_avail_topic: &str,
        topic_prefix: &str,
        _discovery_prefix: &str,
    ) -> Self {
        let safe_loc = hub_location.replace(['.', '-'], "_");
        let id = format!("{}_{}_p{}", topic_prefix, &safe_loc, port);
        let base_topic = format!("{}/{}/port/{}", topic_prefix, hub_location, port);
        let per_hub_avail = format!("{}/{}/status", topic_prefix, hub_location);

        MqttDiscoverySwitch {
            name: format!("USB Hub {} Port {}", hub_location, port),
            uniq_id: id,
            command_topic: format!("{}/set", base_topic),
            state_topic: format!("{}/state", base_topic),
            attributes_topic: format!("{}/attributes", base_topic),
            availability: vec![
                AvailabilityTopic {
                    topic: global_avail_topic.to_string(),
                    payload_available: "online".to_string(),
                    payload_not_available: "offline".to_string(),
                },
                AvailabilityTopic {
                    topic: per_hub_avail,
                    payload_available: "online".to_string(),
                    payload_not_available: "offline".to_string(),
                },
            ],
            payload_on: "ON".to_string(),
            payload_off: "OFF".to_string(),
            state_on: "ON".to_string(),
            state_off: "OFF".to_string(),
            device: MqttDevice {
                identifiers: vec![format!("{}_{}", topic_prefix, &safe_loc)],
                name: format!("USB Hub {}", hub_location),
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
            },
        }
    }

    pub fn config_topic(
        discovery_prefix: &str,
        topic_prefix: &str,
        hub_location: &str,
        port: u8,
    ) -> String {
        let safe_loc = hub_location.replace(['.', '-'], "_");
        format!(
            "{}/switch/{}/p{}_{}/config",
            discovery_prefix, topic_prefix, safe_loc, port
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
