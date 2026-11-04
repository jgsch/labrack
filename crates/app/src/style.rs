//! The stylesheet of the app. The fonts are part of it as data URIs, so the app needs no network
//! and no files next to it (PERF-4).

use dioxus::prelude::*;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const CSS: &str = include_str!("style.css");

/// Latin subsets of the two families (UX-9); the licences (SIL OFL 1.1) are in
/// `project/mockup/mockup/licenses.txt`.
macro_rules! font {
    ($family:literal, $weight:literal, $file:literal) => {
        ($family, $weight, include_bytes!(concat!("../../../project/mockup/mockup/assets/", $file)) as &[u8])
    };
}

const FONTS: [(&str, u16, &[u8]); 6] = [
    font!("Barlow Condensed", 500, "barlow-condensed-500-latin.woff2"),
    font!("Barlow Condensed", 600, "barlow-condensed-600-latin.woff2"),
    font!("Barlow Condensed", 700, "barlow-condensed-700-latin.woff2"),
    font!("IBM Plex Mono", 400, "ibm-plex-mono-400-latin.woff2"),
    font!("IBM Plex Mono", 500, "ibm-plex-mono-500-latin.woff2"),
    font!("IBM Plex Mono", 600, "ibm-plex-mono-600-latin.woff2"),
];

/// The whole stylesheet: the font faces, then the style of the pages.
pub fn stylesheet() -> String {
    let mut css = String::new();
    for (family, weight, data) in FONTS {
        let data = STANDARD.encode(data);
        css += &format!(
            "@font-face{{font-family:'{family}';font-weight:{weight};font-style:normal;font-display:block;src:url(data:font/woff2;base64,{data}) format('woff2')}}\n"
        );
    }
    css + CSS
}

/// The stylesheet as part of the page. The desktop webview applies the font faces only when they come
/// with the page itself; the window and the screenshots of `app-shot` then show the same thing.
#[component]
pub fn Style() -> Element {
    rsx! {
        style { dangerous_inner_html: stylesheet() }
    }
}
