//! The Search page (UX-14, UX-15, UX-25): the IPD and IPX switches with SCAN NOW, the manual add form,
//! and the list of known amplifiers with their editable name, badge, address, source and REMOVE
//! (BEH-17 to BEH-23).

use dioxus::prelude::*;

use crate::control;

/// One row of Known amplifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct KnownAmp {
    pub index: usize,
    pub name: String,
    /// IPD or IPX, as the badge shows it.
    pub protocol: String,
    pub ip: String,
    /// AUTO or MANUAL.
    pub source: String,
    /// The user ceilings (SEC-3): the highest output gain in dB, the highest limiter threshold in dBu.
    pub max_gain_db: Option<f64>,
    pub max_limiter_dbu: Option<f64>,
}

/// A ceiling typed in its box: nothing or `none` is no ceiling (SEC-3).
pub fn ceiling_from_text(text: &str) -> Result<Option<f64>, String> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    match text.replace(',', ".").parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(Some(v)),
        _ => Err(format!("{text} is not a number: type a level in dB or leave the box empty for no ceiling")),
    }
}

/// What the page shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchView {
    pub ipd: bool,
    pub ipx: bool,
    pub scanning: bool,
    pub ipd_count: usize,
    pub ipx_count: usize,
    /// What the last scan did, and why one could not be done (BEH-19).
    pub message: String,
    pub amps: Vec<KnownAmp>,
}

impl Default for SearchView {
    fn default() -> Self {
        SearchView {
            ipd: true,
            ipx: true,
            scanning: false,
            ipd_count: 0,
            ipx_count: 0,
            message: String::new(),
            amps: Vec::new(),
        }
    }
}

impl SearchView {
    /// The line under a protocol's switch (UX-25): searching, off, or scanning now.
    pub fn status_of(&self, ipd: bool) -> String {
        let (on, count) = if ipd { (self.ipd, self.ipd_count) } else { (self.ipx, self.ipx_count) };
        if !on {
            format!("Off · not searching · {count} listed")
        } else if self.scanning {
            "Scanning the network…".into()
        } else {
            format!("Searching · {count} found")
        }
    }

    /// SCAN NOW does nothing when both protocols are off (BEH-17).
    pub fn can_scan(&self) -> bool {
        (self.ipd || self.ipx) && !self.scanning
    }
}

/// A toggle with its status line (UX-25): the whole row is the switch, as on the mockup.
#[component]
pub fn Switch(
    name: String,
    on: bool,
    status: String,
    #[props(default)] on_toggle: EventHandler<bool>,
) -> Element {
    rsx! {
        button {
            class: if on { "switch-row on" } else { "switch-row" },
            role: "switch",
            "aria-checked": "{on}",
            "aria-label": "Search for {name} amplifiers",
            onclick: move |_| on_toggle.call(!on),
            span { class: "switch", "aria-hidden": "true",
                span { class: "knob" }
            }
            span { class: "switch-text",
                span { class: "switch-name", "{name} amplifiers" }
                span { class: "switch-status", "{status}" }
            }
        }
    }
}

