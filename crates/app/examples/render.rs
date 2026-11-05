//! Render the pages of the app to HTML files without a window, for `mise run app-shot`:
//! `cargo run -p labrack-app --example render -- <directory>` writes `<directory>/<page>.html`.

use dioxus::prelude::*;
use labrack_app::control::Bounds;
use labrack_app::search::KnownAmp;
use labrack_app::{
    AmpView, AmplifiersPage, ConfirmButton, Fader, Header, Level, Meter, Page, Status, Style, ValueField,
};

thread_local! {
    static PAGE: std::cell::Cell<Page> = const { std::cell::Cell::new(Page::Amplifiers) };
}

fn root() -> Element {
    rsx! {
        Style {}
        div { class: "frame",
            Header { page: PAGE.get(), all_muted: false }
            main {}
        }
    }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("the directory to write to"));
    std::fs::create_dir_all(&dir).expect("create the directory");
    for page in Page::ALL {
        PAGE.set(page);
        let mut dom = VirtualDom::new(root);
        dom.rebuild_in_place();
        let name = page.label().to_lowercase().replace(" & ", "-");
        let render = dioxus_ssr::render(&dom);
        let html =
            format!("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{render}</body></html>");
        std::fs::write(dir.join(format!("{name}.html")), html).expect("write the page");
    }

    let mut dom = VirtualDom::new(controls);
    dom.rebuild_in_place();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        dioxus_ssr::render(&dom)
    );
    std::fs::write(dir.join("controls.html"), html).expect("write the controls");

    let mut dom = VirtualDom::new(amplifiers_demo);
    dom.rebuild_in_place();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        dioxus_ssr::render(&dom)
    );
    std::fs::write(dir.join("amplifiers-demo.html"), html).expect("write the amplifiers");

    let mut dom = VirtualDom::new(search_demo);
    dom.rebuild_in_place();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        dioxus_ssr::render(&dom)
    );
    std::fs::write(dir.join("search-demo.html"), html).expect("write the search page");

    let mut dom = VirtualDom::new(levels_demo);
    dom.rebuild_in_place();
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        dioxus_ssr::render(&dom)
    );
    std::fs::write(dir.join("levels-demo.html"), html).expect("write the levels page");

    for (name, page) in [
        ("eq-in-demo", eq_in_demo as fn() -> Element),
        ("eq-out-demo", eq_out_demo),
        ("mixer-demo", mixer_demo),
        ("linked-demo", linked_demo),
        ("ask-demo", ask_demo),
        ("eq-inputs-demo", eq_inputs_demo),
        ("eq-outputs-demo", eq_outputs_demo),
    ] {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        let html = format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
            dioxus_ssr::render(&dom)
        );
        std::fs::write(dir.join(format!("{name}.html")), html).expect("write the EQ page");
    }
}

/// A row of the controls, for the visual check of `mise run app-shot` (not a page of the app).
fn controls() -> Element {
    rsx! {
        Style {}
        div { class: "frame",
            div { style: "display:flex;gap:24px;align-items:flex-start",
                Fader { label: "IN 1", value: -6.0, bounds: Bounds::new(-12.0, 12.0) }
                Fader { label: "OUT 1", value: -20.0, bounds: Bounds::new(-60.0, 12.0) }
                div { style: "display:flex;flex-direction:column;gap:12px;width:320px",
                    Meter { level_db: -18.0, gain_reduction_db: 3.0 }
                    Meter { level_db: -6.0, gain_reduction_db: 0.0 }
                    ValueField { label: "OUT 2", value: -6.5, bounds: Bounds::new(-60.0, 12.0) }
                    ConfirmButton { label: "MUTE ALL", confirm_label: "CONFIRM MUTE ALL" }
                }
            }
        }
    }
}

/// Three amplifiers of the mockup's Amplifiers board, for the visual check of the panels.
fn amplifiers_demo() -> Element {
    let mut a = AmpView::new(0, "FOH Main L", "IPD", "192.168.1.21", Status::Online);
    a.levels[0] = Level { label: "OUT 1".into(), db: -18.0, gr: 0.0 };
    a.levels[1] = Level { label: "OUT 2".into(), db: -24.5, gr: 3.0 };
    a.preset = Some("02 · Show".into());
    a.modified = true;
    let b = AmpView::new(1, "Subs", "IPX", "192.168.1.31", Status::Busy);
    let mut c = AmpView::new(2, "Delays", "IPD", "192.168.1.24", Status::Disconnected);
    c.preset = Some("05 · Subs cardioid end-fire".into());
    c.amp_muted = true;
    c.muted = [true, true];
    rsx! {
        Style {}
        div { class: "frame", AmplifiersPage { amps: vec![a, b, c] } }
    }
}

