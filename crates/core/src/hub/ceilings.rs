//! What an amp holds above the user ceilings (SEC-3). The ceilings stop the app from setting a value
//! above them; a recalled preset or a state set elsewhere is out of the app's hands, so it is listed.

use labgruppen::{Channel, LimiterField, Param, Value};

use super::Hub;
use crate::Error;
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
        for output in 0..2u8 {
            let ch = Channel::Output(output);
            for param in [Param::Gain(ch), Param::Limiter { ch, field: LimiterField::Threshold }] {
                let Some(Value::Float(value)) = self.value(id, &param) else { continue };
                let Some(spec) = session.amp.spec(&param) else { continue };
                // the amp's own range is not the question; only the ceiling is
                if let Err(Error::AboveCeiling { ceiling, unit }) =
                    crate::dsp::check(&param, &spec, &Value::Float(value), &ceilings)
                {
                    above.push(Above { param, value, ceiling, unit });
                }
            }
        }
        above
    }
}
