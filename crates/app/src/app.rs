use std::sync::Arc;

use dioxus::prelude::*;
use labgruppen::{Channel, Param, Value};
use labrack_core::{Hub, Lan, Network, Scan, Side, Store, Timing, Unit, Update};
use tokio::sync::broadcast::error::RecvError;

use crate::amplifiers::{AmpView, Level};
use crate::control::Bounds;
use crate::mixeq::{AmpCard, LIMITER_BOXES, LevelRow, LimiterStrip, MixTab, MixeqView, Strip};
use crate::search::{KnownAmp, SearchView};
use crate::{AmplifiersPage, Header, MixeqPage, Page, SearchPage, Style};

/// The amp list from the config file, or an empty one that is never written when the file cannot be
/// read: a file the app does not understand must not be replaced by an empty list.
fn store() -> Store {
    match Store::default_path().map(Store::open) {
        Some(Ok(store)) => store,
        Some(Err(e)) => {
            eprintln!("labrack: the config file cannot be used, working without it: {e}");
            Store::in_memory()
        }
        None => Store::in_memory(),
    }
}

/// The window: the connections of the core, the header and the page chosen in it.
#[component]
pub fn App() -> Element {
    let hub = use_hook(|| Hub::new(store(), Arc::new(Network), Arc::new(Lan), Timing::default()));
    let mut page = use_signal(|| Page::Amplifiers);
    let mut picked = use_signal(|| 0usize);
    let mut tab = use_signal(|| MixTab::Levels);

    // WebKitGTK can be told a scale that is not the screen's, and everything shrinks to a fraction of
    // its size (seen on niri at a fractional scale: a device pixel ratio of -2/96). The page reports
    // its ratio; when it is not the window's scale, the zoom is set to what makes it right.
    let desktop = use_context::<dioxus::desktop::DesktopContext>();
    use_hook(move || {
        let desktop = desktop.clone();
        spawn(async move {
            let scale = desktop.window.scale_factor();
            for _ in 0..20 {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let mut eval = document::eval("dioxus.send(devicePixelRatio)");
                let Ok(dpr) = eval.recv::<f64>().await else { continue };
                if let Some(zoom) = corrected_zoom(dpr, scale) {
                    eprintln!(
                        "labrack: the page reports a scale of {dpr}, the window {scale}; zoom set to {zoom}"
                    );
                    desktop.set_zoom_level(zoom);
                }
                return;
            }
        });
    });

    let add_error = use_signal(String::new);
    let mut scan_message = use_signal(String::new);
    let mut scanning = use_signal(|| false);

    // everything the core tells but the meters, which pages that show them follow themselves
    let mut changes = use_signal(|| 0u64);
    use_future({
        let hub = hub.clone();
        move || {
            let mut updates = hub.subscribe();
            async move {
                loop {
                    match updates.recv().await {
                        Ok(Update::Meters(..)) => {}
                        Ok(Update::Scan(Scan::Started)) => {
                            scanning.set(true);
                            scan_message.set(String::new());
                            changes += 1;
                        }
                        Ok(Update::Scan(Scan::Finished { added, errors })) => {
                            scanning.set(false);
                            scan_message.set(if !errors.is_empty() {
                                errors.join("; ")
                            } else if added.is_empty() {
                                "No new amplifier found".into()
                            } else {
                                format!("Found {} new", added.len())
                            });
                            changes += 1;
                        }
                        Ok(_) | Err(RecvError::Lagged(_)) => changes += 1,
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    });
    changes();

    let amps = views(&hub);
    let ids = std::sync::Arc::new(hub.amps().iter().map(|amp| amp.id).collect::<Vec<_>>());

    rsx! {
        Style {}
        div { class: "frame",
            Header {
                page: page(),
                all_muted: hub.all_muted(),
                on_page: move |next| page.set(next),
                on_mute_all: {
                    let hub = hub.clone();
                    move |_| {
                        if hub.all_muted() {
                            hub.unmute_all();
                        } else {
                            hub.mute_all();
                        }
                    }
                },
            }
            main {
                match page() {
                    Page::Amplifiers => rsx! {
                        AmplifiersPage {
                            amps,
                            on_search: move |_| page.set(Page::Search),
                            on_connect: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, connected)| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = if connected { hub.connect(id) } else { hub.disconnect(id) };
                                    }
                                }
                            },
                            on_ip: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, ip): (usize, String)| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = hub.set_ip(id, &ip);
                                    }
                                }
                            },
                            on_mute: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, output, muted)| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = hub.set_mute(id, output, muted);
                                    }
                                }
                            },
                            on_amp_mute: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, muted): (usize, bool)| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = hub.mute_amp(id, muted);
                                    }
                                }
                            },
                            on_preset: move |_| page.set(Page::MixEq),
                        }
                    },
                    Page::MixEq => rsx! {
                        MixeqPage {
                            view: mixeq_view(&hub, picked(), tab()),
                            on_pick: move |index| picked.set(index),
                            on_tab: move |next| tab.set(next),
                            on_link: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(side, linked): (Side, bool)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let hub = hub.clone();
                                        spawn(async move {
                                            let _ = hub.set_link(id, side, linked).await;
                                        });
                                    }
                                }
                            },
                            on_limiter: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(output, unit, value): (usize, Unit, f64)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let _ = hub.set_limiter(id, output as u8, unit, value);
                                    }
                                }
                            },
                            on_gain: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(channel, kind, db): (usize, u8, f64)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let ch = if kind == 0 { Channel::Input(channel as u8) } else { Channel::Output(channel as u8) };
                                        let _ = hub.set(id, Param::Gain(ch), Value::Float(db));
                                    }
                                }
                            },
                            on_mute: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(channel, kind, muted): (usize, u8, bool)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let ch = if kind == 0 { Channel::Input(channel as u8) } else { Channel::Output(channel as u8) };
                                        let _ = hub.set(id, Param::Mute(ch), Value::Bool(muted));
                                    }
                                }
                            },
                        }
                    },
                    Page::Search => rsx! {
                        SearchPage {
                            view: search_view(&hub, scanning(), scan_message()),
                            add_error: add_error(),
                            on_ipd: {
                                let hub = hub.clone();
                                move |on| {
                                    let mut switches = hub.switches();
                                    switches.ipd = on;
                                    let _ = hub.set_switches(switches);
                                }
                            },
                            on_ipx: {
                                let hub = hub.clone();
                                move |on| {
                                    let mut switches = hub.switches();
                                    switches.ipx = on;
                                    let _ = hub.set_switches(switches);
                                }
                            },
                            on_scan: {
                                let hub = hub.clone();
                                move |_| {
                                    hub.scan_now();
                                }
                            },
                            on_add: {
                                let hub = hub.clone();
                                let mut add_error = add_error;
                                move |(name, ipd, ip): (String, bool, String)| {
                                    let protocol = if ipd { labrack_core::Protocol::Ipd } else { labrack_core::Protocol::Ipx };
                                    match hub.add_manual(&name, protocol, &ip) {
                                        Ok(_) => add_error.set(String::new()),
                                        Err(e) => add_error.set(e.to_string()),
                                    }
                                }
                            },
                            on_rename: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, name): (usize, String)| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = hub.rename(id, &name);
                                    }
                                }
                            },
                            on_remove: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |index| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let _ = hub.remove(id);
                                    }
                                }
                            },
                        }
                    },
                }
            }
        }
    }
}

