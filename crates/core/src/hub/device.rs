//! Settings of the amp itself: the front-panel display (BEH-49) and the network of an IPD (BEH-50). Both are
//! changed only while the amp is ONLINE and not locked.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use labgruppen::{DisplayMode, Network};
use tokio::time::timeout;

use super::{Hub, Shared, Update, lock, wait_loaded};
use crate::dsp::Writer;
use crate::store::AmpId;
use crate::{Error, Result};

impl Hub {
    /// The front-panel display of the amp (BEH-49); `None` when it is not ONLINE or has not told it.
    pub fn display(&self, id: AmpId) -> Option<DisplayMode> {
        lock(&self.0.shared.live).get(&id)?.display
    }

    /// Set the front-panel display (BEH-49). When the amp does not take it, what it has is read again.
    pub async fn set_display(&self, id: AmpId, display: DisplayMode) -> Result<()> {
        let session = self.session(id)?;
        self.permit(id, |rights| rights.complete())?;
        match session.amp.set_display(display).await {
            Ok(()) => {
                self.0.shared.set_display(id, &session.writer, display);
                Ok(())
            }
            Err(e) => {
                if let Ok(Ok(Some(now))) = timeout(Duration::from_secs(3), session.amp.display()).await {
                    self.0.shared.set_display(id, &session.writer, now);
                }
                Err(e.into())
            }
        }
    }

    /// The network settings of the amp (BEH-50); an IPX has none in the app.
    pub async fn network(&self, id: AmpId) -> Result<Network> {
        Ok(self.session(id)?.amp.network().await?)
    }

    /// Change the network settings of an IPD (BEH-50). Checked before anything is written; the amp then
    /// restarts its network. A fixed address becomes the amp's address in the list, so the app connects there.
    pub async fn set_network(&self, id: AmpId, network: Network) -> Result<()> {
        let session = self.session(id)?;
        self.permit(id, |rights| rights.complete())?;
        if let Some(ip) = network.ip {
            check_network(ip, network.mask)?;
            let taken = self.amps().iter().any(|a| a.id != id && a.ip.parse() == Ok(ip));
            if taken {
                return Err(Error::DuplicateIp);
            }
        }
        session.amp.set_network(network).await?;
        if let Some(ip) = network.ip {
            self.set_ip(id, &ip.to_string())?;
        }
        Ok(())
    }
}

/// A fixed address: a contiguous mask from /8 to /30, and an address that is a host of its subnet and not a
/// loopback, multicast or reserved one.
pub fn check_network(ip: Ipv4Addr, mask: Ipv4Addr) -> Result<()> {
    let m = u32::from(mask);
    let prefix = m.leading_ones();
    if prefix + m.trailing_zeros() != 32 || !(8..=30).contains(&prefix) {
        return Err(Error::Invalid("The mask must be like 255.255.255.0 (from /8 to /30)".into()));
    }
    let first = ip.octets()[0];
    let host = u32::from(ip) & !m;
    if first == 0 || first == 127 || first >= 224 || host == 0 || host == !m {
        return Err(Error::Invalid(format!("{ip} is not a usable address with the mask {mask}")));
    }
    Ok(())
}

impl Shared {
    /// The amp of the connection of `token` has this display now; views are told if it differs.
    fn set_display(&self, id: AmpId, token: &Arc<Writer>, display: DisplayMode) {
        let changed = {
            let mut live = lock(&self.live);
            let current =
                |amp: &super::Live| amp.session.as_ref().is_some_and(|s| Arc::ptr_eq(&s.writer, token));
            let Some(amp) = live.get_mut(&id).filter(|amp| current(amp)) else { return };
            let changed = amp.display != Some(display);
            amp.display = Some(display);
            changed
        };
        if changed {
            self.emit(Update::Display(id));
        }
    }
}

/// The display is read once the state is read (BEH-49); the app changes it, and then knows it.
pub(super) async fn read_display(
    shared: Arc<Shared>,
    id: AmpId,
    amp: Arc<dyn labgruppen::Amp>,
    writer: Arc<Writer>,
) {
    if !wait_loaded(&shared, id).await {
        return;
    }
    if let Ok(Ok(Some(display))) = timeout(Duration::from_secs(5), amp.display()).await {
        shared.set_display(id, &writer, display);
    }
}
