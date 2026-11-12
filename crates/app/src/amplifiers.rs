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

/// The user level of an IPD and what it allows (BEH-13, BEH-14).
#[derive(Clone, Debug, PartialEq)]
pub struct AccessView {
    /// The level by name: `User`, `Locked`, ...
    pub user: String,
    /// The amp is locked (BEH-13).
    pub locked: bool,
    /// An IPD that is locked or has a right missing: a login is offered (BEH-15).
    pub needs_login: bool,
    /// An IPX: LOCK or UNLOCK is offered instead (BEH-47).
    pub can_lock: bool,
    /// The right of the output channels: without it nothing is unmuted (BEH-14).
    pub can_unmute: bool,
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
    /// The amp has told the mute of each output. An IPD takes seconds after ONLINE to do it; until then
    /// the output is not shown as unmuted (SEC-7).
    pub mute_known: [bool; 2],
    /// The mute shown is asked for and the amp has not confirmed it: it is not in force yet.
    pub mute_pending: [bool; 2],
    /// The loaded preset, and whether the app saw a change since (BEH-11, BEH-38).
    pub preset: Option<String>,
    pub modified: bool,
    /// The device mode the amp holds (`stereo`, `two_way`); `None` until it is read (BEH-44).
    pub mode: Option<String>,
    /// An action runs on the amp: its mode cannot be changed meanwhile.
    pub busy: bool,
    /// The temperature the amp reports, in percent (BEH-46); `None` until it does and when not ONLINE.
    pub temperature: Option<i32>,
    /// The user level, for an ONLINE IPD (BEH-13).
    pub access: Option<AccessView>,
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
            mute_known: [false, false],
            mute_pending: [false, false],
            preset: None,
            modified: false,
            mode: None,
            busy: false,
            temperature: None,
            access: None,
        }
    }
}

