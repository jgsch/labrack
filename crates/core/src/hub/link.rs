//! Linking channels (BEH-31 to BEH-35). Channel 1 is the master of a pair and channel 2 follows it:
//!
//! * IPD: the amp mirrors writes on channel 1 by itself, and a write on channel 2 is ignored and may
//!   leave it unanswering (docs/ipd-protocol.md). The app writes channel 1 only.
//! * IPX: the amp copies the gain and the limiter dBu once when the link is switched on and then keeps
//!   the channels independent. The app writes channel 1 and channel 2 together.
//!
//! What follows is in [`crate::link`]. Linking copies all of it from channel 1 to channel 2 first, with
//! the link switched off, so it needs nothing of what the amps copy by themselves and never writes
//! channel 2 of a linked IPD. The copy is a bulk change (`SEC-6`): the outputs are muted around it.

use std::time::Duration;

use labgruppen::{Channel, Param, Value};
use tokio::time::timeout;

use super::presets::End;
use super::{Hub, Session, lock};
use crate::link::{self, Side};
use crate::store::AmpId;
use crate::{Error, Result, dsp};

/// Two values of a pair are the same if they are equal, or numbers closer than the amps tell apart: the
/// IPX reads a single Watt back as an integer and keeps 4 decimals, while a value the app wrote is kept
/// as it was written. Within 0.2 %, and at least 0.002, is not a difference worth a warning.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(_) | Value::Int(_), Value::Float(_) | Value::Int(_)) => {
            let (x, y) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
            (x - y).abs() <= 0.002 * x.abs().max(y.abs()).max(1.0)
        }
        _ => a == b,
    }
}

impl Hub {
    /// Whether the pair is linked, as far as the app knows.
    pub fn linked(&self, id: AmpId, side: Side) -> bool {
        let live = lock(&self.0.shared.live);
        live.get(&id).and_then(|amp| amp.values.get(&side.flag())) == Some(&Value::Bool(true))
    }

    /// The settings of channel 1 of a linked pair that channel 2 does not have, by name; empty when the
    /// pair is not linked or the state is not read. An IPD keeps a pair equal itself, so this is for the
    /// IPX: its flag only says the app should keep the pair equal, and a change made on the amp itself
    /// (front panel) leaves channel 2 behind. [`Hub::set_link`] with `true` brings it back.
    pub fn link_drift(&self, id: AmpId, side: Side) -> Vec<Param> {
        let live = lock(&self.0.shared.live);
        let Some(amp) = live.get(&id).filter(|amp| amp.values.get(&side.flag()) == Some(&Value::Bool(true)))
        else {
            return Vec::new();
        };
        let differs = |param: &Param, value: &Value| {
            link::second(param)
                .and_then(|twin| amp.values.get(&twin))
                .is_some_and(|other| !same(value, other))
        };
        let mut drift: Vec<Param> = amp
            .values
            .iter()
            .filter(|(param, _)| link::follows(param) == Some(side) && !link::is_second(param))
            .filter(|(param, value)| differs(param, value))
            .map(|(param, _)| param.clone())
            .collect();
        drift.sort_by_key(ToString::to_string);
        drift
    }

    /// The channels a view shows for a side: a linked pair is one channel (BEH-33, BEH-34).
    pub fn channels(&self, id: AmpId, side: Side) -> Vec<Channel> {
        let (first, second) = match side {
            Side::Inputs => (Channel::Input(0), Channel::Input(1)),
            Side::Outputs => (Channel::Output(0), Channel::Output(1)),
        };
        if self.linked(id, side) { vec![first] } else { vec![first, second] }
    }

    /// Where a write of `param` has to go. A setting of a linked pair goes to channel 1, and also to
    /// channel 2 on the IPX; a write to channel 2 of a linked pair is refused.
    pub(super) fn route(&self, id: AmpId, session: &Session, param: &Param) -> Result<Vec<Param>> {
        let Some(side) = link::follows(param) else { return Ok(vec![param.clone()]) };
        if !self.linked(id, side) {
            return Ok(vec![param.clone()]);
        }
        if link::is_second(param) {
            return Err(Error::Linked);
        }
        let mut targets = vec![param.clone()];
        if session.is_ipx() {
            targets.extend(link::second(param));
        }
        Ok(targets)
    }

    /// Link or unlink a pair (BEH-31, BEH-35). Linking makes channel 2 equal to channel 1 (on an IPX that
    /// is already linked, this is how a drifted pair is synced): every
    /// shared setting is read from channel 1, checked like any value (ranges, ceilings), and written
    /// to channel 2 with the outputs muted, then the link is switched on; the mutes end as they were.
    /// If a value is not acceptable nothing is written. Unlinking changes no value.
    pub async fn set_link(&self, id: AmpId, side: Side, linked: bool) -> Result<()> {
        let session = self.session(id)?;
        let flag = side.flag();
        let shared = &self.0.shared;

        if !linked {
            let _gate = session.gate.lock().await;
            session.amp.set(&flag, Value::Bool(false)).await?;
            shared.observed(id, &flag, &Value::Bool(false), true);
            return Ok(());
        }

        // an IPD keeps a linked pair equal itself, and channel 2 of a linked IPD must not be written
        if !session.is_ipx() && self.linked(id, side) {
            return Ok(());
        }

        let copy = self.plan_copy(id, &session, side).await?;
        let (to_write, switch) = (copy.clone(), flag.clone());
        self.bulk(id, &session, End::AsBefore, true, |amp| async move {
            for result in amp.set_many(&to_write).await {
                result?;
            }
            amp.set(&switch, Value::Bool(true)).await
        })
        .await?;

        // the state for the views: channel 2 first, while the pair is not yet linked in it
        for (param, value) in &copy {
            shared.observed(id, param, value, true);
        }
        shared.observed(id, &flag, &Value::Bool(true), true);
        Ok(())
    }

    /// The writes that make channel 2 equal to channel 1, all checked.
    async fn plan_copy(&self, id: AmpId, session: &Session, side: Side) -> Result<Vec<(Param, Value)>> {
        let masters: Vec<Param> = session
            .amp
            .params()
            .into_iter()
            .filter(|param| link::follows(param) == Some(side) && !link::is_second(param))
            .collect();
        let values = timeout(Duration::from_secs(30), session.amp.get_many(&masters))
            .await
            .map_err(|_| labgruppen::Error::Timeout("reading channel 1".into()))??;
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;

        let mut copy = Vec::new();
        for (master, value) in masters.iter().zip(values) {
            let Some(twin) = link::second(master) else { continue };
            let Some(spec) = session.amp.spec(&twin).filter(|spec| spec.writable) else { continue };
            let checked = dsp::check(&twin, &spec, &value, &ceilings)
                .map_err(|e| Error::Invalid(format!("Cannot link: {twin}: {e}")))?;
            copy.push((twin, checked));
        }
        Ok(copy)
    }
}
