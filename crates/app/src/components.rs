//! The controls every page uses (UX-21 to UX-24, UX-26, UX-8, UX-12). They draw; the logic is in
//! [`crate::control`], so their value rules are tested without a window.

use dioxus::prelude::*;

use crate::control::{self, Bounds, Click, SecondClick};

/// A vertical fader with its dB box and its label, as on the Mix & EQ pages: a click never moves the
/// cap, dragging moves it from where the drag started, keys and the wheel step it (UX-22).
#[component]
pub fn Fader(
    label: String,
    value: f64,
    bounds: Bounds,
    #[props(default = "dB".to_string())] unit: String,
    #[props(default)] on_change: EventHandler<f64>,
) -> Element {
    let mut drag = use_signal(|| None::<control::Drag>);
    let span = (bounds.max - bounds.min).max(f64::MIN_POSITIVE);
    let start = (value - bounds.min) / span * 100.0;
    let text = control::number(value);
    let aria = label.clone();
    let spoken = format!("{text} {unit}");
    let box_label = label.clone();

    let set = move |next: f64| {
        if next != value {
            on_change.call(next);
        }
    };
    rsx! {
        div { class: "fader-strip",
            span { class: "fader-label", "{label}" }
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
                div { class: "fader-cap", style: "bottom: calc({start}% - 8px)" }
            }
            ValueField { label: box_label, value, bounds, unit, on_change }
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
    #[props(default)] on_change: EventHandler<f64>,
) -> Element {
    let mut draft = use_signal(|| None::<String>);
    let error = use_signal(String::new);
    let mut asked = use_signal(|| None::<f64>);
    let shown = draft().unwrap_or_else(|| control::number(value));
    let field = Committed { draft, error, asked, value, bounds, on_change };

    rsx! {
        div { class: "value-field",
            div { class: "value-box",
                input {
                    r#type: "text",
                    inputmode: "decimal",
                    autocomplete: "off",
                    "aria-label": "{label}",
                    value: "{shown}",
                    oninput: move |event| draft.set(Some(event.value())),
                    onblur: move |_| field.commit(),
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
                ErrorText { text: error() }
            }
        }
    }
}

/// What takes a typed value into the amp or refuses it; the pieces of a [`ValueField`].
#[derive(Clone, Copy)]
struct Committed {
    draft: Signal<Option<String>>,
    error: Signal<String>,
    asked: Signal<Option<f64>>,
    value: f64,
    bounds: Bounds,
    on_change: EventHandler<f64>,
}

impl Committed {
    /// Read the draft, if any: take it, or say why not, or ask first.
    fn commit(self) {
        let mut draft = self.draft;
        let mut error = self.error;
        let mut asked = self.asked;
        let Some(raw) = draft() else { return };
        draft.set(None);
        match control::typed_db(&raw, self.bounds) {
            Err(why) => error.set(why.0),
            Ok(next) if next == self.value => error.set(String::new()),
            Ok(next) if control::needs_confirmation(self.value, next) => {
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
/// reader says it when it appears.
#[component]
pub fn ErrorText(text: String) -> Element {
    rsx! {
        span { class: "error-text", role: "alert", "{text}" }
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

    // UX-26: the error is announced
    #[test]
    fn an_error_is_announced() {
        let out = html(|| rsx! { ErrorText { text: "outside -48 to 12 dB" } });
        assert!(out.contains("role=\"alert\""), "{out}");
        assert!(out.contains("error-text"));
    }
}
