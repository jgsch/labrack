//! Mapping between the common [`Param`] vocabulary and IPX parameter addresses, plus value codecs.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::protocol::{Address, Env};
use super::structs::cstr;
use crate::error::{Error, Result};
use crate::model::*;

/// Wire type of a parameter. Everything is a little-endian 4-byte word except text (16 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    /// i32 0/1.
    Bool,
    Int,
    /// f32.
    Float,
    /// i32 index into the label list.
    Enum(&'static [&'static str]),
    /// 16 bytes, latin-1, NUL-terminated.
    Text,
}

/// Where a parameter lives and how it is encoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub addr: Address,
    pub ty: Ty,
    pub unit: &'static str,
    pub writable: bool,
}

impl Slot {
    const fn new(env: u8, channel: u8, item: u8, field: u8, ty: Ty) -> Self {
        Slot { addr: Address::new(env, channel, item, field), ty, unit: "", writable: true }
    }

    const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    const fn read_only(mut self) -> Self {
        self.writable = false;
        self
    }

    pub fn spec(&self) -> ParamSpec {
        let kind = match self.ty {
            Ty::Bool => Kind::Bool,
            Ty::Int => Kind::Int,
            Ty::Float => Kind::Float,
            Ty::Enum(labels) => Kind::Enum(labels),
            Ty::Text => Kind::Text,
        };
        let spec = ParamSpec::new(kind).unit(self.unit);
        if self.writable { spec } else { spec.read_only() }
    }

    /// Number of bytes the value occupies on the wire.
    pub fn wire_len(&self) -> usize {
        if self.ty == Ty::Text { 16 } else { 4 }
    }

    pub fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let bad = |want: &str| Error::InvalidValue(format!("expected {want}, got {value}"));
        let word = match self.ty {
            Ty::Bool => (value.as_bool().ok_or_else(|| bad("on/off"))? as i32).to_le_bytes(),
            Ty::Int => (value.as_f64().ok_or_else(|| bad("an integer"))?.round() as i32).to_le_bytes(),
            Ty::Float => (value.as_f64().ok_or_else(|| bad("a number"))? as f32).to_le_bytes(),
            Ty::Enum(labels) => {
                let index = match value {
                    Value::Text(t) => labels.iter().position(|l| l.eq_ignore_ascii_case(t)),
                    other => {
                        other.as_i64().filter(|i| (0..labels.len() as i64).contains(i)).map(|i| i as usize)
                    }
                };
                (index.ok_or_else(|| bad(&format!("one of {}", labels.join(", "))))? as i32).to_le_bytes()
            }
            Ty::Text => return Err(Error::Unsupported("text parameters are read-only".into())),
        };
        Ok(word.to_vec())
    }

    /// Decode the reply to a single GETPRM. The limiter's derived power field (env 17, field 2) comes
    /// back as an integer there, but as a float inside the bulk blocks (see [`Slot::decode`]).
    pub fn decode_single(&self, payload: &[u8]) -> Result<Value> {
        if self.addr.env == Env::OUTPUT_LIMITER
            && self.addr.field == 2
            && let Ok(word) = <[u8; 4]>::try_from(payload)
        {
            return Ok(Value::Float(i32::from_le_bytes(word) as f64));
        }
        self.decode(payload)
    }

    pub fn decode(&self, payload: &[u8]) -> Result<Value> {
        if self.ty == Ty::Text {
            return Ok(Value::Text(cstr(payload)));
        }
        let word: [u8; 4] = payload.try_into().map_err(|_| {
            Error::Protocol(format!("expected 4 bytes at {:?}, got {}", self.addr, payload.len()))
        })?;
        Ok(match self.ty {
            Ty::Bool => Value::Bool(i32::from_le_bytes(word) != 0),
            Ty::Int => Value::Int(i32::from_le_bytes(word) as i64),
            Ty::Float => Value::Float((f32::from_le_bytes(word) as f64 * 1e4).round() / 1e4),
            Ty::Enum(labels) => {
                let i = i32::from_le_bytes(word);
                let label = usize::try_from(i).ok().and_then(|i| labels.get(i));
                Value::Text(
                    label
                        .ok_or_else(|| {
                            Error::Protocol(format!("enum code {i} out of range at {:?}", self.addr))
                        })?
                        .to_string(),
                )
            }
            Ty::Text => unreachable!(),
        })
    }
}

