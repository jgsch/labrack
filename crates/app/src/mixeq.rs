//! Mix & EQ (UX-16 to UX-20). This first part: which tabs exist and what they are called (UX-16,
//! UX-17), and the Levels tab (UX-19). The Mixer and the EQ tabs follow in the same change.

use dioxus::prelude::*;
use labrack_core::{Limiter, Side, Unit};

use crate::control::{self, Bounds};
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

/// What the page draws.
#[derive(Clone, Debug, PartialEq)]
pub struct MixeqView {
    pub amps: Vec<AmpCard>,
    pub selected: usize,
    pub tab: MixTab,
    pub linked_inputs: bool,
    pub linked_outputs: bool,
    pub rows: Vec<LevelRow>,
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

/// An IN or OUT card: the fader with its meter, and the dB box and MUTE of the same size beside it
/// (UX-19). `out` draws it in the outputs' colour.
#[component]
pub fn LevelCard(
    strip: Strip,
    #[props(default)] out: bool,
    #[props(default)] on_gain: EventHandler<f64>,
    #[props(default)] on_mute: EventHandler<bool>,
) -> Element {
    let muted = strip.muted;
    rsx! {
        section { class: if out { "level-card out" } else { "level-card" }, "aria-label": "{strip.label}",
            span { class: "legend", "{strip.label}" }
            div { class: if out && muted { "card-body dim" } else { "card-body" },
                Fader {
                    label: "{strip.label}",
                    value: strip.db,
                    bounds: MIX_GAIN,
                    show_label: false,
                    show_box: false,
                    levels: vec![strip.level_db],
                    on_change: move |db| on_gain.call(db),
                }
                div { class: "card-side",
                    ValueField { label: "{strip.label}", value: strip.db, bounds: MIX_GAIN, on_change: move |db| on_gain.call(db) }
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
                                div { class: "value-field",
                                    div { class: "value-box",
                                        input { r#type: "text", disabled: true, value: "—", "aria-label": "{name}" }
                                        span { class: "value-unit", "{suffix}" }
                                    }
                                }
                            }
                        }
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
                MixTab::Mixer => rsx! { section { class: "tab-panel in", p { class: "todo", "The Mixer tab is next." } } },
                _ => rsx! { section { class: "tab-panel {side_class}", p { class: "todo", "The EQ tabs are next." } } },
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