/// The Search page with a few amplifiers, for the visual check of the page.
fn search_demo() -> Element {
    let view = labrack_app::SearchView {
        ipd_count: 2,
        ipx_count: 1,
        message: "Found 2 new".into(),
        amps: vec![
            KnownAmp {
                index: 0,
                name: "FOH Main L".into(),
                protocol: "IPD".into(),
                ip: "192.168.1.21".into(),
                source: "AUTO".into(),
            },
            KnownAmp {
                index: 1,
                name: "Subs".into(),
                protocol: "IPX".into(),
                ip: "192.168.1.31".into(),
                source: "AUTO".into(),
            },
            KnownAmp {
                index: 2,
                name: "Delays".into(),
                protocol: "IPD".into(),
                ip: "192.168.1.24".into(),
                source: "MANUAL".into(),
            },
        ],
        ..labrack_app::SearchView::default()
    };
    rsx! {
        Style {}
        div { class: "frame", labrack_app::SearchPage { view } }
    }
}

/// The Levels tab with the figures of the mockup's board, for the visual check of the page.
fn levels_demo() -> Element {
    levels_page(false)
}

/// The Levels tab linked, as on the mockup's board 07: one row, two meters per fader.
fn linked_demo() -> Element {
    levels_page(true)
}

fn levels_page(linked: bool) -> Element {
    let strip = |label: &str, level_db: f64| labrack_app::Strip {
        label: label.into(),
        db: 0.0,
        level_db,
        muted: false,
    };
    let limiter = |label: &str, gr_db: f64| labrack_app::LimiterStrip {
        label: label.into(),
        gr_db,
        figures: Some(labrack_core::Limiter::from_vpk(80.0, 4.0)),
        bounds: [Bounds::new(0.0, 100_000.0); 4],
    };
    let card = |name: &str, level_db: f64| labrack_app::AmpCard { name: name.into(), level_db, gr_db: 0.0 };
    let view = labrack_app::MixeqView {
        amps: vec![card("FOH Main L", -33.0), card("FOH Main R", -27.0), card("Subs", -21.0)],
        presets: demo_presets(),
        rows: vec![
            labrack_app::LevelRow {
                input: strip("IN 1", -24.0),
                output: strip("OUT 1", -24.0),
                limiter: limiter("LIMITER 1", 0.0),
            },
            labrack_app::LevelRow {
                input: strip("IN 2", -18.0),
                output: strip("OUT 2", -18.0),
                limiter: limiter("LIMITER 2", if linked { 8.0 } else { 0.0 }),
            },
        ],
        linked_inputs: linked,
        linked_outputs: linked,
        ..labrack_app::MixeqView::default()
    };
    rsx! {
        Style {}
        div { class: "frame",
            Header { page: Page::MixEq, all_muted: false }
            main { labrack_app::MixeqPage { view } }
        }
    }
}

/// An EQ tab with the bands of the mockup's boards (04 for an input, 05 for an output).
fn eq_demo(tab: labrack_app::MixTab, gains: [f64; 10]) -> Element {
    use labgruppen::Channel;
    use labrack_app::eq::{Band, Filter, default_band};
    let on = [true, true, false, true, false, true, true, false, true, false];
    let bands = (0..10u8)
        .map(|i| Band { on: on[usize::from(i)], gain: gains[usize::from(i)], ..default_band(i) })
        .collect();
    let output = matches!(tab, labrack_app::MixTab::Out1 | labrack_app::MixTab::Outputs);
    let linked = matches!(tab, labrack_app::MixTab::Inputs | labrack_app::MixTab::Outputs);
    let eq = labrack_app::EqView {
        channel: if output { Channel::Output(0) } else { Channel::Input(0) },
        bands,
        filters: output.then(|| {
            (
                Filter { on: true, kind: "butterworth_24".into(), freq: 40.0 },
                Filter { on: false, kind: "linkwitz_riley_24".into(), freq: 120.0 },
            )
        }),
        delay_ms: 0.0,
        delay_restore: None,
        bounds: labrack_app::EqBounds {
            freq: Bounds::new(20.0, 20000.0),
            gain: Bounds::new(-12.0, 12.0),
            q: Bounds::new(0.2, 25.0),
            delay: Bounds::new(0.0, 2000.0),
            hpf: Bounds::new(19.0, 20000.0),
            lpf: Bounds::new(20.0, 20001.0),
        },
    };
    let card = |name: &str, level_db: f64| labrack_app::AmpCard { name: name.into(), level_db, gr_db: 0.0 };
    let view = labrack_app::MixeqView {
        amps: vec![card("FOH Main L", -33.0), card("FOH Main R", -27.0), card("Subs", -21.0)],
        tab,
        linked_inputs: linked,
        linked_outputs: linked,
        eq: Some(eq),
        presets: demo_presets(),
        ..labrack_app::MixeqView::default()
    };
    rsx! {
        Style {}
        div { class: "frame",
            Header { page: Page::MixEq, all_muted: false }
            main { labrack_app::MixeqPage { view } }
        }
    }
}

