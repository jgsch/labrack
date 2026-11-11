//! Safe writes to the DSP (SEC-2 to SEC-4): what may be set, what a value may be, and how fast.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use labgruppen::{Channel, Kind, LimiterField, Param, ParamSpec, Value, limiter};
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::store::Ceilings;
use crate::{Error, Result};

/// What a control may be set to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub min: Option<f64>,
    /// The model's maximum, or the user's ceiling if that is lower.
    pub max: Option<f64>,
    pub unit: &'static str,
}

/// The controls that can be set through [`crate::Hub::set`]: levels, mutes, the mixer, EQ, filters and
/// delays (BEH-27 to BEH-30). The limiter, link, compressors, polarity and names have their own
/// changes, or are not part of v1.
pub(crate) fn controllable(param: &Param) -> bool {
    match param {
        Param::Gain(_) | Param::Mute(_) | Param::Delay(_) | Param::Eq { .. } => true,
        Param::Xover { ch, .. } => !ch.is_input(),
        Param::Mixer { ch, .. } => ch.is_input(),
        _ => false,
    }
}

/// What a settings file may carry: the controls above, and the output limiter, which `Hub::set`
/// does not take because its figures must be written together (see `Hub::set_limiter`).
pub(crate) fn restorable(param: &Param) -> bool {
    controllable(param)
        || matches!(
            param,
            Param::Limiter {
                ch: Channel::Output(_),
                field: LimiterField::Threshold
                    | LimiterField::Impedance
                    | LimiterField::Power
                    | LimiterField::ThresholdDbu
            }
        )
}

/// The limiter threshold in dBu, as the amp stores it, that setting `param` to `value` means, if it is
/// one: an IPX holds it as Vpk and as dBu, an IPD as dBu. The user ceiling is in that dBu (of that amp).
fn threshold_dbu(param: &Param, unit: &str, value: f64) -> Option<f64> {
    match param {
        Param::Limiter { ch: Channel::Output(_), field: LimiterField::Threshold } => Some(match unit {
            "Vpk" => limiter::dbu(value),
            "dBV" => value + limiter::DBV_TO_DBU,
            _ => value,
        }),
        Param::Limiter { ch: Channel::Output(_), field: LimiterField::ThresholdDbu } => Some(value),
        _ => None,
    }
}

/// The user ceiling that applies to `param` (SEC-3): the output gain.
fn ceiling(param: &Param, ceilings: &Ceilings) -> Option<f64> {
    match param {
        Param::Gain(Channel::Output(_)) => ceilings.max_output_gain_db,
        _ => None,
    }
}

pub(crate) fn range(param: &Param, spec: &ParamSpec, ceilings: &Ceilings) -> Range {
    let max = match (spec.max, ceiling(param, ceilings)) {
        (Some(model), Some(user)) => Some(model.min(user)),
        (model, user) => user.or(model),
    };
    Range { min: spec.min, max, unit: spec.unit }
}

/// Check `value` for `param` before anything is sent (SEC-2, SEC-3) and return it as the amp will
/// hold it. `spec` is the amp's own. Whether the control may be set at all is for the caller.
pub(crate) fn check(param: &Param, spec: &ParamSpec, value: &Value, ceilings: &Ceilings) -> Result<Value> {
    if !spec.writable {
        return Err(Error::NotControllable);
    }
    let (fits, wanted) = match (spec.kind, value) {
        (Kind::Bool, Value::Bool(_)) => (true, ""),
        (Kind::Int | Kind::Float, Value::Int(_) | Value::Float(_)) => (true, ""),
        (Kind::Enum(_) | Kind::Text, Value::Text(_)) => (true, ""),
        (Kind::Bool, _) => (false, "on or off"),
        (Kind::Int | Kind::Float, _) => (false, "a number"),
        (Kind::Enum(_) | Kind::Text, _) => (false, "a label"),
    };
    if !fits {
        return Err(Error::Invalid(format!("{param}: expected {wanted}")));
    }
    let checked = spec.check(param, value).map_err(|e| match e {
        labgruppen::Error::InvalidValue(why) => Error::Invalid(why),
        other => Error::Invalid(other.to_string()),
    })?;
    if let Some(v) = checked.as_f64()
        && let Some((ceiling, unit)) = above_ceiling(param, spec.unit, v, ceilings)
    {
        return Err(Error::AboveCeiling { ceiling, unit });
    }
    Ok(checked)
}

