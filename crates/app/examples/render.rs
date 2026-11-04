//! Render the pages of the app to HTML files without a window, for `mise run app-shot`:
//! `cargo run -p labrack-app --example render -- <directory>` writes `<directory>/<page>.html`.

use dioxus::prelude::*;
use labrack_app::control::Bounds;
use labrack_app::{ConfirmButton, Fader, Header, Meter, Page, Style, ValueField};

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
