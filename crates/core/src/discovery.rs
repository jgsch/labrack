use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use async_trait::async_trait;
use labgruppen::amps::{ipd, ipx};
use tokio::task::JoinSet;

use crate::Protocol;

/// How amps are looked for. The app uses [`Lan`]; tests say what is "heard" and note what is searched.
/// A finder searches for one protocol only, for `window`, and is only called for protocols that are
/// switched on (BEH-17).
#[async_trait]
pub trait Finder: Send + Sync + 'static {
    async fn find(&self, protocol: Protocol, window: Duration) -> labgruppen::Result<Vec<Ipv4Addr>>;
}

/// The local network.
///
/// * IPX: listens for the amps' UDP broadcasts, and asks every address of the /24 subnet of each network
///   interface on the control port: the broadcasts were never seen on a real network.
/// * IPD: listens for their UDP announcements, and probes every address of the /24 subnet of each
///   network interface of the computer, as the vendor app does for one network (the amps were not seen
///   announcing). The amps need not be on the network of the default route.
pub struct Lan;

#[async_trait]
impl Finder for Lan {
    async fn find(&self, protocol: Protocol, window: Duration) -> labgruppen::Result<Vec<Ipv4Addr>> {
        match protocol {
            Protocol::Ipx => {
                let subnets = subnets_of(local_ipv4s());
                let (heard, probed) = tokio::join!(ipx::discover(window), scan_ipx(subnets, window));
                let mut found: Vec<Ipv4Addr> = probed.iter().filter_map(|(ip, _)| address(ip)).collect();
                match heard {
                    // the address in the packet is the one the vendor app connects to
                    Ok(heard) => found
                        .extend(heard.iter().filter_map(|d| address(&d.ip).or_else(|| address(&d.source)))),
                    Err(e) if found.is_empty() => return Err(e),
                    Err(_) => {}
                }
                found.sort();
                found.dedup();
                Ok(found)
            }
            Protocol::Ipd => {
                let subnets = subnets_of(local_ipv4s());
                let (heard, probed) = tokio::join!(ipd::listen(window), scan_all(subnets, window));
                let mut found: Vec<Ipv4Addr> = probed.iter().filter_map(|(ip, _)| address(ip)).collect();
                match heard {
                    Ok(heard) => found.extend(heard.iter().filter_map(|(ip, _)| address(ip))),
                    Err(e) if found.is_empty() => return Err(e),
                    Err(_) => {}
                }
                Ok(found)
            }
        }
    }
}

fn address(text: &str) -> Option<Ipv4Addr> {
    text.trim().parse().ok()
}

/// The IPv4 addresses of the computer's network interfaces.
fn local_ipv4s() -> Vec<Ipv4Addr> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    interfaces
        .into_iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .collect()
}

/// The /24 subnets (`192.168.1`) to probe: those of the given addresses, each once, in order.
/// Loopback, link-local and unspecified addresses lead to no amp.
fn subnets_of(addresses: impl IntoIterator<Item = Ipv4Addr>) -> Vec<String> {
    let mut subnets: Vec<String> = addresses
        .into_iter()
        .filter(|ip| !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified())
        .map(subnet_of)
        .collect();
    subnets.sort();
    subnets.dedup();
    subnets
}

/// Probe all the subnets at the same time, so that the scan takes one window and not one per network.
async fn scan_all(subnets: Vec<String>, window: Duration) -> Vec<(String, ipd::Announcement)> {
    let mut set = JoinSet::new();
    for subnet in subnets {
        set.spawn(async move { ipd::scan(&subnet, window).await });
    }
    let mut found = Vec::new();
    while let Some(hits) = set.join_next().await {
        found.extend(hits.unwrap_or_default());
    }
    found
}

/// Ask all the subnets for an IPX at the same time.
async fn scan_ipx(subnets: Vec<String>, window: Duration) -> Vec<(String, String)> {
    let mut set = JoinSet::new();
    for subnet in subnets {
        set.spawn(async move { ipx::scan(&subnet, window).await });
    }
    let mut found = Vec::new();
    while let Some(hits) = set.join_next().await {
        found.extend(hits.unwrap_or_default());
    }
    found
}

/// `192.168.1` for 192.168.1.20, the form `ipd::scan` and `ipx::scan` take.
fn subnet_of(ip: Ipv4Addr) -> String {
    let [a, b, c, _] = ip.octets();
    format!("{a}.{b}.{c}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    // BEH-18
    #[test]
    fn the_scanned_subnet_is_the_slash_24_of_an_address() {
        assert_eq!(subnet_of(ip("192.168.1.20")), "192.168.1");
        assert_eq!(subnet_of(ip("10.0.255.1")), "10.0.255");
    }

    // BEH-18: every network the computer is on is scanned, not only the one of its default route
    #[test]
    fn every_network_of_the_computer_is_scanned_once() {
        let own = [ip("192.168.0.5"), ip("192.168.1.20"), ip("192.168.1.21"), ip("10.0.0.7")];
        assert_eq!(subnets_of(own), ["10.0.0", "192.168.0", "192.168.1"]);
    }

    // BEH-18: addresses that lead nowhere an amp could be are not probed
    #[test]
    fn loopback_link_local_and_unspecified_addresses_are_not_scanned() {
        let own = [ip("127.0.0.1"), ip("169.254.3.4"), ip("0.0.0.0"), ip("172.17.0.1")];
        assert_eq!(subnets_of(own), ["172.17.0"]);
        assert!(subnets_of([]).is_empty());
    }

    // BEH-19: only IPv4 text counts as an address
    #[test]
    fn addresses_are_ipv4_text() {
        assert_eq!(address(" 10.0.0.1 "), Some("10.0.0.1".parse().unwrap()));
        for bad in ["", "abc", "10.0.0", "::1", "10.0.0.256"] {
            assert_eq!(address(bad), None, "{bad:?}");
        }
    }
}
