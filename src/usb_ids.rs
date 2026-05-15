use log::{debug, info};
use std::collections::HashMap;

pub struct UsbIds {
    vendors: HashMap<u16, String>,
    products: HashMap<(u16, u16), String>,
}

impl UsbIds {
    pub fn load() -> Option<Self> {
        let paths = ["/usr/share/usb.ids", "/usr/share/hwdata/usb.ids"];
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                info!("Loaded USB IDs from {}", path);
                return Some(Self::parse(&content));
            }
        }
        debug!("No USB IDs file found at /usr/share/usb.ids or /usr/share/hwdata/usb.ids");
        None
    }

    fn parse(content: &str) -> Self {
        let mut vendors: HashMap<u16, String> = HashMap::new();
        let mut products: HashMap<(u16, u16), String> = HashMap::new();
        let mut current_vid: Option<u16> = None;

        for line in content.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('\t') {
                let tab_count = line.bytes().take_while(|&b| b == b'\t').count();
                if tab_count > 1 {
                    continue;
                }
                let content = line[tab_count..].trim();
                if content.len() >= 4
                    && let Ok(pid) = u16::from_str_radix(&content[..4], 16)
                {
                    let name = content[4..].trim().to_string();
                    if !name.is_empty()
                        && let Some(vid) = current_vid
                    {
                        products.entry((vid, pid)).or_insert(name);
                    }
                }
            } else if line.len() >= 4
                && line.as_bytes()[0].is_ascii_hexdigit()
                && line.as_bytes()[1].is_ascii_hexdigit()
                && line.as_bytes()[2].is_ascii_hexdigit()
                && line.as_bytes()[3].is_ascii_hexdigit()
            {
                if let Ok(vid) = u16::from_str_radix(&line[..4], 16) {
                    let name = line[4..].trim().to_string();
                    if !name.is_empty() {
                        vendors.entry(vid).or_insert(name);
                    }
                    current_vid = Some(vid);
                } else {
                    current_vid = None;
                }
            } else {
                current_vid = None;
            }
        }

        info!(
            "Parsed USB IDs: {} vendors, {} products",
            vendors.len(),
            products.len()
        );

        UsbIds { vendors, products }
    }

    pub fn lookup_vendor(&self, vid: u16) -> Option<&str> {
        self.vendors.get(&vid).map(|s| s.as_str())
    }

    pub fn lookup_product(&self, vid: u16, pid: u16) -> Option<&str> {
        self.products.get(&(vid, pid)).map(|s| s.as_str())
    }
}
