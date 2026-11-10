use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use labgruppen::{Channel, EqField, MixerField, Param, Value, XoverField};
use labrack_core::{AmpId, Hub, Lan, Network, Preset, Scan, Side, Store, Timing, Unit, Update};
use tokio::sync::broadcast::error::RecvError;

use crate::amplifiers::{AmpView, Level};
use crate::control::Bounds;
use crate::eq::{self, Band, Filter};
use crate::mixeq::{
    AmpCard, EqBounds, EqView, LIMITER_BOXES, LevelRow, LimiterStrip, MixRow, MixTab, MixeqView,
    SOURCE_NAMES, Strip,
};
use crate::presets::{Ask, ConfirmDialog, PresetView, loaded_text, slot_text};
use crate::search::{KnownAmp, SearchView};
use crate::{AmplifiersPage, Header, MixeqPage, Page, SearchPage, Style, Toasts};

/// The amp list from the config file, or an empty one that is never written when the file cannot be
/// read: a file the app does not understand must not be replaced by an empty list. The text says so in
/// the window (BEH-40).
fn store() -> (Store, Option<String>) {
    store_at(Store::default_path())
}

fn store_at(path: Option<std::path::PathBuf>) -> (Store, Option<String>) {
    match path.map(Store::open) {
        Some(Ok(store)) => (store, None),
        Some(Err(e)) => {
            eprintln!("labrack: the config file cannot be used, working without it: {e}");
            (
                Store::in_memory(),
                Some(format!("The config file cannot be used ({e}). The amps you add are not saved.")),
            )
        }
        None => (Store::in_memory(), None),
    }
}

