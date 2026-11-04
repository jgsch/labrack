//! Mix & EQ (UX-16 to UX-20): which tabs exist and what they are called (UX-16, UX-17), the Levels
//! tab (UX-19), the EQ tabs (UX-20) and the Mixer (UX-18).

use dioxus::prelude::*;
use labgruppen::{Channel, EQ_TYPES, EqField, MixerField, Param, Value, XOVER_TYPES, XoverField};
use labrack_core::{Limiter, Side, Unit};

use crate::control::{self, Bounds};
use crate::eq::{self, Band, Filter};
use crate::{Fader, ValueField};

/// The six tabs of UX-16, plus the shared ones a linked pair shows instead of two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixTab {
    Mixer,
    Levels,
    In1,
    In2,
    Out1,
    Out2,
    /// Both inputs on one tab: they are linked.
    Inputs,
    /// Both outputs on one tab: they are linked.
    Outputs,
}

impl MixTab {
    pub fn label(self) -> &'static str {
        match self {
            MixTab::Mixer => "MIXER",
            MixTab::Levels => "LEVELS",
            MixTab::In1 => "INPUT 1 · EQ",
            MixTab::In2 => "INPUT 2 · EQ",
            MixTab::Out1 => "OUTPUT 1 · EQ",
            MixTab::Out2 => "OUTPUT 2 · EQ",
            MixTab::Inputs => "INPUTS 1 + 2 · EQ",
            MixTab::Outputs => "OUTPUTS 1 + 2 · EQ",
        }
    }

    /// The pair a tab belongs to, which its LINK button links: the Mixer feeds the inputs, the Levels
    /// tab is the outputs' (as on the mockup).
    pub fn side(self) -> Side {
        match self {
            MixTab::Mixer | MixTab::In1 | MixTab::In2 | MixTab::Inputs => Side::Inputs,
            MixTab::Levels | MixTab::Out1 | MixTab::Out2 | MixTab::Outputs => Side::Outputs,
        }
    }

    /// The channel an EQ tab edits: a shared tab edits channel 1, which channel 2 follows (BEH-31).
    pub fn channel(self) -> Option<Channel> {
        match self {
            MixTab::In1 | MixTab::Inputs => Some(Channel::Input(0)),
            MixTab::In2 => Some(Channel::Input(1)),
            MixTab::Out1 | MixTab::Outputs => Some(Channel::Output(0)),
            MixTab::Out2 => Some(Channel::Output(1)),
            MixTab::Mixer | MixTab::Levels => None,
        }
    }

    /// The tabs in the row (UX-17): a linked pair is one tab instead of two.
    pub fn row(linked_inputs: bool, linked_outputs: bool) -> Vec<MixTab> {
        let mut tabs = vec![MixTab::Mixer, MixTab::Levels];
        if linked_inputs {
            tabs.push(MixTab::Inputs);
        } else {
            tabs.extend([MixTab::In1, MixTab::In2]);
        }
        if linked_outputs {
            tabs.push(MixTab::Outputs);
        } else {
            tabs.extend([MixTab::Out1, MixTab::Out2]);
        }
        tabs
    }

    /// Where a tab shows while its pair is linked, or not (the tab row changed under the user).
    pub fn visible(self, linked_inputs: bool, linked_outputs: bool) -> Option<MixTab> {
        let row = MixTab::row(linked_inputs, linked_outputs);
        if row.contains(&self) {
            return Some(self);
        }
        Some(match self {
            MixTab::In1 | MixTab::In2 => MixTab::Inputs,
            MixTab::Out1 | MixTab::Out2 => MixTab::Outputs,
            MixTab::Inputs => MixTab::In1,
            MixTab::Outputs => MixTab::Out1,
            other => other,
        })
    }
}

/// One amplifier in the picker: its name and the level and gain reduction of its loudest output.
#[derive(Clone, Debug, PartialEq)]
pub struct AmpCard {
    pub name: String,
    pub level_db: f64,
    pub gr_db: f64,
}

/// An IN or OUT card of the Levels tab: its gain, the signal level its meter shows, and its mute.
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    pub label: String,
    pub db: f64,
    pub level_db: f64,
    pub muted: bool,
}

/// The LIMITER card: its gain reduction and its four linked figures (BEH-5), `None` until they are
/// read. `bounds` is what each box takes, in the order of [`LIMITER_BOXES`].
#[derive(Clone, Debug, PartialEq)]
pub struct LimiterStrip {
    pub label: String,
    pub gr_db: f64,
    pub figures: Option<Limiter>,
    pub bounds: [Bounds; 4],
}

/// One row of the Levels tab (UX-19): IN n, OUT n, LIMITER n.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelRow {
    pub input: Strip,
    pub output: Strip,
    pub limiter: LimiterStrip,
}

