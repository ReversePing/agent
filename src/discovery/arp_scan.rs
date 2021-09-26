use super::reverse_dns::DiscoveredHost;
use libarp::client::ArpClient;
use std::{net::IpAddr, time::Duration};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("arp resolution io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct DiscoveredHostWithMac {
    pub host: DiscoveredHost,
    pub mac: String,
}

pub async fn scan(hosts: Vec<DiscoveredHost>) -> Vec<DiscoveredHostWithMac> {
    let arps = hosts
        .into_iter()
        .map(|h| async move { resolve_simple(h).await });

    futures::future::join_all(arps)
        .await
        .into_iter()
        .filter_map(std::convert::identity)
        .collect()
}

async fn resolve_simple(host: DiscoveredHost) -> Option<DiscoveredHostWithMac> {
    let mut client = ArpClient::new();

    let ip = match host.ip {
        IpAddr::V4(v4) => v4,
        _ => return None,
    };

    let mac = client
        .ip_to_mac(ip, Some(Duration::from_secs(2)))
        .await
        .ok()?;

    Some(DiscoveredHostWithMac {
        host,
        mac: mac.to_string(),
    })
}