/// What the Amplifiers page draws, from the state of the hub.
pub fn views(hub: &Hub) -> Vec<AmpView> {
    let amps = hub.amps();
    amps.iter()
        .enumerate()
        .map(|(index, amp)| {
            let status = hub.status(amp.id).unwrap_or(labrack_core::Status::Disconnected);
            let mut view = AmpView::new(
                index,
                &amp.name,
                if amp.protocol == labrack_core::Protocol::Ipd { "IPD" } else { "IPX" },
                &amp.ip,
                status,
            );
            view.online = status == labrack_core::Status::Online;
            if let Some(meters) = hub.meters(amp.id) {
                let gr = meters.extra.get("gain_reduction_out").cloned().unwrap_or_default();
                for output in 0..2 {
                    let db = meters.output.get(output).copied().unwrap_or(-60.0);
                    view.levels[output] = Level {
                        label: format!("OUT {}", output + 1),
                        db,
                        gr: gr.get(output).copied().unwrap_or(0.0),
                    };
                }
            }
            for output in 0..2 {
                let param = Param::Mute(Channel::Output(output as u8));
                view.muted[output] = hub.value(amp.id, &param) == Some(Value::Bool(true));
            }
            view.amp_muted = hub.amp_muted(amp.id);
            view.preset = amp.preset.as_ref().map(|p| format!("{:02} · {}", p.number, p.name));
            view.modified = hub.modified(amp.id);
            view
        })
        .collect()
}

/// What the Search page draws: the switches, what the last scan did, and the known amplifiers.
pub fn search_view(hub: &Hub, scanning: bool, message: String) -> SearchView {
    let amps = hub.amps();
    let switches = hub.switches();
    let count = |protocol| amps.iter().filter(|amp| amp.protocol == protocol).count();
    SearchView {
        ipd: switches.ipd,
        ipx: switches.ipx,
        scanning,
        ipd_count: count(labrack_core::Protocol::Ipd),
        ipx_count: count(labrack_core::Protocol::Ipx),
        message,
        amps: amps
            .iter()
            .enumerate()
            .map(|(index, amp)| KnownAmp {
                index,
                name: amp.name.clone(),
                protocol: if amp.protocol == labrack_core::Protocol::Ipd { "IPD" } else { "IPX" }.into(),
                ip: amp.ip.clone(),
                source: match amp.source {
                    labrack_core::Source::Auto => "AUTO",
                    labrack_core::Source::Manual => "MANUAL",
                }
                .into(),
            })
            .collect(),
    }
}

