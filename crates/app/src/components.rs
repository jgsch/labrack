//! The controls every page uses (UX-21 to UX-24, UX-26, UX-8, UX-12). They draw; the logic is in
//! [`crate::control`], so their value rules are tested without a window.

use dioxus::prelude::*;

use crate::control::{self, Bounds, Click, SecondClick};

/// A vertical fader with its dB scale, its box and its label, as on the Mix & EQ pages: a click never
/// moves the cap, dragging moves it from where the drag started, keys and the wheel step it (UX-22).
/// The level meters (`levels`, in dB on the fader's own scale; two for a stereo pair) run inside the
/// track, behind the cap. `scale` is what the ruler shows when it is wider than what may be set
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
    #[props(default)] levels: Vec<f64>,
    #[props(default)] scale: Option<Bounds>,
    #[props(default)] on_change: EventHandler<f64>,
) -> Element {
    let mut drag = use_signal(|| None::<control::Drag>);
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
    // green to -12 dB, yellow to 0 dB, red above, wherever those sit on this fader's scale
    let colours = format!(
        "linear-gradient(to top,var(--green) 0%,var(--green) {y}%,var(--yellow) {y}%,var(--yellow) {r}%,var(--red) {r}%,var(--red) 100%)",
        y = at(-12.0),
        r = at(0.0)
    );
    let lanes = control::meter_lanes(levels.len());
    // the dark track of each meter, and one in the middle without any
    let grooves: &[&str] = if lanes.is_empty() { control::meter_lanes(1) } else { lanes };
    let meters: Vec<(String, f64)> = levels
        .iter()
        .zip(lanes)
        .map(|(&db, lane)| (lane.to_string(), if db.is_finite() { 100.0 - at(db) } else { 100.0 }))
        .collect();

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
                        let y = event.data().client_coordinates().y;
                        set(d.value_at(y, bounds));
                    }
                },
                onpointerup: move |_| drag.set(None),
                onpointercancel: move |_| drag.set(None),
                onkeydown: move |event| {
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
                    if let Some(next) = control::step(value, step, bounds) {
                        set(next);
                    }
                },
                for lane in grooves.iter() {
                    div { class: "fader-groove {lane}", "aria-hidden": "true" }
                }
                for (lane, cover) in meters {
                    // the colours never change; only the dark cover on top of them moves
                    div { class: "fader-meter {lane}", "aria-hidden": "true", style: "background: {colours}",
                        div { class: "fader-meter-cover", style: "height: {cover}%" }
                    }
                }
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
    let field =
        Committed { draft, error, asked, value, bounds, unit: unit.clone(), ask_raise, ask_from, on_change };

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
                    span { "Raise by {control::number(next - value)} {unit}?" }
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
            Ok(next) if self.ask_raise && control::needs_confirmation(self.value, next, self.ask_from) => {
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
            class: if is_armed { "button confirm-armed" } else { "button" },
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
pub fn Meter(level_db: f64, #[props(default = 0.0)] gain_reduction_db: f64) -> Element {
    let level = control::meter_fill(level_db);
    let gr = control::gr_fill(gain_reduction_db);
    rsx! {
        div {
            class: "meter",
            role: "meter",
            "aria-label": "Level",
            "aria-valuemin": "{control::METER_FLOOR}",
            "aria-valuemax": "0",
            "aria-valuenow": "{control::number(level_db)}",
            div { class: "meter-level",
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
) -> Element {
    rsx! {
        div { class: "toasts",
            if let Some(text) = busy {
                div { class: "toast busy", role: "status",
                    span { class: "spinner", "aria-hidden": "true" }
                    span { "{text}" }
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
