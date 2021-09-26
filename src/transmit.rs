use std::collections::HashMap;

use crate::discovery::{DeviceName, DiscoveredDevice};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to transmit ping report")]
    Send(#[from] reqwest::Error),
    #[error("server transmit failed: {0}")]
    Server(reverseping::ApiError<String>),
}

#[derive(Clone)]
pub struct Transmitter {
    agent_id: String,
    client: reqwest::Client,
}

impl Transmitter {
    const API_ORIGIN: &'static str = "https://api.reverseping.net";

    pub fn new<S: Into<String>>(agent_id: S) -> Self {
        Self {
            agent_id: agent_id.into(),
            client: reqwest::Client::new(),
        }
    }
    pub async fn send(&self, devices: HashMap<DeviceName, DiscoveredDevice>) -> Result<(), Error> {
        let api_origin = std::env::var("API_ORIGIN").unwrap_or(Self::API_ORIGIN.to_string());
        let url = format!("{}/{}", api_origin, &self.agent_id);

        let devices = devices
            .into_iter()
            .map(|(name, device)| {
                let DiscoveredDevice {
                    hostname,
                    local_address,
                    mac,
                    meta,
                    ping_ms,
                    vendor: _,
                }: DiscoveredDevice = device;
                (
                    name,
                    reverseping::DevicePing {
                        hostname,
                        local_address: Some(local_address.to_string()),
                        mac: Some(mac),
                        meta,
                        ping_ms: Some(ping_ms as u64),
                        friendly_name: None,
                        is_agent: false
                    },
                )
            })
            .collect();

        let report = reverseping::PingReport { devices };
        let response = self.client.post(url).json(&report).send().await?;
        if !response.status().is_success() {
            let error = response.json().await?;
            return Err(Error::Server(error));
        }

        Ok(())
    }
}
