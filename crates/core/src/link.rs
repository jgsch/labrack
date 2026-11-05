//! Which settings a linked pair shares (BEH-31). Channel 1 is the master: channel 2 follows it.
//!
//! A link shares the level: the gain and the output limiter. The EQ, the output filters, the delay and
//! the mixer stay per channel, and so does the mute (a link must never unmute anything, `SEC-7`).
//! What the IPD's own link flag mirrors is wider ([`mirrored`]), which is why the app does not use it.

use labgruppen::{Channel, LimiterField, Param};

/// The two channels of an amp that can be linked in pairs: the DSP inputs, or the outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Inputs,
    Outputs,
}

impl Side {
    /// Channel 1, the master of the pair.
    pub fn master(self) -> Channel {
        match self {
            Side::Inputs => Channel::Input(0),
            Side::Outputs => Channel::Output(0),
        }
    }

    /// The link flag lives on the first channel of the pair on both families.
    pub(crate) fn flag(self) -> Param {
        Param::Link(self.master())
    }
}

/// The pair a link flag belongs to.
pub(crate) fn flag_side(param: &Param) -> Option<Side> {
    match param {
        Param::Link(Channel::Input(0)) => Some(Side::Inputs),
        Param::Link(Channel::Output(0)) => Some(Side::Outputs),
        _ => None,
    }
}

pub(crate) fn side_of(channel: Channel) -> Side {
    match channel {
        Channel::Input(_) => Side::Inputs,
        Channel::Output(_) => Side::Outputs,
    }
}

/// The pair a setting is shared in, if `param` is one a linked pair shares: the gain and the output
/// limiter (BEH-31).
pub(crate) fn follows(param: &Param) -> Option<Side> {
    let shared = match param {
        Param::Gain(_) => true,
        Param::Limiter { ch, field } => {
            !ch.is_input()
                && matches!(
                    field,
                    LimiterField::Threshold
                        | LimiterField::Impedance
                        | LimiterField::Power
                        | LimiterField::ThresholdDbu
                )
        }
        _ => false,
    };
    shared.then(|| side_of(param.channel()))
}

/// The pair a setting is mirrored in by the IPD's own link flag, which copies everything of channel 1 to
/// channel 2: the gain, the delay, the EQ, the output filters and the output limiter (BEH-32).
pub(crate) fn mirrored(param: &Param) -> Option<Side> {
    let shared = match param {
        Param::Gain(_) | Param::Delay(_) | Param::Eq { .. } => true,
        Param::Xover { ch, .. } => !ch.is_input(),
        _ => return follows(param),
    };
    shared.then(|| side_of(param.channel()))
}

/// Whether `param` is a setting of channel 2.
pub(crate) fn is_second(param: &Param) -> bool {
    param.channel().index() == 1
}

/// The same setting on channel 2, for a setting of channel 1.
pub(crate) fn second(param: &Param) -> Option<Param> {
    let to = |c: Channel| match c {
        Channel::Input(0) => Some(Channel::Input(1)),
        Channel::Output(0) => Some(Channel::Output(1)),
        _ => None,
    };
    Some(match param.clone() {
        Param::Gain(c) => Param::Gain(to(c)?),
        Param::Delay(c) => Param::Delay(to(c)?),
        Param::Eq { ch, band, field } => Param::Eq { ch: to(ch)?, band, field },
        Param::Xover { ch, filter, field } => Param::Xover { ch: to(ch)?, filter, field },
        Param::Limiter { ch, field } => Param::Limiter { ch: to(ch)?, field },
        _ => return None,
    })
}

