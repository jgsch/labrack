//! Linking channels (BEH-31 to BEH-35). Channel 1 is the master of a pair and channel 2 follows it, in
//! the level only (what follows is in [`crate::link`]):
//!
//! * IPX: the amp keeps the link flag, copies the gain and the limiter dBu once when it is switched
//!   on and then keeps the channels independent. The app writes channel 1 and channel 2 together.
//! * IPD: the amp's own flag mirrors everything of channel 1 (EQ included) and ignores channel 2, so
//!   the app does not switch it on: the link is the app's, kept in the config, and the app writes both
//!   channels. A flag found on is respected: the amp keeps channel 2 equal, and a write of channel 2 is
//!   ignored and may leave the amp unanswering (docs/ipd-protocol.md), so it is refused.
//!
//! Linking copies the shared settings from channel 1 to channel 2 first, so it needs nothing of what
//! the amps copy by themselves. The copy is a bulk change (`SEC-6`): the outputs are muted around it.
//! The same machinery copies the EQ of a channel to the other one ([`Hub::copy_eq`], BEH-33).

use std::collections::HashMap;

use labgruppen::{Channel, Param, Value};

use super::presets::End;
use super::{Hub, Session, lock};
use crate::link::{self, Side};
use crate::store::AmpId;
use crate::{Error, Result, dsp};

/// The settings of channel 1 that channel 2 does not have, by name, in `values`.
pub(super) fn side_drift(values: &HashMap<Param, Value>, side: Side) -> Vec<Param> {
    let differs = |param: &Param, value: &Value| {
        link::second(param).and_then(|twin| values.get(&twin)).is_some_and(|other| !same(value, other))
    };
    let mut drift: Vec<Param> = values
        .iter()
        .filter(|(param, _)| link::follows(param) == Some(side) && !link::is_second(param))
        .filter(|(param, value)| differs(param, value))
        .map(|(param, _)| param.clone())
        .collect();
    drift.sort_by_key(ToString::to_string);
    drift
}

/// Two values of a pair are the same if they are equal, or numbers closer than the amps tell apart: the
/// IPX reads a single Watt back as an integer and keeps 4 decimals, while a value the app wrote is kept
/// as it was written. Within 0.2 %, and at least 0.002, is not a difference worth a warning.
pub(super) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(_) | Value::Int(_), Value::Float(_) | Value::Int(_)) => {
            let (x, y) = (a.as_f64().unwrap_or(f64::NAN), b.as_f64().unwrap_or(f64::NAN));
            (x - y).abs() <= 0.002 * x.abs().max(y.abs()).max(1.0)
        }
        _ => a == b,
    }
}

impl Hub {
    /// Whether the pair is shown linked: the amp flags it linked and its channels are equal. The app
    /// shows what is stored: a pair that is flagged linked but whose channels differ is shown as two
    /// channels ([`Hub::link_diverged`]).
    pub fn linked(&self, id: AmpId, side: Side) -> bool {
        let kept = self.0.shared.store().get(id).is_some_and(|amp| amp.links[side as usize]);
        let live = lock(&self.0.shared.live);
        live.get(&id).is_some_and(|amp| {
            let flagged = amp.values.get(&side.flag()) == Some(&Value::Bool(true));
            match amp.session.as_ref() {
                Some(session) if !session.is_ipx() => flagged || kept,
                _ => flagged && !amp.diverged[side as usize],
            }
        })
    }

    /// Whether the IPD's own flag is on for the pair: the amp then keeps all of channel 2 equal to
    /// channel 1 and ignores what is written to it (BEH-32).
    pub fn amp_keeps_equal(&self, id: AmpId, side: Side) -> bool {
        let live = lock(&self.0.shared.live);
        live.get(&id).is_some_and(|amp| {
            amp.session.as_ref().is_some_and(|session| !session.is_ipx())
                && amp.values.get(&side.flag()) == Some(&Value::Bool(true))
        })
    }

    /// The amp flags the pair linked, but its channels differ, so the pair is shown as two channels with
    /// the values stored. Found when the state is read, or when the amp reports a change (front panel);
    /// the app writes nothing for it (`SEC-1`). The channels are shown linked again if they become equal
    /// on the amp. [`Hub::set_link`] with `true` makes channel 2 equal to channel 1; with `false` it
    /// clears the flag.
    pub fn link_diverged(&self, id: AmpId, side: Side) -> bool {
        let live = lock(&self.0.shared.live);
        live.get(&id).is_some_and(|amp| amp.diverged[side as usize])
    }

