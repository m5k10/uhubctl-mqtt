use log::{info, warn};
use std::collections::HashMap;

pub struct UsbIds {
    vendors: HashMap<u16, String>,
    products: HashMap<(u16, u16), String>,
}

impl UsbIds {
    pub fn load(custom_paths: Option<&[&str]>) -> Option<Self> {
        let default_paths: &[&str] = &["/usr/share/usb.ids", "/usr/share/hwdata/usb.ids"];
        let paths = custom_paths.unwrap_or(default_paths);
        for path in paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                info!("Loaded USB IDs from {}", path);
                return Some(Self::parse(&content));
            }
        }
        let tried = paths.join(", ");
        warn!("No USB IDs file found at {}", tried);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_with_custom_path() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "1234  Vendor Name\n\t5678  Product Name\n").unwrap();
        let path = tmp.path().to_str().unwrap();
        let ids = UsbIds::load(Some(&[path]));
        assert!(ids.is_some());
        let ids = ids.unwrap();
        assert_eq!(ids.lookup_vendor(0x1234), Some("Vendor Name"));
        assert_eq!(ids.lookup_product(0x1234, 0x5678), Some("Product Name"));
    }

    #[test]
    fn test_load_with_multiple_paths() {
        let mut tmp1 = tempfile::NamedTempFile::new().unwrap();
        write!(tmp1, "AAAA  First Vendor\n").unwrap();
        let mut tmp2 = tempfile::NamedTempFile::new().unwrap();
        write!(tmp2, "BBBB  Second Vendor\n").unwrap();
        let paths = [
            "/nonexistent/path/usb.ids",
            tmp1.path().to_str().unwrap(),
            tmp2.path().to_str().unwrap(),
        ];
        let ids = UsbIds::load(Some(&paths));
        assert!(ids.is_some());
        let ids = ids.unwrap();
        // Stops at first readable file (tmp1 with "First Vendor")
        assert_eq!(ids.lookup_vendor(0xAAAA), Some("First Vendor"));
        assert_eq!(ids.lookup_vendor(0xBBBB), None);
    }

    #[test]
    fn test_load_with_nonexistent_path() {
        let ids = UsbIds::load(Some(&["/nonexistent/usb.ids"]));
        assert!(ids.is_none());
    }

    #[test]
    fn test_load_with_defaults() {
        let ids = UsbIds::load(None);
        let _ = ids;
    }
}