/// The same setting on channel 1, for a setting of channel 2.
pub(crate) fn first(param: &Param) -> Option<Param> {
    let to = |c: Channel| match c {
        Channel::Input(1) => Some(Channel::Input(0)),
        Channel::Output(1) => Some(Channel::Output(0)),
        _ => None,
    };
    Some(match param.clone() {
        Param::Gain(c) => Param::Gain(to(c)?),
        Param::Delay(c) => Param::Delay(to(c)?),
        Param::Eq { ch, band, field } => Param::Eq { ch: to(ch)?, band, field },
        Param::Xover { ch, filter, field } => Param::Xover { ch: to(ch)?, filter, field },
        Param::Limiter { ch, field } => Param::Limiter { ch: to(ch)?, field },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(s: &str) -> Param {
        s.parse().unwrap()
    }

    // BEH-32: first and second are each other's way back
    #[test]
    fn first_undoes_second() {
        for name in
            ["out1.gain", "in1.delay", "out1.eq.3.freq", "out1.xover.hp.freq", "out1.limiter.threshold"]
        {
            let one = param(name);
            assert_eq!(first(&second(&one).unwrap()), Some(one), "{name}");
        }
        assert_eq!(first(&param("out1.gain")), None);
    }

    // BEH-31: a link shares the level, the gain and the output limiter, nothing else
    #[test]
    fn the_shared_settings() {
        for (name, side) in [
            ("in1.gain", Side::Inputs),
            ("in2.gain", Side::Inputs),
            ("out1.gain", Side::Outputs),
            ("out2.gain", Side::Outputs),
            ("out1.limiter.threshold", Side::Outputs),
            ("out1.limiter.impedance", Side::Outputs),
            ("out2.limiter.power", Side::Outputs),
            ("out1.limiter.threshold_dbu", Side::Outputs),
        ] {
            assert_eq!(follows(&param(name)), Some(side), "{name}");
        }
    }

    // BEH-31, SEC-7: the EQs are never merged, and a link carries no mute
    #[test]
    fn the_eqs_the_delay_the_mixer_and_the_mutes_stay_per_channel() {
        for name in [
            "in1.eq.0.gain",
            "in2.eq.9.q",
            "out1.eq.3.freq",
            "out2.eq.0.type",
            "out1.xover.hp.freq",
            "out2.xover.lp.enabled",
            "in1.delay",
            "out2.delay",
            "in1.mute",
            "out1.mute",
            "in1.mixer.aes1.level",
            "in2.mixer.analog1.mute",
            "in1.link",
            "out1.link",
            "out1.polarity",
            "in1.name",
            "out1.limiter.release",
            "in1.limiter.threshold",
            "in1.comp.ratio",
            "out1.comp.gain",
        ] {
            assert_eq!(follows(&param(name)), None, "{name}");
        }
    }

    // BEH-32: what the IPD's own flag mirrors is wider: the reason the app does not use it
    #[test]
    fn the_ipd_flag_mirrors_everything_of_a_channel() {
        for (name, side) in [
            ("in1.gain", Side::Inputs),
            ("in1.delay", Side::Inputs),
            ("in1.eq.9.q", Side::Inputs),
            ("out2.eq.0.type", Side::Outputs),
            ("out1.xover.hp.freq", Side::Outputs),
            ("out1.limiter.threshold", Side::Outputs),
        ] {
            assert_eq!(mirrored(&param(name)), Some(side), "{name}");
        }
        assert_eq!(mirrored(&param("out1.mute")), None);
        assert_eq!(mirrored(&param("in1.mixer.aes1.level")), None);
    }

    #[test]
    fn the_twin_of_a_setting_is_the_same_on_channel_2() {
        for (one, two) in [
            ("in1.gain", "in2.gain"),
            ("out1.delay", "out2.delay"),
            ("out1.eq.3.freq", "out2.eq.3.freq"),
            ("out1.xover.lp.type", "out2.xover.lp.type"),
            ("out1.limiter.threshold_dbu", "out2.limiter.threshold_dbu"),
        ] {
            assert_eq!(second(&param(one)), Some(param(two)), "{one}");
            assert!(!is_second(&param(one)) && is_second(&param(two)));
            assert_eq!(mirrored(&param(two)), mirrored(&param(one)));
        }
        // a twin has no twin; what is not shared has none
        assert_eq!(second(&param("out2.gain")), None);
        assert_eq!(second(&param("out1.mute")), None);
    }

    #[test]
    fn the_flag_is_on_the_first_channel() {
        assert_eq!(Side::Inputs.flag(), param("in1.link"));
        assert_eq!(Side::Outputs.flag(), param("out1.link"));
        assert_eq!(Side::Outputs.master(), Channel::Output(0));
    }
}