/// Address and encoding of `param`, `None` when the IPX does not have it.
pub fn slot(param: &Param) -> Option<Slot> {
    let ch = param.channel();
    let c = ch.index();
    if c >= 2 {
        return None;
    }
    let input = ch.is_input();
    let level_env = if input { Env::INPUT_LEVEL } else { Env::OUTPUT_LEVEL };
    let s = |env, item, field, ty| Slot::new(env, c, item, field, ty);
    Some(match *param {
        Param::Gain(_) => s(level_env, 0, 0, Ty::Float).unit("dB"),
        Param::Mute(_) => s(level_env, 0, 1, Ty::Bool),
        Param::Polarity(_) if !input => s(level_env, 0, 2, Ty::Bool),
        Param::Polarity(_) => return None,
        Param::Link(_) => s(level_env, 0, if input { 2 } else { 3 }, Ty::Bool),
        Param::Name(_) => s(level_env, 0, if input { 3 } else { 4 }, Ty::Text).read_only(),
        Param::Delay(_) => {
            s(if input { Env::INPUT_DELAY } else { Env::OUTPUT_DELAY }, 0, 1, Ty::Float).unit("ms")
        }
        Param::Eq { band, field, .. } if band < EQ_BANDS => {
            let env = if input { Env::INPUT_EQ } else { Env::OUTPUT_EQ };
            match field {
                EqField::Enabled => s(env, band, 0, Ty::Bool),
                EqField::Type => s(env, band, 1, Ty::Enum(EQ_TYPES)),
                EqField::Freq => s(env, band, 2, Ty::Int).unit("Hz"),
                EqField::Gain => s(env, band, 3, Ty::Float).unit("dB"),
                EqField::Q => s(env, band, 4, Ty::Float),
            }
        }
        Param::Eq { .. } => return None,
        Param::Xover { filter, field, .. } if !input => {
            let item = match filter {
                XoverFilter::Highpass => 0,
                XoverFilter::Lowpass => 1,
            };
            match field {
                XoverField::Enabled => s(Env::OUTPUT_XOVER, item, 0, Ty::Bool),
                XoverField::Type => s(Env::OUTPUT_XOVER, item, 1, Ty::Enum(XOVER_TYPES)),
                XoverField::Freq => s(Env::OUTPUT_XOVER, item, 2, Ty::Int).unit("Hz"),
            }
        }
        Param::Xover { .. } => return None,
        Param::Compressor { field, .. } => {
            let env = if input { Env::INPUT_COMPRESSOR } else { Env::OUTPUT_COMPRESSOR };
            match field {
                CompressorField::Threshold => s(env, 0, 0, Ty::Float),
                CompressorField::Attack => s(env, 0, 1, Ty::Int).unit("ms"),
                CompressorField::Hold => s(env, 0, 2, Ty::Int).unit("ms"),
                CompressorField::Release => s(env, 0, 3, Ty::Int).unit("ms"),
                CompressorField::Ratio => s(env, 0, 4, Ty::Float),
                CompressorField::Gain => s(env, 0, 5, Ty::Float).unit("dB"),
            }
        }
        Param::Limiter { field, .. } if input => match field {
            LimiterField::Threshold => s(Env::INPUT_RMS_LIMITER, 0, 0, Ty::Float).unit("dB"),
            LimiterField::Release => s(Env::INPUT_RMS_LIMITER, 0, 1, Ty::Int).unit("ms"),
            _ => return None,
        },
        Param::Limiter { field, .. } => {
            let env = Env::OUTPUT_LIMITER;
            match field {
                LimiterField::Threshold => s(env, 0, 0, Ty::Float).unit("Vpk"),
                LimiterField::Impedance => s(env, 0, 1, Ty::Float).unit("ohm"),
                LimiterField::Power => s(env, 0, 2, Ty::Float).unit("W"),
                LimiterField::ThresholdDbu => s(env, 0, 3, Ty::Float).unit("dBu"),
                LimiterField::Release => s(env, 0, 4, Ty::Int).unit("ms"),
            }
        }
        Param::Mixer { src, field, .. } if input && (src as usize) < MIXER_SOURCES.len() => match field {
            MixerField::Level => s(Env::INPUT_MIXER, src, 0, Ty::Float).unit("dB"),
            MixerField::Mute => s(Env::INPUT_MIXER, src, 1, Ty::Bool),
        },
        Param::Mixer { .. } => return None,
    })
}

