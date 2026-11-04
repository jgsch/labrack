use std::sync::Arc;

use async_trait::async_trait;
use labgruppen::{Amp, Driver};

use crate::Protocol;

/// How a connection to an amp is made. The app uses [`Network`]; tests reach the IPD simulator and
/// the fake IPX through their own.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    async fn connect(&self, protocol: Protocol, ip: &str) -> labgruppen::Result<Arc<dyn Amp>>;
}

/// The amps' control ports on the network.
pub struct Network;

#[async_trait]
impl Connector for Network {
    async fn connect(&self, protocol: Protocol, ip: &str) -> labgruppen::Result<Arc<dyn Amp>> {
        let driver = match protocol {
            Protocol::Ipd => Driver::Ipd,
            Protocol::Ipx => Driver::Ipx,
        };
        labgruppen::connect(ip, Some(driver)).await
    }
}