/// A row of the Mixer (UX-18): the input it feeds and its four sources, each a [`Strip`] named after
/// the source.
#[derive(Clone, Debug, PartialEq)]
pub struct MixRow {
    pub label: String,
    pub sources: Vec<Strip>,
}

/// The names of the four sources, in the order of [`labgruppen::MIXER_SOURCES`] (BEH-3).
pub const SOURCE_NAMES: [&str; 4] = ["ANALOG 1", "ANALOG 2", "AES 1", "AES 2"];

/// What each box of an EQ tab takes: the model's ranges (SEC-2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBounds {
    pub freq: Bounds,
    pub gain: Bounds,
    pub q: Bounds,
    pub delay: Bounds,
    pub hpf: Bounds,
    pub lpf: Bounds,
}

/// One EQ tab (UX-20): the channel it edits, its ten bands, the high-pass and low-pass filters of an
/// output (`None` on an input), and its delay.
#[derive(Clone, Debug, PartialEq)]
pub struct EqView {
    pub channel: Channel,
    pub bands: Vec<Band>,
    pub filters: Option<(Filter, Filter)>,
    pub delay_ms: f64,
    /// The time ticking DELAY again restores: the last one it was unticked from, while the app runs.
    pub delay_restore: Option<f64>,
    pub bounds: EqBounds,
}

/// What the page draws.
#[derive(Clone, Debug, PartialEq)]
pub struct MixeqView {
    pub amps: Vec<AmpCard>,
    pub selected: usize,
    pub tab: MixTab,
    pub linked_inputs: bool,
    pub linked_outputs: bool,
    pub rows: Vec<LevelRow>,
    /// The Mixer's two rows, and the range of its levels (the model's: the IPD tops out at 0 dB).
    pub mixer: Vec<MixRow>,
    pub mixer_bounds: Bounds,
    /// The open EQ tab's, when one is open.
    pub eq: Option<EqView>,
}

impl Default for MixeqView {
    fn default() -> Self {
        MixeqView {
            amps: Vec::new(),
            selected: 0,
            tab: MixTab::Levels,
            linked_inputs: false,
            linked_outputs: false,
            rows: Vec::new(),
            mixer: Vec::new(),
            mixer_bounds: Bounds { min: -48.0, max: 0.0 },
            eq: None,
        }
    }
}

/// The gain range of the faders of a page: the caller passes the model's, `MX` is the mockup's.
pub const MIX_GAIN: Bounds = Bounds { min: -48.0, max: 12.0 };

/// The four boxes of a limiter, in the mockup's order: what they set, their name and their unit.
pub const LIMITER_BOXES: [(Unit, &str, &str); 4] = [
    (Unit::Ohm, "Load", "Ω"),
    (Unit::Vpk, "V peak", "V"),
    (Unit::Watt, "Power", "W"),
    (Unit::Dbu, "Level", "dBu"),
];

/// The gain reduction scale of a limiter, in dB of reduction (UX-19: as long as the faders).
const GR_MARKS: [f64; 5] = [0.0, 3.0, 6.0, 9.0, 12.0];

/// An IN or OUT card, or a source of the Mixer: the fader with its meter, and the dB box and MUTE of
/// the same size beside it (UX-18, UX-19). `out` draws it in the outputs' colour; a muted OUT is dimmed
/// whole, a muted source only its fader (`mixer`).
#[component]
pub fn LevelCard(
    strip: Strip,
    #[props(default)] out: bool,
    #[props(default)] mixer: bool,
    #[props(default = MIX_GAIN)] bounds: Bounds,
    #[props(default)] on_gain: EventHandler<f64>,
    #[props(default)] on_mute: EventHandler<bool>,
) -> Element {
    let muted = strip.muted;
    let class = match (out, mixer) {
        (true, _) => "level-card out",
        (_, true) => "level-card mix",
        _ => "level-card",
    };
    rsx! {
        section { class, "aria-label": "{strip.label}",
            span { class: "legend", "{strip.label}" }
            div { class: if out && muted { "card-body dim" } else { "card-body" },
                div { class: if mixer && muted { "card-fader dim" } else { "card-fader" },
                    Fader {
                        label: "{strip.label}",
                        value: strip.db,
                        bounds,
                        show_label: false,
                        show_box: false,
                        levels: vec![strip.level_db],
                        on_change: move |db| on_gain.call(db),
                    }
                }
                div { class: "card-side",
                    ValueField { label: "{strip.label}", value: strip.db, bounds, on_change: move |db| on_gain.call(db) }
                    button {
                        class: if muted { "card-button muted" } else { "card-button" },
                        "aria-pressed": "{muted}",
                        "aria-label": if muted { "Unmute {strip.label}" } else { "Mute {strip.label}" },
                        onclick: move |_| on_mute.call(!muted),
                        "MUTE"
                    }
                }
            }
        }
    }
}

