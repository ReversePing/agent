use futures::future;
use ipnetwork::IpNetworkError;
use itertools::Itertools;
use std::{net::IpAddr, time::Duration};
use surge_ping::SurgeError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid ip address found")]
    IpError(#[from] IpNetworkError),
    #[error("failed to ping device")]
    SurgePing(#[from] SurgeError),
}

#[derive(Debug)]
pub struct PingResult {
    pub ip: IpAddr,
    pub duration: Duration,
}

pub async fn ping_subnet(ip: IpAddr, mask: IpAddr) -> Result<Vec<PingResult>, Error> {
    let network = ipnetwork::IpNetwork::with_netmask(ip, mask)?;
    dbg!(network);

    let chunked = network
        .into_iter()
        .chunks(200)
        .into_iter()
        .map(|chunk| chunk.collect_vec())
        .collect::<Vec<Vec<IpAddr>>>();

    let mut results = vec![];
    for chunk in chunked {
        let next = ping_ips(chunk).await;
        results.extend(next);
    }

    Ok(results)
}

async fn ping_ips(ips: Vec<IpAddr>) -> Vec<PingResult> {
    let pingers = ips
        .into_iter()
        .map(surge_ping::Pinger::new)
        .filter_map(Result::ok)
        .map(|mut pinger| {
            pinger.timeout(Duration::from_secs(2));
            async move { pinger.ping(0).await }
        });

    future::join_all(pingers)
        .await
        .into_iter()
        .filter_map(Result::ok)
        .map(|(reply, duration)| PingResult {
            ip: reply.source,
            duration,
        })
        .collect()
}