/// The user ceiling (SEC-3) that `value` of `param`, held in `unit`, is above, with its unit; the
/// amp's own range is not asked, so a value read from the amp that is out of it is still told. A
/// limiter threshold is held to the dBu ceiling in whatever unit it is held.
pub(crate) fn above_ceiling(
    param: &Param,
    unit: &'static str,
    value: f64,
    ceilings: &Ceilings,
) -> Option<(f64, &'static str)> {
    if let Some(limit) = ceiling(param, ceilings)
        && value > limit
    {
        return Some((limit, unit));
    }
    let limit = ceilings.max_limiter_dbu?;
    let dbu = threshold_dbu(param, unit, value)?;
    (dbu > limit + 1e-9).then(|| {
        let ceiling = match unit {
            "Vpk" => limiter::vpk_from_dbu(limit),
            "dBV" => limit - limiter::DBV_TO_DBU,
            _ => limit,
        };
        (ceiling, unit)
    })
}

/// Keeps the latest value per parameter and says when it may go out: the first at once, then not
/// before `interval` after the previous one (SEC-4). Times are passed in, so it is tested without a
/// clock.
pub(crate) struct Coalescer {
    interval: Duration,
    slots: HashMap<Param, Slot>,
}

#[derive(Default)]
struct Slot {
    waiting: Option<Value>,
    last_sent: Option<Instant>,
}

impl Slot {
    /// How long until the waiting value may go out; zero if it may now.
    fn due_in(&self, interval: Duration, now: Instant) -> Option<Duration> {
        self.waiting.as_ref()?;
        Some(match self.last_sent {
            Some(sent) => (sent + interval).saturating_duration_since(now),
            None => Duration::ZERO,
        })
    }
}

impl Coalescer {
    pub fn new(interval: Duration) -> Coalescer {
        Coalescer { interval, slots: HashMap::new() }
    }

    /// A newer value replaces one that is still waiting.
    pub fn push(&mut self, param: Param, value: Value) {
        self.slots.entry(param).or_default().waiting = Some(value);
    }

    /// Drop what is waiting; what was sent stays counted for the interval.
    pub fn clear(&mut self) {
        for slot in self.slots.values_mut() {
            slot.waiting = None;
        }
    }

    /// Drop the output unmutes that wait to be written; a mute and every other value stays.
    pub fn drop_unmutes(&mut self) {
        for (param, slot) in &mut self.slots {
            if matches!(param, Param::Mute(Channel::Output(_))) && slot.waiting == Some(Value::Bool(false)) {
                slot.waiting = None;
            }
        }
    }

    /// The output mutes that wait to be written as MUTED.
    pub fn muting(&self) -> [bool; 2] {
        let mut muting = [false; 2];
        for (param, slot) in &self.slots {
            if let (Param::Mute(Channel::Output(output)), Some(Value::Bool(true))) = (param, &slot.waiting)
                && let Some(flag) = muting.get_mut(usize::from(*output))
            {
                *flag = true;
            }
        }
        muting
    }

    /// The values that may go out at `now` (they count as sent), and how long until the next one may.
    pub fn take_due(&mut self, now: Instant) -> (Vec<(Param, Value)>, Option<Duration>) {
        let mut due = Vec::new();
        let mut next: Option<Duration> = None;
        for (param, slot) in &mut self.slots {
            match slot.due_in(self.interval, now) {
                Some(Duration::ZERO) => {
                    due.extend(slot.waiting.take().map(|v| (param.clone(), v)));
                    slot.last_sent = Some(now);
                }
                Some(wait) => next = Some(next.map_or(wait, |n| n.min(wait))),
                None => {}
            }
        }
        // a mute goes first: it does not wait for the settings that are due with it (SEC-6)
        due.sort_by_key(|(param, value)| !(matches!(param, Param::Mute(_)) && *value == Value::Bool(true)));
        (due, next)
    }
}

