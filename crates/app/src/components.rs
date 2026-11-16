//! The controls every page uses (UX-21 to UX-24, UX-26, UX-8, UX-12). They draw; the logic is in
//! [`crate::control`], so their value rules are tested without a window.

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;

use crate::control::{self, Bounds, Click, Scale, SecondClick};

/// A vertical fader with its dB scale, its box and its label, as on the Mix & EQ pages: a click never
/// moves the cap, dragging moves it from where the drag started, keys and the wheel step it (UX-22).
/// The signal it sets is measured by a [`SignalMeter`] beside it, on a ruler of its own (UX-19).
/// `scale` is what the ruler shows when it is wider than what may be set
/// (the Mixer's ruler is the Levels' +12 to -48, its faders stop at the model's 0 dB).
#[component]
pub fn Fader(
    label: String,
    value: f64,
    bounds: Bounds,
    #[props(default = "dB".to_string())] unit: String,
    /// The strip's own heading already names it on the Mix & EQ pages.
    #[props(default = true)]
    show_label: bool,
    /// The Levels cards put the box beside the fader themselves.
    #[props(default = true)]
    show_box: bool,
    #[props(default)] scale: Option<Bounds>,
    #[props(default)] on_change: EventHandler<f64>,
) -> Element {
    let mut drag = use_signal(|| None::<control::Drag>);
    let mut last_raise = use_signal(|| None::<std::time::Duration>);
    let clock = use_hook(|| {
        let started = std::time::Instant::now();
        move || started.elapsed()
    });
    let ruler = scale.unwrap_or(bounds);
    let span = (ruler.max - ruler.min).max(f64::MIN_POSITIVE);
    let at = move |db: f64| ((db - ruler.min) / span).clamp(0.0, 1.0) * 100.0;
    let start = at(value);
    let text = control::number(value);
    let aria = label.clone();
    let spoken = format!("{text} {unit}");
    let box_label = label.clone();
    let marks = control::scale_marks(ruler);
    // a tick every 6 dB, on a track as tall as --fader-h
    let tick = 6.0 / span;
    let zero = ruler.contains(0.0).then(|| at(0.0));

    let set = move |next: f64| {
        if next != value {
            on_change.call(next);
        }
    };
    rsx! {
        div { class: if show_label { "fader-strip framed" } else { "fader-strip" },
            if show_label {
                span { class: "fader-label", "{label}" }
            }
            div { class: "fader-body", style: "--tick: calc(var(--fader-h) * {tick})",
            div { class: "fader-scale", "aria-hidden": "true",
                // UX-19: the scale says what its numbers are, above them
                span { class: "scale-unit", "dB" }
                for (db, top) in marks.iter().map(|&db| (db, 100.0 - at(db))) {
                    span { style: "top: {top}%", "{control::signed(db)}" }
                }
            }
            div {
                class: "fader-track",
                role: "slider",
                tabindex: "0",
                "aria-label": "{aria}",
                "aria-orientation": "vertical",
                "aria-valuemin": "{bounds.min}",
                "aria-valuemax": "{bounds.max}",
                "aria-valuenow": "{text}",
                "aria-valuetext": "{spoken}",
                onpointerdown: move |event| {
                    let y = event.data().client_coordinates().y;
                    drag.set(Some(control::Drag::start(y, value)));
                },
                onpointermove: move |event| {
                    if let Some(d) = drag() {
                        let data = event.data();
                        let held = data.held_buttons().contains(MouseButton::Primary);
                        match d.value_while(held, data.client_coordinates().y, bounds) {
                            Some(next) => set(next),
                            None => drag.set(None),
                        }
                    }
                },
                onpointerup: move |_| drag.set(None),
                onpointercancel: move |_| drag.set(None),
                // the track gets no pointer-up when the button is released elsewhere: a drag that left it is over,
                // or coming back with the button held would resume from where it started (SEC-4)
                onpointerleave: move |_| drag.set(None),
                onkeydown: move |event| {
                    // a held key repeats 30 times a second: one press, one step (SEC-4)
                    if event.is_auto_repeating() {
                        event.prevent_default();
                        return;
                    }
                    if let Some(step) = control::key_step(&event.key().to_string())
                        && let Some(next) = control::step(value, step, bounds)
                    {
                        event.prevent_default();
                        set(next);
                    }
                },
                onwheel: move |event| {
                    event.prevent_default();
                    let step = control::wheel_step(event.delta().strip_units().y);
                    let now = clock();
                    // raising is spaced out; lowering is not (SEC-4)
                    if step > 0.0 && !control::wheel_raise_allowed(last_raise(), now) {
                        return;
                    }
                    if let Some(next) = control::step(value, step, bounds) {
                        if step > 0.0 {
                            last_raise.set(Some(now));
                        }
                        set(next);
                    }
                },
                div { class: "fader-groove", "aria-hidden": "true" }
                div { class: "fader-ticks", "aria-hidden": "true" }
                if let Some(z) = zero {
                    div { class: "fader-zero", "aria-hidden": "true", style: "bottom: {z}%" }
                }
                div { class: "fader-cap", style: "bottom: calc({start}% - 8px)" }
            }
            }
            if show_box {
                ValueField { label: box_label, value, bounds, unit, on_change }
            }
        }
    }
}

