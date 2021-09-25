mod arp_scan;
mod ping;
mod reverse_dns;
mod ssdp;

use reverse_dns::reverse_dns;
use std::{
    collections::HashMap,
    fmt::Display,
    iter::FromIterator,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
use tokio::net::UdpSocket;

/// an attempt at a uniquely identifiable name for the device
pub type DeviceName = String;

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub local_address: IpAddr,
    pub ping_ms: u128,
    pub hostname: Option<String>,
    pub mac: String,
    pub vendor: Option<String>,
    pub meta: Option<String>,
}

impl Display for DiscoveredDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = format!(
            "{} - {} - {} - {} - {} ({}ms)",
            self.mac,
            self.vendor.as_ref().map(|s| s.as_str()).unwrap_or("?"),
            &self.local_address,
            self.hostname.as_ref().map(|s| s.as_str()).unwrap_or("?"),
            self.meta.as_ref().map(|s| s.as_str()).unwrap_or("?"),
            self.ping_ms
        );
        f.write_str(data.as_str())
    }
}

pub async fn discover_devices(
) -> Result<HashMap<DeviceName, DiscoveredDevice>, Box<dyn std::error::Error>> {
    // 0. discover the network settings: our IP + netmask
    let network_iface = get_network_interface_ip_with_masks().await?;

    // 1. ping entire range
    let results = ping::ping_subnet(network_iface.ip, network_iface.mask).await?;
    // dbg!(&results);

    // 2. dns reverse lookup
    let results = reverse_dns(results).await?;
    // dbg!(&results);

    // 3. arp scan (get mac addresses)
    let results = arp_scan::scan(results).await;
    // dbg!(&results);

    // 4. check upnp devices with ssdp
    let services = ssdp::discover_services().await.ok().unwrap_or_default();

    let discovered = results.into_iter().map(|mut device| {
        let name = device.mac.clone();

        let service = services.get(&device.host.ip);
        if let Some(model) = service.map(|s| s.model_name.clone()).flatten() {
            device.host.meta.push(model);
        }

        let hostname = device
            .host
            .hostname
            .or(service.map(|s| s.friendly_name.clone()).flatten());

        let vendor = librp::get_vendor_for_mac(&device.mac);

        let meta = if device.host.meta.is_empty() {
            None
        } else {
            Some(device.host.meta.join(", "))
        };

        (
            name,
            DiscoveredDevice {
                local_address: device.host.ip,
                hostname: hostname,
                mac: device.mac,
                ping_ms: device.host.ping_duration.as_millis(),
                vendor,
                meta,
            },
        )
    });

    Ok(HashMap::from_iter(discovered))
}

#[derive(Debug, Clone)]
struct Iface {
    name: String,
    ip: IpAddr,
    mask: IpAddr,
}

async fn get_network_interface_ip_with_masks() -> Result<Iface, Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect("8.8.8.8:80").await?;
    let local_addr = socket.local_addr()?.ip();

    let ifaces =
        ifcfg::IfCfg::get().map_err(|e| format!("error getting network config: {:?}", e))?;

    let result = ifaces
        .into_iter()
        .map(|f| {
            let name = f.name.clone();
            f.addresses
                .into_iter()
                .filter(|addr| addr.address.map(|a| a.ip()) == Some(local_addr))
                .map(|addr| match (addr.address, addr.mask) {
                    (Some(ip), Some(mask)) => Some(Iface {
                        name: name.clone(),
                        ip: ip.ip(),
                        mask: mask.ip(),
                    }),
                    (Some(ip), None) => Some(Iface {
                        name: name.clone(),
                        ip: ip.ip(),
                        mask: if ip.is_ipv4() {
                            IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0))
                        } else {
                            IpAddr::V6(Ipv6Addr::new(255, 255, 255, 255, 255, 255, 255, 0))
                        },
                    }),
                    _ => None,
                })
                .collect::<Vec<Option<Iface>>>()
        })
        .flatten()
        .filter_map(std::convert::identity)
        .next()
        .ok_or("No network interface found")?;

    Ok(result)
}
