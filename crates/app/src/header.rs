use dioxus::prelude::*;

use crate::Page;
use crate::control::{CONFIRM_WINDOW, Click, SecondClick};

/// The title of the app and its subtitle, spread letter by letter over the width of the title (UX-2).
const SUBTITLE: [&str; 14] = ["D", "S", "P", "", "C", "O", "N", "T", "R", "O", "L", "L", "E", "R"];

/// What a click on the button asks for: the opposite of what it showed. The state is the one the button
/// was drawn with, not the hub's at click time, which may have moved since (SEC-5).
pub(crate) fn mute_all_target(all_muted_as_shown: bool) -> bool {
    !all_muted_as_shown
}

/// Title, the three tabs (UX-1) and MUTE ALL (UX-3). The button is on every page: whatever page is
/// open, the outputs are one click away from silence. Tabs are links and MUTE ALL a button, so the
/// keyboard reaches all of them (UX-4).
///
/// UNMUTE ALL needs a second click within three seconds (SEC-5, UX-24); MUTE ALL acts at once.
#[component]
pub fn Header(
    page: Page,
    all_muted: bool,
    #[props(default)] on_page: EventHandler<Page>,
    /// What the click asks for, as the label said it: `true` mutes, `false` unmutes (SEC-5).
    #[props(default)]
    on_mute_all: EventHandler<bool>,
) -> Element {
    // UNMUTE ALL takes a second click within three seconds (UX-24, SEC-5); MUTE ALL does not wait
    let mut armed = use_signal(SecondClick::default);
    let clock = use_hook(|| {
        let started = std::time::Instant::now();
        move || started.elapsed()
    });
    let asking = all_muted && armed().armed(clock());
    let label = match (all_muted, asking) {
        (false, _) => "MUTE ALL",
        (true, false) => "UNMUTE ALL",
        (true, true) => "CONFIRM UNMUTE ALL",
    };

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
                onclick: move |_| {
                    if !all_muted {
                        on_mute_all.call(mute_all_target(all_muted));
                        return;
                    }
                    let mut s = armed();
                    match s.click(clock()) {
                        Click::Confirmed => {
                            armed.set(s);
                            on_mute_all.call(mute_all_target(all_muted));
                        }
                        Click::Armed | Click::Expired => {
                            armed.set(s);
                            // the label goes back to UNMUTE ALL when the window is over, without another event
                            spawn(async move {
                                tokio::time::sleep(CONFIRM_WINDOW + std::time::Duration::from_millis(100)).await;
                                armed.set(armed());
                            });
                        }
                    }
                },
                "{label}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SEC-5: MUTE ALL asks to mute and UNMUTE ALL to unmute, whatever the hub says by the time of the click
    #[test]
    fn the_click_asks_for_what_the_label_said() {
        assert!(mute_all_target(false), "MUTE ALL mutes");
        assert!(!mute_all_target(true), "UNMUTE ALL unmutes");
    }
}
