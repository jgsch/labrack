//! Driver registry. To add a new amp family: add a module here, give it a [`Driver`] variant and a
//! match arm in [`connect`] and [`Driver::port`].

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::amp::Amp;
use crate::error::{Error, Result};

#[cfg(feature = "ipd")]
pub mod ipd;
#[cfg(feature = "ipx")]
pub mod ipx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    Ipx,
    Ipd,
}

impl Driver {
    pub const ALL: &'static [Driver] = &[Driver::Ipx, Driver::Ipd];

    pub fn name(self) -> &'static str {
        match self {
            Driver::Ipx => "ipx",
            Driver::Ipd => "ipd",
        }
    }

    pub fn from_name(s: &str) -> Option<Driver> {
        Driver::ALL.iter().copied().find(|d| d.name().eq_ignore_ascii_case(s))
    }

    /// TCP control port.
    pub fn port(self) -> u16 {
        match self {
            Driver::Ipx => 50168,
            Driver::Ipd => 50675,
        }
    }
}

/// Which driver's control port is open on `host`. Errors when none or several are.
pub async fn detect(host: &str) -> Result<Driver> {
    let probe = |d: Driver| {
        let addr = format!("{host}:{}", d.port());
        async move {
            // Over Wi-Fi an IPD can take more than a second to accept.
            matches!(timeout(Duration::from_secs(4), TcpStream::connect(addr)).await, Ok(Ok(_))).then_some(d)
        }
    };
    // First open port wins; a closed port would otherwise cost the whole timeout.
    let (mut a, mut b) = (std::pin::pin!(probe(Driver::Ipx)), std::pin::pin!(probe(Driver::Ipd)));
    let (mut a_done, mut b_done) = (false, false);
    while !(a_done && b_done) {
        let hit = tokio::select! {
            r = &mut a, if !a_done => { a_done = true; r }
            r = &mut b, if !b_done => { b_done = true; r }
        };
        if let Some(d) = hit {
            return Ok(d);
        }
    }
    Err(Error::Timeout(format!("no amp control port open on {host}")))
}

/// Connect to an amp. `driver = None` detects the family from the open control port.
pub async fn connect(host: &str, driver: Option<Driver>) -> Result<Arc<dyn Amp>> {
    let driver = match driver {
        Some(d) => d,
        None => {
            let d = detect(host).await?;
            // The IPX serves one controller; give it a moment to notice the probe connection closed.
            tokio::time::sleep(Duration::from_millis(300)).await;
            d
        }
    };
    match driver {
        #[cfg(feature = "ipx")]
        Driver::Ipx => Ok(Arc::new(ipx::IpxAmp::connect(host).await?)),
        #[cfg(feature = "ipd")]
        Driver::Ipd => Ok(Arc::new(ipd::IpdAmp::connect(host).await?)),
        #[allow(unreachable_patterns)]
        d => Err(Error::Unsupported(format!("driver {} is not compiled in", d.name()))),
    }
}
