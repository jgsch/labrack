//! The output limiter (BEH-5, BEH-28), shown and edited in Vpk, Ohm, Watt or dBu on both families.
//!
//! * IPX: the amp stores all four figures. A threshold edit writes them together, as the vendor app
//!   does, so whichever one the DSP limits on has the new level. A load edit writes the load and the
//!   watts only: it must not move a threshold (the stored dBu may differ from the Vpk, as seen on a
//!   real amp, and nobody knows which one counts).
//! * IPD: the amp stores the dBu threshold. The load is the app's, per output, in the config.

use labgruppen::limiter as math;
use labgruppen::{Channel, LimiterField, Param, Value};

use super::{Hub, Session, lock};
use crate::dsp::Range;
use crate::limiter::{Limiter, Unit, snap};
use crate::{Error, Result};

const OUTPUTS: u8 = 2;

fn param(output: u8, field: LimiterField) -> Param {
    Param::Limiter { ch: Channel::Output(output), field }
}

fn is_ipx(session: &Session) -> bool {
    session.amp.info().driver == "ipx"
}

fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

impl Hub {
    /// The limiter of an output as the amp holds it, in the four units; `None` when the amp is not
    /// ONLINE, the state is not read yet, or there is no such output.
    pub fn limiter(&self, id: crate::AmpId, output: u8) -> Option<Limiter> {
        let session = self.session(id).ok()?;
        self.limiter_of(id, output, &session)
    }

    fn limiter_of(&self, id: crate::AmpId, output: u8, session: &Session) -> Option<Limiter> {
        if output >= OUTPUTS {
            return None;
        }
        let number = |field: LimiterField| -> Option<f64> {
            lock(&self.0.shared.live).get(&id)?.values.get(&param(output, field))?.as_f64()
        };
        if is_ipx(session) {
            Some(Limiter::from_vpk(number(LimiterField::Threshold)?, number(LimiterField::Impedance)?))
        } else {
            let ohm = *self.0.shared.store().get(id)?.load_ohms.get(usize::from(output))?;
            Some(Limiter::from_dbu(number(LimiterField::Threshold)?, ohm))
        }
    }

    /// The dBu range the amp takes for the threshold of an output.
    fn dbu_bounds(session: &Session, output: u8) -> Option<(f64, f64)> {
        let spec = session.amp.spec(&param(output, LimiterField::Threshold))?;
        let (min, max) = (spec.min?, spec.max?);
        // the IPX range is in Vpk; round away the error of the way back
        Some(if spec.unit == "Vpk" { (round6(math::dbu(min)), round6(math::dbu(max))) } else { (min, max) })
    }

    /// What the control of `unit` may be set to: the amp's range, ended by the user ceiling (SEC-3),
    /// in that unit. Watts need the load, which the app may not know yet.
    pub fn limiter_range(&self, id: crate::AmpId, output: u8, unit: Unit) -> Option<Range> {
        let session = self.session(id).ok()?;
        if output >= OUTPUTS {
            return None;
        }
        if unit == Unit::Ohm {
            let (min, max) = math::OHM_RANGE;
            return Some(Range { min: Some(min), max: Some(max), unit: unit.label() });
        }
        let (lo, hi) = Self::dbu_bounds(&session, output)?;
        let ceiling = self.0.shared.store().get(id)?.ceilings.max_limiter_dbu;
        let top = ceiling.map_or(hi, |c| c.min(hi));
        // the load is only needed for watts
        let ohm = self.limiter_of(id, output, &session).map(|l| l.ohm);
        let at = |dbu: f64| match (unit, ohm) {
            (Unit::Watt, None) => None,
            (_, ohm) => Some(Limiter::from_dbu(dbu, ohm.unwrap_or(0.0)).in_unit(unit)),
        };
        Some(Range { min: Some(at(lo)?), max: Some(at(top)?), unit: unit.label() })
    }

    /// Change the load of an output (BEH-5): the same as entering it in [`Unit::Ohm`].
    pub fn set_load(&self, id: crate::AmpId, output: u8, ohm: f64) -> Result<()> {
        self.set_limiter(id, output, Unit::Ohm, ohm)
    }

    /// Change the limiter of an output of an amp that is ONLINE: the threshold in Vpk, Watt or dBu, or
    /// the load in Ohm. Everything is checked before anything is sent: the range of the amp (SEC-2)
    /// and the user ceiling, which holds in every unit (SEC-3). The values are written like any
    /// control (SEC-4); what the amp answers arrives as [`crate::Update::Changed`].
    pub fn set_limiter(&self, id: crate::AmpId, output: u8, unit: Unit, value: f64) -> Result<()> {
        if output >= OUTPUTS {
            return Err(Error::NotControllable);
        }
        let session = self.session(id)?;
        let label = unit.label();
        if !value.is_finite() || (unit != Unit::Ohm && value <= 0.0) {
            return Err(Error::Invalid(format!("{label} must be a positive number")));
        }

        let ipx = is_ipx(&session);
        let load =
            || self.0.shared.store().get(id).and_then(|a| a.load_ohms.get(usize::from(output)).copied());
        let current = match self.limiter_of(id, output, &session) {
            Some(current) => current,
            // an IPD whose threshold is not read yet can still take a new load
            None if !ipx && unit == Ohm => Limiter::from_dbu(0.0, load().ok_or(Error::UnknownAmp)?),
            None => return Err(Error::NotLoaded),
        };

        let mut new = current.edit(unit, value);
        if unit == Unit::Ohm {
            let (min, max) = math::OHM_RANGE;
            if !(min..=max).contains(&value) {
                return Err(Error::Invalid(format!("The load must be between {min} and {max} Ohm")));
            }
        } else {
            let (lo, hi) = Self::dbu_bounds(&session, output).ok_or(Error::NotControllable)?;
            let ceiling = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings.max_limiter_dbu;
            let top = ceiling.map_or(hi, |c| c.min(hi));
            // an edge reached through another unit is the edge, not a float away from it
            let dbu = snap(new.dbu, (lo, top));
            if dbu != new.dbu {
                new = Limiter::from_dbu(dbu, current.ohm);
            }
            if !(lo..=top).contains(&new.dbu) {
                let bounds = |dbu| Limiter::from_dbu(dbu, current.ohm).in_unit(unit);
                return Err(match ceiling {
                    Some(c) if c < hi && new.dbu > top => {
                        Error::AboveCeiling { ceiling: bounds(top), unit: label }
                    }
                    _ => Error::Invalid(format!(
                        "{label} must be between {:.2} and {:.2}",
                        bounds(lo),
                        bounds(hi)
                    )),
                });
            }
        }

        let threshold = unit != Unit::Ohm;
        match (ipx, threshold) {
            (true, true) => session.writer.push_all([
                (param(output, LimiterField::Threshold), Value::Float(new.vpk)),
                (param(output, LimiterField::Impedance), Value::Float(new.ohm)),
                (param(output, LimiterField::Power), Value::Float(new.watt)),
                (param(output, LimiterField::ThresholdDbu), Value::Float(new.dbu)),
            ]),
            (true, false) => session.writer.push_all([
                (param(output, LimiterField::Impedance), Value::Float(new.ohm)),
                (param(output, LimiterField::Power), Value::Float(new.watt)),
            ]),
            (false, true) => {
                session.writer.push(param(output, LimiterField::Threshold), Value::Float(new.dbu))
            }
            (false, false) => {
                self.0.shared.store().set_load(id, output, new.ohm)?;
                self.0.shared.emit(crate::Update::Settings(id));
            }
        }
        Ok(())
    }
}

use Unit::Ohm;
