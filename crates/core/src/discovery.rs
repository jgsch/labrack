use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::Duration;

use async_trait::async_trait;
use labgruppen::amps::{ipd, ipx};

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
/// * IPX: listens for the amps' UDP broadcasts.
/// * IPD: listens for their UDP announcements, and probes every address of the /24 subnet of the
///   computer's default route, as the vendor app does (the amps were not seen announcing).
pub struct Lan;

#[async_trait]
impl Finder for Lan {
    async fn find(&self, protocol: Protocol, window: Duration) -> labgruppen::Result<Vec<Ipv4Addr>> {
        match protocol {
            Protocol::Ipx => {
                let heard = ipx::discover(window).await?;
                // the address in the packet is the one the vendor app connects to
                Ok(heard.iter().filter_map(|d| address(&d.ip).or_else(|| address(&d.source))).collect())
            }
            Protocol::Ipd => {
                let subnet = local_ipv4().map(subnet_of);
                let (heard, probed) = tokio::join!(ipd::listen(window), async {
                    match &subnet {
                        Some(subnet) => ipd::scan(subnet, window).await,
                        None => Vec::new(),
                    }
                });
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

/// The address the computer uses towards the outside. Connecting a UDP socket only picks the route;
/// nothing is sent.
fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// `192.168.1` for 192.168.1.20, the form `ipd::scan` takes.
fn subnet_of(ip: Ipv4Addr) -> String {
    let [a, b, c, _] = ip.octets();
    format!("{a}.{b}.{c}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // BEH-18
    #[test]
    fn the_scanned_subnet_is_the_slash_24_of_the_computer() {
        assert_eq!(subnet_of("192.168.1.20".parse().unwrap()), "192.168.1");
        assert_eq!(subnet_of("10.0.255.1".parse().unwrap()), "10.0.255");
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
