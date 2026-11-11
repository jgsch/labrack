//! The logic of the shared controls, without a window (ARCH-1, ARCH-5): a fader's value from a drag,
//! a key or the wheel (UX-22), a typed value (UX-21, UX-23), the second click of a confirmation
//! (UX-24), and the figures of a meter (UX-8, UX-12). The components in [`crate::components`] only
//! draw what these functions return, so all of it is tested without WebKitGTK.

use std::time::Duration;

/// What a control may take: the model's range (SEC-2), never the fader's own idea of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: f64,
    pub max: f64,
}

impl Bounds {
    pub fn new(min: f64, max: f64) -> Bounds {
        Bounds { min, max }
    }

    pub fn clamp(self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }

    pub fn contains(self, value: f64) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// A key that steps a fader, and by how much (UX-22).
pub fn key_step(key: &str) -> Option<f64> {
    match key {
        "ArrowUp" | "ArrowRight" => Some(0.5),
        "ArrowDown" | "ArrowLeft" => Some(-0.5),
        "PageUp" => Some(3.0),
        "PageDown" => Some(-3.0),
        _ => None,
    }
}

/// Raising by the wheel is spaced out by this much: a fast spin or a touchpad's inertia sends hundreds
/// of notches a second (UX-22, SEC-4).
pub const WHEEL_RAISE_GAP: Duration = Duration::from_millis(200);

/// Whether a wheel notch that raises a level may act at `now`, given when the last one did.
pub fn wheel_raise_allowed(last: Option<Duration>, now: Duration) -> bool {
    last.is_none_or(|at| now.saturating_sub(at) >= WHEEL_RAISE_GAP)
}

/// A wheel notch: up is louder (UX-22).
pub fn wheel_step(delta_y: f64) -> f64 {
    if delta_y < 0.0 { 0.5 } else { -0.5 }
}

/// Where a fader was grabbed: the pointer's height and the value at that moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub y: f64,
    pub value: f64,
}

impl Drag {
    pub fn start(y: f64, value: f64) -> Drag {
        Drag { y, value }
    }

    /// The value while the pointer is at `y`: relative to where the drag started, 0.2 dB per pixel,
    /// up is louder, but never more than [`RAISE_ASK`] above where the drag started (a flick cannot
    /// jump a level; let go and drag again to go on). A click that does not move changes nothing, and
    /// the range is never left.
    pub fn value_at(self, y: f64, bounds: Bounds) -> f64 {
        bounds.clamp((self.value + (self.y - y) * 0.2).min(self.value + RAISE_ASK))
    }

    /// [`Drag::value_at`] while the button is held; `None` once it is not, which ends the drag. A
    /// release outside the track sends the fader no pointer-up.
    pub fn value_while(self, held: bool, y: f64, bounds: Bounds) -> Option<f64> {
        held.then(|| self.value_at(y, bounds))
    }
}

/// A fader moved by a key or the wheel: one step, kept inside the range. `None` when the key does not
/// move a fader or the value is already at that end.
pub fn step(value: f64, delta: f64, bounds: Bounds) -> Option<f64> {
    let next = bounds.clamp(value + delta);
    (next != value).then_some(next)
}

/// Why typed text was not taken (UX-23, UX-26).
#[derive(Clone, Debug, PartialEq)]
pub struct NotTaken(pub String);

/// A value typed into a box: taken on Enter or blur only, never while typing (UX-23).
///
/// Accepts a decimal number, a comma as the decimal separator, a minus sign, an optional `dB`, and
/// `off` / `-inf` for the bottom of the range. Refuses what is not a number, NaN, infinity, and a
/// value outside the range instead of clamping it (SEC-2).
pub fn typed_db(text: &str, bounds: Bounds) -> Result<f64, NotTaken> {
    typed(text, bounds, "dB")
}