/// The dBu range of an output limiter threshold: the IPD's floor and its highest ceiling (4800). The IPX
/// reports no range, and a real IPX 1200 holds 69 Vpk = 35.98 dBu, above the 33.8 dBu of the IPD 1200,
/// so the ceiling of the same model number does not do. The user ceilings are the tight limit.
pub const LIMITER_DBU: (f64, f64) = (16.78, 39.83);
/// The load impedance, in ohm, that can be entered. It only scales the watts.
pub const LIMITER_OHM: (f64, f64) = (0.5, 100.0);
/// The watts that follow from the other two ranges, rounded up.
pub const LIMITER_WATT: (f64, f64) = (0.0, 12_000.0);

fn limiter_range(param: &Param) -> Option<(f64, f64)> {
    match *param {
        Param::Limiter { ch: Channel::Output(_), field } => Some(match field {
            LimiterField::Threshold => {
                (limiter::vpk_from_dbu(LIMITER_DBU.0), limiter::vpk_from_dbu(LIMITER_DBU.1))
            }
            LimiterField::Impedance => LIMITER_OHM,
            LimiterField::Power => LIMITER_WATT,
            LimiterField::ThresholdDbu => LIMITER_DBU,
            LimiterField::Release => return None,
        }),
        _ => None,
    }
}

/// What the IPX accepts: the slot's type and unit, and a range for the basic controls (shared with the
/// IPD) and the output limiter.
pub fn spec(param: &Param) -> Option<ParamSpec> {
    let spec = slot(param)?.spec();
    Some(match shared_range(param).or_else(|| limiter_range(param)) {
        Some((min, max)) => spec.range(min, max),
        None => spec,
    })
}

fn build_all() -> Vec<Param> {
    use LimiterField::*;
    let mut candidates = Vec::new();
    for ch in [Channel::Input(0), Channel::Input(1), Channel::Output(0), Channel::Output(1)] {
        candidates.extend([Param::Gain(ch), Param::Mute(ch), Param::Polarity(ch), Param::Link(ch)]);
        candidates.extend([Param::Name(ch), Param::Delay(ch)]);
        for band in 0..EQ_BANDS {
            candidates.extend(EqField::ALL.iter().map(|&field| Param::Eq { ch, band, field }));
        }
        for filter in [XoverFilter::Highpass, XoverFilter::Lowpass] {
            candidates.extend(XoverField::ALL.iter().map(|&field| Param::Xover { ch, filter, field }));
        }
        candidates.extend(CompressorField::ALL.iter().map(|&field| Param::Compressor { ch, field }));
        candidates.extend(
            [Threshold, Impedance, Power, ThresholdDbu, Release].map(|field| Param::Limiter { ch, field }),
        );
        for src in 0..MIXER_SOURCES.len() as u8 {
            candidates.extend(MixerField::ALL.iter().map(|&field| Param::Mixer { ch, src, field }));
        }
    }
    candidates.retain(|p| slot(p).is_some());
    candidates
}

/// All supported parameters in canonical order: inputs, then outputs, each in table order.
pub fn all() -> &'static [Param] {
    static ALL: OnceLock<Vec<Param>> = OnceLock::new();
    ALL.get_or_init(build_all)
}

/// The parameter at `addr`, if it has a common name (used to turn change notifications into events).
pub fn param_for(addr: Address) -> Option<&'static Param> {
    static REVERSE: OnceLock<HashMap<Address, Param>> = OnceLock::new();
    REVERSE.get_or_init(|| all().iter().filter_map(|p| Some((slot(p)?.addr, p.clone()))).collect()).get(&addr)
}