fn eq_in_demo() -> Element {
    eq_demo(labrack_app::MixTab::In1, [2.4, -0.8, 0.0, -1.6, 0.0, -2.4, 0.0, 0.0, -0.8, 0.0])
}

fn eq_out_demo() -> Element {
    eq_demo(labrack_app::MixTab::Out1, [-0.8, 1.6, 0.0, 0.8, 0.0, 0.0, 2.4, 0.0, 1.6, 0.0])
}

/// The shared EQ tabs of a linked amp (boards 08 and 09).
fn eq_inputs_demo() -> Element {
    eq_demo(labrack_app::MixTab::Inputs, [2.4, -0.8, 0.0, -1.6, 0.0, -2.4, 0.0, 0.0, -0.8, 0.0])
}

fn eq_outputs_demo() -> Element {
    eq_demo(labrack_app::MixTab::Outputs, [-0.8, 1.6, 0.0, 0.8, 0.0, 0.0, 2.4, 0.0, 1.6, 0.0])
}

/// The Mixer with the sources of the mockup's board 02: ANALOG 1 into IN 1, ANALOG 2 into IN 2.
fn mixer_demo() -> Element {
    let row = |n: usize, open: usize, level_db: f64| labrack_app::MixRow {
        label: format!("IN {n}"),
        sources: labrack_app::mixeq::SOURCE_NAMES
            .iter()
            .enumerate()
            .map(|(src, name)| labrack_app::Strip {
                label: (*name).into(),
                db: if src == open { 0.0 } else { -48.0 },
                level_db: if src == open { level_db } else { -120.0 },
                muted: src != open,
            })
            .collect(),
    };
    let card = |name: &str, level_db: f64| labrack_app::AmpCard { name: name.into(), level_db, gr_db: 0.0 };
    let view = labrack_app::MixeqView {
        amps: vec![card("FOH Main L", -33.0), card("FOH Main R", -27.0), card("Subs", -21.0)],
        tab: labrack_app::MixTab::Mixer,
        presets: demo_presets(),
        mixer: vec![row(1, 0, -27.0), row(2, 1, -3.0)],
        ..labrack_app::MixeqView::default()
    };
    rsx! {
        Style {}
        div { class: "frame",
            Header { page: Page::MixEq, all_muted: false }
            main { labrack_app::MixeqPage { view } }
        }
    }
}

/// The presets of the mockup's boards: the factory default in slot 1, loaded.
fn demo_presets() -> labrack_app::presets::PresetView {
    let mut slots: Vec<(u32, Option<String>)> = (1..=50).map(|n| (n, None)).collect();
    slots[0].1 = Some("Factory default".into());
    labrack_app::presets::PresetView {
        slots,
        loaded: Some((1, "Factory default".into())),
        modified: false,
        message: None,
        online: true,
    }
}

/// The popup of a recall, over the Levels tab, for the visual check of UX-24.
fn ask_demo() -> Element {
    let card = |name: &str| labrack_app::AmpCard { name: name.into(), level_db: -30.0, gr_db: 0.0 };
    let view = labrack_app::MixeqView {
        amps: vec![card("FOH Main L")],
        presets: demo_presets(),
        asking: Some(labrack_app::presets::Ask::Recall { slot: 1, name: "Factory default".into() }),
        ..labrack_app::MixeqView::default()
    };
    rsx! {
        Style {}
        div { class: "frame",
            Header { page: Page::MixEq, all_muted: false }
            main { labrack_app::MixeqPage { view } }
        }
    }
}
