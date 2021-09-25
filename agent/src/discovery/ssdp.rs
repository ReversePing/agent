use futures::prelude::*;
use reqwest::Client;
use ssdp_client::SearchTarget;
use std::{collections::HashMap, net::IpAddr, str::FromStr, time::Duration};

#[derive(Debug, Clone)]
pub struct Service {
    pub location: String,
    pub ip: IpAddr,
    pub friendly_name: Option<String>,
    pub model_name: Option<String>,
    pub vendor: Option<String>,
}

pub async fn discover_services() -> Result<HashMap<IpAddr, Service>, Box<dyn std::error::Error>> {
    let search_target = SearchTarget::RootDevice;
    let mut responses = ssdp_client::search(&search_target, Duration::from_secs(3), 2).await?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let mut services = HashMap::<IpAddr, Service>::new();
    while let Some(Ok(search)) = responses.next().await {
        let url = match url::Url::parse(search.location()) {
            Ok(url) => url,
            _ => continue,
        };

        let ip = match url
            .host_str()
            .map(IpAddr::from_str)
            .map(Result::ok)
            .flatten()
        {
            Some(ip) => ip,
            _ => continue,
        };

        if services.contains_key(&ip) {
            continue;
        }

        let device = match get_service_description(&client, search.location()).await {
            Ok(dev) => dev.device,
            Err(e) => {
                dbg!(e);
                continue;
            }
        };

        services.insert(
            ip,
            Service {
                location: search.location().to_string(),
                ip,
                friendly_name: device.friendly_name,
                model_name: device.model_name,
                vendor: device.manufacturer,
            },
        );
    }

    Ok(services)
}

#[derive(Debug, serde::Deserialize)]
struct DescriptionRoot {
    device: Device,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    friendly_name: Option<String>,
    manufacturer: Option<String>,
    model_name: Option<String>,
}

async fn get_service_description(
    client: &Client,
    location: &str,
) -> Result<DescriptionRoot, Box<dyn std::error::Error>> {
    let xml = client.get(location).send().await?.text().await?;
    Ok(serde_xml_rs::from_str(&xml)?)
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_discovery() {
        let res = super::discover_services().await.expect("msg");
        dbg!(res);
    }
}