/// The SIGNAL column of a card (UX-19): the measured level, one bar or two for a linked pair, on its own
/// ruler, the scale of the amp's front panel, every 12 dB down from the top with a tick every 6 dB.
#[component]
pub fn SignalMeter(levels: Vec<f64>, scale: control::MeterScale) -> Element {
    let at = move |db: f64| scale.fill(db) * 100.0;
    let colours = format!(
        "linear-gradient(to top,var(--green) 0%,var(--green) {y}%,var(--yellow) {y}%,var(--yellow) {r}%,var(--red) {r}%,var(--red) 100%)",
        y = control::number(at(scale.yellow)),
        r = control::number(at(scale.red))
    );
    let tick = 6.0 / (scale.top - scale.floor);
    let marks = control::scale_marks(Bounds::new(scale.floor, scale.top));
    let lanes = control::meter_lanes(levels.len());
    let bars: Vec<(&str, String)> =
        levels.iter().zip(lanes).map(|(&db, &lane)| (lane, control::number(100.0 - at(db)))).collect();
    rsx! {
        div { class: "card-column signal",
            span { class: "column-title", "SIGNAL" }
            div { class: "signal-meter", "aria-hidden": "true", style: "--tick: calc(var(--fader-h) * {tick})",
                div { class: "signal-bars",
                    for (lane, cover) in bars {
                        // the colours never change; only the dark cover on top of them moves
                        div { class: "signal-bar {lane}", style: "background: {colours}",
                            div { class: "signal-cover", style: "height: {cover}%" }
                        }
                    }
                }
                div { class: "signal-scale",
                    span { class: "scale-unit", "dB" }
                    for (db, top) in marks.iter().map(|&db| (db, 100.0 - at(db))) {
                        span { style: "top: {top}%", "{control::signed(db)}" }
                    }
                }
            }
        }
    }
}