/// The values of one connection waiting to be written. It lives as long as the connection: when that
/// ends, what is still waiting is dropped, never sent to a later connection (SEC-1).
pub(crate) struct Writer {
    coalescer: Mutex<Coalescer>,
    pub wake: Notify,
}

impl Writer {
    pub fn new(interval: Duration) -> Writer {
        Writer { coalescer: Mutex::new(Coalescer::new(interval)), wake: Notify::new() }
    }

    /// Values that belong together are queued together, so they go out in one batch.
    pub fn push_all(&self, values: impl IntoIterator<Item = (Param, Value)>) {
        let mut coalescer = self.coalescer.lock().unwrap_or_else(|e| e.into_inner());
        for (param, value) in values {
            coalescer.push(param, value);
        }
        drop(coalescer);
        self.wake.notify_one();
    }

    /// A bulk change replaces the settings these values were meant for.
    pub fn clear(&self) {
        self.coalescer.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// A bulk change leaves the outputs muted: an unmute still waiting must not be written after it.
    pub fn drop_unmutes(&self) {
        self.coalescer.lock().unwrap_or_else(|e| e.into_inner()).drop_unmutes();
    }

    /// The output mutes the user asked for that the amp has not got yet (see [`Coalescer::muting`]).
    pub fn muting(&self) -> [bool; 2] {
        self.coalescer.lock().unwrap_or_else(|e| e.into_inner()).muting()
    }

    pub fn take_due(&self) -> (Vec<(Param, Value)>, Option<Duration>) {
        self.coalescer.lock().unwrap_or_else(|e| e.into_inner()).take_due(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(s: &str) -> Param {
        s.parse().unwrap()
    }

    fn f(v: f64) -> Value {
        Value::Float(v)
    }

    fn float(min: f64, max: f64) -> ParamSpec {
        ParamSpec::new(Kind::Float).range(min, max).unit("dB")
    }

    const NONE: Ceilings = Ceilings { max_output_gain_db: None, max_limiter_dbu: None };

    // SEC-2: the edges of the range are in, the next float beyond is out
    #[test]
    fn the_range_is_closed_and_tight() {
        let spec = float(-48.0, 12.0);
        let gain = param("in1.gain");
        for ok in [-48.0, 0.0, 12.0] {
            assert_eq!(check(&gain, &spec, &f(ok), &NONE).unwrap(), f(ok));
        }
        for bad in [(-48.0f64).next_down(), 12.0f64.next_up(), 1e300, -1e300] {
            assert!(matches!(check(&gain, &spec, &f(bad), &NONE), Err(Error::Invalid(_))), "{bad}");
        }
    }

    // SEC-2
    #[test]
    fn nan_and_infinity_never_pass() {
        let gain = param("in1.gain");
        for spec in [float(-48.0, 12.0), ParamSpec::new(Kind::Float), ParamSpec::new(Kind::Int)] {
            for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert!(matches!(check(&gain, &spec, &f(bad), &NONE), Err(Error::Invalid(_))), "{bad}");
            }
        }
    }

    // SEC-2: a value of the wrong kind is not converted
    #[test]
    fn a_value_must_have_the_kind_of_the_control() {
        let gain = param("in1.gain");
        let number = float(-48.0, 12.0);
        for bad in [Value::Bool(true), Value::Text("1".into())] {
            assert!(matches!(check(&gain, &number, &bad, &NONE), Err(Error::Invalid(_))), "{bad:?}");
        }
        let mute = param("out1.mute");
        let bool_spec = ParamSpec::new(Kind::Bool);
        for bad in [Value::Int(1), f(1.0), Value::Text("on".into())] {
            assert!(matches!(check(&mute, &bool_spec, &bad, &NONE), Err(Error::Invalid(_))), "{bad:?}");
        }
        assert_eq!(check(&mute, &bool_spec, &Value::Bool(true), &NONE).unwrap(), Value::Bool(true));
        let kind = param("out1.eq.0.type");
        let labels = ParamSpec::new(Kind::Enum(&["bell", "notch"]));
        assert!(matches!(check(&kind, &labels, &Value::Int(1), &NONE), Err(Error::Invalid(_))));
        assert!(matches!(check(&kind, &labels, &Value::Text("saw".into()), &NONE), Err(Error::Invalid(_))));
        assert_eq!(
            check(&kind, &labels, &Value::Text("Notch".into()), &NONE).unwrap(),
            Value::Text("notch".into())
        );
    }

    // integers are rounded, then range-checked
    #[test]
    fn integers_are_rounded_before_the_range_check() {
        let freq = param("out1.eq.0.freq");
        let spec = ParamSpec::new(Kind::Int).range(20.0, 20000.0).unit("Hz");
        assert_eq!(check(&freq, &spec, &f(1000.4), &NONE).unwrap(), Value::Int(1000));
        assert_eq!(check(&freq, &spec, &f(19.6), &NONE).unwrap(), Value::Int(20));
        assert!(check(&freq, &spec, &f(19.4), &NONE).is_err());
    }

    // SEC-3: a value out of the amp's own range is still told when it is above a ceiling, and the IPX's
    // dBu threshold is held to the dBu ceiling
    #[test]
    fn a_value_outside_the_model_range_is_still_above_a_ceiling() {
        let zero = Ceilings { max_output_gain_db: Some(0.0), ..NONE };
        let out2 = param("out2.gain");
        assert_eq!(above_ceiling(&out2, "dB", 15.0, &zero), Some((0.0, "dB")));
        assert_eq!(above_ceiling(&out2, "dB", 0.0, &zero), None);
        let lim = Ceilings { max_limiter_dbu: Some(30.0), ..NONE };
        let dbu = param("out1.limiter.threshold_dbu");
        assert_eq!(above_ceiling(&dbu, "dBu", 35.0, &lim), Some((30.0, "dBu")));
        assert_eq!(above_ceiling(&dbu, "dBu", 30.0, &lim), None);
        let vpk = param("out1.limiter.threshold");
        assert!(above_ceiling(&vpk, "Vpk", 69.0, &lim).is_some());
        assert_eq!(above_ceiling(&param("in1.gain"), "dB", 99.0, &zero), None);
    }

    // SEC-3: the ceiling is a closed bound on output gain, and only on that
    #[test]
    fn the_output_gain_ceiling() {
        let spec = float(-48.0, 12.0);
        let ceilings = Ceilings { max_output_gain_db: Some(-6.0), ..NONE };
        let out = param("out2.gain");
        assert_eq!(check(&out, &spec, &f(-6.0), &ceilings).unwrap(), f(-6.0));
        assert_eq!(check(&out, &spec, &f(-48.0), &ceilings).unwrap(), f(-48.0));
        let above = check(&out, &spec, &f((-6.0f64).next_up()), &ceilings);
        assert!(
            matches!(above, Err(Error::AboveCeiling { ceiling, unit: "dB" }) if ceiling == -6.0),
            "{above:?}"
        );
        // above the model's own range it is a range error, ceiling or not
        assert!(matches!(check(&out, &spec, &f(13.0), &ceilings), Err(Error::Invalid(_))));
        // not for inputs, the mixer or other numbers
        assert!(check(&param("in1.gain"), &spec, &f(12.0), &ceilings).is_ok());
        assert!(check(&param("out1.delay"), &float(0.0, 2000.0), &f(100.0), &ceilings).is_ok());
    }

    #[test]
    fn the_range_shown_ends_at_the_lower_of_model_and_ceiling() {
        let spec = float(-48.0, 12.0);
        let at =
            |c: Option<f64>| range(&param("out1.gain"), &spec, &Ceilings { max_output_gain_db: c, ..NONE });
        assert_eq!(at(None).max, Some(12.0));
        assert_eq!(at(Some(-6.0)).max, Some(-6.0));
        assert_eq!(at(Some(20.0)).max, Some(12.0), "a ceiling above the model changes nothing");
        assert_eq!(at(Some(-6.0)).min, Some(-48.0));
        let unbounded = ParamSpec::new(Kind::Float);
        let r = range(&param("out1.gain"), &unbounded, &Ceilings { max_output_gain_db: Some(3.0), ..NONE });
        assert_eq!((r.min, r.max), (None, Some(3.0)));
    }

    #[test]
    fn only_the_basic_controls_are_controllable() {
        for yes in [
            "in1.gain",
            "out2.gain",
            "in1.mute",
            "out1.mute",
            "in2.delay",
            "out1.delay",
            "in1.eq.9.q",
            "out2.eq.0.type",
            "out1.xover.hp.freq",
            "out2.xover.lp.enabled",
            "in2.mixer.aes2.level",
            "in1.mixer.analog1.mute",
        ] {
            assert!(controllable(&param(yes)), "{yes}");
        }
        for no in [
            "out1.limiter.threshold",
            "in1.limiter.threshold",
            "in1.comp.ratio",
            "out1.comp.gain",
            "out1.polarity",
            "in1.link",
            "out1.link",
            "in1.name",
            "in1.xover.hp.freq",
            "out1.mixer.aes1.level",
        ] {
            assert!(!controllable(&param(no)), "{no}");
        }
    }

    // SEC-3: a limiter threshold is held to the dBu ceiling. An IPX holds the threshold as Vpk and as dBu,
    // an IPD as dBV, 2.22 dB below its dBu.
    #[test]
    fn the_limiter_ceiling_holds_for_vpk_and_dbu_entries() {
        let ceilings = Ceilings { max_limiter_dbu: Some(30.0), ..NONE };
        let dbu = ParamSpec::new(Kind::Float).range(16.78, 36.78).unit("dBu"); // an IPX
        let dbv = ParamSpec::new(Kind::Float).range(16.78, 36.78).unit("dBV"); // an IPD
        let vpk = ParamSpec::new(Kind::Float).range(7.0, 110.0).unit("Vpk");
        let at_30 = limiter::vpk_from_dbu(30.0);
        let ipd_30 = 30.0 - limiter::DBV_TO_DBU;

        for (name, spec, ok, over) in [
            ("out1.limiter.threshold", &dbv, ipd_30, ipd_30 + 0.01),
            ("out1.limiter.threshold_dbu", &dbu, 30.0, 30.01),
            ("out2.limiter.threshold", &vpk, at_30, at_30 * 1.001),
        ] {
            let p = param(name);
            assert_eq!(check(&p, spec, &f(ok), &ceilings).unwrap(), f(ok), "{name}");
            let got = check(&p, spec, &f(over), &ceilings);
            assert!(matches!(got, Err(Error::AboveCeiling { .. })), "{name}: {got:?}");
            assert!(check(&p, spec, &f(over), &NONE).is_ok(), "{name}: no ceiling");
        }
        // the ceiling in the unit of the entry: an IPD has it in dBV
        let got = check(&param("out1.limiter.threshold"), &dbv, &f(ipd_30 + 1.0), &ceilings);
        assert!(
            matches!(got, Err(Error::AboveCeiling { ceiling, unit: "dBV" }) if (ceiling - ipd_30).abs() < 1e-9)
        );
        let got = check(&param("out1.limiter.threshold"), &vpk, &f(at_30 * 1.1), &ceilings);
        assert!(
            matches!(got, Err(Error::AboveCeiling { ceiling, unit: "Vpk" }) if (ceiling - at_30).abs() < 1e-9)
        );
        // not the load, the watts, the release or an input
        let other = ParamSpec::new(Kind::Float).range(0.0, 1e6);
        for name in ["out1.limiter.impedance", "out1.limiter.power", "in1.limiter.threshold"] {
            assert!(check(&param(name), &other, &f(99_999.0), &ceilings).is_ok(), "{name}");
        }
    }

    // a file may carry the output limiter, but `Hub::set` does not take it
    #[test]
    fn files_carry_the_output_limiter_and_set_does_not_take_it() {
        for name in [
            "out1.limiter.threshold",
            "out2.limiter.impedance",
            "out1.limiter.power",
            "out2.limiter.threshold_dbu",
        ] {
            assert!(restorable(&param(name)) && !controllable(&param(name)), "{name}");
        }
        for name in
            ["out1.limiter.release", "in1.limiter.threshold", "in1.comp.ratio", "out1.polarity", "out1.link"]
        {
            assert!(!restorable(&param(name)), "{name}");
        }
        for name in ["in1.gain", "out1.mute", "out1.eq.0.q", "out2.xover.hp.freq", "in2.mixer.aes1.level"] {
            assert!(restorable(&param(name)), "{name}");
        }
    }

    #[test]
    fn a_read_only_control_is_not_controllable() {
        let spec = float(-48.0, 12.0).read_only();
        assert!(matches!(check(&param("in1.gain"), &spec, &f(0.0), &NONE), Err(Error::NotControllable)));
    }

    // SEC-4 ---------------------------------------------------------------------------------------

    const STEP: Duration = Duration::from_millis(50);

    #[test]
    fn the_first_value_goes_out_at_once() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        assert_eq!(c.take_due(t0), (vec![], None));
        c.push(param("out1.gain"), f(1.0));
        assert_eq!(c.take_due(t0).0, [(param("out1.gain"), f(1.0))]);
        assert_eq!(c.take_due(t0), (vec![], None), "sent values do not go out twice");
    }

    #[test]
    fn the_next_value_waits_for_the_interval_and_the_latest_wins() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        let gain = param("out1.gain");
        c.push(gain.clone(), f(1.0));
        c.take_due(t0);

        c.push(gain.clone(), f(2.0));
        c.push(gain.clone(), f(3.0));
        let (due, wait) = c.take_due(t0 + Duration::from_millis(20));
        assert_eq!((due, wait), (vec![], Some(Duration::from_millis(30))));
        let (due, wait) = c.take_due(t0 + Duration::from_millis(49));
        assert_eq!((due, wait), (vec![], Some(Duration::from_millis(1))));
        let (due, wait) = c.take_due(t0 + STEP);
        assert_eq!((due, wait), (vec![(gain.clone(), f(3.0))], None));

        // the interval counts from the last send, not from the last push
        c.push(gain.clone(), f(4.0));
        let (due, wait) = c.take_due(t0 + Duration::from_millis(60));
        assert_eq!((due, wait), (vec![], Some(Duration::from_millis(40))));
    }