/// The window: the connections of the core, the header and the page chosen in it.
#[component]
pub fn App() -> Element {
    let (hub, config_problem) = use_hook(|| {
        let (store, problem) = store();
        (Hub::new(store, Arc::new(Network), Arc::new(Lan), Timing::default()), problem)
    });
    let mut config_problem = use_signal(|| config_problem);
    let mut page = use_signal(|| Page::Amplifiers);
    let mut picked = use_signal(|| 0usize);
    let mut tab = use_signal(|| MixTab::Levels);
    // the delay each channel had when its DELAY was unticked, so ticking it restores it (UX-20)
    let mut delays = use_signal(HashMap::<(AmpId, Channel), f64>::new);
    // the presets each amp has stored, read when it goes ONLINE and after a store (BEH-36)
    let preset_lists = use_signal(HashMap::<AmpId, Vec<Preset>>::new);
    // why the last action of an amp failed, and what an amp is busy with (the toasts, UX-29)
    let preset_errors: Errors = use_signal(HashMap::new);
    let busy: Busy = use_signal(HashMap::new);
    // the action that mutes the outputs, waiting for the user's yes (UX-24)
    let mut asking = use_signal(|| None::<(AmpId, Ask)>);

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

    // everything the core tells; the meters redraw the pages that show them at most every 100 ms (PERF-2)
    let mut changes = use_signal(|| 0u64);
    use_future({
        let hub = hub.clone();
        move || {
            let mut updates = hub.subscribe();
            let hub = hub.clone();
            async move {
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut moved = false;
                loop {
                    let update = tokio::select! {
                        update = updates.recv() => update,
                        _ = tick.tick() => {
                            if moved && page().shows_meters() {
                                moved = false;
                                changes += 1;
                            }
                            continue;
                        }
                    };
                    match update {
                        Ok(Update::Meters(..)) => moved = true,
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
                        Ok(Update::Status(id, labrack_core::Status::Online) | Update::Preset(id)) => {
                            read_presets(hub.clone(), id, preset_lists);
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

    let mut amps = views(&hub);
    let ids = std::sync::Arc::new(hub.amps().iter().map(|amp| amp.id).collect::<Vec<_>>());
    for amp in amps.iter_mut() {
        amp.busy = ids.get(amp.index).is_some_and(|id| busy.read().contains_key(id));
    }
    // what runs and what failed show on every page, bottom right (UX-29)
    let toast_busy = busy.read().values().next().cloned();
    let toast_error = preset_errors.read().values().next().cloned().or_else(|| config_problem.read().clone());
    // the mode of an amp is asked about on the Amplifiers page; the other questions on Mix & EQ
    let mode_ask =
        asking.read().clone().filter(|(_, ask)| matches!(ask, Ask::Mode(_)) && page() == Page::Amplifiers);

    rsx! {
        Style {}
        div { class: "frame",
            Header {
                page: page(),
                all_muted: hub.all_muted(),
                on_page: move |next| page.set(next),
                on_mute_all: {
                    let hub = hub.clone();
                    move |muted: bool| {
                        if muted {
                            hub.mute_all();
                        } else {
                            hub.unmute_all();
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
                                        refused(preset_errors, id, hub.set_ip(id, &ip));
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
                            // the preset of an amp opens Mix & EQ on that amp
                            on_preset: move |index: usize| {
                                picked.set(index);
                                page.set(Page::MixEq);
                            },
                            // the mode changes which input feeds which output: it asks first (UX-24)
                            on_mode: {
                                let ids = ids.clone();
                                move |(index, mode): (usize, String)| {
                                    if let Some(id) = ids.get(index).copied()
                                        && !busy.read().contains_key(&id)
                                    {
                                        asking.set(Some((id, Ask::Mode(mode))));
                                    }
                                }
                            },
                        }
                    },
                    Page::MixEq => rsx! {
                        MixeqPage {
                            view: {
                                let mut view = mixeq_view(&hub, picked(), tab(), &delays.read());
                                if let Some(id) = ids.get(picked()).copied() {
                                    view.presets = preset_view(&hub, id, &preset_lists.read());
                                    view.busy = busy.read().get(&id).cloned();
                                    view.asking = asking.read().as_ref().filter(|(at, _)| *at == id).map(|(_, ask)| ask.clone());
                                }
                                view
                            },
                            on_recall: {
                                let ids = ids.clone();
                                move |(slot, name): (u32, String)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        asking.set(Some((id, Ask::Recall { slot, name })));
                                    }
                                }
                            },
                            on_store: {
                                let ids = ids.clone();
                                move |(slot, name): (u32, String)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let replaces = preset_lists
                                            .read()
                                            .get(&id)
                                            .and_then(|list| list.iter().find(|p| p.number == slot))
                                            .map(|p| slot_text(slot, Some(&p.name)));
                                        asking.set(Some((id, Ask::Store { slot, name, replaces })));
                                    }
                                }
                            },
                            on_open: {
                                let ids = ids.clone();
                                move |_| {
                                    let Some(id) = ids.get(picked()).copied() else { return };
                                    spawn(async move {
                                        let file = rfd::AsyncFileDialog::new()
                                            .set_title("Open a settings file")
                                            .add_filter("Settings file", &["json"])
                                            .pick_file()
                                            .await;
                                        if let Some(file) = file {
                                            asking.set(Some((id, Ask::Load { path: file.path().to_path_buf() })));
                                        }
                                    });
                                }
                            },
                            on_save_file: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |_| {
                                    let Some(id) = ids.get(picked()).copied() else { return };
                                    let name = hub.amps().iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_default();
                                    let hub = hub.clone();
                                    spawn(async move {
                                        let file = rfd::AsyncFileDialog::new()
                                            .set_title("Save the settings to a file")
                                            .add_filter("Settings file", &["json"])
                                            .set_file_name(format!("{name}.json"))
                                            .save_file()
                                            .await;
                                        let Some(file) = file else { return };
                                        let path = file.path().to_path_buf();
                                        said(preset_errors, id, hub.save_file(id, &path).await);
                                    });
                                }
                            },
                            on_answer: {
                                let hub = hub.clone();
                                move |yes: bool| {
                                    let Some((id, ask)) = asking.take() else { return };
                                    if yes && !busy.read().contains_key(&id) {
                                        spawn(run_ask(hub.clone(), id, ask, preset_errors, busy));
                                    }
                                }
                            },
                            on_pick: move |index| picked.set(index),
                            on_tab: move |next| tab.set(next),
                            on_link: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(side, linked): (Side, bool)| {
                                    let Some(id) = ids.get(picked()).copied() else { return };
                                    if busy.read().contains_key(&id) {
                                        return;
                                    }
                                    if linked {
                                        // linking mutes the outputs while it runs: it asks first (UX-24)
                                        asking.set(Some((id, Ask::Link(side))));
                                    } else {
                                        spawn(set_link(hub.clone(), id, side, false, preset_errors, busy));
                                    }
                                }
                            },
                            // copying an EQ mutes the outputs while it runs: it asks first (UX-24)
                            on_copy: {
                                let ids = ids.clone();
                                move |_| {
                                    if let (Some(id), Some(ch)) = (ids.get(picked()).copied(), tab().channel())
                                        && !busy.read().contains_key(&id)
                                    {
                                        asking.set(Some((id, Ask::CopyEq(ch))));
                                    }
                                }
                            },
                            on_set: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |writes: Vec<(Param, Value)>| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        for (param, value) in writes {
                                            refused(preset_errors, id, hub.set(id, param, value));
                                        }
                                    }
                                }
                            },
                            on_delay: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(ch, on, ms): (Channel, bool, f64)| {
                                    let Some(id) = ids.get(picked()).copied() else { return };
                                    let to = if on {
                                        delays.read().get(&(id, ch)).copied()
                                    } else {
                                        if ms > 0.0 {
                                            delays.write().insert((id, ch), ms);
                                        }
                                        Some(0.0)
                                    };
                                    if let Some(to) = to {
                                        refused(preset_errors, id, hub.set(id, Param::Delay(ch), Value::Float(to)));
                                    }
                                }
                            },
                            on_limiter: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(output, unit, value): (usize, Unit, f64)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        refused(preset_errors, id, hub.set_limiter(id, output as u8, unit, value));
                                    }
                                }
                            },
                            on_gain: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(channel, kind, db): (usize, u8, f64)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let ch = if kind == 0 { Channel::Input(channel as u8) } else { Channel::Output(channel as u8) };
                                        refused(preset_errors, id, hub.set(id, Param::Gain(ch), Value::Float(db)));
                                    }
                                }
                            },
                            on_mute: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(channel, kind, muted): (usize, u8, bool)| {
                                    if let Some(id) = ids.get(picked()).copied() {
                                        let ch = if kind == 0 { Channel::Input(channel as u8) } else { Channel::Output(channel as u8) };
                                        refused(preset_errors, id, hub.set(id, Param::Mute(ch), Value::Bool(muted)));
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
                                        refused(preset_errors, id, hub.rename(id, &name));
                                    }
                                }
                            },
                            // the order of the list is the order of the Amplifiers page and of Mix & EQ (BEH-45);
                            // the amp that is picked on Mix & EQ stays the one that is picked
                            on_move: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |(index, to): (usize, usize)| {
                                    let Some(id) = ids.get(index).copied() else { return };
                                    if hub.move_amp(id, to).is_ok() {
                                        if picked() == index {
                                            picked.set(to);
                                        } else if picked() == to {
                                            picked.set(index);
                                        }
                                    }
                                }
                            },
                            on_remove: {
                                let hub = hub.clone();
                                let ids = ids.clone();
                                move |index| {
                                    if let Some(id) = ids.get(index).copied() {
                                        let removed = hub.remove(id);
                                        if removed.is_ok() {
                                            picked.set(picked_after_removal(picked(), index));
                                        }
                                        refused(preset_errors, id, removed);
                                    }
                                }
                            },
                        }
                    },
                }
            }
            if let Some((id, ask)) = mode_ask {
                ConfirmDialog {
                    amp: hub.amps().iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_default(),
                    ask,
                    on_confirm: {
                        let hub = hub.clone();
                        move |_| {
                            let Some((id, ask)) = asking.take() else { return };
                            if !busy.read().contains_key(&id) {
                                spawn(run_ask(hub.clone(), id, ask, preset_errors, busy));
                            }
                        }
                    },
                    on_cancel: move |_| {
                        asking.set(None);
                    },
                }
            }
            Toasts {
                busy: toast_busy,
                error: toast_error,
                on_dismiss: {
                    let mut errors = preset_errors;
                    move |_| {
                        errors.write().clear();
                        config_problem.set(None);
                    }
                },
            }
        }
    }
}