/// A value typed into a box of `unit`, read as [`typed_db`] reads a level: the unit may follow the
/// number, a frequency may be written in kHz (`1k`, `1.5 kHz`), and `off` is the bottom of a level
/// only. The reason for a refusal names the unit.
pub fn typed(text: &str, bounds: Bounds, unit: &str) -> Result<f64, NotTaken> {
    let level = unit.eq_ignore_ascii_case("dB");
    let what = if level { "a level" } else { "a number" };
    let t = text.trim().to_lowercase().replace(',', ".");
    let t = t.replace('\u{2212}', "-");
    let lower = unit.to_lowercase();
    let t = t.strip_suffix(lower.as_str()).unwrap_or(&t).trim();
    let (t, scale) = match t.strip_suffix('k') {
        Some(rest) if lower == "hz" => (rest.trim(), 1000.0),
        _ => (t, 1.0),
    };
    let value = match t {
        "off" | "-inf" | "-infinity" if level => bounds.min,
        "" => return Err(NotTaken(format!("Enter {what}"))),
        _ => match t.parse::<f64>() {
            Ok(v) if v.is_finite() => v * scale,
            _ => return Err(NotTaken(format!("{text:?} is not {what}"))),
        },
    };
    if !bounds.contains(value) {
        let why =
            format!("{} is outside {} to {} {unit}", number(value), number(bounds.min), number(bounds.max));
        return Err(NotTaken(why.trim_end().into()));
    }
    Ok(value)
}

/// A typed value that raises a level by more than this asks first (UX-24).
pub const RAISE_ASK: f64 = 6.0;

/// Whether taking `next` over `current` has to be confirmed: a raise of more than 6 dB, counted from
/// `from` at least. An EQ band counts from flat (0 dB): taking a cut of -12 dB to -1 dB is no boost.
pub fn needs_confirmation(current: f64, next: f64, from: f64) -> bool {
    next - current.max(from) > RAISE_ASK
}

/// What a figure is, for counting a raise in dB (UX-24): a level is already dB, a peak voltage is
/// 20 dB per decade, a power 10 dB per decade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    #[default]
    Db,
    Volts,
    Watts,
}

/// By how many dB taking `next` over `current` raises the level; from nothing it is infinite.
pub fn raise_db(current: f64, next: f64, scale: Scale) -> f64 {
    let per_decade = match scale {
        Scale::Db => return next - current,
        Scale::Volts => 20.0,
        Scale::Watts => 10.0,
    };
    if next <= 0.0 {
        f64::NEG_INFINITY
    } else if current <= 0.0 {
        f64::INFINITY
    } else {
        per_decade * (next / current).log10()
    }
}

/// [`needs_confirmation`] for a figure of `scale`.
pub fn needs_confirmation_in(current: f64, next: f64, from: f64, scale: Scale) -> bool {
    match scale {
        Scale::Db => needs_confirmation(current, next, from),
        _ => raise_db(current, next, scale) > RAISE_ASK,
    }
}

/// A confirmation that takes two clicks on the same control (UX-24, SEC-5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SecondClick {
    armed: Option<Duration>,
}

/// What a click did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// The first click: the button now asks for the second.
    Armed,
    /// The second click, in time: do it.
    Confirmed,
    /// A click after the window: it arms again rather than doing it.
    Expired,
}

/// How long the second click has (UX-24).
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(3);

impl SecondClick {
    pub fn armed(&self, now: Duration) -> bool {
        self.armed.is_some_and(|at| now.saturating_sub(at) <= CONFIRM_WINDOW)
    }

    /// The click at `now`: arms, confirms, or arms again when the window had passed. Anything that
    /// happened outside the control cancels it.
    pub fn click(&mut self, now: Duration) -> Click {
        match self.armed {
            Some(at) if now.saturating_sub(at) <= CONFIRM_WINDOW => {
                self.armed = None;
                Click::Confirmed
            }
            Some(_) => {
                self.armed = Some(now);
                Click::Expired
            }
            None => {
                self.armed = Some(now);
                Click::Armed
            }
        }
    }

    pub fn cancel(&mut self) {
        self.armed = None;
    }
}

