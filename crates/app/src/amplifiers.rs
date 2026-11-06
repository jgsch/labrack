//! The Amplifiers page (UX-11 to UX-13, BEH-8 to BEH-12, BEH-41): one panel per amplifier with its
//! status, its address, its preset and its two outputs, and an empty state that points to Search.

use dioxus::prelude::*;

use crate::Status;
use crate::control;

/// What one output of an amplifier shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub label: String,
    /// The latest level, in dB.
    pub db: f64,
    /// Gain reduction, in dB.
    pub gr: f64,
}

/// One amplifier, as the page draws it. The hub fills this; the panel itself is pure, so it can be
/// rendered and checked without a connection.
#[derive(Clone, Debug, PartialEq)]
pub struct AmpView {
    pub index: usize,
    pub name: String,
    /// IPD or IPX, as the badge shows it.
    pub protocol: String,
    pub ip: String,
    pub status: Status,
    pub online: bool,
    /// MUTE / UNMUTE of the whole amp (BEH-24).
    pub amp_muted: bool,
    pub levels: [Level; 2],
    pub muted: [bool; 2],
    /// The loaded preset, and whether the app saw a change since (BEH-11, BEH-38).
    pub preset: Option<String>,
    pub modified: bool,
    /// The device mode the amp holds (`stereo`, `two_way`); `None` until it is read (BEH-44).
    pub mode: Option<String>,
    /// An action runs on the amp: its mode cannot be changed meanwhile.
    pub busy: bool,
}

impl AmpView {
    /// An amplifier with nothing known yet: used before its state is read.
    pub fn new(index: usize, name: &str, protocol: &str, ip: &str, status: Status) -> AmpView {
        AmpView {
            index,
            name: name.to_string(),
            protocol: protocol.to_string(),
            ip: ip.to_string(),
            status,
            online: status == Status::Online,
            amp_muted: false,
            levels: [
                Level { label: "OUT 1".into(), db: control::METER_FLOOR, gr: 0.0 },
                Level { label: "OUT 2".into(), db: control::METER_FLOOR, gr: 0.0 },
            ],
            muted: [false, false],
            preset: None,
            modified: false,
            mode: None,
            busy: false,
        }
    }
}

/// The colour of a status line and its dot (BEH-8), from the palette of UX-8.
pub fn status_colour(status: Status) -> &'static str {
    match status {
        Status::Online => "#a6e3a1",
        Status::Connecting | Status::Reconnecting => "#fab387",
        Status::Busy => "#f9e2af",
        Status::Unreachable | Status::CheckIp => "#f38ba8",
        Status::Disconnected => "#a6adc8",
    }
}

/// Whether the address is shown in red: it is invalid or the amp cannot be reached (UX-11, BEH-8).
pub fn ip_wrong(status: Status) -> bool {
    matches!(status, Status::CheckIp | Status::Unreachable)
}

