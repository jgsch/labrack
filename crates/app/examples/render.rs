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
