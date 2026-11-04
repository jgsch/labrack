//! Amp-independent vocabulary: channels, parameters, values, meters, status and snapshots.
//!
//! Every parameter has a stable text name (`out1.eq.3.freq`) that is used by the CLI and in JSON
//! snapshots. Enumerations (EQ type, crossover shape) are exchanged as portable labels, so a
//! snapshot taken from one amp family can be restored on another where the parameter exists.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

macro_rules! name_enum {
    ($(#[$m:meta])* pub enum $n:ident { $($v:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $n { $($v),+ }
        impl $n {
            pub const ALL: &'static [$n] = &[$($n::$v),+];
            pub fn name(self) -> &'static str { match self { $($n::$v => $s),+ } }
            pub fn from_name(s: &str) -> Option<$n> { match s { $($s => Some($n::$v),)+ _ => None } }
        }
        impl fmt::Display for $n {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.name()) }
        }
    };
}

name_enum!(pub enum EqField { Enabled => "enabled", Type => "type", Freq => "freq", Gain => "gain", Q => "q" });
name_enum!(pub enum XoverFilter { Highpass => "hp", Lowpass => "lp" });
name_enum!(pub enum XoverField { Enabled => "enabled", Type => "type", Freq => "freq" });
name_enum!(pub enum CompressorField {
    Threshold => "threshold", Attack => "attack", Hold => "hold", Release => "release",
    Ratio => "ratio", Gain => "gain"
});
name_enum!(pub enum LimiterField {
    Threshold => "threshold", Release => "release", Impedance => "impedance", Power => "power",
    ThresholdDbu => "threshold_dbu"
});
name_enum!(pub enum MixerField { Level => "level", Mute => "mute" });

/// Mixer sources, in wire order (the IPD calls them analog A/B, aes A/B).
pub const MIXER_SOURCES: [&str; 4] = ["analog1", "analog2", "aes1", "aes2"];
pub const EQ_BANDS: u8 = 10;

/// Portable EQ filter types (same order as the IPX wire codes 0..7).
pub const EQ_TYPES: &[&str] =
    &["bell", "notch", "allpass", "low_shelf", "high_shelf", "band_pass", "high_pass", "low_pass"];
/// Portable crossover shapes (same order as the IPX wire codes 0..9).
pub const XOVER_TYPES: &[&str] = &[
    "butterworth_6",
    "bessel_6",
    "butterworth_12",
    "bessel_12",
    "linkwitz_riley_12",
    "butterworth_18",
    "bessel_18",
    "butterworth_24",
    "bessel_24",
    "linkwitz_riley_24",
];

/// A physical channel, 0-based. Inputs are the DSP input channels (A/B), outputs the amp outputs (1/2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    Input(u8),
    Output(u8),
}

impl Channel {
    pub fn index(self) -> u8 {
        match self {
            Channel::Input(i) | Channel::Output(i) => i,
        }
    }
    pub fn is_input(self) -> bool {
        matches!(self, Channel::Input(_))
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Channel::Input(i) => write!(f, "in{}", i + 1),
            Channel::Output(i) => write!(f, "out{}", i + 1),
        }
    }
}

impl FromStr for Channel {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::Parse(format!("bad channel {s:?} (use in1, in2, out1, out2)"));
        let (ctor, num): (fn(u8) -> Channel, &str) = if let Some(n) = s.strip_prefix("in") {
            (Channel::Input, n)
        } else if let Some(n) = s.strip_prefix("out") {
            (Channel::Output, n)
        } else {
            return Err(bad());
        };
        match num.parse::<u8>() {
            Ok(n) if n >= 1 => Ok(ctor(n - 1)),
            _ => Err(bad()),
        }
    }
}

/// One controllable (or readable) setting of an amp.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    Gain(Channel),
    Mute(Channel),
    Polarity(Channel),
    Delay(Channel),
    Name(Channel),
    /// Stereo link flag (on the first channel of the pair).
    Link(Channel),
    Eq {
        ch: Channel,
        band: u8,
        field: EqField,
    },
    Xover {
        ch: Channel,
        filter: XoverFilter,
        field: XoverField,
    },
    Compressor {
        ch: Channel,
        field: CompressorField,
    },
    Limiter {
        ch: Channel,
        field: LimiterField,
    },
    /// Input matrix: how much of source `src` feeds DSP input channel `ch`.
    Mixer {
        ch: Channel,
        src: u8,
        field: MixerField,
    },
}