/// A box with a typed value (UX-21, UX-23): it is read on Enter or when the box loses focus, never
/// while typing. A value that is not a number, or outside the range, is refused with a red text
/// beside the box (UX-26, SEC-2). A raise of more than 6 dB asks first (UX-24).
#[component]
pub fn ValueField(
    label: String,
    value: f64,
    bounds: Bounds,
    #[props(default = "dB".to_string())] unit: String,
    /// A raise of more than 6 dB asks first (UX-24); a load or a power is not a level.
    #[props(default = true)]
    ask_raise: bool,
    /// A raise is counted from this level at least: 0 for an EQ band, where a cut eased off is no boost.
    #[props(default = f64::NEG_INFINITY)]
    ask_from: f64,
    /// What the figure is: the raise that asks first is counted in dB of level (UX-24).
    #[props(default)]
    scale: Scale,
    /// Decimals shown: one for levels, two for a Q.
    #[props(default = 1)]
    digits: usize,
    #[props(default)] disabled: bool,
    #[props(default)] on_change: EventHandler<f64>,
) -> Element {
    let mut draft = use_signal(|| None::<String>);
    let error = use_signal(String::new);
    let mut asked = use_signal(|| None::<f64>);
    let shown = draft().unwrap_or_else(|| control::number_to(value, digits));
    let field = Committed {
        draft,
        error,
        asked,
        value,
        bounds,
        unit: unit.clone(),
        ask_raise,
        ask_from,
        scale,
        on_change,
    };

    rsx! {
        div { class: "value-field",
            div { class: "value-box",
                input {
                    r#type: "text",
                    inputmode: "decimal",
                    autocomplete: "off",
                    "aria-label": "{label}",
                    disabled,
                    value: "{shown}",
                    oninput: move |event| draft.set(Some(event.value())),
                    onblur: {
                        let field = field.clone();
                        move |_| field.commit()
                    },
                    onkeydown: move |event| {
                        if event.key().to_string() == "Enter" {
                            event.prevent_default();
                            field.commit();
                        }
                    },
                }
                span { class: "value-unit", "{unit}" }
            }
            if let Some(next) = asked() {
                div { class: "confirm-dialog", role: "alertdialog", "aria-label": "Confirm the raise",
                    span {
                        if scale == Scale::Db {
                            "Raise by {control::number(next - value)} {unit}?"
                        } else {
                            "Raise by {control::number(control::raise_db(value, next, scale))} dB?"
                        }
                    }
                    button {
                        class: "confirm yes",
                        onclick: move |_| {
                            asked.set(None);
                            on_change.call(next);
                        },
                        "RAISE"
                    }
                    button { class: "confirm no", onclick: move |_| asked.set(None), "CANCEL" }
                }
            }
            if !error().is_empty() {
                ErrorText { text: error(), floating: true }
            }
        }
    }
}

/// How long the error of a typed value stays.
const ERROR_SHOWN: std::time::Duration = std::time::Duration::from_secs(3);

/// What takes a typed value into the amp or refuses it; the pieces of a [`ValueField`].
#[derive(Clone)]
struct Committed {
    draft: Signal<Option<String>>,
    error: Signal<String>,
    asked: Signal<Option<f64>>,
    value: f64,
    bounds: Bounds,
    unit: String,
    ask_raise: bool,
    ask_from: f64,
    scale: Scale,
    on_change: EventHandler<f64>,
}

impl Committed {
    /// Read the draft, if any: take it, or say why not, or ask first.
    fn commit(&self) {
        let mut draft = self.draft;
        let mut error = self.error;
        let mut asked = self.asked;
        let Some(raw) = draft() else { return };
        draft.set(None);
        match control::typed(&raw, self.bounds, &self.unit) {
            Err(why) => {
                let text = why.0;
                error.set(text.clone());
                // it fades after a moment (UX-26)
                spawn(async move {
                    tokio::time::sleep(ERROR_SHOWN).await;
                    if error() == text {
                        error.set(String::new());
                    }
                });
            }
            Ok(next) if next == self.value => error.set(String::new()),
            Ok(next)
                if self.ask_raise
                    && control::needs_confirmation_in(self.value, next, self.ask_from, self.scale) =>
            {
                error.set(String::new());
                asked.set(Some(next));
            }
            Ok(next) => {
                error.set(String::new());
                self.on_change.call(next);
            }
        }
    }
}

/// MUTE / UNMUTE, with the arming of a second click that a dangerous action needs (UX-24, SEC-5).
/// The label says what the next click does.
#[component]
pub fn ConfirmButton(
    label: String,
    /// Shown while the second click is armed.
    confirm_label: String,
    /// The class of the button, `confirm-armed` is added while it asks.
    #[props(default = "button".to_string())]
    class: String,
    #[props(default)] on_click: EventHandler<()>,
) -> Element {
    let mut armed = use_signal(SecondClick::default);
    // a fresh clock each render is enough: the window is 3 s and the button re-renders on click
    let now = use_hook(|| {
        let started = std::time::Instant::now();
        move || started.elapsed()
    });
    let state = armed();
    let is_armed = state.armed(now());
    rsx! {
        button {
            class: if is_armed { "{class} confirm-armed" } else { "{class}" },
            "aria-pressed": "{is_armed}",
            onclick: move |_| {
                let mut s = armed();
                match s.click(now()) {
                    Click::Confirmed => {
                        armed.set(s);
                        on_click.call(());
                    }
                    Click::Armed | Click::Expired => armed.set(s),
                }
            },
            if is_armed { "{confirm_label}" } else { "{label}" }
        }
    }
}

