//! Mapping of the common [`Param`] vocabulary to IPD keys (from the IntelliDrive skins' default.cfg).
//!
//! Channels: inputs A/B = 0/1, outputs 1/2 = 0x80/0x81. All three models (IPD 1200/2400/4800) share
//! the map; only the output threshold ceiling differs.

use std::collections::HashMap;

use super::protocol::*;
use crate::error::{Error, Result};
use crate::model::*;

/// Value type of a slot, with its enum code table where relevant.
#[derive(Clone, Copy, Debug)]
pub enum Ty {
    Bool,
    Int,
    Float,
    Enum(&'static [&'static str], &'static [u8]),
    Text,
}

#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub key: Key,
    pub ty: Ty,
}

pub fn wire_channel(ch: Channel) -> Option<u8> {
    match ch {
        Channel::Input(i) if i < 2 => Some(i),
        Channel::Output(i) if i < 2 => Some(0x80 + i),
        _ => None,
    }
}

pub fn slot(p: &Param) -> Option<Slot> {
    let ch = wire_channel(p.channel())?;
    let (s, m, num, ty) = match *p {
        Param::Gain(_) => (S_GAIN, M_GAIN, 0, Ty::Float),
        Param::Mute(_) => (S_MUTE, M_MUTE, 0, Ty::Bool),
        Param::Polarity(_) => (S_INVERT, M_INVERT, 0, Ty::Bool),
        Param::Delay(_) => (S_DELAY, M_DELAY, 0, Ty::Float),
        Param::Name(_) => (S_CHANNEL, M_SHORT_NAME, 0, Ty::Text),
        Param::Link(c) if c.index() == 0 => (S_LINK, M_LINK, 2, Ty::Bool),
        Param::Link(_) => return None,
        Param::Eq { band, field, .. } => {
            let (m, ty) = match field {
                EqField::Enabled => (M_ON, Ty::Bool),
                EqField::Type => (M_TYPE, Ty::Enum(EQ_TYPES, &PEQ_CODES)),
                EqField::Freq => (M_FREQ, Ty::Int),
                EqField::Gain => (M_GAIN, Ty::Float),
                EqField::Q => (M_Q, Ty::Float),
            };
            (S_PEQ, m, band, ty)
        }
        Param::Xover { filter, field, .. } => {
            let s = if filter == XoverFilter::Highpass { S_HPF } else { S_LPF };
            let (m, ty) = match field {
                XoverField::Enabled => (M_ON, Ty::Bool),
                XoverField::Type => (M_TYPE, Ty::Enum(XOVER_TYPES, &XOVER_CODES)),
                XoverField::Freq => (M_FREQ, Ty::Int),
            };
            (s, m, 0, ty)
        }
        Param::Compressor { field, .. } => {
            let (m, ty) = match field {
                CompressorField::Threshold => (M_THRESHOLD, Ty::Float),
                CompressorField::Attack => (M_ATTACK, Ty::Int),
                CompressorField::Hold => (M_HOLD, Ty::Int),
                CompressorField::Release => (M_RELEASE, Ty::Int),
                CompressorField::Ratio => (M_RATIO, Ty::Float),
                CompressorField::Gain => (M_GAIN, Ty::Float),
            };
            (S_COMPRESSOR, m, 0, ty)
        }
        Param::Limiter { field, .. } => {
            let (m, ty) = match field {
                LimiterField::Threshold => (M_THRESHOLD, Ty::Float),
                LimiterField::Release => (M_RELEASE, Ty::Int),
                _ => return None,
            };
            (S_LIMITER, m, 0, ty)
        }
        Param::Mixer { ch: c, src, field } => {
            if !c.is_input() || src > 3 {
                return None;
            }
            match field {
                // The skin names this control "mute_a1".."mute_b4"; polarity (1 = muted) is unverified.
                MixerField::Mute => (S_MIXER, M_ON, src, Ty::Bool),
                MixerField::Level => (S_MIXER, M_GAIN, src, Ty::Float),
            }
        }
    };
    Some(Slot { key: Key::new(s, m, ch, num), ty })
}

