use super::ping::PingResult;
use itertools::Itertools;
use std::{net::IpAddr, time::Duration, vec};
use thiserror::Error;
use trust_dns_resolver::error::ResolveError;

use std::str::FromStr;
use tokio::net::UdpSocket;
use trust_dns_client::{
    client::{AsyncClient, ClientHandle},
    proto::error::ProtoError,
};
use trust_dns_client::{
    error::ClientError,
    rr::{DNSClass, Name, RData, RecordType},
};
use trust_dns_client::{
    op::Query,
    proto::{xfer::DnsRequestOptions, DnsHandle},
    udp::UdpClientStream,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("dns resolution error")]
    Dns(#[from] ResolveError),
    #[error("dns proto error")]
    DnsProto(#[from] ProtoError),
    #[error("dns client error")]
    DnsClient(#[from] ClientError),
}

#[derive(Debug, Clone)]
pub struct DiscoveredHost {
    pub ip: IpAddr,
    pub ping_duration: Duration,
    pub hostname: Option<String>,
    pub meta: Vec<String>,
}

pub async fn reverse_dns(pings: Vec<PingResult>) -> Result<Vec<DiscoveredHost>, Error> {
    let reverse_lookups = pings
        .into_iter()
        .map(|ping| async move { reverse_dns_ip(ping).await });

    futures::future::join_all(reverse_lookups)
        .await
        .into_iter()
        .collect()
}

#[derive(Debug)]
struct ResolvedHost {
    hostname: Option<String>,
    meta: Vec<String>,
}

async fn resolve2(ip: IpAddr) -> Result<ResolvedHost, Error> {
    let stream = UdpClientStream::<UdpSocket>::new((ip, 5353).into());
    let client = AsyncClient::connect(stream);

    let (mut client, bg) = client.await?;

    tokio::spawn(bg);

    // Create a query future
    let arpa_name = arpa_name(ip);
    let result = client
        .query(Name::from_str(&arpa_name)?, DNSClass::ANY, RecordType::ANY)
        .await;
    dbg!(&result);

    let mut resp = match result {
        Ok(r) => r,
        Err(_) => {
            return Ok(ResolvedHost {
                hostname: None,
                meta: vec![],
            })
        }
    };

    // get first PTR hostname
    let mut hostname = None;
    let mut responses = resp.take_answers().into_iter();
    while let Some(RData::PTR(name)) = responses.next().map(|r| r.rdata().clone()) {
        hostname = Some(name.to_string());
        break;
    }

    // collect txt info
    let meta = resp
        .additionals()
        .iter()
        .map(|r| r.rdata())
        .map(|r| match r {
            RData::TXT(val) => Some(val.to_string()),
            _ => None,
        })
        .filter_map(std::convert::identity)
        .collect();

    Ok(ResolvedHost { hostname, meta })
}

async fn reverse_dns_ip(ping: PingResult) -> Result<DiscoveredHost, Error> {
    let ResolvedHost { hostname, meta } = resolve2(ping.ip).await?;

    Ok(DiscoveredHost {
        hostname,
        meta,
        ip: ping.ip,
        ping_duration: ping.duration,
    })
}

fn arpa_name(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => format!(
            "{}.in-addr.arpa",
            v4.octets().to_vec().iter().rev().join(".")
        ),
        IpAddr::V6(v6) => format!(
            "{}.ip6.arpa",
            v6.octets()
                .to_vec()
                .iter()
                .rev()
                .map(|o| hex::encode([*o]).chars().rev().join("."))
                .join(".")
        ),
    }
}

#[allow(unused)]
async fn resolve3(ip: IpAddr, service: &str) -> Result<(), Error> {
    // let (mdns_stream, mdns_handle) = MdnsClientStream::new_ipv4(MdnsQueryType::OneShot, None, None);
    // let (mut client, mdns_bg) = AsyncClient::new(mdns_stream, mdns_handle, None).await?;
    // tokio::spawn(mdns_bg);

    //
    let stream =
        UdpClientStream::<UdpSocket>::with_timeout((ip, 5353).into(), Duration::from_secs(10));
    let (mut client, bg) = AsyncClient::connect(stream).await?;
    tokio::spawn(bg);

    let name = Name::from_str(service)?;
    let mut query = Query::query(name, RecordType::ANY);
    query.set_query_class(DNSClass::ANY);
    // query.set_mdns_unicast_response(false);

    let mut result = client
        .lookup(
            query,
            DnsRequestOptions {
                expects_multiple_responses: true,
                use_edns: false,
            },
        )
        .await?;
    dbg!(&result);
    let answers = result
        .take_answers()
        .into_iter()
        .map(|r| r.to_string())
        .collect::<Vec<String>>();
    let add = result
        .take_additionals()
        .into_iter()
        .map(|r| r.to_string())
        .collect::<Vec<String>>();
    dbg!(answers);
    dbg!(add);
    Ok(())
}

#[cfg(test)]
mod tests {
    use trust_dns_resolver::dns_sd::DnsSdHandle;

    use super::*;

    #[tokio::test]
    async fn test_resolution() {
        pretty_env_logger::init();

        // dig -x 192.168.4.211 @224.0.0.251 -p 5353
        let ip: IpAddr = "192.168.4.42".parse().expect("invalid ip");
        let result = reverse_dns_ip(PingResult {
            ip,
            duration: Default::default(),
        })
        .await
        .expect("failed to resolve");

        assert!(result.hostname.is_some());
    }

    #[test]
    fn test_arpa_name() {
        let n1 = arpa_name("192.168.0.14".parse().unwrap());
        assert_eq!(n1, "14.0.168.192.in-addr.arpa".to_string());

        let n2 = arpa_name("fe80::c68:428d:340d:c9b5".parse().unwrap());
        assert_eq!(
            n2,
            "5.b.9.c.d.0.4.3.d.8.2.4.8.6.c.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.e.f.ip6.arpa".to_string()
        );
    }

    #[tokio::test]
    async fn test_resolution_2() {
        pretty_env_logger::init();
        let host = resolve2("192.168.4.23".parse().unwrap())
            .await
            .expect("failed");

        assert_eq!(host.hostname.unwrap(), "xenos.local.");
        assert_eq!(host.meta, vec!["model=iMac18,3osxvers=20".to_string()]);
    }

    // #[tokio::test]
    // async fn test_resolution_3() {
    //     std::env::set_var("RUST_LOG", "debug");
    //     pretty_env_logger::init();

    //     resolve3("224.0.0.251".parse().unwrap())
    //         .await
    //         .expect("failed");
    // }

    #[tokio::test]
    async fn test_dnssd() {
        std::env::set_var("RUST_LOG", "debug");
        pretty_env_logger::init();

        let ip: IpAddr = "224.0.0.251".parse().unwrap();

        let multiscast_ns = NameServerConfig {
            socket_addr: SocketAddr::from((ip, 5353)),
            protocol: trust_dns_resolver::config::Protocol::Udp,
            tls_dns_name: None,
            trust_nx_responses: true,
        };

        let config = ResolverConfig::from_parts(None, vec![], vec![multiscast_ns]);
        let opts = ResolverOpts {
            timeout: Duration::from_secs(2),
            ..Default::default()
        };

        let resolver = TokioAsyncResolver::new(config, opts, TokioHandle).unwrap();

        const ALL: &'static str = "_services._dns-sd._udp.local";
        const HTTP: &'static str = "_http._tcp.local";
        let name = Name::from_str(ALL).unwrap();

        let res = resolver
            .lookup(
                name,
                RecordType::ANY,
                DnsRequestOptions {
                    expects_multiple_responses: true,
                    use_edns: false,
                },
            )
            .await
            .unwrap();
        let r: Vec<String> = res.iter().map(|f| f.to_string()).collect();
        dbg!(r);
    }

    #[tokio::test]
    async fn test_client_dns_sd() {
        std::env::set_var("RUST_LOG", "debug");
        pretty_env_logger::init();

        const ALL: &'static str = "_services._dns-sd._udp.local";
        const HTTP: &'static str = "_http._tcp.local";
        const AD: &'static str = "_airdrop._tcp.local";

        let ip: IpAddr = "224.0.0.251".parse().unwrap();
        let r = super::resolve3(ip, AD).await.unwrap();
        dbg!(r);
    }
}