    #[test]
    fn a_value_after_a_pause_goes_out_at_once() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        c.push(param("out1.gain"), f(1.0));
        c.take_due(t0);
        c.push(param("out1.gain"), f(2.0));
        assert_eq!(c.take_due(t0 + Duration::from_secs(5)).0, [(param("out1.gain"), f(2.0))]);
    }

    #[test]
    fn parameters_do_not_hold_each_other_back() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        c.push(param("out1.gain"), f(1.0));
        c.take_due(t0);
        c.push(param("out1.gain"), f(2.0));
        c.push(param("in1.gain"), f(5.0));
        let (due, wait) = c.take_due(t0 + Duration::from_millis(10));
        assert_eq!((due, wait), (vec![(param("in1.gain"), f(5.0))], Some(Duration::from_millis(40))));
    }

    // SEC-6: what waits as a MUTE can be seen, and only that: not an unmute, not another control
    #[test]
    fn waiting_mutes_can_be_seen() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        assert_eq!(c.muting(), [false, false]);
        c.push(param("out2.mute"), Value::Bool(true));
        c.push(param("out1.mute"), Value::Bool(false));
        c.push(param("in1.mute"), Value::Bool(true));
        c.push(param("out1.gain"), f(1.0));
        assert_eq!(c.muting(), [false, true]);
        c.push(param("out1.mute"), Value::Bool(true));
        assert_eq!(c.muting(), [true, true]);
        c.take_due(t0);
        assert_eq!(c.muting(), [false, false], "written: no longer waiting");
        c.push(param("out1.mute"), Value::Bool(true));
        c.clear();
        assert_eq!(c.muting(), [false, false]);
    }

    #[test]
    fn the_wait_is_the_shortest_one() {
        let t0 = Instant::now();
        let mut c = Coalescer::new(STEP);
        c.push(param("out1.gain"), f(1.0));
        c.take_due(t0);
        c.push(param("in1.gain"), f(1.0));
        c.take_due(t0 + Duration::from_millis(20));
        c.push(param("out1.gain"), f(2.0));
        c.push(param("in1.gain"), f(2.0));
        let (due, wait) = c.take_due(t0 + Duration::from_millis(25));
        assert_eq!((due, wait), (vec![], Some(Duration::from_millis(25))));
    }
}