    /// The settings of channel 1 of a flagged pair that channel 2 does not have, by name; empty when the
    /// pair is not flagged linked or the state is not read. For saying what differs.
    pub fn link_drift(&self, id: AmpId, side: Side) -> Vec<Param> {
        let live = lock(&self.0.shared.live);
        match live.get(&id).filter(|amp| amp.values.get(&side.flag()) == Some(&Value::Bool(true))) {
            Some(amp) => side_drift(&amp.values, side),
            None => Vec::new(),
        }
    }

    /// The channels a view shows for a side: a linked pair is one channel (BEH-33, BEH-34).
    pub fn channels(&self, id: AmpId, side: Side) -> Vec<Channel> {
        let (first, second) = match side {
            Side::Inputs => (Channel::Input(0), Channel::Input(1)),
            Side::Outputs => (Channel::Output(0), Channel::Output(1)),
        };
        if self.linked(id, side) { vec![first] } else { vec![first, second] }
    }

    /// Where a write of `param` has to go. A shared setting of a linked pair goes to channel 1 and to
    /// channel 2; a write to channel 2 of a linked pair is refused. An IPD that links a pair itself
    /// mirrors all of channel 1 (BEH-32): channel 1 only, and channel 2 of anything it mirrors is refused.
    pub(super) fn route(&self, id: AmpId, session: &Session, param: &Param) -> Result<Vec<Param>> {
        if !session.is_ipx()
            && let Some(side) = link::mirrored(param)
            && self.amp_keeps_equal(id, side)
        {
            return if link::is_second(param) { Err(Error::Linked) } else { Ok(vec![param.clone()]) };
        }
        let Some(side) = link::follows(param) else { return Ok(vec![param.clone()]) };
        if !self.linked(id, side) {
            return Ok(vec![param.clone()]);
        }
        if link::is_second(param) {
            return Err(Error::Linked);
        }
        Ok(std::iter::once(param.clone()).chain(link::second(param)).collect())
    }

    /// Link or unlink a pair (BEH-31, BEH-35). Linking makes channel 2 equal to channel 1 in what the
    /// pair shares: every shared setting is read from channel 1, checked like any value (ranges,
    /// ceilings), and written to channel 2 with the outputs muted; the mutes end as they were. An IPX
    /// then gets its flag switched on in the amp; an IPD never does (BEH-32), its link is kept in the
    /// config. If a value is not acceptable nothing is written. Unlinking changes no value.
    pub async fn set_link(&self, id: AmpId, side: Side, linked: bool) -> Result<()> {
        let session = self.session(id)?;
        let flag = side.flag();
        self.permit_channel(id, flag.channel())?;
        let shared = &self.0.shared;
        let ipx = session.is_ipx();

        if !linked {
            // an IPD whose own flag is on (front panel, another tool) gets it switched off: the user asks.
            // The app's own link writes nothing, so it does not wait for the amp (the mute polling holds it
            // for as long as an IPD takes to answer)
            if ipx || self.amp_keeps_equal(id, side) {
                let _gate = session.gate.lock().await;
                session.amp.set(&flag, Value::Bool(false)).await?;
                shared.observed(id, &flag, &Value::Bool(false), true);
            }
            shared.store().set_link(id, side as usize, false)?;
            if shared.set_diverged(id, side, false) {
                shared.emit(crate::Update::LinkChanged(id, side));
            }
            shared.emit(crate::Update::Settings(id));
            return Ok(());
        }

        if !self.loaded(id) {
            return Err(Error::NotLoaded);
        }

        // nothing to do for a pair shown linked. A pair shown as two channels (an IPX that flags it
        // linked while its channels differ) copies channel 1 to channel 2 and shows it linked again.
        if self.linked(id, side) {
            return Ok(());
        }

        let copy = self.plan_copy(id, &session, side).await?;
        let (to_write, switch) = (copy.clone(), flag.clone());
        self.bulk(id, &session, End::AsBefore, true, |amp| async move {
            for result in amp.set_many(&to_write).await {
                result?;
            }
            if ipx { amp.set(&switch, Value::Bool(true)).await } else { Ok(()) }
        })
        .await?;

        // the state for the views: channel 2 first, while the pair is not yet linked in it
        for (param, value) in &copy {
            shared.observed(id, param, value, true);
        }
        if ipx {
            shared.observed(id, &flag, &Value::Bool(true), true);
        } else {
            // the load of an IPD limiter is the app's: a linked pair has one
            let mut store = shared.store();
            if side == Side::Outputs
                && let Some(load) = store.get(id).map(|amp| amp.load_ohms[0])
            {
                store.set_load(id, 1, load)?;
            }
            store.set_link(id, side as usize, true)?;
        }
        if shared.set_diverged(id, side, false) {
            shared.emit(crate::Update::LinkChanged(id, side));
        }
        shared.emit(crate::Update::Settings(id));
        Ok(())
    }