/// One amplifier's panel (UX-11 to UX-13).
#[component]
pub fn AmplifierPanel(
    view: AmpView,
    #[props(default)] on_connect: EventHandler<bool>,
    #[props(default)] on_ip: EventHandler<String>,
    #[props(default)] on_mute: EventHandler<(u8, bool)>,
    #[props(default)] on_amp_mute: EventHandler<bool>,
    #[props(default)] on_preset: EventHandler<()>,
    #[props(default)] on_mode: EventHandler<String>,
) -> Element {
    let colour = status_colour(view.status);
    let preset = view.preset.clone().unwrap_or_else(|| "—".into());
    let ip_style = if ip_wrong(view.status) { "color: #f38ba8" } else { "" };
    let muted = view.muted;
    let level_muted = view.muted;
    let online = view.online;
    let mode = view.mode.clone();
    rsx! {
        section { class: "amp-panel",
            div { class: "amp-head",
                div { class: "amp-ident",
                    span { class: "amp-name", "{view.name}" }
                    div { class: "amp-status-row",
                        span { class: "status", style: "color: {colour}",
                            span { class: "dot", style: "background: {colour}" }
                            "{view.status.label()}"
                        }
                        span { class: "amp-badge", "{view.protocol}" }
                        input {
                            class: "ip-input",
                            r#type: "text",
                            inputmode: "decimal",
                            autocomplete: "off",
                            "aria-label": "IP address of {view.name}",
                            value: "{view.ip}",
                            style: "{ip_style}",
                            onchange: move |event| on_ip.call(event.value()),
                        }
                    }
                }
                div { class: "amp-actions",
                    button {
                        class: "small-button",
                        "aria-label": if view.status == Status::Disconnected { "Connect" } else { "Disconnect" },
                        onclick: move |_| on_connect.call(view.status == Status::Disconnected),
                        if view.status == Status::Disconnected { "CONNECT" } else { "DISCONNECT" }
                    }
                    button {
                        class: if view.amp_muted { "amp-mute muted" } else { "amp-mute" },
                        disabled: !online,
                        "aria-pressed": "{view.amp_muted}",
                        onclick: move |_| on_amp_mute.call(!view.amp_muted),
                        if view.amp_muted { "UNMUTE" } else { "MUTE" }
                    }
                }
            }
            div { class: "amp-body",
                div { class: "amp-info",
                    div { class: "amp-box",
                        span { class: "label", "MODE" }
                        div { class: "mode-choice", role: "group", "aria-label": "Mode of {view.name}",
                            for choice in labgruppen::DEVICE_MODES.iter().copied() {
                                button {
                                    key: "{choice}",
                                    class: if mode.as_deref() == Some(choice) { "mode-button on" } else { "mode-button" },
                                    "aria-pressed": "{mode.as_deref() == Some(choice)}",
                                    disabled: !online || mode.is_none() || view.busy,
                                    onclick: move |_| on_mode.call(choice.to_string()),
                                    "{crate::presets::mode_label(choice)}"
                                }
                            }
                        }
                    }
                    button { class: "amp-box preset-link", onclick: move |_| on_preset.call(()),
                        span { class: "label", "PRESET" }
                        span { class: "preset-name", "{preset}" }
                        if view.modified {
                            span { class: "modified", role: "img", title: "Changed since it was loaded", "aria-label": "Modified" }
                        }
                    }
                }
                div { class: "amp-meter",
                    for (output, level) in view.levels.iter().enumerate() {
                        div { class: "channel",
                            div { class: "channel-text",
                                span { class: "channel-name", "{level.label}" }
                                span { class: "channel-figures",
                                    if !online {
                                        "no signal"
                                    } else if level_muted[output] {
                                        "MUTED · GR {control::number(level.gr)}"
                                    } else {
                                        "{control::number(level.db)} dB · GR {control::number(level.gr)}"
                                    }
                                }
                            }
                            crate::Meter { level_db: if online && !level_muted[output] { level.db } else { control::METER_FLOOR }, gain_reduction_db: level.gr }
                            button {
                                class: if muted[output] { "m-button muted" } else { "m-button" },
                                disabled: !online,
                                "aria-pressed": "{muted[output]}",
                                "aria-label": if muted[output] {
                                    "Unmute output {output + 1} of {view.name}"
                                } else {
                                    "Mute output {output + 1} of {view.name}"
                                },
                                onclick: move |_| on_mute.call((output as u8, !muted[output])),
                                "M"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The page: the amplifier list in order, or what to do when it is empty (BEH-41).
#[component]
pub fn AmplifiersPage(
    amps: Vec<AmpView>,
    #[props(default)] on_search: EventHandler<()>,
    #[props(default)] on_connect: EventHandler<(usize, bool)>,
    #[props(default)] on_ip: EventHandler<(usize, String)>,
    #[props(default)] on_mute: EventHandler<(usize, u8, bool)>,
    #[props(default)] on_amp_mute: EventHandler<(usize, bool)>,
    #[props(default)] on_preset: EventHandler<usize>,
    #[props(default)] on_mode: EventHandler<(usize, String)>,
) -> Element {
    if amps.is_empty() {
        return rsx! {
            div { class: "empty-state",
                "No amplifier yet. Open the Search tab to find amplifiers on the network, or add one manually."
                button { class: "button", onclick: move |_| on_search.call(()), "SEARCH" }
            }
        };
    }
    rsx! {
        div { class: "amp-list",
            for view in amps {
                AmplifierPanel {
                    key: "{view.index}",
                    on_connect: move |connected| on_connect.call((view.index, connected)),
                    on_ip: move |ip| on_ip.call((view.index, ip)),
                    on_mute: move |(output, muted)| on_mute.call((view.index, output, muted)),
                    on_amp_mute: move |muted| on_amp_mute.call((view.index, muted)),
                    on_preset: move |_| on_preset.call(view.index),
                    on_mode: move |mode| on_mode.call((view.index, mode)),
                    view: view.clone(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static VIEW: std::cell::RefCell<Option<AmpView>> = const { std::cell::RefCell::new(None) };
    }

    #[component]
    fn Probe() -> Element {
        let view = VIEW.with(|v| v.borrow().clone()).expect("a view");
        rsx! { AmplifierPanel { view } }
    }

    #[component]
    fn Empty() -> Element {
        rsx! { AmplifiersPage { amps: vec![] } }
    }

    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    fn panel_html(view: AmpView) -> String {
        VIEW.with(|v| *v.borrow_mut() = Some(view));
        html(Probe)
    }

    fn panel(status: Status) -> String {
        panel_html(AmpView::new(0, "FOH Main L", "IPD", "192.168.1.21", status))
    }

    // UX-11, BEH-8: the panel shows the name, the protocol, the status and the address, and no index
    #[test]
    fn a_panel_shows_the_amplifier() {
        let out = panel(Status::Online);
        assert!(!out.contains(">01<"), "no index: {out}");
        assert!(out.contains("class=\"amp-badge\">IPD<"), "the protocol: {out}");
        assert!(out.contains("FOH Main L"));
        assert!(out.contains("ONLINE"));
        assert!(out.contains("192.168.1.21"));
        assert!(out.contains("DISCONNECT"));
    }

    // BEH-8: each status has its text, and its colour
    #[test]
    fn every_status_is_shown() {
        for (status, colour) in [
            (Status::Online, "#a6e3a1"),
            (Status::Connecting, "#fab387"),
            (Status::Reconnecting, "#fab387"),
            (Status::Busy, "#f9e2af"),
            (Status::Unreachable, "#f38ba8"),
            (Status::CheckIp, "#f38ba8"),
            (Status::Disconnected, "#a6adc8"),
        ] {
            let out = panel(status);
            assert!(out.contains(status.label()), "{status:?}");
            assert!(out.contains(colour), "{status:?} is not {colour}");
        }
        assert_eq!(status_colour(Status::Online), "#a6e3a1");
        assert_eq!(status_colour(Status::Disconnected), "#a6adc8");
    }

    // UX-11, BEH-8: an address that is invalid or unreachable is red; a good one is not
    #[test]
    fn a_bad_address_is_red() {
        assert!(ip_wrong(Status::Unreachable) && ip_wrong(Status::CheckIp));
        assert!(!ip_wrong(Status::Online));
        assert!(panel(Status::Unreachable).contains("color: #f38ba8"));
        assert!(panel(Status::CheckIp).contains("color: #f38ba8"));
        assert!(!panel(Status::Online).contains("color: #f38ba8"));
    }

    // BEH-10, UX-13: a muted output says so, its button is pressed and its meter is empty
    #[test]
    fn a_muted_output_says_so() {
        let mut view = AmpView::new(0, "Subs", "IPX", "192.168.1.31", Status::Online);
        view.muted[0] = true;
        view.levels[0].db = -12.0;
        let out = panel_html(view);
        assert!(out.contains("MUTED"), "{out}");
        assert!(out.contains("aria-pressed=\"true\""), "{out}");
        assert!(out.contains("m-button muted"), "{out}");
        assert!(out.contains("Mute output 2 of Subs"), "the other output offers to mute: {out}");
    }

    // BEH-10: an amp that is not ONLINE has no working controls
    #[test]
    fn an_offline_amp_has_no_controls() {
        let out = panel(Status::Unreachable);
        assert_eq!(out.matches("disabled").count(), 5, "two outputs, the amp and the two modes: {out}");
        assert!(out.contains("no signal"));
    }

    // BEH-41: the empty state says what to do
    #[test]
    fn the_empty_state_points_to_search() {
        let out = html(Empty);
        assert!(out.contains("No amplifier yet"), "{out}");
        assert!(out.contains("Search"), "{out}");
    }

    // BEH-11: the preset, with a dot when the app saw a change
    #[test]
    fn the_preset_shows_modified() {
        let with = |modified| {
            let mut view = AmpView::new(0, "Monitors", "IPX", "192.168.1.34", Status::Online);
            view.preset = Some("05 · Subs".into());
            view.modified = modified;
            view
        };
        let out = panel_html(with(true));
        assert!(out.contains("05 · Subs"));
        assert!(out.contains("class=\"modified\""), "a dot: {out}");
        assert!(!panel_html(with(false)).contains("modified"));
    }

    // UX-11, BEH-44: every amp shows its device mode, the one the amp holds is pressed, none until it is read
    #[test]
    fn the_device_mode_is_shown_and_pressed() {
        let mut view = AmpView::new(0, "FOH", "IPX", "192.168.1.21", Status::Online);
        let out = panel_html(view.clone());
        assert!(out.contains(">MODE<") && out.contains(">Stereo<") && out.contains(">2 ways<"), "{out}");
        assert!(
            !out.contains("mode-button on") && out.matches("disabled").count() == 2,
            "only the two modes are disabled while it is not read: {out}"
        );
        view.mode = Some("two_way".into());
        let out = panel_html(view.clone());
        assert!(out.contains("mode-button on") && out.matches("mode-button on").count() == 1, "{out}");
        assert_eq!(out.matches("aria-pressed=\"true\"").count(), 1, "{out}");
        let on = out.find("mode-button on").unwrap();
        assert!(
            out[on..].contains(">2 ways<") && !out[on..].contains(">Stereo<"),
            "2 ways is the pressed one: {out}"
        );
        assert!(!out.contains("disabled"), "{out}");
        view.busy = true;
        assert!(panel_html(view).contains("disabled"), "no change while an action runs");
    }

    // UX-11: an amp without a preset shows a dash
    #[test]
    fn no_preset_is_a_dash() {
        assert!(panel(Status::Online).contains("—"));
    }
}