impl Param {
    pub fn channel(&self) -> Channel {
        match *self {
            Param::Gain(c)
            | Param::Mute(c)
            | Param::Polarity(c)
            | Param::Delay(c)
            | Param::Name(c)
            | Param::Link(c) => c,
            Param::Eq { ch, .. }
            | Param::Xover { ch, .. }
            | Param::Compressor { ch, .. }
            | Param::Limiter { ch, .. }
            | Param::Mixer { ch, .. } => ch,
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Param::Gain(c) => write!(f, "{c}.gain"),
            Param::Mute(c) => write!(f, "{c}.mute"),
            Param::Polarity(c) => write!(f, "{c}.polarity"),
            Param::Delay(c) => write!(f, "{c}.delay"),
            Param::Name(c) => write!(f, "{c}.name"),
            Param::Link(c) => write!(f, "{c}.link"),
            Param::Eq { ch, band, field } => write!(f, "{ch}.eq.{band}.{field}"),
            Param::Xover { ch, filter, field } => write!(f, "{ch}.xover.{filter}.{field}"),
            Param::Compressor { ch, field } => write!(f, "{ch}.comp.{field}"),
            Param::Limiter { ch, field } => write!(f, "{ch}.limiter.{field}"),
            Param::Mixer { ch, src, field } => {
                write!(f, "{ch}.mixer.{}.{field}", MIXER_SOURCES.get(*src as usize).copied().unwrap_or("?"))
            }
        }
    }
}

impl FromStr for Param {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::Parse(format!("unknown parameter {s:?} (see `labgruppen params`)"));
        let parts: Vec<&str> = s.split('.').collect();
        let ch: Channel = parts.first().ok_or_else(bad)?.parse()?;
        let p = match parts[1..] {
            ["gain"] => Param::Gain(ch),
            ["mute"] => Param::Mute(ch),
            ["polarity"] => Param::Polarity(ch),
            ["delay"] => Param::Delay(ch),
            ["name"] => Param::Name(ch),
            ["link"] => Param::Link(ch),
            ["eq", band, field] => {
                let band: u8 = band.parse().map_err(|_| bad())?;
                if band >= EQ_BANDS {
                    return Err(bad());
                }
                Param::Eq { ch, band, field: EqField::from_name(field).ok_or_else(bad)? }
            }
            ["xover", filter, field] => Param::Xover {
                ch,
                filter: XoverFilter::from_name(filter).ok_or_else(bad)?,
                field: XoverField::from_name(field).ok_or_else(bad)?,
            },
            ["comp", field] => {
                Param::Compressor { ch, field: CompressorField::from_name(field).ok_or_else(bad)? }
            }
            ["limiter", field] => {
                Param::Limiter { ch, field: LimiterField::from_name(field).ok_or_else(bad)? }
            }
            ["mixer", src, field] => Param::Mixer {
                ch,
                src: MIXER_SOURCES.iter().position(|n| *n == src).ok_or_else(bad)? as u8,
                field: MixerField::from_name(field).ok_or_else(bad)?,
            },
            _ => return Err(bad()),
        };
        Ok(p)
    }
}

/// A value in engineering units (dB, Hz, ms, ...). Enumerations are label strings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(*b as u8 as f64),
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Text(_) => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Bool(b) => Some(*b as i64),
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            Value::Float(f) => Some(*f != 0.0),
            Value::Text(t) => match t.to_ascii_lowercase().as_str() {
                "1" | "on" | "true" | "yes" => Some(true),
                "0" | "off" | "false" | "no" => Some(false),
                _ => None,
            },
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Text(t) = self { Some(t) } else { None }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", if *b { "on" } else { "off" }),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(v) => write!(f, "{}", (v * 10000.0).round() / 10000.0),
            Value::Text(t) => f.write_str(t),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    /// One of these labels.
    Enum(&'static [&'static str]),
    Text,
}

/// Range of the basic controls both families have: gain, delay, EQ frequency / gain / Q, crossover
/// frequency and mixer level. These are the IPD ranges (from the vendor app). The IPX reports none
/// (docs/ipx-protocol.md: amp-side ranges not determined), so its specs take these too; the IPX may
/// accept more, never less is assumed to be safe.
pub fn shared_range(param: &Param) -> Option<(f64, f64)> {
    Some(match *param {
        Param::Gain(_) => (-48.0, 12.0),
        Param::Delay(_) => (0.0, 2000.0),
        Param::Eq { field: EqField::Freq, .. } => (20.0, 20000.0),
        Param::Eq { field: EqField::Gain, .. } => (-12.0, 12.0),
        Param::Eq { field: EqField::Q, .. } => (0.2, 25.0),
        Param::Xover { filter: XoverFilter::Highpass, field: XoverField::Freq, .. } => (19.0, 20000.0),
        Param::Xover { filter: XoverFilter::Lowpass, field: XoverField::Freq, .. } => (20.0, 20001.0),
        Param::Mixer { field: MixerField::Level, .. } => (-48.0, 0.0),
        _ => return None,
    })
}

/// What an amp accepts for a parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub kind: Kind,
    pub unit: &'static str,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub writable: bool,
}