/// The Mixer tab (UX-18, BEH-27): a row per input, its four sources with their level and mute into
/// it, and where they go (→ IN n, which opens the EQ of that input).
#[component]
pub fn MixerPage(
    rows: Vec<MixRow>,
    bounds: Bounds,
    #[props(default)] on_set: EventHandler<Vec<(Param, Value)>>,
    #[props(default)] on_tab: EventHandler<MixTab>,
) -> Element {
    rsx! {
        section { class: "tab-panel in",
            for (input, row) in rows.into_iter().enumerate() {
                div { class: "mixer-row", key: "{input}",
                    div { class: "mixer-cards",
                        for (src, source) in row.sources.into_iter().enumerate() {
                            LevelCard {
                                key: "{src}",
                                strip: source,
                                mixer: true,
                                bounds,
                                on_gain: move |db| on_set.call(vec![(mixer_param(input, src, MixerField::Level), Value::Float(db))]),
                                on_mute: move |muted| on_set.call(vec![(mixer_param(input, src, MixerField::Mute), Value::Bool(muted))]),
                            }
                        }
                    }
                    button {
                        class: "mixer-go",
                        "aria-label": "Open the EQ of {row.label}",
                        onclick: move |_| on_tab.call(if input == 0 { MixTab::In1 } else { MixTab::In2 }),
                        span { class: "arrow", "aria-hidden": "true", "→" }
                        span { class: "target", "{row.label}" }
                    }
                }
            }
        }
    }
}

/// The level or mute of source `src` into input `input`.
fn mixer_param(input: usize, src: usize, field: MixerField) -> Param {
    Param::Mixer { ch: Channel::Input(input as u8), src: src as u8, field }
}