/// Global MODEL_NUMBER -> model name.
pub const MODELS_BY_NUMBER: [&str; 3] = ["IPD2400", "IPD1200", "IPD4800"];

/// Output threshold ceiling (dBu) per model; the floor is 16.78 dBu.
pub fn output_threshold_max(model: &str) -> f64 {
    match model {
        "IPD2400" => 36.78,
        "IPD4800" => 39.83,
        _ => 33.8,
    }
}

pub fn spec(p: &Param, model: &str) -> Option<ParamSpec> {
    slot(p)?;
    let out = !p.channel().is_input();
    let f = || ParamSpec::new(Kind::Float);
    Some(match *p {
        Param::Gain(_) => f().range(-48.0, 12.0).unit("dB"),
        Param::Mute(_) | Param::Polarity(_) | Param::Link(_) => ParamSpec::new(Kind::Bool),
        Param::Delay(_) => f().range(0.0, 2000.0).unit("ms"),
        Param::Name(_) => ParamSpec::new(Kind::Text),
        Param::Eq { field, .. } => match field {
            EqField::Enabled => ParamSpec::new(Kind::Bool),
            EqField::Type => ParamSpec::new(Kind::Enum(EQ_TYPES)),
            EqField::Freq => ParamSpec::new(Kind::Int).range(20.0, 20000.0).unit("Hz"),
            EqField::Gain => f().range(-12.0, 12.0).unit("dB"),
            EqField::Q => f().range(0.2, 25.0),
        },
        Param::Xover { filter, field, .. } => match field {
            XoverField::Enabled => ParamSpec::new(Kind::Bool),
            XoverField::Type => ParamSpec::new(Kind::Enum(XOVER_TYPES)),
            XoverField::Freq if filter == XoverFilter::Highpass => {
                ParamSpec::new(Kind::Int).range(19.0, 20000.0).unit("Hz")
            }
            XoverField::Freq => ParamSpec::new(Kind::Int).range(20.0, 20001.0).unit("Hz"),
        },
        Param::Compressor { field, .. } => match field {
            // The vendor config says 36.78 at most, but a real IPD2400 reports 37.82 / 39.78 (its default).
            CompressorField::Threshold if out => f().range(16.78, 39.83).unit("dBu"),
            CompressorField::Threshold => f().range(-24.0, 39.83).unit("dB"),
            CompressorField::Attack | CompressorField::Hold | CompressorField::Release => {
                ParamSpec::new(Kind::Int).range(1.0, 10000.0).unit("ms")
            }
            CompressorField::Ratio => f().range(1.2, 25.0),
            CompressorField::Gain => f().range(-12.0, 12.0).unit("dB"),
        },
        Param::Limiter { field, .. } => match field {
            LimiterField::Threshold if out => f().range(16.78, output_threshold_max(model)).unit("dBu"),
            LimiterField::Threshold => f().range(-24.0, 39.83).unit("dB"),
            _ => ParamSpec::new(Kind::Int).range(10.0, 1000.0).unit("ms"),
        },
        Param::Mixer { field, .. } => match field {
            MixerField::Level => f().range(-48.0, 0.0).unit("dB"),
            MixerField::Mute => ParamSpec::new(Kind::Bool),
        },
    })
}

/// Every supported parameter, in a stable order (inputs, then outputs).
pub fn all_params() -> Vec<Param> {
    let mut v = Vec::new();
    for ch in [Channel::Input(0), Channel::Input(1), Channel::Output(0), Channel::Output(1)] {
        v.extend([Param::Gain(ch), Param::Mute(ch), Param::Polarity(ch), Param::Delay(ch), Param::Name(ch)]);
        if ch.index() == 0 {
            v.push(Param::Link(ch));
        }
        for band in 0..EQ_BANDS {
            v.extend(EqField::ALL.iter().map(|&field| Param::Eq { ch, band, field }));
        }
        for &filter in XoverFilter::ALL {
            v.extend(XoverField::ALL.iter().map(|&field| Param::Xover { ch, filter, field }));
        }
        v.extend(CompressorField::ALL.iter().map(|&field| Param::Compressor { ch, field }));
        v.extend([LimiterField::Threshold, LimiterField::Release].map(|field| Param::Limiter { ch, field }));
        if ch.is_input() {
            for src in 0..4 {
                v.extend(MixerField::ALL.iter().map(|&field| Param::Mixer { ch, src, field }));
            }
        }
    }
    v
}