/// Read the presets an amp has stored, for the preset bar.
fn read_presets(hub: Hub, id: AmpId, mut lists: Signal<HashMap<AmpId, Vec<Preset>>>) {
    spawn(async move {
        if let Ok(list) = hub.presets(id).await {
            lists.write().insert(id, list);
        }
    });
}

/// Why the last action of each amp failed: a toast until it is clicked or 12 s have passed (UX-29).
type Errors = Signal<HashMap<AmpId, String>>;

/// What each amp is busy with, for the spinner (UX-29): an IPD takes seconds for a bulk change.
type Busy = Signal<HashMap<AmpId, String>>;

/// How long an error toast stays.
const ERROR_TOAST: std::time::Duration = std::time::Duration::from_secs(12);

/// Keep the outcome of an action of an amp: its error, or none after a success.
fn said(mut errors: Errors, id: AmpId, done: labrack_core::Result<()>) {
    match done {
        Ok(()) => {
            errors.write().remove(&id);
        }
        Err(e) => {
            let text = e.to_string();
            errors.write().insert(id, text.clone());
            spawn(async move {
                tokio::time::sleep(ERROR_TOAST).await;
                if errors.read().get(&id) == Some(&text) {
                    errors.write().remove(&id);
                }
            });
        }
    };
}

