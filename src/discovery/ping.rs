use futures::future;
use ipnetwork::IpNetworkError;
use itertools::Itertools;
use std::{net::IpAddr, time::Duration};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid ip address found")]
    IpError(#[from] IpNetworkError),
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
    let pingers = ips.into_iter().map(ping);

    future::join_all(pingers)
        .await
        .into_iter()
        .filter_map(Result::ok)
        .map(|(ip, duration)| PingResult { ip, duration })
        .collect()
}

#[cfg(not(windows))]
async fn ping(ip: IpAddr) -> Result<(IpAddr, Duration), Box<dyn std::error::Error>> {
    let mut pinger = surge_ping::Pinger::new(ip)?;
    pinger.timeout(Duration::from_secs(2));
    let (_, duration) = pinger.ping(0).await?;
    Ok((ip, duration))
}

#[cfg(windows)]
async fn ping(ip: IpAddr) -> Result<(IpAddr, Duration), Box<dyn std::error::Error>> {
    let mut pinger = winping::AsyncPinger::new();
    pinger.set_timeout(2);
    let buf = winping::Buffer::with_data(vec![0]);
    let duration = pinger.send(ip, buf).await.result?;
    Ok((ip, duration))
}
