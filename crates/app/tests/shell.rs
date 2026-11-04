//! The window shell: header, tabs and style (UX-1 to UX-10, UX-27, PERF-4). The components are rendered
//! to HTML without a window; how they look is checked against the mockup with `mise run app-shot`.

use dioxus::prelude::*;
use labrack_app::{Header, Page, stylesheet};

thread_local! {
    static CASE: std::cell::Cell<(Page, bool)> = const { std::cell::Cell::new((Page::Amplifiers, false)) };
}

fn root() -> Element {
    let (page, all_muted) = CASE.get();
    rsx! { Header { page, all_muted } }
}

fn header(page: Page, all_muted: bool) -> String {
    CASE.set((page, all_muted));
    let mut dom = VirtualDom::new(root);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// The text and the attributes of every `<tag ...>text</tag>`, in order.
fn elements<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, String)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        let attrs = &rest[open.len()..end];
        let body_end = rest.find(&close).unwrap_or(rest.len());
        let text: String = rest[end + 1..body_end].replace("&#38;", "&").replace("&amp;", "&");
        out.push((attrs, text));
        rest = &rest[body_end..];
    }
    out
}

fn has(attrs: &str, name: &str, value: &str) -> bool {
    attrs.contains(&format!("{name}=\"{value}\""))
}

// UX-1: Amplifiers, Mix & EQ, Search, in that order
#[test]
fn three_tabs_in_the_header() {
    let html = header(Page::Amplifiers, false);
    let tabs: Vec<_> = elements(&html, "a").into_iter().map(|(_, text)| text).collect();
    assert_eq!(tabs, ["Amplifiers", "Mix & EQ", "Search"]);
    assert_eq!(Page::ALL, [Page::Amplifiers, Page::MixEq, Page::Search]);
}

// UX-1: the active tab is the page shown, and only that one
#[test]
fn the_active_tab_is_the_page() {
    for page in Page::ALL {
        let html = header(page, false);
        let tabs = elements(&html, "a");
        let current: Vec<_> = tabs.iter().filter(|(attrs, _)| has(attrs, "aria-current", "page")).collect();
        assert_eq!(current.len(), 1, "{page:?}");
        assert_eq!(current[0].1, page.label());
        let active: Vec<_> = tabs.iter().filter(|(attrs, _)| has(attrs, "class", "tab active")).collect();
        assert_eq!(active.len(), 1, "{page:?}");
        assert_eq!(active[0].1, page.label());
    }
}

// UX-3: MUTE ALL is on the Amplifiers page only
#[test]
fn mute_all_is_on_the_amplifiers_page_only() {
    for page in Page::ALL {
        let html = header(page, false);
        let buttons = elements(&html, "button");
        match page {
            Page::Amplifiers => assert_eq!(buttons.len(), 1),
            _ => assert!(buttons.is_empty(), "{page:?}"),
        }
    }
}

// UX-3, SEC-5: the button offers what it does. UNMUTE ALL needs the confirmation of `app-controls`,
// so until that exists it cannot be clicked.
#[test]
fn mute_all_becomes_unmute_all() {
    let html = header(Page::Amplifiers, false);
    let (attrs, text) = &elements(&html, "button")[0];
    assert_eq!(text, "MUTE ALL");
    assert!(has(attrs, "aria-pressed", "false"));
    assert!(!attrs.contains("disabled"), "{attrs}");

    let html = header(Page::Amplifiers, true);
    let (attrs, text) = &elements(&html, "button")[0];
    assert_eq!(text, "UNMUTE ALL");
    assert!(has(attrs, "aria-pressed", "true"));
    assert!(attrs.contains("disabled"), "UNMUTE ALL waits for its confirmation: {attrs}");
}

// UX-2: the title, and the subtitle spread over the width of the title letter by letter
#[test]
fn title_and_subtitle() {
    let html = header(Page::Amplifiers, false);
    let title = &elements(&html, "h1")[0];
    assert_eq!(title.1.to_uppercase(), "LAB.RACK");

    let at = html.find("aria-label=\"DSP CONTROLLER\"").expect("the subtitle is named for screen readers");
    let subtitle = &html[at..html[at..].find("</div>").map(|n| at + n).unwrap()];
    let letters: Vec<_> = elements(subtitle, "span").into_iter().map(|(_, text)| text).collect();
    assert_eq!(letters.concat(), "DSPCONTROLLER");
    assert_eq!(letters.len(), 14, "13 letters and the gap between the words");
}

// UX-4: tabs are links and MUTE ALL a button, so the keyboard reaches them; the focus is drawn
#[test]
fn everything_is_reached_by_keyboard() {
    let html = header(Page::Amplifiers, false);
    for (attrs, text) in elements(&html, "a") {
        assert!(attrs.contains("href="), "{text} has no href: {attrs}");
        assert!(!attrs.contains("tabindex=\"-"), "{text}");
    }
    let css = stylesheet();
    assert!(css.contains("button:focus-visible,a:focus-visible{outline:2px solid #b4befe"), "focus outline");
}

// UX-5 to UX-8, UX-10, UX-27: the palette, the layout and the hover of the mockup
#[test]
fn the_style_of_the_mockup() {
    let css = stylesheet();
    for colour in [
        "#11111b", "#1e1e2e", "#181825", "#45475a", "#585b70", "#313244", "#cdd6f4", "#a6adc8", "#b4befe",
        "#74c7ec", "#cba6f7", "#a6e3a1", "#f9e2af", "#f38ba8", "#fab387", "#eba0ac",
    ] {
        assert!(css.contains(colour), "{colour}");
    }
    assert!(css.contains("padding:28px 40px 40px"), "40 px at the sides");
    assert!(css.contains("button:hover,a.tab:hover{filter:brightness(1.18)}"), "hover brightens");
    assert!(css.contains("'IBM Plex Mono'") && css.contains("'Barlow Condensed'"));
}

// UX-10: a window narrower than the boards scrolls; the header never overlaps itself, and the labels
// of the tabs and of MUTE ALL stay on one line (seen in a 633 px wide window of a tiling desktop)
#[test]
fn a_narrow_window_scrolls_instead_of_overlapping() {
    let css = stylesheet();
    let rule = |selector: &str| {
        let at = css.find(&format!("\n{selector}{{")).unwrap_or_else(|| panic!("no rule for {selector}"));
        &css[at..css[at..].find('}').map(|n| at + n).unwrap()]
    };
    assert!(rule(".frame").contains("min-width:900px"));
    assert!(rule(".tab").contains("white-space:nowrap"));
    assert!(rule(".mute-all").contains("white-space:nowrap"));
}

// UX-9, PERF-4: both fonts are in the stylesheet itself, and nothing is fetched from the network
#[test]
fn fonts_are_bundled_and_nothing_is_fetched() {
    let css = stylesheet();
    for (family, weight) in [
        ("Barlow Condensed", 500),
        ("Barlow Condensed", 600),
        ("Barlow Condensed", 700),
        ("IBM Plex Mono", 400),
        ("IBM Plex Mono", 500),
        ("IBM Plex Mono", 600),
    ] {
        let face = format!("font-family:'{family}';font-weight:{weight};");
        let at = css.find(&face).unwrap_or_else(|| panic!("no @font-face for {family} {weight}"));
        assert!(css[at..].starts_with(&face));
        assert!(css[at..].contains("src:url(data:font/woff2;base64,"), "{family} {weight} is not embedded");
    }
    for page in Page::ALL {
        let all = format!("{css}{}", header(page, false));
        for scheme in ["http://", "https://", "//fonts", "@import"] {
            assert!(!all.contains(scheme), "{scheme} found");
        }
    }
}
