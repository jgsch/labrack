//! Render the pages of the app to HTML files without a window, for `mise run app-shot`:
//! `cargo run -p labrack-app --example render -- <directory>` writes `<directory>/<page>.html`.

use dioxus::prelude::*;
use labrack_app::{Header, Page, stylesheet};

thread_local! {
    static PAGE: std::cell::Cell<Page> = const { std::cell::Cell::new(Page::Amplifiers) };
}

fn root() -> Element {
    rsx! {
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
        let html = format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>{}</style></head><body>{}</body></html>",
            stylesheet(),
            dioxus_ssr::render(&dom)
        );
        std::fs::write(dir.join(format!("{name}.html")), html).expect("write the page");
    }
}