/// One output's meter: a segmented bar in green, yellow and red, with the peach gain reduction bar
/// under it (UX-8, UX-12). The label on the left and the figures on the right are the caller's.
#[component]
pub fn Meter(
    level_db: f64,
    #[props(default = 0.0)] gain_reduction_db: f64,
    #[props(default)] scale: control::MeterScale,
) -> Element {
    let level = scale.fill(level_db);
    let gr = control::gr_fill(gain_reduction_db);
    let stops = format!(
        "--meter-yellow: {}%; --meter-red: {}%",
        control::number(scale.fill(scale.yellow) * 100.0),
        control::number(scale.fill(scale.red) * 100.0)
    );
    rsx! {
        div {
            class: "meter",
            role: "meter",
            "aria-label": "Level",
            "aria-valuemin": "{control::number(scale.floor)}",
            "aria-valuemax": "{control::number(scale.top)}",
            "aria-valuenow": "{control::number(level_db)}",
            div { class: "meter-level", style: "{stops}",
                div { class: "meter-cover", style: "width: {(1.0 - level) * 100.0}%" }
            }
            div { class: "meter-gr",
                div { class: "meter-gr-fill", style: "width: {gr * 100.0}%" }
            }
        }
    }
}

/// A message in red, beside the control that caused it (UX-26). It is a live region, so a screen
/// reader says it when it appears. A `floating` one lies over the page instead of moving it.
#[component]
pub fn ErrorText(text: String, #[props(default)] floating: bool) -> Element {
    rsx! {
        span { class: if floating { "error-text floating" } else { "error-text" }, role: "alert", "{text}" }
    }
}