    /// Copy the EQ of the other channel of the pair to `to` (BEH-33): the ten bands and, on outputs, the
    /// HPF and LPF; not the delay. Read from the state, checked like any value (ranges), written with
    /// the outputs muted; the mutes end as they were. The channels are not linked by it.
    pub async fn copy_eq(&self, id: AmpId, to: Channel) -> Result<()> {
        let session = self.session(id)?;
        self.permit_channel(id, to)?;
        if !self.loaded(id) {
            return Err(Error::NotLoaded);
        }
        let side = link::side_of(to);
        if !session.is_ipx() && self.amp_keeps_equal(id, side) {
            return Err(Error::Linked);
        }
        let from = match to {
            Channel::Input(n) => Channel::Input(1 - n.min(1)),
            Channel::Output(n) => Channel::Output(1 - n.min(1)),
        };
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;
        let mut copy = Vec::new();
        for (param, value) in self.values(id) {
            let twin = match param {
                Param::Eq { ch, band, field } if ch == from => Param::Eq { ch: to, band, field },
                Param::Xover { ch, filter, field } if ch == from => Param::Xover { ch: to, filter, field },
                _ => continue,
            };
            let Some(spec) = session.amp.spec(&twin).filter(|spec| spec.writable) else { continue };
            let checked = dsp::check(&twin, &spec, &value, &ceilings)
                .map_err(|e| Error::Invalid(format!("Cannot copy: {twin}: {e}")))?;
            copy.push((twin, checked));
        }
        let to_write = copy.clone();
        self.bulk(id, &session, End::AsBefore, true, |amp| async move {
            for result in amp.set_many(&to_write).await {
                result?;
            }
            Ok(())
        })
        .await?;
        for (param, value) in &copy {
            self.0.shared.observed(id, param, value, true);
        }
        Ok(())
    }

    /// The writes that make channel 2 equal to channel 1, all checked.
    async fn plan_copy(&self, id: AmpId, session: &Session, side: Side) -> Result<Vec<(Param, Value)>> {
        // channel 1 is read from the state the app holds, which is complete (the state is read) and kept
        // current by the amp's own reports: asking an IPD again takes seconds
        let state = self.values(id);
        let (masters, values): (Vec<Param>, Vec<Value>) = session
            .amp
            .params()
            .into_iter()
            .filter(|param| link::follows(param) == Some(side) && !link::is_second(param))
            .filter_map(|param| state.get(&param).cloned().map(|value| (param, value)))
            .unzip();
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;

        let mut copy = Vec::new();
        for (master, value) in masters.iter().zip(values) {
            let Some(twin) = link::second(master) else { continue };
            let Some(spec) = session.amp.spec(&twin).filter(|spec| spec.writable) else { continue };
            // channel 1 may hold a hair more than the range (the vendor app stores 36.02 dBu for the
            // 35.99 dBu of 69 Vpk): copy the nearest value in range. The ceilings still decide below.
            let value = match (value.as_f64(), &value) {
                (Some(x), Value::Float(_)) => Value::Float(
                    x.clamp(spec.min.unwrap_or(f64::NEG_INFINITY), spec.max.unwrap_or(f64::INFINITY)),
                ),
                _ => value,
            };
            let checked = dsp::check(&twin, &spec, &value, &ceilings)
                .map_err(|e| Error::Invalid(format!("Cannot link: {twin}: {e}")))?;
            copy.push((twin, checked));
        }
        Ok(copy)
    }
}