/// A click that unmutes an output this soon after a click that muted it is ignored: the second click of a
/// nervous double-click on MUTE would otherwise undo it (SEC-5, SEC-7). The user can unmute right after.
pub const UNMUTE_GUARD: Duration = Duration::from_millis(600);

/// Whether a click that unmutes may act at `now`, given when the output was last muted by a click.
pub fn unmute_allowed(last_muted: Option<Duration>, now: Duration) -> bool {
    last_muted.is_none_or(|at| now.saturating_sub(at) >= UNMUTE_GUARD)
}

/// The bottom of a level meter, in dB.
pub const METER_FLOOR: f64 = -60.0;
/// The top of the gain reduction bar, in dB of reduction.
pub const GR_TOP: f64 = 12.0;

/// How much of a meter is lit, 0 to 1 (UX-8, UX-12). Silent and below the floor light nothing.
pub fn meter_fill(level_db: f64) -> f64 {
    if !level_db.is_finite() {
        return 0.0;
    }
    ((level_db - METER_FLOOR) / -METER_FLOOR).clamp(0.0, 1.0)
}

/// How much of the peach gain reduction bar is lit, 0 to 1.
pub fn gr_fill(gr_db: f64) -> f64 {
    if !gr_db.is_finite() || gr_db <= 0.0 {
        return 0.0;
    }
    (gr_db / GR_TOP).clamp(0.0, 1.0)
}

/// The colour of the lit part of a meter at `fill`: green below 60 %, yellow to 85 %, red above (UX-8).
pub fn meter_colour(fill: f64) -> &'static str {
    if fill > 0.85 {
        "#f38ba8"
    } else if fill > 0.60 {
        "#f9e2af"
    } else {
        "#a6e3a1"
    }
}

/// The labelled marks of a fader's scale: every 12 dB down from the top, the bottom included
/// (+12, 0, -12 ... -48 on the mockup's -48..12).
pub fn scale_marks(bounds: Bounds) -> Vec<f64> {
    let mut marks = Vec::new();
    let mut db = bounds.max;
    while db >= bounds.min - 1e-9 && marks.len() < 20 {
        marks.push(db);
        db -= 12.0;
    }
    marks
}

/// A scale mark as the mockup writes it: a plus on a raise, a true minus sign on a cut.
pub fn signed(db: f64) -> String {
    let text = number(db.abs());
    if db > 0.0 {
        format!("+{text}")
    } else if db < 0.0 {
        format!("\u{2212}{text}")
    } else {
        text
    }
}

/// Where the meters of a fader run inside its track: one in the middle, or two side by side for a
/// linked pair (BEH-34).
pub fn meter_lanes(count: usize) -> &'static [&'static str] {
    match count {
        0 => &[],
        1 => &["mono"],
        _ => &["left", "right"],
    }
}

/// A number as the boxes show it: no trailing zeros, at most one decimal for tenths.
pub fn number(value: f64) -> String {
    number_to(value, 1)
}

