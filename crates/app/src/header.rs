use dioxus::prelude::*;

use crate::Page;

/// The title of the app and its subtitle, spread letter by letter over the width of the title (UX-2).
const SUBTITLE: [&str; 14] = ["D", "S", "P", "", "C", "O", "N", "T", "R", "O", "L", "L", "E", "R"];

/// Title, the three tabs (UX-1) and MUTE ALL (UX-3). The button is on every page: whatever page is
/// open, the outputs are one click away from silence. Tabs are links and MUTE ALL a button, so the
/// keyboard reaches all of them (UX-4).
///
/// UNMUTE ALL needs a second click (SEC-5, UX-24); the confirmation is `app-controls`, and until it
/// exists the button is disabled.
#[component]
pub fn Header(
    page: Page,
    all_muted: bool,
    #[props(default)] on_page: EventHandler<Page>,
    #[props(default)] on_mute_all: EventHandler<()>,
) -> Element {
    rsx! {
        header { class: "header",
            div { class: "brand",
                h1 { "Lab.Rack" }
                div { class: "subtitle", "aria-label": "DSP CONTROLLER",
                    for letter in SUBTITLE {
                        if letter.is_empty() {
                            span { class: "gap" }
                        } else {
                            span { "{letter}" }
                        }
                    }
                }
            }
            nav { class: "tabs",
                for tab in Page::ALL {
                    a {
                        class: if tab == page { "tab active" } else { "tab" },
                        href: tab.anchor(),
                        "aria-current": if tab == page { "page" },
                        onclick: move |event| {
                            event.prevent_default();
                            on_page.call(tab);
                        },
                        "{tab.label()}"
                    }
                }
            }
            button {
                class: "mute-all",
                "aria-pressed": "{all_muted}",
                disabled: all_muted,
                onclick: move |_| on_mute_all.call(()),
                if all_muted { "UNMUTE ALL" } else { "MUTE ALL" }
            }
        }
    }
}