/// Show why the core refused an action of an amp (a duplicate IP, a linked channel 2, a missing right). A
/// success says nothing: a fader that moves all the time must not clear the toast of another action.
fn refused(errors: Errors, id: AmpId, done: labrack_core::Result<()>) {
    if done.is_err() {
        said(errors, id, done);
    }
}

/// Show `what` as in progress for the amp while `work` runs.
async fn working<T>(mut busy: Busy, id: AmpId, what: String, work: impl Future<Output = T>) -> T {
    busy.write().insert(id, what);
    let done = work.await;
    busy.write().remove(&id);
    done
}

/// Link or split one pair (UX-17).
async fn set_link(hub: Hub, id: AmpId, side: Side, linked: bool, errors: Errors, busy: Busy) {
    let what = format!(
        "{} the {}…",
        if linked { "Linking" } else { "Unlinking" },
        if side == Side::Inputs { "inputs" } else { "outputs" }
    );
    let done = working(busy, id, what, hub.set_link(id, side, linked)).await;
    said(errors, id, done);
}

/// Do what the user said yes to (UX-24). What it did shows in the running preset, or in an error toast.
async fn run_ask(hub: Hub, id: AmpId, ask: Ask, errors: Errors, busy: Busy) {
    let what = ask.progress();
    let done = working(busy, id, what, run_ask_inner(hub.clone(), id, ask)).await;
    said(errors, id, done);
}

async fn run_ask_inner(hub: Hub, id: AmpId, ask: Ask) -> labrack_core::Result<()> {
    match ask {
        Ask::Link(side) => hub.set_link(id, side, true).await,
        Ask::CopyEq(to) => hub.copy_eq(id, to).await,
        Ask::Mode(mode) => hub.set_device_mode(id, &mode).await,
        Ask::Recall { slot, .. } => hub.recall_preset(id, slot).await,
        // the list of presets is read again on the Preset update that a store causes
        Ask::Store { slot, name, .. } => hub.store_preset(id, slot, &name).await,
        Ask::Load { path } => hub.load_file(id, &path).await.map(|_| ()),
    }
}