/// The class of the temperature text (BEH-46): orange from 80% to 90%, red above 90%.
pub fn temperature_class(percent: i32) -> &'static str {
    match percent {
        ..80 => "amp-temp",
        80..=90 => "amp-temp warm",
        _ => "amp-temp hot",
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
    #[props(default)] on_login: EventHandler<()>,
    #[props(default)] on_lock: EventHandler<bool>,
) -> Element {
    let colour = status_colour(view.status);
    let preset = view.preset.clone().unwrap_or_else(|| "—".into());
    let ip_colour = if ip_wrong(view.status) { "color: #f38ba8; " } else { "" };
    // as wide as the address, so the gaps to the protocol and to the status are the same
    let ip_style = format!("{ip_colour}width: {}ch", view.ip.chars().count().max(7));
    let muted = view.muted;
    let level_muted = view.muted;
    let mute_known = view.mute_known;
    let mute_pending = view.mute_pending;
    let online = view.online;
    let mode = view.mode.clone();
    let access = view.access.clone().filter(|_| online);
    let needs_login = access.as_ref().is_some_and(|a| a.needs_login);
    let can_lock = access.as_ref().is_some_and(|a| a.can_lock);
    let locked = access.as_ref().is_some_and(|a| a.locked);
    // without the right of the outputs nothing is unmuted; turning a mute on is never held back (SEC-6)
    let may_unmute = access.as_ref().is_none_or(|a| a.can_unmute);
    rsx! {
        section { class: "amp-panel",
            div { class: "amp-head",
                div { class: "amp-ident",
                    span { class: "amp-name", "{view.name}" }
                    div { class: "amp-status-row",
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
                        span { class: "amp-badge", "{view.protocol}" }
                        span { class: "status", style: "color: {colour}",
                            span { class: "dot", style: "background: {colour}" }
                            "{view.status.label()}"
                        }
                        if let Some(percent) = view.temperature.filter(|_| online) {
                            span { class: temperature_class(percent), "aria-label": "Temperature of {view.name}", "TEMP {percent}%" }
                        }
                        if let Some(access) = access.clone() {
                            span { class: if access.needs_login || access.locked { "amp-temp warm" } else { "amp-temp" }, "LEVEL {access.user.to_uppercase()}" }
                        }
                    }
                }
                div { class: "amp-actions",
                    if can_lock {
                        button {
                            class: "small-button",
                            "aria-label": if locked { "Unlock {view.name}" } else { "Lock {view.name}" },
                            disabled: view.busy,
                            onclick: move |_| on_lock.call(!locked),
                            if locked { "UNLOCK" } else { "LOCK" }
                        }
                    } else if needs_login {
                        button {
                            class: "small-button",
                            "aria-label": "Log in to {view.name}",
                            onclick: move |_| on_login.call(()),
                            "LOGIN"
                        }
                    }
                    button {
                        class: "small-button",
                        "aria-label": if view.status == Status::Disconnected { "Connect" } else { "Disconnect" },
                        onclick: move |_| on_connect.call(view.status == Status::Disconnected),
                        if view.status == Status::Disconnected { "CONNECT" } else { "DISCONNECT" }
                    }
                    button {
                        class: if view.amp_muted { "amp-mute muted" } else { "amp-mute" },
                        disabled: !online || (view.amp_muted && !may_unmute),
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
                                    } else if !mute_known[output] {
                                        "mute unknown"
                                    } else if mute_pending[output] {
                                        if level_muted[output] { "MUTING…" } else { "UNMUTING…" }
                                    } else if level_muted[output] {
                                        "MUTED · GR {control::number(level.gr)}"
                                    } else {
                                        "{control::number(level.db)} dB · GR {control::number(level.gr)}"
                                    }
                                }
                            }
                            crate::Meter { level_db: if online && !level_muted[output] { level.db } else { control::METER_FLOOR }, gain_reduction_db: level.gr }
                            button {
                                class: if mute_pending[output] {
                                    "m-button pending"
                                } else if muted[output] {
                                    "m-button muted"
                                } else if !mute_known[output] {
                                    "m-button unknown"
                                } else {
                                    "m-button"
                                },
                                disabled: !online || (muted[output] && !may_unmute),
                                "aria-pressed": "{muted[output]}",
                                "aria-label": if muted[output] {
                                    "Unmute output {output + 1} of {view.name}"
                                } else if !mute_known[output] {
                                    "Mute output {output + 1} of {view.name} (its state is not known yet)"
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
    #[props(default)] on_login: EventHandler<usize>,
    #[props(default)] on_lock: EventHandler<(usize, bool)>,
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
                    on_login: move |_| on_login.call(view.index),
                    on_lock: move |locked| on_lock.call((view.index, locked)),
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

    // BEH-46, UX-11: an ONLINE amp that reports a temperature shows it in percent after its status
    #[test]
    fn a_panel_shows_the_temperature() {
        let with = |status, temperature| {
            let mut view = AmpView::new(0, "FOH Main L", "IPD", "192.168.1.21", status);
            view.temperature = temperature;
            panel_html(view)
        };
        assert!(with(Status::Online, Some(61)).contains(">TEMP 61%<"));
        assert!(!with(Status::Online, None).contains("TEMP"));
        // orange from 80% to 90%, red above
        assert!(with(Status::Online, Some(79)).contains("class=\"amp-temp\""));
        assert!(with(Status::Online, Some(80)).contains("class=\"amp-temp warm\""));
        assert!(with(Status::Online, Some(90)).contains("class=\"amp-temp warm\""));
        assert!(with(Status::Online, Some(91)).contains("class=\"amp-temp hot\""));
        assert!(!with(Status::Reconnecting, Some(61)).contains("TEMP"));
    }

    // BEH-13, UX-30: an ONLINE IPD shows its level; a login is offered when one is needed, and unmuting needs the
    // right of the outputs
    #[test]
    fn a_panel_shows_the_user_level_and_the_login() {
        let with = |status, access| {
            let mut view = AmpView::new(0, "FOH Main L", "IPD", "192.168.1.21", status);
            view.access = access;
            view.muted = [true, false];
            view.mute_known = [true, true];
            panel_html(view)
        };
        let user = |needs_login, can_unmute| {
            Some(AccessView { user: "User".into(), locked: false, needs_login, can_lock: false, can_unmute })
        };
        let out = with(Status::Online, user(false, true));
        assert!(out.contains(">LEVEL USER<") && !out.contains("LOGIN"), "{out}");
        assert!(!out.contains("amp-temp warm"), "{out}");
        let out = with(Status::Online, user(true, false));
        assert!(out.contains("amp-temp warm\">LEVEL USER<") && out.contains(">LOGIN<"), "{out}");
        // the muted output cannot be unmuted, the other can still be muted
        let tag = |label: &str| {
            let end = out.find(label).unwrap_or_else(|| panic!("{label}: {out}"));
            let start = out[..end].rfind("<button").unwrap();
            out[start..end + out[end..].find('>').unwrap()].to_string()
        };
        assert!(tag("Unmute output 1 of FOH Main L").contains("disabled"));
        assert!(!tag("Mute output 2 of FOH Main L").contains("disabled"));
        assert!(!tag("Log in to FOH Main L").contains("disabled"));
        assert!(!with(Status::Online, None).contains("LEVEL"));
        assert!(!with(Status::Reconnecting, user(true, false)).contains("LEVEL"));
    }

    // BEH-47, UX-30: an IPX has LOCK or UNLOCK where an IPD has LOGIN, and reads Locked or Unlocked
    #[test]
    fn an_ipx_panel_has_lock_and_unlock() {
        let with = |locked| {
            let mut view = AmpView::new(0, "Subs", "IPX", "192.168.1.31", Status::Online);
            view.access = Some(AccessView {
                user: if locked { "Locked" } else { "Unlocked" }.into(),
                locked,
                needs_login: false,
                can_lock: true,
                can_unmute: !locked,
            });
            panel_html(view)
        };
        let out = with(false);
        assert!(out.contains(">LOCK<") && !out.contains("UNLOCK<") && !out.contains("LOGIN"), "{out}");
        assert!(out.contains(">LEVEL UNLOCKED<") && !out.contains("amp-temp warm"), "{out}");
        let out = with(true);
        assert!(out.contains(">UNLOCK<") && !out.contains(">LOCK<") && !out.contains("LOGIN"), "{out}");
        assert!(out.contains("amp-temp warm\">LEVEL LOCKED<"), "{out}");
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
        view.mute_known = [true, true];
        view.levels[0].db = -12.0;
        let out = panel_html(view);
        assert!(out.contains("MUTED"), "{out}");
        assert!(out.contains("aria-pressed=\"true\""), "{out}");
        assert!(out.contains("m-button muted"), "{out}");
        assert!(out.contains("Mute output 2 of Subs"), "the other output offers to mute: {out}");
    }

    // SEC-6: a MUTE the amp has not confirmed is not shown as in force
    #[test]
    fn a_mute_the_amp_has_not_confirmed_says_so() {
        let mut view = AmpView::new(0, "Subs", "IPX", "192.168.1.31", Status::Online);
        view.muted = [true, false];
        view.mute_known = [true, true];
        view.mute_pending = [true, false];
        let out = panel_html(view);
        assert_eq!(out.matches("MUTING…").count(), 1, "{out}");
        assert!(out.contains("m-button pending"), "{out}");
        assert!(!out.contains("m-button muted"), "{out}");
        assert!(!out.contains("MUTED"), "{out}");
    }

    // SEC-7: until the amp has told a mute, the output is not shown as unmuted
    #[test]
    fn an_unknown_mute_is_not_shown_as_unmuted() {
        let mut view = AmpView::new(0, "Subs", "IPD", "192.168.1.31", Status::Online);
        view.mute_known = [true, false];
        let out = panel_html(view);
        assert_eq!(out.matches("mute unknown").count(), 1, "{out}");
        assert!(out.contains("m-button unknown"), "{out}");
        assert!(out.contains("its state is not known yet"), "{out}");
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

    // UX-11: WebKitGTK sizes the box of the DISCONNECT and MUTE buttons from the text of MUTE and ignores
    // its flex-basis, so the button overflowed the panel by the difference (measured in the real window):
    // a button needs a width of its own
    #[test]
    fn the_mute_button_has_a_width_of_its_own() {
        let css = crate::style::stylesheet();
        let rule = css.lines().find(|l| l.starts_with(".amp-mute{")).expect("the rule");
        assert!(rule.contains(";width:118px;"), "{rule}");
    }

    // UX-11: the address box is as wide as the address, so the gaps in the status line are equal
    #[test]
    fn the_address_box_is_as_wide_as_the_address() {
        assert!(panel(Status::Online).contains("width: 12ch"), "192.168.1.21 has 12 characters");
        let out = panel_html(AmpView::new(0, "FOH", "IPD", "192.168.1.183", Status::CheckIp));
        assert!(out.contains("color: #f38ba8; width: 13ch"), "{out}");
    }

    // UX-11: an amp without a preset shows a dash
    #[test]
    fn no_preset_is_a_dash() {
        assert!(panel(Status::Online).contains("—"));
    }
}