/// The LIMITER card: the gain reduction bar with its scale, and the four linked boxes (BEH-5, UX-19).
#[component]
pub fn LimiterCard(limiter: LimiterStrip, #[props(default)] on_change: EventHandler<(Unit, f64)>) -> Element {
    let gr = control::gr_fill(limiter.gr_db) * 100.0;
    let label = limiter.label.clone();
    rsx! {
        section { class: "level-card limiter", "aria-label": "{limiter.label}",
            span { class: "legend", "{limiter.label}" }
            div { class: "card-body",
                div { class: "gr-meter", "aria-hidden": "true",
                    div { class: "gr-bar",
                        div { class: "gr-fill", style: "height: {gr}%" }
                    }
                    div { class: "gr-scale",
                        for db in GR_MARKS {
                            span { style: "top: {db / control::GR_TOP * 100.0}%", "{control::signed(-db)}" }
                        }
                    }
                }
                div { class: "limiter-boxes",
                    for (i, (unit, name, suffix)) in LIMITER_BOXES.into_iter().enumerate() {
                        div { class: "limiter-box", key: "{name}",
                            span { class: "box-name", "{name}" }
                            if let Some(figures) = limiter.figures {
                                ValueField {
                                    label: "Limiter {name} of {label} in {suffix}",
                                    value: figures.in_unit(unit),
                                    bounds: limiter.bounds[i],
                                    unit: suffix,
                                    ask_raise: unit == Unit::Dbu,
                                    on_change: move |value| on_change.call((unit, value)),
                                }
                            } else {
                                NoValue { label: "{name}", unit: suffix }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A box with nothing to show: a value not read yet, or one its type does not have (UX-20).
#[component]
pub fn NoValue(label: String, #[props(default)] unit: String) -> Element {
    rsx! {
        div { class: "value-field",
            div { class: "value-box",
                input { r#type: "text", disabled: true, value: "—", "aria-label": "{label}" }
                span { class: "value-unit", "{unit}" }
            }
        }
    }
}

/// The frequencies of the graph's grid lines, and how they are written.
const GRID_HZ: [(f64, &str); 8] = [
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1k"),
    (2000.0, "2k"),
    (5000.0, "5k"),
    (10000.0, "10k"),
];
/// The gains of the graph's grid lines.
const GRID_DB: [f64; 4] = [12.0, 6.0, -6.0, -12.0];

/// A HPF or LPF row of an output (BEH-30): on/off, frequency and shape.
#[component]
fn FilterRow(
    channel: Channel,
    high: bool,
    filter: Filter,
    bounds: Bounds,
    on_set: EventHandler<Vec<(Param, Value)>>,
) -> Element {
    let name = if high { "HPF" } else { "LPF" };
    let long = if high { "High-pass" } else { "Low-pass" };
    let param = move |field| eq::filter_param(channel, high, field);
    rsx! {
        div { class: if filter.on { "eq-extra-row" } else { "eq-extra-row off" },
            label { class: "eq-extra-name",
                input {
                    r#type: "checkbox",
                    checked: filter.on,
                    onchange: move |e| on_set.call(vec![(param(XoverField::Enabled), Value::Bool(e.checked()))]),
                }
                "{name}"
            }
            ValueField {
                label: "{long} frequency in hertz, for example 80 or 1k",
                value: filter.freq,
                bounds,
                unit: "Hz",
                ask_raise: false,
                on_change: move |hz| on_set.call(vec![(param(XoverField::Freq), Value::Float(hz))]),
            }
            select {
                "aria-label": "{long} filter type",
                value: "{filter.kind}",
                onchange: move |e| on_set.call(vec![(param(XoverField::Type), Value::Text(e.value()))]),
                for kind in XOVER_TYPES {
                    option { value: "{kind}", selected: *kind == filter.kind, "{eq::filter_label(kind)}" }
                }
            }
        }
    }
}

/// One band row of the 10-BAND EQ (BEH-29): on, number, type, frequency, gain and Q.
#[component]
fn BandRow(
    channel: Channel,
    band: u8,
    value: Band,
    bounds: EqBounds,
    on_set: EventHandler<Vec<(Param, Value)>>,
) -> Element {
    let n = band + 1;
    let param = move |field| Param::Eq { ch: channel, band, field };
    let set = move |field, v| on_set.call(vec![(param(field), v)]);
    rsx! {
        div { class: if value.on { "eq-row" } else { "eq-row off" },
            label { class: "eq-on",
                input {
                    r#type: "checkbox",
                    checked: value.on,
                    "aria-label": "Band {n} enabled",
                    onchange: move |e| set(EqField::Enabled, Value::Bool(e.checked())),
                }
            }
            span { class: "eq-n", "{n}" }
            select {
                "aria-label": "Band {n} type",
                value: "{value.kind}",
                onchange: move |e| set(EqField::Type, Value::Text(e.value())),
                for kind in EQ_TYPES {
                    option { value: "{kind}", selected: *kind == value.kind, "{eq::eq_label(kind)}" }
                }
            }
            ValueField {
                label: "Band {n} frequency in hertz, for example 1k",
                value: value.freq,
                bounds: bounds.freq,
                unit: "Hz",
                ask_raise: false,
                on_change: move |hz| set(EqField::Freq, Value::Float(hz)),
            }
            if eq::has_gain(&value.kind) {
                ValueField {
                    label: "Band {n} gain in decibels",
                    value: value.gain,
                    bounds: bounds.gain,
                    on_change: move |db| set(EqField::Gain, Value::Float(db)),
                }
            } else {
                div { class: "eq-none", NoValue { label: "Band {n} gain", unit: "dB" } }
            }
            ValueField {
                label: "Band {n} width Q",
                value: value.q,
                bounds: bounds.q,
                unit: "",
                digits: 2,
                ask_raise: false,
                on_change: move |q| set(EqField::Q, Value::Float(q)),
            }
        }
    }
}

/// An EQ tab (UX-20): the curve with its handles, the filters and delay under it, and the ten bands.
/// Every change goes out as the writes it makes.
#[component]
pub fn EqPage(
    eq: EqView,
    #[props(default)] on_set: EventHandler<Vec<(Param, Value)>>,
    /// DELAY ticked or unticked, with the time it showed (UX-20).
    #[props(default)]
    on_delay: EventHandler<(Channel, bool, f64)>,
) -> Element {
    let ch = eq.channel;
    // ticked with nothing to restore: the box is open for a time, on this channel only
    let mut armed = use_signal(|| None::<Channel>);
    let delay_on = eq.delay_ms > 0.0 || armed() == Some(ch);
    let delay_ms = eq.delay_ms;
    let restore = eq.delay_restore;
    let side = if ch.is_input() { "in" } else { "out" };
    let filters: Vec<(Filter, bool)> =
        eq.filters.iter().flat_map(|(hp, lp)| [(hp.clone(), true), (lp.clone(), false)]).collect();
    let (line, fill) = eq::curve(&eq.bands, &filters);
    let handles: Vec<(u8, f64, f64)> = eq
        .bands
        .iter()
        .enumerate()
        .filter(|(_, b)| b.on)
        .map(|(i, b)| {
            let gain = if eq::has_gain(&b.kind) { b.gain } else { 0.0 };
            (i as u8 + 1, eq::x_of(b.freq) * 100.0, eq::y_of(gain) * 100.0)
        })
        .collect();
    let bounds = eq.bounds;
    rsx! {
        section { class: "tab-panel {side}",
            div { class: "eq-layout",
                div { class: "eq-left",
                    div { class: "eq-graph", "aria-hidden": "true",
                        svg {
                            "viewBox": "0 0 1000 300",
                            "preserveAspectRatio": "none",
                            for (hz, _) in GRID_HZ {
                                line { class: "grid", x1: "{eq::x_of(hz) * 1000.0}", y1: "0", x2: "{eq::x_of(hz) * 1000.0}", y2: "300" }
                            }
                            for db in GRID_DB {
                                line { class: "grid", x1: "0", y1: "{eq::y_of(db) * 300.0}", x2: "1000", y2: "{eq::y_of(db) * 300.0}" }
                            }
                            line { class: "zero", x1: "0", y1: "150", x2: "1000", y2: "150" }
                            path { class: "fill", d: "{fill}" }
                            path { class: "curve", d: "{line}" }
                        }
                        for db in [12.0, 6.0, 0.0, -6.0, -12.0] {
                            span { class: "eq-db", style: "top: {eq::y_of(db) * 100.0 - 1.5}%", "{control::signed(db)}" }
                        }
                        for (hz, text) in GRID_HZ {
                            span { class: "eq-hz", style: "left: {eq::x_of(hz) * 100.0}%", "{text}" }
                        }
                        for (n, left, top) in handles {
                            span { class: "eq-handle", style: "left: {left}%; top: {top}%", "{n}" }
                        }
                    }
                    div { class: "eq-extra {side}",
                        if let Some((hp, lp)) = eq.filters.clone() {
                            FilterRow { channel: ch, high: true, filter: hp, bounds: bounds.hpf, on_set }
                            FilterRow { channel: ch, high: false, filter: lp, bounds: bounds.lpf, on_set }
                        }
                        div { class: if delay_on { "eq-extra-row" } else { "eq-extra-row off" },
                            label { class: "eq-extra-name",
                                input {
                                    r#type: "checkbox",
                                    checked: delay_on,
                                    "aria-label": "Delay on",
                                    onchange: move |e| {
                                        let on = e.checked();
                                        armed.set((on && restore.is_none()).then_some(ch));
                                        on_delay.call((ch, on, delay_ms));
                                    },
                                }
                                "DELAY"
                            }
                            ValueField {
                                label: if ch.is_input() { "Input delay in milliseconds" } else { "Delay in milliseconds" },
                                value: eq.delay_ms,
                                bounds: bounds.delay,
                                unit: "ms",
                                ask_raise: false,
                                disabled: !delay_on,
                                on_change: move |ms| on_set.call(vec![(Param::Delay(ch), Value::Float(ms))]),
                            }
                        }
                    }
                }
                div { class: "eq-right",
                    div { class: "eq-head",
                        h3 { "10-band EQ" }
                        div { class: "eq-actions",
                            button { class: "button eq-action", onclick: move |_| on_set.call(eq::all_off(ch)), "ALL OFF" }
                            button { class: "button eq-action", onclick: move |_| on_set.call(eq::reset(ch)), "RESET" }
                        }
                    }
                    div { class: "eq-row eq-columns",
                        span { class: "center", "On" }
                        span { "#" }
                        span { "Type" }
                        span { class: "right", "Freq" }
                        span { class: "right", "Gain" }
                        span { class: "right", "Q" }
                    }
                    for (band, value) in eq.bands.iter().enumerate() {
                        BandRow { key: "{band}", channel: ch, band: band as u8, value: value.clone(), bounds, on_set }
                    }
                }
            }
        }
    }
}

/// The Levels tab (UX-19): two rows of IN, OUT and LIMITER, in the outputs' panel.
#[component]
pub fn LevelsPage(
    view: MixeqView,
    #[props(default)] on_gain: EventHandler<(usize, u8, f64)>,
    #[props(default)] on_mute: EventHandler<(usize, u8, bool)>,
    #[props(default)] on_limiter: EventHandler<(usize, Unit, f64)>,
) -> Element {
    rsx! {
        section { class: "tab-panel out",
            for (channel, row) in view.rows.iter().enumerate() {
                div { class: "levels-row", key: "{channel}",
                    LevelCard {
                        strip: row.input.clone(),
                        on_gain: move |db| on_gain.call((channel, 0, db)),
                        on_mute: move |muted| on_mute.call((channel, 0, muted)),
                    }
                    LevelCard {
                        strip: row.output.clone(),
                        out: true,
                        on_gain: move |db| on_gain.call((channel, 1, db)),
                        on_mute: move |muted| on_mute.call((channel, 1, muted)),
                    }
                    LimiterCard {
                        limiter: row.limiter.clone(),
                        on_change: move |(unit, value)| on_limiter.call((channel, unit, value)),
                    }
                }
            }
        }
    }
}

/// The Mix & EQ page: the amplifier, the tab row (UX-16, UX-17) and the tab itself.
#[component]
pub fn MixeqPage(
    view: MixeqView,
    #[props(default)] on_pick: EventHandler<usize>,
    #[props(default)] on_tab: EventHandler<MixTab>,
    #[props(default)] on_link: EventHandler<(Side, bool)>,
    #[props(default)] on_gain: EventHandler<(usize, u8, f64)>,
    #[props(default)] on_mute: EventHandler<(usize, u8, bool)>,
    #[props(default)] on_limiter: EventHandler<(usize, Unit, f64)>,
    #[props(default)] on_set: EventHandler<Vec<(Param, Value)>>,
    #[props(default)] on_delay: EventHandler<(Channel, bool, f64)>,
) -> Element {
    let tabs = MixTab::row(view.linked_inputs, view.linked_outputs);
    let tab = view.tab.visible(view.linked_inputs, view.linked_outputs).unwrap_or(MixTab::Levels);
    let side = tab.side();
    let linked = if side == Side::Inputs { view.linked_inputs } else { view.linked_outputs };
    let side_class = if side == Side::Inputs { "in" } else { "out" };
    let link_class = match (linked, side) {
        (false, _) => "link-button",
        (true, Side::Inputs) => "link-button on in",
        (true, Side::Outputs) => "link-button on out",
    };
    let pair = if side == Side::Inputs { "IN" } else { "OUT" };
    rsx! {
        div { class: "mixeq",
            div { class: "amp-picker",
                for (index, amp) in view.amps.iter().enumerate() {
                    button {
                        class: if index == view.selected { "picker-card picked" } else { "picker-card" },
                        "aria-pressed": "{index == view.selected}",
                        onclick: move |_| on_pick.call(index),
                        span { class: "picker-title",
                            span { class: "picker-index", "{index + 1:02}" }
                            span { class: "picker-name", "{amp.name}" }
                        }
                        span { class: "picker-bars", "aria-hidden": "true",
                            span { class: "picker-level",
                                span { style: "width: {control::meter_fill(amp.level_db) * 100.0}%" }
                            }
                            span { class: "picker-gr",
                                span { style: "width: {control::gr_fill(amp.gr_db) * 100.0}%" }
                            }
                        }
                    }
                }
            }
            div { class: "tab-row", role: "group", "aria-label": "Sections",
                for t in tabs {
                    button {
                        class: format!("mix-tab {}{}", if t.side() == Side::Inputs { "in" } else { "out" }, if t == tab { " on" } else { "" }),
                        "aria-pressed": "{t == tab}",
                        onclick: move |_| on_tab.call(t),
                        "{t.label()}"
                    }
                }
                if tab != MixTab::Mixer {
                    button {
                        class: link_class,
                        "aria-pressed": "{linked}",
                        "aria-label": if linked { "Unlink {pair} 1 and {pair} 2" } else { "Link {pair} 1 and {pair} 2" },
                        onclick: move |_| on_link.call((side, !linked)),
                        if linked { "⛓ LINKED · click to split" } else { "⛓ LINK 1 + 2" }
                    }
                }
            }
            match tab {
                MixTab::Levels => rsx! { LevelsPage { view: view.clone(), on_gain, on_mute, on_limiter } },
                MixTab::Mixer => rsx! { MixerPage { rows: view.mixer.clone(), bounds: view.mixer_bounds, on_set, on_tab } },
                _ => match view.eq.clone() {
                    Some(eq) => rsx! { EqPage { eq, on_set, on_delay } },
                    None => rsx! { section { class: "tab-panel {side_class}" } },
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX-16, UX-17: six tabs; a linked pair shows one tab instead of two
    #[test]
    fn the_tab_row_follows_the_links() {
        assert_eq!(MixTab::row(false, false).len(), 6);
        assert_eq!(MixTab::In1.label(), "INPUT 1 · EQ");
        assert_eq!(
            MixTab::row(true, false),
            [MixTab::Mixer, MixTab::Levels, MixTab::Inputs, MixTab::Out1, MixTab::Out2]
        );
        assert_eq!(
            MixTab::row(false, true),
            [MixTab::Mixer, MixTab::Levels, MixTab::In1, MixTab::In2, MixTab::Outputs]
        );
        assert_eq!(MixTab::row(true, true).len(), 4);
    }

    // UX-17: a tab that disappears under the user shows the shared tab instead, and back
    #[test]
    fn a_hidden_tab_falls_back_to_the_shared_one() {
        assert_eq!(MixTab::In2.visible(true, false), Some(MixTab::Inputs));
        assert_eq!(MixTab::Out1.visible(false, true), Some(MixTab::Outputs));
        assert_eq!(MixTab::In1.visible(false, false), Some(MixTab::In1));
        assert_eq!(MixTab::Levels.visible(true, true), Some(MixTab::Levels));
        assert_eq!(MixTab::Mixer.visible(true, true), Some(MixTab::Mixer));
        assert_eq!(MixTab::Outputs.visible(false, false), Some(MixTab::Out1), "unlinked under the user");
    }

    // UX-17: the LINK button of a tab links its own pair; the Levels tab is the outputs'
    #[test]
    fn each_tab_links_its_own_pair() {
        assert_eq!(MixTab::Levels.side(), Side::Outputs);
        assert_eq!(MixTab::Out2.side(), Side::Outputs);
        assert_eq!(MixTab::In1.side(), Side::Inputs);
        assert_eq!(MixTab::Inputs.side(), Side::Inputs);
    }

    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn view(linked_inputs: bool) -> MixeqView {
        let strip = |label: &str| Strip { label: label.into(), db: -6.0, level_db: -20.0, muted: false };
        MixeqView {
            amps: vec![AmpCard { name: "FOH".into(), level_db: -20.0, gr_db: 0.0 }],
            linked_inputs,
            rows: vec![LevelRow {
                input: strip("IN 1"),
                output: strip("OUT 1"),
                limiter: LimiterStrip {
                    label: "LIMITER 1".into(),
                    gr_db: 3.0,
                    figures: Some(Limiter::from_dbu(31.26, 4.0)),
                    bounds: [Bounds::new(1.0, 32.0); 4],
                },
            }],
            ..MixeqView::default()
        }
    }

    thread_local! {
        static VIEW: std::cell::RefCell<MixeqView> = std::cell::RefCell::new(MixeqView::default());
    }

    #[component]
    fn Probe() -> Element {
        rsx! { MixeqPage { view: VIEW.with(|v| v.borrow().clone()) } }
    }

    fn page_html(v: MixeqView) -> String {
        VIEW.with(|s| *s.borrow_mut() = v);
        html(Probe)
    }

    // UX-16, UX-17: the row shows the tabs of the links, and the link button says its state
    #[test]
    fn the_page_shows_the_row_and_the_links() {
        let out = page_html(view(false));
        assert_eq!(out.matches("INPUT 1 · EQ").count(), 1, "{out}");
        assert!(out.contains("INPUT 2 · EQ"));
        assert!(out.contains("LINK 1 + 2"));
        assert!(out.contains("Link OUT 1 and OUT 2"), "the Levels tab links the outputs: {out}");

        let out = page_html(view(true));
        assert!(out.contains("INPUTS 1 + 2 · EQ"), "one tab when linked: {out}");
        assert!(!out.contains("INPUT 2 · EQ"));
    }

    // UX-20, BEH-31: an EQ tab edits its channel; a shared tab edits channel 1
    #[test]
    fn each_eq_tab_edits_its_channel() {
        assert_eq!(MixTab::In2.channel(), Some(Channel::Input(1)));
        assert_eq!(MixTab::Inputs.channel(), Some(Channel::Input(0)));
        assert_eq!(MixTab::Out2.channel(), Some(Channel::Output(1)));
        assert_eq!(MixTab::Outputs.channel(), Some(Channel::Output(0)));
        assert_eq!(MixTab::Levels.channel(), None);
    }

    fn eq_view(ch: Channel) -> EqView {
        let mut bands: Vec<Band> = (0..10).map(eq::default_band).collect();
        bands[1].on = true;
        bands[1].gain = 3.0;
        let b = Bounds::new(-12.0, 12.0);
        EqView {
            channel: ch,
            bands,
            filters: (!ch.is_input()).then(|| {
                (
                    Filter { on: true, kind: "butterworth_24".into(), freq: 40.0 },
                    Filter { on: false, kind: "linkwitz_riley_24".into(), freq: 120.0 },
                )
            }),
            delay_ms: 10.0,
            delay_restore: None,
            bounds: EqBounds { freq: b, gain: b, q: b, delay: b, hpf: b, lpf: b },
        }
    }

    #[component]
    fn EqProbe(ch: Channel) -> Element {
        rsx! { EqPage { eq: eq_view(ch) } }
    }

    fn eq_html(ch: Channel) -> String {
        let mut dom = VirtualDom::new_with_props(EqProbe, EqProbeProps { ch });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // UX-20, BEH-29: ten bands with their boxes; a shelf has no Q, a band that is on has a handle
    #[test]
    fn the_eq_tab_has_the_ten_bands() {
        let out = eq_html(Channel::Output(0));
        assert_eq!(out.matches("class=\"eq-row\"").count() + out.matches("class=\"eq-row off\"").count(), 10);
        assert_eq!(out.matches("class=\"eq-row off\"").count(), 9, "only band 2 is on: {out}");
        assert!(out.contains("aria-label=\"Band 10 type\""));
        assert!(
            out.contains("Band 1 width Q") && out.contains("Band 10 width Q"),
            "the shelves have a Q: {out}"
        );
        assert!(!out.contains("value=\"—\""), "no box without a value: {out}");
        assert_eq!(out.matches("class=\"eq-handle\"").count(), 1, "one band on, one handle");
        assert!(out.contains(">Low shelf<") && out.contains(">High-pass<"), "the amps' types: {out}");
        assert!(out.contains("10-band EQ") && out.contains("ALL OFF") && out.contains("RESET"));
        assert!(out.contains("class=\"curve\""));
    }

    // UX-20, BEH-30: an output has HPF and LPF with the amps' shapes, and delay; an input only delay
    #[test]
    fn outputs_add_the_filters() {
        let out = eq_html(Channel::Output(0));
        assert!(out.contains(">HPF") && out.contains(">LPF"), "{out}");
        assert!(out.contains(">Linkwitz-Riley 24 dB/oct<") && out.contains(">Bessel 12 dB/oct<"));
        assert!(out.contains("DELAY<") && out.contains("aria-label=\"Delay on\""), "{out}");
        assert!(!out.contains("m of air"));
        let input = eq_html(Channel::Input(1));
        assert!(!input.contains(">HPF") && !input.contains(">LPF"), "{input}");
        assert!(input.contains("DELAY<") && input.contains("Input delay in milliseconds"));
        assert!(input.contains("tab-panel in"), "in the inputs' colour");
    }

    // UX-20: DELAY is ticked while there is a delay; at 0 ms it is unticked and its box closed
    #[test]
    fn delay_is_ticked_while_there_is_one() {
        let out = eq_html(Channel::Input(0));
        assert!(out.contains("checked=true") || out.contains("checked=\"true\""), "10 ms: {out}");
        assert!(!out.contains("class=\"eq-extra-row off\""), "{out}");

        let mut dom = VirtualDom::new(|| {
            let mut eq = eq_view(Channel::Input(0));
            eq.delay_ms = 0.0;
            rsx! { EqPage { eq } }
        });
        dom.rebuild_in_place();
        let off = dioxus_ssr::render(&dom);
        assert!(off.contains("class=\"eq-extra-row off\""), "0 ms: {off}");
        assert!(off.contains("disabled"), "the box is closed: {off}");
    }

    // UX-16: the EQ tab shows the open channel's EQ
    #[test]
    fn the_page_shows_the_open_eq_tab() {
        let v = MixeqView { tab: MixTab::Out2, eq: Some(eq_view(Channel::Output(1))), ..view(false) };
        let out = page_html(v);
        assert!(out.contains("10-band EQ") && out.contains(">HPF"), "{out}");
        assert!(!out.contains("LIMITER 1"), "not the Levels tab");
    }

    // UX-18, BEH-27: the Mixer has a row per input with its four sources, their level and mute, and
    // where they go; the range is the model's (the IPD mixer tops out at 0 dB)
    #[test]
    fn the_mixer_has_the_four_sources_of_each_input() {
        let rows = (1..=2)
            .map(|n| MixRow {
                label: format!("IN {n}"),
                sources: SOURCE_NAMES
                    .iter()
                    .map(|name| Strip {
                        label: (*name).into(),
                        db: -6.0,
                        level_db: -30.0,
                        muted: *name != "ANALOG 1",
                    })
                    .collect(),
            })
            .collect();
        let v = MixeqView { tab: MixTab::Mixer, mixer: rows, ..view(false) };
        let out = page_html(v);
        for name in SOURCE_NAMES {
            assert_eq!(out.matches(&format!("class=\"legend\">{name}<")).count(), 2, "{name}: {out}");
        }
        assert_eq!(out.matches("role=\"slider\"").count(), 8);
        assert!(out.contains("aria-valuemax=\"0\""), "the mixer's range: {out}");
        assert_eq!(out.matches("card-fader dim").count(), 6, "the muted sources are dimmed");
        assert!(out.contains("Open the EQ of IN 1") && out.contains("Open the EQ of IN 2"));
        assert!(!out.contains("LINK 1 + 2"), "no link button on the Mixer");
        assert!(out.contains("tab-panel in"));
    }

    // UX-19: the three cards of a row, each named once, with a meter in the fader, the box and MUTE
    // beside it, and the limiter's four linked boxes and its gain reduction
    #[test]
    fn the_levels_row_has_the_three_cards() {
        let out = page_html(view(false));
        for name in ["IN 1", "OUT 1", "LIMITER 1"] {
            assert_eq!(out.matches(&format!("class=\"legend\">{name}<")).count(), 1, "{name}: {out}");
        }
        assert_eq!(out.matches("role=\"slider\"").count(), 2, "IN and OUT have a fader, not the limiter");
        assert_eq!(out.matches("fader-meter mono").count(), 2, "{out}");
        assert_eq!(out.matches("class=\"card-side\"").count(), 2);
        for name in ["Load", "V peak", "Power", "Level"] {
            assert!(out.contains(&format!("class=\"box-name\">{name}<")), "{name}: {out}");
        }
        assert!(out.contains("value=\"31.3\""), "the dBu: {out}");
        assert!(out.contains("height: 25%"), "3 dB of the 12 dB gain reduction scale: {out}");
        assert!(out.contains("Mute OUT 1"));
    }
}