/// What the Mix & EQ page draws for one amplifier.
pub fn mixeq_view(hub: &Hub, selected: usize, tab: MixTab) -> MixeqView {
    let amps = hub.amps();
    let Some(amp) = amps.get(selected) else {
        return MixeqView { tab, ..MixeqView::default() };
    };
    let id = amp.id;
    let gain = |ch: Channel| hub.value(id, &Param::Gain(ch)).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let mute = |ch: Channel| hub.value(id, &Param::Mute(ch)) == Some(Value::Bool(true));
    let meters = hub.meters(id).unwrap_or_default();
    let at = |list: &[f64], i: usize, none: f64| list.get(i).copied().unwrap_or(none);
    let gr = meters.extra.get("gain_reduction_out").cloned().unwrap_or_default();
    let strip =
        |label: String, ch: Channel, level_db: f64| Strip { label, db: gain(ch), level_db, muted: mute(ch) };
    let rows = (0..2u8)
        .map(|i| {
            let n = usize::from(i);
            let bounds = LIMITER_BOXES.map(|(unit, ..)| {
                let range = hub.limiter_range(id, i, unit);
                Bounds::new(
                    range.and_then(|r| r.min).unwrap_or(0.0),
                    range.and_then(|r| r.max).unwrap_or(100_000.0),
                )
            });
            LevelRow {
                input: strip(format!("IN {}", n + 1), Channel::Input(i), at(&meters.input, n, -120.0)),
                output: strip(format!("OUT {}", n + 1), Channel::Output(i), at(&meters.output, n, -120.0)),
                limiter: LimiterStrip {
                    label: format!("LIMITER {}", n + 1),
                    gr_db: at(&gr, n, 0.0),
                    figures: hub.limiter(id, i),
                    bounds,
                },
            }
        })
        .collect();
    let cards = amps
        .iter()
        .map(|a| {
            let meters = hub.meters(a.id).unwrap_or_default();
            let gr = meters.extra.get("gain_reduction_out").cloned().unwrap_or_default();
            AmpCard {
                name: a.name.clone(),
                level_db: meters.output.iter().copied().fold(-120.0, f64::max),
                gr_db: gr.iter().copied().fold(0.0, f64::max),
            }
        })
        .collect();
    MixeqView {
        amps: cards,
        selected,
        tab,
        linked_inputs: hub.linked(id, Side::Inputs),
        linked_outputs: hub.linked(id, Side::Outputs),
        rows,
    }
}

/// The zoom that makes the page report `scale`, or `None` when it already does (within 1 %) or when
/// what it reports cannot be used. A page's device pixel ratio is proportional to its zoom, so the
/// zoom to apply is `scale / dpr`.
pub fn corrected_zoom(dpr: f64, scale: f64) -> Option<f64> {
    // the broken ratio is negative, so only zero and non-numbers are unusable
    if !dpr.is_finite() || !scale.is_finite() || dpr == 0.0 || scale <= 0.0 {
        return None;
    }
    let zoom = scale / dpr;
    ((zoom - 1.0).abs() > 0.01).then_some(zoom)
}

#[cfg(test)]
mod tests {
    use super::corrected_zoom;

    // PERF-1: a page whose scale is right is left alone, whatever the screen
    #[test]
    fn a_right_scale_is_not_touched() {
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
            assert_eq!(corrected_zoom(scale, scale), None, "{scale}");
            assert_eq!(corrected_zoom(scale * 1.005, scale), None, "{scale}");
        }
    }

    // PERF-1: WebKitGTK on niri at a fractional scale reported -2/96 for a scale of 2; the zoom that
    // makes it right is -96, and the page then reports 2
    #[test]
    fn a_wrong_scale_is_corrected() {
        assert_eq!(corrected_zoom(-2.0 / 96.0, 2.0), Some(-96.0));
        assert_eq!(corrected_zoom(0.5, 2.0), Some(4.0));
        assert_eq!(corrected_zoom(1.0, 1.5), Some(1.5));
    }

    // a report that cannot be used is left alone
    #[test]
    fn an_unusable_scale_is_ignored() {
        for (dpr, scale) in [(0.0, 2.0), (f64::NAN, 2.0), (f64::INFINITY, 2.0), (2.0, f64::NAN), (2.0, 0.0)] {
            assert_eq!(corrected_zoom(dpr, scale), None, "{dpr} {scale}");
        }
    }
}