pub fn reverse_map() -> HashMap<Key, Param> {
    all_params().into_iter().filter_map(|p| Some((slot(&p)?.key, p))).collect()
}

/// Convert a checked value to its wire form.
pub fn to_wire(slot: &Slot, v: &Value) -> Result<Raw> {
    let bad = || Error::InvalidValue(format!("{v} does not fit key {}", slot.key));
    Ok(match slot.ty {
        Ty::Bool => Raw::Int(v.as_bool().ok_or_else(bad)? as i64),
        Ty::Int => Raw::Int(v.as_i64().ok_or_else(bad)?),
        Ty::Float => Raw::Int(to_raw(slot.key.member_id, v.as_f64().ok_or_else(bad)?)),
        Ty::Enum(labels, codes) => {
            let label = v.as_str().ok_or_else(bad)?;
            let i = labels.iter().position(|l| l.eq_ignore_ascii_case(label)).ok_or_else(bad)?;
            Raw::Int(codes[i] as i64)
        }
        Ty::Text => Raw::Text(v.to_string()),
    })
}

pub fn from_wire(slot: &Slot, raw: &Raw) -> Value {
    match (slot.ty, raw) {
        (Ty::Bool, Raw::Int(i)) => Value::Bool(*i != 0),
        (Ty::Int, Raw::Int(i)) => Value::Int(*i),
        (Ty::Float, Raw::Int(i)) => Value::Float(from_raw(slot.key.member_id, *i)),
        (Ty::Enum(labels, codes), Raw::Int(i)) => match codes.iter().position(|c| *c as i64 == *i) {
            Some(n) => Value::Text(labels[n].to_string()),
            None => Value::Text(format!("unknown({i})")),
        },
        (_, Raw::Text(t)) => Value::Text(t.trim_end().to_string()),
        (_, Raw::Int(i)) => Value::Int(*i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_matches_python_keys() {
        let k = |s: &str| slot(&s.parse().unwrap()).unwrap().key;
        assert_eq!(k("out1.gain"), Key::new(0, 0, 0x80, 0));
        assert_eq!(k("in2.eq.3.freq"), Key::new(5, 1, 1, 3));
        assert_eq!(k("out2.xover.hp.type"), Key::new(4, 3, 0x81, 0));
        assert_eq!(k("in1.mixer.aes1.level"), Key::new(1, 0, 0, 2));
        assert_eq!(k("in1.link"), Key::new(10, 25, 0, 2));
        assert_eq!(k("out1.link"), Key::new(10, 25, 0x80, 2));
        assert!(slot(&"out2.link".parse().unwrap()).is_none());
        assert!(slot(&"out1.mixer.aes1.level".parse().unwrap()).is_none());
        assert_eq!(all_params().len(), 4 * (5 + 50 + 6 + 6 + 2) + 2 + 16);
        assert_eq!(reverse_map().len(), all_params().len());
    }

    #[test]
    fn value_conversion() {
        let s = slot(&"out1.eq.0.type".parse().unwrap()).unwrap();
        assert!(matches!(to_wire(&s, &Value::Text("high_shelf".into())), Ok(Raw::Int(132))));
        assert_eq!(from_wire(&s, &Raw::Int(136)), Value::Text("low_pass".into()));
        let g = slot(&"in1.gain".parse().unwrap()).unwrap();
        assert!(matches!(to_wire(&g, &Value::Float(-2.5)), Ok(Raw::Int(-250))));
        assert_eq!(from_wire(&g, &Raw::Int(-250)), Value::Float(-2.5));
    }
}