/// The level a muted input would have: its loudest source that is on (connector signal, mixer gain, whether
/// the source is muted in the mixer), at the input gain. The amp reports nothing for a muted input, and the
/// user wants to see whether there is sound (UX-19).
fn muted_input_level(sources: &[(Option<f64>, f64, bool)], gain_db: f64) -> f64 {
    sources
        .iter()
        .filter(|(_, _, off)| !off)
        .filter_map(|(signal, mixer_db, _)| signal.map(|s| s + mixer_db))
        .fold(None, |top: Option<f64>, level| Some(top.map_or(level, |t| t.max(level))))
        .map_or(-120.0, |level| level + gain_db)
}

/// The level a source's meter shows in the Mixer: its signal at the mixer's gain. A muted source keeps
/// its meter, to see which channel has sound (UX-18); one without a meter (AES on an IPX) shows none.
fn source_level(signal_db: Option<f64>, gain_db: f64) -> f64 {
    signal_db.map_or(-120.0, |signal| signal + gain_db)
}

/// What the preset bar shows for an amp (BEH-36 to BEH-38).
fn preset_view(hub: &Hub, id: AmpId, lists: &HashMap<AmpId, Vec<Preset>>) -> PresetView {
    let stored = lists.get(&id);
    let slots = hub
        .preset_slots(id)
        .map(|slots| {
            slots
                .map(|n| {
                    (n, stored.and_then(|list| list.iter().find(|p| p.number == n)).map(|p| p.name.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    PresetView {
        slots,
        loaded: hub
            .amps()
            .into_iter()
            .find(|a| a.id == id)
            .and_then(|a| a.preset)
            .map(|p| (p.number, p.name)),
        modified: hub.modified(id),
        online: hub.status(id) == Some(labrack_core::Status::Online),
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
                view.muted[output] = hub.shown(amp.id, &param) == Some(Value::Bool(true));
            }
            view.amp_muted = hub.amp_muted(amp.id);
            view.preset = amp.preset.as_ref().map(|p| loaded_text(p.number, &p.name));
            view.mode = hub.device_mode(amp.id);
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
pub fn mixeq_view(
    hub: &Hub,
    selected: usize,
    tab: MixTab,
    delays: &HashMap<(AmpId, Channel), f64>,
) -> MixeqView {
    let amps = hub.amps();
    let Some(amp) = amps.get(selected) else {
        return MixeqView { tab, ..MixeqView::default() };
    };
    let id = amp.id;
    let gain = |ch: Channel| hub.shown(id, &Param::Gain(ch)).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let mute = |ch: Channel| hub.shown(id, &Param::Mute(ch)) == Some(Value::Bool(true));
    let meters = hub.meters(id).unwrap_or_default();
    let at = |list: &[f64], i: usize, none: f64| list.get(i).copied().unwrap_or(none);
    let gr = meters.extra.get("gain_reduction_out").cloned().unwrap_or_default();
    let strip =
        |label: String, ch: Channel, level_db: f64| Strip { label, db: gain(ch), level_db, muted: mute(ch) };
    // an amp blanks the meter of a muted input; the sound that arrives is estimated from the connectors
    let raw = meters.extra.get("raw_input").cloned().unwrap_or_default();
    let input_level = |i: u8| {
        let ch = Channel::Input(i);
        if !mute(ch) {
            return at(&meters.input, usize::from(i), -120.0);
        }
        let sources: Vec<(Option<f64>, f64, bool)> = (0..4u8)
            .map(|src| {
                let param = |field| Param::Mixer { ch, src, field };
                let level = hub.shown(id, &param(MixerField::Level)).and_then(|v| v.as_f64());
                let off = hub.shown(id, &param(MixerField::Mute)) == Some(Value::Bool(true));
                (raw.get(usize::from(src)).copied(), level.unwrap_or(0.0), off)
            })
            .collect();
        muted_input_level(&sources, gain(ch))
    };
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
                input: strip(format!("IN {}", n + 1), Channel::Input(i), input_level(i)),
                output: strip(format!("OUT {}", n + 1), Channel::Output(i), at(&meters.output, n, -120.0)),
                limiter: LimiterStrip {
                    label: format!("LIMITER {}", n + 1),
                    gr_db: at(&gr, n, 0.0),
                    figures: hub.limiter_shown(id, i),
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
    // the IPD's source meters, before the mixer; the IPX has none, and its cards show no level
    let sources = meters.extra.get("raw_input").cloned().unwrap_or_default();
    let mixer = (0..2u8)
        .map(|i| MixRow {
            label: format!("IN {}", i + 1),
            sources: SOURCE_NAMES
                .iter()
                .enumerate()
                .map(|(src, name)| {
                    let param = |field| Param::Mixer { ch: Channel::Input(i), src: src as u8, field };
                    let db = hub.shown(id, &param(MixerField::Level)).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let muted =
                        hub.shown(id, &param(MixerField::Mute)).and_then(|v| v.as_bool()).unwrap_or(false);
                    let level_db = source_level(sources.get(src).copied(), db);
                    Strip { label: (*name).into(), db, level_db, muted }
                })
                .collect(),
        })
        .collect();
    let level = Param::Mixer { ch: Channel::Input(0), src: 0, field: MixerField::Level };
    let range = hub.range(id, &level);
    let (min, max) = labgruppen::shared_range(&level).unwrap_or((-48.0, 0.0));
    let mixer_bounds =
        Bounds::new(range.and_then(|r| r.min).unwrap_or(min), range.and_then(|r| r.max).unwrap_or(max));
    let linked_inputs = hub.linked(id, Side::Inputs);
    let linked_outputs = hub.linked(id, Side::Outputs);
    let eq = tab
        .channel()
        .map(|ch| EqView { delay_restore: delays.get(&(id, ch)).copied(), ..eq_view(hub, id, ch) });
    MixeqView {
        amps: cards,
        selected,
        tab,
        linked_inputs,
        linked_outputs,
        rows,
        mixer,
        mixer_bounds,
        eq,
        presets: PresetView::default(),
        asking: None,
        busy: None,
    }
}

/// What an EQ tab draws for one channel (UX-20); what is not read yet shows the default band.
fn eq_view(hub: &Hub, id: AmpId, ch: Channel) -> EqView {
    let value = |param: &Param| hub.shown(id, param);
    let number = |param: Param, none: f64| value(&param).and_then(|v| v.as_f64()).unwrap_or(none);
    let on = |param: Param| value(&param).and_then(|v| v.as_bool()).unwrap_or(false);
    let text = |param: Param, none: &str| {
        value(&param).and_then(|v| v.as_str().map(str::to_string)).unwrap_or(none.into())
    };
    let bounds = |param: Param| {
        let range = hub.range(id, &param);
        let (min, max) = labgruppen::shared_range(&param).unwrap_or((0.0, 100_000.0));
        Bounds::new(range.and_then(|r| r.min).unwrap_or(min), range.and_then(|r| r.max).unwrap_or(max))
    };
    let field = |band, field| Param::Eq { ch, band, field };
    let bands = (0..labgruppen::EQ_BANDS)
        .map(|band| {
            let none = eq::default_band(band);
            Band {
                on: on(field(band, EqField::Enabled)),
                kind: text(field(band, EqField::Type), &none.kind),
                freq: number(field(band, EqField::Freq), none.freq),
                gain: number(field(band, EqField::Gain), none.gain),
                q: number(field(band, EqField::Q), none.q),
            }
        })
        .collect();
    let filter = |high: bool, freq: f64, kind: &str| Filter {
        on: on(eq::filter_param(ch, high, XoverField::Enabled)),
        kind: text(eq::filter_param(ch, high, XoverField::Type), kind),
        freq: number(eq::filter_param(ch, high, XoverField::Freq), freq),
    };
    let filters = (!ch.is_input())
        .then(|| (filter(true, 40.0, "butterworth_24"), filter(false, 20000.0, "linkwitz_riley_24")));
    EqView {
        channel: ch,
        bands,
        filters,
        delay_ms: number(Param::Delay(ch), 0.0),
        delay_restore: None,
        bounds: EqBounds {
            freq: bounds(field(0, EqField::Freq)),
            gain: bounds(field(0, EqField::Gain)),
            q: bounds(field(0, EqField::Q)),
            delay: bounds(Param::Delay(ch)),
            hpf: bounds(eq::filter_param(Channel::Output(0), true, XoverField::Freq)),
            lpf: bounds(eq::filter_param(Channel::Output(0), false, XoverField::Freq)),
        },
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

/// The index of the amp picked on Mix & EQ once the amp at `removed` is gone: the amps below it move up
/// one place, so the picked one stays the same amp (BEH-45). When the picked amp itself is removed, the
/// index stays: the next amp takes its place.
pub fn picked_after_removal(picked: usize, removed: usize) -> usize {
    if removed < picked { picked - 1 } else { picked }
}

#[cfg(test)]
mod tests {
    use super::{corrected_zoom, muted_input_level, picked_after_removal, refused, source_level, store_at};
    use dioxus::prelude::*;
    use labrack_core::{Error, Protocol, Store};

    // BEH-40: a config file that cannot be read is said in the window, and is left as it is
    #[test]
    fn an_unreadable_config_is_reported_and_left_alone() {
        let path = std::env::temp_dir().join(format!("labrack-config-{}.toml", std::process::id()));
        std::fs::write(&path, "amps = [[[ not toml").unwrap();
        let (mut store, problem) = store_at(Some(path.clone()));
        let problem = problem.expect("a message for the window");
        assert!(problem.contains("not saved"), "{problem}");
        store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "amps = [[[ not toml", "never written");
        std::fs::remove_file(&path).unwrap();
        assert!(store_at(None).1.is_none());
    }

    // UX-26, UX-29: what the core refuses shows as an error toast; a success clears nothing
    #[test]
    fn a_refusal_is_kept_and_a_success_says_nothing() {
        fn page() -> Element {
            let id = Store::in_memory().add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
            let errors = use_signal(std::collections::HashMap::new);
            use_hook(|| {
                refused(errors, id, Err(Error::NotOnline));
                refused(errors, id, Ok(()));
            });
            rsx! { for text in errors.read().values() { p { "{text}" } } }
        }
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        let out = dioxus_ssr::render(&dom);
        assert!(out.contains("<p>"), "the refusal is shown: {out}");
    }

    // BEH-45: removing an amp above the picked one leaves the same amp picked
    #[test]
    fn removing_an_amp_above_the_picked_one_keeps_it_picked() {
        assert_eq!(picked_after_removal(2, 0), 1, "above: the picked amp moved up");
        assert_eq!(picked_after_removal(2, 1), 1);
        assert_eq!(picked_after_removal(2, 2), 2, "the picked one itself: the next takes its place");
        assert_eq!(picked_after_removal(2, 3), 2, "below: nothing moves");
        assert_eq!(picked_after_removal(0, 0), 0);
    }

    // UX-19: a muted input still shows the sound that arrives: the loudest source that is on, through the
    // mixer and the input gain; sources switched off in the mixer or without a meter count for nothing
    #[test]
    fn a_muted_input_shows_the_sound_that_arrives() {
        let sources = [(Some(-28.0), -3.0, false), (Some(-20.0), 0.0, true), (None, 0.0, false)];
        assert_eq!(muted_input_level(&sources, -8.0), -39.0, "ANALOG 1 only: -28 -3 -8");
        let both = [(Some(-28.0), 0.0, false), (Some(-20.0), 0.0, false)];
        assert_eq!(muted_input_level(&both, 0.0), -20.0, "the loudest");
        assert_eq!(muted_input_level(&[(None, 0.0, false)], 0.0), -120.0);
        assert_eq!(muted_input_level(&[], 0.0), -120.0);
    }

    // UX-18: a muted source keeps its meter (the caller does not even pass the mute); no meter, no level
    #[test]
    fn a_source_meter_is_its_signal_at_the_mixer_gain() {
        assert_eq!(source_level(Some(-28.0), -6.0), -34.0);
        assert_eq!(source_level(Some(-28.0), 0.0), -28.0);
        assert_eq!(source_level(None, 0.0), -120.0, "AES on an IPX has no meter");
    }

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