/// The Search page.
#[component]
pub fn SearchPage(
    view: SearchView,
    #[props(default = String::new())] add_error: String,
    #[props(default)] on_ipd: EventHandler<bool>,
    #[props(default)] on_ipx: EventHandler<bool>,
    #[props(default)] on_scan: EventHandler<()>,
    #[props(default)] on_add: EventHandler<(String, bool, String)>,
    #[props(default)] on_rename: EventHandler<(usize, String)>,
    #[props(default)] on_remove: EventHandler<usize>,
    /// The amp at the first index goes to the second one (BEH-45).
    #[props(default)]
    on_move: EventHandler<(usize, usize)>,
    /// A ceiling box of the amp at the index was changed: (index, max gain text, max limiter text) (SEC-3).
    #[props(default)]
    on_ceilings: EventHandler<(usize, String, String)>,
) -> Element {
    let count = view.amps.len();
    let mut name = use_signal(String::new);
    let mut ip = use_signal(String::new);
    let mut ipd_form = use_signal(|| true);
    let scan = view.can_scan();
    let ipd_status = view.status_of(true);
    let ipx_status = view.status_of(false);
    rsx! {
        div { class: "search-page",
            div { class: "search-grid",
                section { class: "panel", "aria-label": "Search the network",
                    span { class: "caption", "SEARCH THE NETWORK FOR" }
                    Switch { name: "IPD", on: view.ipd, status: ipd_status, on_toggle: move |on| on_ipd.call(on) }
                    Switch { name: "IPX", on: view.ipx, status: ipx_status, on_toggle: move |on| on_ipx.call(on) }
                    button {
                        class: "button scan",
                        disabled: !scan,
                        onclick: move |_| on_scan.call(()),
                        if view.scanning { "SCANNING…" } else { "SCAN NOW" }
                    }
                    if !view.message.is_empty() {
                        span { class: "scan-message", role: "status", "{view.message}" }
                    }
                }
                section { class: "panel", "aria-label": "Add an amplifier manually",
                    span { class: "caption", "ADD AN AMPLIFIER MANUALLY" }
                    label { class: "field",
                        "NAME"
                        input {
                            r#type: "text",
                            autocomplete: "off",
                            "aria-label": "Name of the amplifier to add",
                            value: "{name}",
                            oninput: move |event| name.set(event.value()),
                        }
                    }
                    div { class: "add-line",
                        div { class: "field",
                            "PROTOCOL"
                            div { class: "protocol-pick",
                                button {
                                    class: if ipd_form() { "protocol picked" } else { "protocol" },
                                    "aria-pressed": "{ipd_form()}",
                                    onclick: move |_| ipd_form.set(true),
                                    "IPD"
                                }
                                button {
                                    class: if ipd_form() { "protocol" } else { "protocol picked" },
                                    "aria-pressed": "{!ipd_form()}",
                                    onclick: move |_| ipd_form.set(false),
                                    "IPX"
                                }
                            }
                        }
                        label { class: "field grow",
                            "IP ADDRESS"
                            input {
                                r#type: "text",
                                inputmode: "decimal",
                                autocomplete: "off",
                                placeholder: "192.168.1.50",
                                "aria-label": "IP address of the amplifier to add",
                                value: "{ip}",
                                oninput: move |event| ip.set(event.value()),
                            }
                        }
                    }
                    div { class: "add-action",
                        button {
                            class: "button add",
                            onclick: move |_| {
                                on_add.call((name(), ipd_form(), ip()));
                                name.set(String::new());
                                ip.set(String::new());
                            },
                            "ADD AMP"
                        }
                        if !add_error.is_empty() {
                            crate::ErrorText { text: add_error }
                        }
                    }
                }
            }
            section { class: "panel known", "aria-label": "Known amplifiers",
                span { class: "caption", "KNOWN AMPLIFIERS" }
                if view.amps.is_empty() {
                    div { class: "empty-state known-empty", "No amplifier yet. Turn on a search, or add one manually." }
                }
                for amp in view.amps {
                    div { class: "known-row", key: "{amp.index}",
                        span { class: "known-index", "{amp.index + 1:02}" }
                        div { class: "known-move", role: "group", "aria-label": "Order of amplifier {amp.index + 1}",
                            button {
                                class: "known-move-button up",
                                "aria-label": "Move amplifier {amp.index + 1} up",
                                disabled: amp.index == 0,
                                onclick: move |_| on_move.call((amp.index, amp.index.saturating_sub(1))),
                            }
                            button {
                                class: "known-move-button down",
                                "aria-label": "Move amplifier {amp.index + 1} down",
                                disabled: amp.index + 1 >= count,
                                onclick: move |_| on_move.call((amp.index, amp.index + 1)),
                            }
                        }
                        input {
                            class: "known-name",
                            r#type: "text",
                            autocomplete: "off",
                            "aria-label": "Name of amplifier {amp.index + 1}",
                            value: "{amp.name}",
                            onchange: move |event| on_rename.call((amp.index, event.value())),
                        }
                        span { class: if amp.protocol == "IPD" { "badge ipd" } else { "badge ipx" }, "{amp.protocol}" }
                        span { class: "known-ip", "{amp.ip}" }
                        span { class: "known-source", "{amp.source}" }
                        label { class: "known-ceiling",
                            "MAX GAIN dB"
                            input {
                                r#type: "text",
                                inputmode: "decimal",
                                autocomplete: "off",
                                placeholder: "none",
                                "aria-label": "Highest output gain of amplifier {amp.index + 1}",
                                value: amp.max_gain_db.map(control::number).unwrap_or_default(),
                                onchange: move |event| on_ceilings.call((amp.index, event.value(), amp.max_limiter_dbu.map(control::number).unwrap_or_default())),
                            }
                        }
                        label { class: "known-ceiling",
                            "MAX LIMITER dBu"
                            input {
                                r#type: "text",
                                inputmode: "decimal",
                                autocomplete: "off",
                                placeholder: "none",
                                "aria-label": "Highest limiter threshold of amplifier {amp.index + 1}",
                                value: amp.max_limiter_dbu.map(control::number).unwrap_or_default(),
                                onchange: move |event| on_ceilings.call((amp.index, amp.max_gain_db.map(control::number).unwrap_or_default(), event.value())),
                            }
                        }
                        button {
                            class: "known-remove",
                            "aria-label": "Remove amplifier {amp.index + 1}",
                            onclick: move |_| on_remove.call(amp.index),
                            "REMOVE"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    thread_local! {
        static VIEW: std::cell::RefCell<Option<SearchView>> = const { std::cell::RefCell::new(None) };
        static ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }

    #[component]
    fn Probe() -> Element {
        let view = VIEW.with(|v| v.borrow().clone()).unwrap_or_default();
        let error = ERROR.with(|e| e.borrow().clone());
        rsx! { SearchPage { view, add_error: error } }
    }

    fn page_html(view: SearchView, error: &str) -> String {
        VIEW.with(|v| *v.borrow_mut() = Some(view));
        ERROR.with(|e| *e.borrow_mut() = error.to_string());
        html(Probe)
    }

    fn known(index: usize, protocol: &str, source: &str) -> KnownAmp {
        KnownAmp {
            index,
            name: format!("Amp {}", index + 1),
            protocol: protocol.into(),
            ip: format!("192.168.1.{}", 20 + index),
            source: source.into(),
            max_gain_db: None,
            max_limiter_dbu: None,
        }
    }

    // SEC-3: a ceiling box takes a number, or nothing for none
    #[test]
    fn a_ceiling_is_a_number_or_nothing() {
        assert_eq!(ceiling_from_text(""), Ok(None));
        assert_eq!(ceiling_from_text(" None "), Ok(None));
        assert_eq!(ceiling_from_text("-6"), Ok(Some(-6.0)));
        assert_eq!(ceiling_from_text("3,5"), Ok(Some(3.5)));
        assert!(ceiling_from_text("loud").is_err());
        assert!(ceiling_from_text("inf").is_err());
        assert!(ceiling_from_text("NaN").is_err());
    }

    // SEC-3, UX-15: each known amplifier has its two ceiling boxes, filled with the ceilings it has
    #[test]
    fn each_known_amplifier_shows_its_ceilings() {
        let mut amp = known(0, "IPX", "MANUAL");
        amp.max_gain_db = Some(-6.0);
        let out = page_html(SearchView { amps: vec![amp], ..SearchView::default() }, "");
        assert!(out.contains("Highest output gain of amplifier 1"), "{out}");
        assert!(out.contains("Highest limiter threshold of amplifier 1"), "{out}");
        assert!(out.contains("value=\"-6\"") && out.contains("placeholder=\"none\""), "{out}");
    }

    // UX-14, UX-25: both switches, with a status line that says what they are doing
    #[test]
    fn the_switches_and_their_lines() {
        let mut view = SearchView { ipd_count: 2, ipx_count: 1, ..SearchView::default() };
        let out = page_html(view.clone(), "");
        assert_eq!(out.matches("role=\"switch\"").count(), 2);
        assert!(out.contains("aria-checked=\"true\""));
        assert!(out.contains("IPD amplifiers"));
        assert!(out.contains("Searching · 2 found<"), "the mockup's line: {out}");

        view.ipx = false;
        let out = page_html(view, "");
        assert!(out.contains("Off · not searching · 1 listed"));
        assert!(out.contains("aria-checked=\"false\""));
    }

    // UX-25: SCAN NOW is dimmed while a scan runs, or when both protocols are off (BEH-17)
    #[test]
    fn scan_now_is_dimmed_when_it_can_do_nothing() {
        let out = page_html(SearchView::default(), "");
        assert!(!out.contains("disabled"), "{out}");

        let scanning = SearchView { scanning: true, ..SearchView::default() };
        let out = page_html(scanning, "");
        assert!(out.contains("SCANNING…"));
        assert!(out.contains("disabled"));

        let off = SearchView { ipd: false, ipx: false, ..SearchView::default() };
        let out = page_html(off, "");
        assert!(out.contains("disabled"));
    }

    // UX-15: a row per amp, with the badge of its protocol, the address, the source and REMOVE
    #[test]
    fn known_amplifiers_rows() {
        let view = SearchView {
            amps: vec![known(0, "IPD", "AUTO"), known(1, "IPX", "MANUAL")],
            ..SearchView::default()
        };
        let out = page_html(view, "");
        assert!(out.contains("01") && out.contains("02"));
        assert!(out.contains("badge ipd") && out.contains("badge ipx"));
        assert!(out.contains("192.168.1.20") && out.contains("192.168.1.21"));
        assert!(out.contains("AUTO") && out.contains("MANUAL"));
        assert_eq!(out.matches("REMOVE").count(), 2);
        assert!(out.contains("value=\"Amp 1\""), "the name is editable: {out}");
    }

    // UX-15, BEH-45: a row can go up or down, the first cannot go up and the last cannot go down
    #[test]
    fn known_amplifiers_can_be_moved() {
        let view = SearchView {
            amps: vec![known(0, "IPD", "AUTO"), known(1, "IPX", "MANUAL"), known(2, "IPX", "AUTO")],
            ..SearchView::default()
        };
        let out = page_html(view, "");
        assert_eq!(out.matches("Move amplifier").count(), 6, "{out}");
        assert!(out.contains("Move amplifier 2 up") && out.contains("Move amplifier 2 down"));
        assert_eq!(out.matches("disabled").count(), 2, "the first up and the last down: {out}");
        let first_up = out.find("Move amplifier 1 up").unwrap();
        assert!(out[first_up..first_up + 120].contains("disabled"), "{out}");
        let last_down = out.find("Move amplifier 3 down").unwrap();
        assert!(out[last_down..last_down + 120].contains("disabled"), "{out}");

        let one = SearchView { amps: vec![known(0, "IPD", "AUTO")], ..SearchView::default() };
        assert_eq!(page_html(one, "").matches("disabled").count(), 2, "alone, it cannot move");
    }

    // BEH-20 to BEH-22: the form has its three fields, and the error sits beside them
    #[test]
    fn the_add_form_and_its_error() {
        let out = page_html(SearchView::default(), "Enter a valid IPv4 address");
        assert!(
            out.contains(">NAME<") && out.contains(">PROTOCOL<") && out.contains(">IP ADDRESS<"),
            "{out}"
        );
        assert!(out.contains("placeholder=\"192.168.1.50\""));
        assert!(out.contains("ADD AMP"));
        assert!(out.contains("Enter a valid IPv4 address"));
        assert!(out.contains("error-text"));
    }

    // the page says when there is nothing to list
    #[test]
    fn an_empty_list_says_so() {
        assert!(page_html(SearchView::default(), "").contains("No amplifier yet"));
    }
}
