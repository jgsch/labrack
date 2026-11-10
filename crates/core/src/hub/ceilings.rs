//! What an amp holds above the user ceilings (SEC-3). The ceilings stop the app from setting a value
//! above them; a recalled preset or a state set elsewhere is out of the app's hands, so it is listed.

use labgruppen::{Channel, LimiterField, Param, Value};

use super::Hub;
use crate::store::AmpId;

/// A setting of the amp that is above a ceiling.
#[derive(Clone, Debug, PartialEq)]
pub struct Above {
    pub param: Param,
    pub value: f64,
    /// The ceiling, in the unit of the setting.
    pub ceiling: f64,
    pub unit: &'static str,
}

impl Hub {
    /// The output gains and limiter thresholds the amp holds above the ceilings of the amp, in list
    /// order of outputs; empty when the amp is not ONLINE, its state is not read, or nothing is above.
    /// Nothing is written: lowering a ceiling or recalling a preset moves no control (SEC-3).
    pub fn above_ceilings(&self, id: AmpId) -> Vec<Above> {
        let Ok(session) = self.session(id) else { return Vec::new() };
        let Some(ceilings) = self.0.shared.store().get(id).map(|amp| amp.ceilings) else { return Vec::new() };
        let mut above = Vec::new();
        // an IPX holds the threshold as Vpk and as dBu, which can disagree: the dBu is the one held to
        // the ceiling (an IPD has the one)
        let threshold = if session.is_ipx() { LimiterField::ThresholdDbu } else { LimiterField::Threshold };
        for output in 0..2u8 {
            let ch = Channel::Output(output);
            let shown = [Param::Gain(ch), Param::Limiter { ch, field: LimiterField::Threshold }];
            for param in shown {
                let held = match &param {
                    Param::Limiter { .. } => Param::Limiter { ch, field: threshold },
                    other => other.clone(),
                };
                let Some(Value::Float(value)) = self.value(id, &held) else { continue };
                let Some(spec) = session.amp.spec(&held) else { continue };
                // the amp's own range is not the question; only the ceiling is
                if let Some((ceiling, unit)) = crate::dsp::above_ceiling(&held, spec.unit, value, &ceilings) {
                    above.push(Above { param, value, ceiling, unit });
                }
            }
        }
        above
    }
}