impl ParamSpec {
    pub const fn new(kind: Kind) -> Self {
        ParamSpec { kind, unit: "", min: None, max: None, writable: true }
    }
    pub const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }
    pub const fn range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
    pub const fn read_only(mut self) -> Self {
        self.writable = false;
        self
    }

    /// Convert `v` to the value type of this parameter and check the range / enum label.
    pub fn check(&self, name: &dyn fmt::Display, v: &Value) -> Result<Value> {
        let bad = |why: &str| Error::InvalidValue(format!("{name}: {why} (got {v})"));
        let out = match self.kind {
            Kind::Bool => Value::Bool(v.as_bool().ok_or_else(|| bad("expected on/off"))?),
            Kind::Int => {
                let f = v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| bad("expected an integer"))?;
                Value::Int(f.round() as i64)
            }
            Kind::Float => {
                Value::Float(v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| bad("expected a number"))?)
            }
            Kind::Enum(labels) => {
                let label = match v {
                    Value::Text(t) => t.to_ascii_lowercase(),
                    other => labels
                        .get(other.as_i64().ok_or_else(|| bad("expected a label"))? as usize)
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                };
                if !labels.contains(&label.as_str()) {
                    return Err(bad(&format!("expected one of {}", labels.join(", "))));
                }
                Value::Text(label)
            }
            Kind::Text => Value::Text(v.to_string()),
        };
        if let Some(x) = out.as_f64().filter(|_| matches!(self.kind, Kind::Int | Kind::Float))
            && (self.min.is_some_and(|m| x < m) || self.max.is_some_and(|m| x > m))
        {
            return Err(bad(&format!(
                "out of range [{}, {}] {}",
                self.min.map_or("-inf".into(), |m| m.to_string()),
                self.max.map_or("inf".into(), |m| m.to_string()),
                self.unit
            )));
        }
        Ok(out)
    }
}

/// Latest meter readings, in dB. One entry per channel (index 0 = channel 1 / A).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Meters {
    pub input: Vec<f64>,
    pub output: Vec<f64>,
    /// Driver-specific extra meters (gain reduction, amp stage, voltage, ...).
    pub extra: BTreeMap<String, Vec<f64>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub driver: String,
    pub model: String,
    pub name: String,
    pub firmware: String,
    pub preset: Option<PresetInfo>,
    pub temperatures: Vec<i32>,
    /// Anything else worth showing (user level, faults, impedance, versions, ...).
    pub extra: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PresetInfo {
    pub index: u32,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AmpInfo {
    pub driver: &'static str,
    pub host: String,
    pub model: String,
}

/// All settings of an amp as `{param name: value}`; the JSON file format for backups and presets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: u32,
    pub driver: String,
    pub model: String,
    pub params: BTreeMap<String, Value>,
}

impl Snapshot {
    pub const FORMAT: u32 = 1;

    pub fn new(driver: &str, model: &str) -> Self {
        Snapshot { format: Self::FORMAT, driver: driver.into(), model: model.into(), params: BTreeMap::new() }
    }
}

/// Result of [`crate::Amp::restore`].
#[derive(Clone, Debug, Default, Serialize)]
pub struct RestoreReport {
    pub applied: usize,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub enum AmpEvent {
    Meters(Meters),
    Changed(Param, Value),
    Disconnected(Option<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_names_roundtrip() {
        for s in [
            "in1.gain",
            "out2.mute",
            "out1.polarity",
            "in2.delay",
            "out1.name",
            "in1.link",
            "out2.eq.9.q",
            "out1.xover.hp.freq",
            "out2.xover.lp.type",
            "in1.comp.ratio",
            "out1.limiter.threshold_dbu",
            "in2.mixer.aes2.level",
        ] {
            let p: Param = s.parse().unwrap();
            assert_eq!(p.to_string(), s);
        }
        assert!("out0.gain".parse::<Param>().is_err());
        assert!("out1.eq.10.q".parse::<Param>().is_err());
        assert!("out1.bogus".parse::<Param>().is_err());
    }

    /// SEC-2: NaN and infinity never pass, whatever the kind or range.
    #[test]
    fn spec_check_rejects_non_finite() {
        let specs = [
            ParamSpec::new(Kind::Float),
            ParamSpec::new(Kind::Float).range(-48.0, 12.0),
            ParamSpec::new(Kind::Int),
            ParamSpec::new(Kind::Int).range(0.0, 100.0),
        ];
        for s in specs {
            for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert!(s.check(&"x", &Value::Float(v)).is_err(), "{s:?} accepted {v}");
            }
            assert!(s.check(&"x", &Value::Float(1.0)).is_ok());
        }
    }

    #[test]
    fn spec_check() {
        let s = ParamSpec::new(Kind::Float).range(-48.0, 12.0).unit("dB");
        assert_eq!(s.check(&"x", &Value::Int(3)).unwrap(), Value::Float(3.0));
        assert!(s.check(&"x", &Value::Float(13.0)).is_err());
        let e = ParamSpec::new(Kind::Enum(EQ_TYPES));
        assert_eq!(e.check(&"x", &Value::Text("Bell".into())).unwrap(), Value::Text("bell".into()));
        assert!(e.check(&"x", &Value::Text("nope".into())).is_err());
        let b = ParamSpec::new(Kind::Bool);
        assert_eq!(b.check(&"x", &Value::Text("on".into())).unwrap(), Value::Bool(true));
    }
}