/// What is in progress and the last error, at the bottom right of the window: a spinner while an action of
/// an amp runs (an IPD takes seconds), and a message that wraps and goes by itself or when clicked (UX-29).
#[component]
pub fn Toasts(
    #[props(default)] busy: Option<String>,
    #[props(default)] error: Option<String>,
    #[props(default)] on_dismiss: EventHandler<()>,
    /// What the amps hold above the user ceilings (SEC-3): it stays until dismissed, apart from the errors.
    #[props(default)]
    warnings: Vec<String>,
    #[props(default)] on_dismiss_warnings: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "toasts",
            if let Some(text) = busy {
                div { class: "toast busy", role: "status",
                    span { class: "spinner", "aria-hidden": "true" }
                    span { "{text}" }
                }
            }
            if !warnings.is_empty() {
                div { class: "toast warning", role: "alert", onclick: move |_| on_dismiss_warnings.call(()),
                    span { { warnings.join(" ") } }
                    button { class: "toast-close", "aria-label": "Dismiss the warning", "×" }
                }
            }
            if let Some(text) = error {
                div { class: "toast error", role: "alert", onclick: move |_| on_dismiss.call(()),
                    span { "{text}" }
                    button { class: "toast-close", "aria-label": "Dismiss", "×" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // the components draw the values of `control`; the rules are tested there. What is checked here is
    // what a screen reader and the keyboard see.
    use super::*;

    /// Render a page built inside the runtime (the controls hold signals, which need one).
    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // UX-22, UX-4: the fader is a slider the keyboard reaches, and says where it is
    #[test]
    fn the_fader_is_a_slider() {
        let out = html(|| {
            rsx! {
                Fader { label: "IN 1", value: -6.0, bounds: Bounds::new(-48.0, 12.0) }
            }
        });
        assert!(out.contains("role=\"slider\""), "{out}");
        assert!(out.contains("aria-orientation=\"vertical\""));
        assert!(out.contains("aria-valuemin=\"-48\""));
        assert!(out.contains("aria-valuemax=\"12\""));
        assert!(out.contains("aria-valuenow=\"-6\""));
        assert!(out.contains("aria-valuetext=\"-6 dB\""));
        assert!(out.contains("tabindex=\"0\""));
        assert!(out.contains("inputmode=\"decimal\""), "the box takes a typed value");
        // UX-19: the scale says what its numbers are, above them
        let unit = out.find("class=\"scale-unit\"").unwrap_or_else(|| panic!("the unit: {out}"));
        let first_mark = out.find(">+12<").unwrap();
        assert!(unit < first_mark && out[unit..first_mark].contains(">dB<"), "{out}");
    }

    // UX-19: the SIGNAL meter has its own ruler, the scale it is given, and fills on it
    #[test]
    fn the_signal_meter_has_its_own_ruler() {
        let out = html(|| rsx! { SignalMeter { levels: vec![-30.0], scale: control::MeterScale::IPX } });
        assert!(out.contains(">SIGNAL<"), "{out}");
        let unit = out.find(">dB<").unwrap_or_else(|| panic!("the unit: {out}"));
        assert!(unit < out.find(">0<").unwrap(), "the unit above the numbers: {out}");
        assert!(out.contains(">−60<") && !out.contains(">+12<"), "0 at the top, -60 at the bottom: {out}");
        assert!(out.contains("signal-bar mono") && out.contains("height: 50%"), "-30 dB is half lit: {out}");

        let out =
            html(|| rsx! { SignalMeter { levels: vec![-18.0, -6.0], scale: control::MeterScale::INPUT } });
        assert!(out.contains(">+12<") && out.contains(">−48<"), "{out}");
        assert!(out.contains("signal-bar left") && out.contains("signal-bar right"), "{out}");
        assert!(
            out.contains("height: 50%") && out.contains("height: 30%"),
            "-18 and -6 dB on -48..+12: {out}"
        );
    }

    // UX-23: the box shows the value, not a draft
    #[test]
    fn the_value_box_shows_the_value() {
        let out = html(|| {
            rsx! {
                ValueField { label: "OUT 1", value: -6.5, bounds: Bounds::new(-60.0, 12.0) }
            }
        });
        assert!(out.contains("value=\"-6.5\""), "{out}");
        assert!(out.contains("aria-label=\"OUT 1\""));
    }

    // UX-8, UX-12: the meter shows the level and the gain reduction apart
    #[test]
    fn the_meter_has_the_two_bars() {
        let out = html(|| {
            rsx! {
                Meter { level_db: -30.0, gain_reduction_db: 6.0 }
            }
        });
        assert!(out.contains("role=\"meter\""), "{out}");
        // the unlit half is covered from the right, and the gain reduction is half of its bar
        assert_eq!(out.matches("width: 50%").count(), 2, "half lit and half reduced: {out}");
        assert!(out.contains("meter-gr-fill"), "{out}");
    }

    // UX-29: a spinner for what runs, a message for what failed, nothing when there is neither
    #[test]
    fn the_toasts_show_what_runs_and_what_failed() {
        let none = html(|| rsx! { Toasts {} });
        assert!(!none.contains("toast busy") && !none.contains("toast error"), "{none}");
        let both = html(|| {
            rsx! { Toasts { busy: "Copying the EQ…".to_string(), error: "No answer".to_string() } }
        });
        assert!(both.contains("class=\"spinner\"") && both.contains("Copying the EQ…"), "{both}");
        assert!(both.contains("role=\"alert\"") && both.contains("No answer"), "{both}");
    }

    // SEC-3, UX-29: a ceiling warning is a toast of its own, which an error does not replace
    #[test]
    fn a_ceiling_warning_is_a_toast_of_its_own() {
        let out = html(|| {
            rsx! {
                Toasts { warnings: vec!["Left: out1 gain 3 is above".to_string()], error: "No answer".to_string() }
            }
        });
        assert!(out.contains("toast warning") && out.contains("Left: out1 gain 3 is above"), "{out}");
        assert!(out.contains("toast error") && out.contains("No answer"), "{out}");
        assert!(!html(|| rsx! { Toasts {} }).contains("toast warning"));
    }

    // UX-26: the error is announced
    #[test]
    fn an_error_is_announced() {
        let out = html(|| rsx! { ErrorText { text: "outside -48 to 12 dB" } });
        assert!(out.contains("role=\"alert\""), "{out}");
        assert!(out.contains("error-text"));
        assert!(!out.contains("floating"), "beside the control by default");
    }

    // UX-26: the error of a typed value lies over the page, so that it moves nothing
    #[test]
    fn a_floating_error_does_not_take_room() {
        let out = html(|| rsx! { ErrorText { text: "outside -48 to 12 dB", floating: true } });
        assert!(out.contains("error-text floating"), "{out}");
    }
}