/// A number with at most `digits` decimals and no trailing zeros (a Q of 0.71 needs two).
pub fn number_to(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return "off".into();
    }
    let text = format!("{value:.digits$}");
    if !text.contains('.') {
        return text;
    }
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAIN: Bounds = Bounds { min: -48.0, max: 12.0 };

    // UX-22: a click never changes the value; a drag moves it from where it started
    #[test]
    fn a_click_changes_nothing_and_a_drag_is_relative() {
        let drag = Drag::start(400.0, -6.0);
        assert_eq!(drag.value_at(400.0, GAIN), -6.0, "a click");
        assert_eq!(drag.value_at(395.0, GAIN), -5.0, "5 px up is +1 dB");
        assert_eq!(drag.value_at(410.0, GAIN), -8.0, "10 px down is -2 dB");
        // relative, not absolute: grabbing the cap high on the track does not jump
        let low = Drag::start(700.0, -12.0);
        assert_eq!(low.value_at(700.0, GAIN), -12.0);
        assert_eq!(low.value_at(690.0, GAIN), -10.0);
    }

    // UX-22, SEC-4: a drag ends when the button is no longer held, e.g. it was released outside the
    // track, where the fader gets no pointer-up; the next pass of the mouse moves nothing
    #[test]
    fn a_drag_ends_when_the_button_is_released_elsewhere() {
        let drag = Drag::start(400.0, -6.0);
        assert_eq!(drag.value_while(true, 395.0, GAIN), Some(-5.0));
        assert_eq!(drag.value_while(false, 395.0, GAIN), None);
        assert_eq!(drag.value_while(false, 400.0, GAIN), None);
    }

    // SEC-2: a drag never leaves the range of the model
    #[test]
    fn a_drag_stops_at_the_range() {
        let drag = Drag::start(0.0, 10.0);
        assert_eq!(drag.value_at(-1000.0, GAIN), 12.0);
        assert_eq!(Drag::start(0.0, -1.0).value_at(-1000.0, Bounds::new(-60.0, 0.0)), 0.0);
        assert_eq!(drag.value_at(1000.0, GAIN), -48.0);
        assert_eq!(Drag::start(0.0, 0.5).value_at(-10.0, Bounds::new(0.0, 0.0)), 0.0);
    }

    // UX-22: keys and the wheel step 0.5 dB, Page Up and Page Down 3 dB
    #[test]
    fn keys_and_the_wheel_step() {
        assert_eq!(key_step("ArrowUp"), Some(0.5));
        assert_eq!(key_step("ArrowRight"), Some(0.5));
        assert_eq!(key_step("ArrowDown"), Some(-0.5));
        assert_eq!(key_step("ArrowLeft"), Some(-0.5));
        assert_eq!(key_step("PageUp"), Some(3.0));
        assert_eq!(key_step("PageDown"), Some(-3.0));
        for other in ["Enter", "Tab", "a", "Home", "End"] {
            assert_eq!(key_step(other), None, "{other}");
        }
        assert_eq!(wheel_step(-1.0), 0.5);
        assert_eq!(wheel_step(1.0), -0.5);

        assert_eq!(step(0.0, 0.5, GAIN), Some(0.5));
        assert_eq!(step(11.8, 0.5, GAIN), Some(12.0), "stops at the top");
        assert_eq!(step(12.0, 0.5, GAIN), None, "already at the top: nothing to send");
        assert_eq!(step(-47.9, -3.0, GAIN), Some(-48.0));
        assert_eq!(step(-48.0, -0.5, GAIN), None);
    }

    // UX-22, SEC-2: the fader's own range comes from the model, not the mockup's -48..12. The IPD
    // mixer tops out at 0 dB
    #[test]
    fn the_range_is_the_model_s() {
        let mixer = Bounds::new(-60.0, 0.0);
        assert_eq!(step(0.0, 0.5, mixer), None);
        assert_eq!(step(-0.2, 0.5, mixer), Some(0.0));
        assert_eq!(Drag::start(0.0, -1.0).value_at(-100.0, mixer), 0.0);
        assert!(typed_db("0.5", mixer).is_err());
    }

    // UX-21, UX-23: typed values are read on commit, not while typing; the forms people type
    #[test]
    fn typed_values() {
        for (text, value) in [
            ("-6", -6.0),
            ("-6.5", -6.5),
            (" -6,5 ", -6.5),
            ("\u{2212}6.5", -6.5),
            ("-6.5 dB", -6.5),
            ("0", 0.0),
            ("+3", 3.0),
            ("off", -48.0),
            ("-inf", -48.0),
        ] {
            assert_eq!(typed_db(text, GAIN), Ok(value), "{text:?}");
        }
        // typing -20 passes through -2 on the way: only the committed text is read
        let current = -9.0;
        assert_eq!(typed_db("-2", GAIN), Ok(-2.0));
        assert_ne!(typed_db("-2", GAIN).unwrap(), current);
    }

    // UX-26, SEC-2: what is not a level, and what is outside the range, is refused with the reason
    #[test]
    fn a_bad_typed_value_is_refused() {
        for text in ["", "  ", "abc", "1.2.3", "NaN", "inf", "-infinity!", "--3"] {
            assert!(typed_db(text, GAIN).is_err(), "{text:?}");
        }
        for text in ["12.5", "-49", "1e9"] {
            let err = typed_db(text, GAIN).unwrap_err();
            assert!(err.0.contains("outside"), "{text}: {err:?}");
        }
        assert!(typed_db("12", GAIN).is_ok(), "the edge is in the range");
        assert!(typed_db("-48", GAIN).is_ok());
    }

    // UX-24: a raise of more than 6 dB by typing asks first
    #[test]
    fn a_big_raise_asks_first() {
        let any = f64::NEG_INFINITY;
        assert!(!needs_confirmation(-12.0, -6.0, any), "6 dB exactly");
        assert!(needs_confirmation(-12.0, -5.9, any));
        assert!(!needs_confirmation(-2.0, -8.0, any), "a cut never asks");
        assert!(!needs_confirmation(0.0, 0.0, any));
    }

    // UX-24: a limiter typed in volts or watts asks when it raises the level by more than 6 dB
    #[test]
    fn a_big_raise_in_volts_or_watts_asks_first() {
        let any = f64::NEG_INFINITY;
        assert!(needs_confirmation_in(10.0, 20.0, any, Scale::Volts), "+6.02 dB");
        assert!(!needs_confirmation_in(10.0, 19.9, any, Scale::Volts));
        assert!(needs_confirmation_in(100.0, 400.0, any, Scale::Watts), "+6.02 dB");
        assert!(!needs_confirmation_in(100.0, 390.0, any, Scale::Watts));
        assert!(!needs_confirmation_in(69.0, 10.0, any, Scale::Volts), "a cut never asks");
        assert!(needs_confirmation_in(0.0, 50.0, any, Scale::Watts), "from nothing is a raise");
        assert!(!needs_confirmation_in(50.0, 0.0, any, Scale::Watts));
        assert!((raise_db(10.0, 20.0, Scale::Volts) - 6.0206).abs() < 1e-3);
        assert!((raise_db(100.0, 200.0, Scale::Watts) - 3.0103).abs() < 1e-3);
    }

    // UX-24: an EQ band counts a raise from flat, so easing a cut off does not ask
    #[test]
    fn easing_an_eq_cut_off_does_not_ask() {
        assert!(!needs_confirmation(-12.0, -1.0, 0.0), "still a cut");
        assert!(!needs_confirmation(-12.0, 6.0, 0.0), "a 6 dB boost");
        assert!(needs_confirmation(-12.0, 6.1, 0.0));
        assert!(needs_confirmation(2.0, 8.1, 0.0), "from where it is when that is above flat");
    }

    // UX-24, SEC-5: the second click within 3 s confirms, after it the button only arms again
    #[test]
    fn a_confirmation_takes_two_clicks() {
        let mut c = SecondClick::default();
        assert_eq!(c.click(Duration::ZERO), Click::Armed);
        assert!(c.armed(Duration::from_millis(2999)));
        assert_eq!(c.click(Duration::from_millis(2999)), Click::Confirmed);
        assert!(!c.armed(Duration::from_millis(2999)), "used up");

        // too late: it arms again instead of confirming
        assert_eq!(c.click(Duration::ZERO), Click::Armed);
        assert_eq!(c.click(Duration::from_millis(3001)), Click::Expired);
        assert!(c.armed(Duration::from_millis(3001)), "armed again");
        assert_eq!(c.click(Duration::from_millis(5000)), Click::Confirmed);

        // another control, or anything else, cancels it
        c.cancel();
        assert!(!c.armed(Duration::from_millis(5000)));
    }

    // SEC-5, SEC-7: the second click of a double-click on MUTE does not unmute
    #[test]
    fn a_click_right_after_a_mute_does_not_unmute() {
        let at = Duration::from_secs(10);
        assert!(unmute_allowed(None, at), "never muted by a click");
        assert!(!unmute_allowed(Some(at), at), "the same instant");
        assert!(!unmute_allowed(Some(at), at + Duration::from_millis(599)));
        assert!(unmute_allowed(Some(at), at + Duration::from_millis(600)));
    }

    // UX-8, UX-12: the meter is green, then yellow, then red; the gain reduction bar is separate
    #[test]
    fn the_meter_fills_from_the_floor() {
        assert_eq!(meter_fill(-60.0), 0.0);
        assert_eq!(meter_fill(-100.0), 0.0);
        assert_eq!(meter_fill(-30.0), 0.5);
        assert_eq!(meter_fill(0.0), 1.0);
        assert_eq!(meter_fill(6.0), 1.0, "above 0 dB stays full");
        assert_eq!(meter_fill(f64::NEG_INFINITY), 0.0);

        assert_eq!(meter_colour(0.2), "#a6e3a1");
        assert_eq!(meter_colour(0.6), "#a6e3a1");
        assert_eq!(meter_colour(0.61), "#f9e2af");
        assert_eq!(meter_colour(0.85), "#f9e2af");
        assert_eq!(meter_colour(0.86), "#f38ba8");

        assert_eq!(gr_fill(0.0), 0.0);
        assert_eq!(gr_fill(-3.0), 0.0);
        assert_eq!(gr_fill(6.0), 0.5);
        assert_eq!(gr_fill(20.0), 1.0);
    }

    // UX-19: the fader's scale is labelled every 12 dB, as on the mockup, whatever the model's range
    #[test]
    fn the_fader_scale_is_labelled_every_12_db() {
        assert_eq!(scale_marks(GAIN), [12.0, 0.0, -12.0, -24.0, -36.0, -48.0]);
        assert_eq!(scale_marks(Bounds::new(-60.0, 0.0)), [0.0, -12.0, -24.0, -36.0, -48.0, -60.0]);
        assert_eq!(signed(12.0), "+12");
        assert_eq!(signed(0.0), "0");
        assert_eq!(signed(-24.0), "\u{2212}24");
    }

    // BEH-34: one meter in the middle of a fader, two side by side for a linked pair
    #[test]
    fn a_fader_has_one_or_two_meters() {
        assert!(meter_lanes(0).is_empty());
        assert_eq!(meter_lanes(1), ["mono"]);
        assert_eq!(meter_lanes(2), ["left", "right"]);
    }

    // the boxes show whole decibels without a needless decimal
    #[test]
    fn numbers_are_shown_plainly() {
        assert_eq!(number(-6.0), "-6");
        assert_eq!(number(-6.5), "-6.5");
        assert_eq!(number(0.0), "0");
        assert_eq!(number(f64::NEG_INFINITY), "off");
        assert_eq!(number_to(0.71, 2), "0.71");
        assert_eq!(number_to(1.4, 2), "1.4");
        assert_eq!(number_to(1000.0, 2), "1000");
        assert_eq!(number_to(-0.001, 1), "0");
    }

    // UX-20, UX-26: the EQ boxes take hertz (and kHz), Q and milliseconds; a refusal names the unit
    #[test]
    fn typed_values_in_other_units() {
        let hz = Bounds::new(20.0, 20000.0);
        assert_eq!(typed("1k", hz, "Hz"), Ok(1000.0));
        assert_eq!(typed("1.5 kHz", hz, "Hz"), Ok(1500.0));
        assert_eq!(typed("80 Hz", hz, "Hz"), Ok(80.0));
        assert_eq!(typed("2,5k", hz, "Hz"), Ok(2500.0));
        assert!(typed("off", hz, "Hz").is_err(), "off is a level only");
        let err = typed("10", hz, "Hz").unwrap_err();
        assert_eq!(err.0, "10 is outside 20 to 20000 Hz");
        assert!(typed("1k", Bounds::new(0.0, 2000.0), "ms").is_err(), "k is for hertz only");
        assert_eq!(typed("12.5", Bounds::new(0.0, 2000.0), "ms"), Ok(12.5));
    }
}
