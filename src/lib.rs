use mac_oui::Oui;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingReport {
    pub devices: HashMap<String, DevicePing>,
}

lazy_static::lazy_static! {
    pub static ref MAC_DB: Result<Oui, String> = Oui::default();
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePing {
    pub ping_ms: Option<u64>,
    pub local_address: Option<String>,
    pub mac: Option<String>,
    pub hostname: Option<String>,
    pub meta: Option<String>,
    pub friendly_name: Option<String>,

    #[serde(default)]
    pub is_agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError<T> {
    pub error: T,
}

impl<T> Display for ApiError<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        format!("API Error: {}", self.error).fmt(f)
    }
}

pub fn get_vendor_for_mac(mac: &str) -> Option<String> {
    MAC_DB
        .as_ref()
        .map(|db| {
            db.lookup_by_mac(mac)
                .ok()
                .flatten()
                .map(|entry| entry.company_name.clone())
        })
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_mac() {
        assert_eq!(
            super::get_vendor_for_mac("60:12:8b:8f:38:ac").unwrap(),
            "Apple, Inc".to_string()
        )
    }
}
