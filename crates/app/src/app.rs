use std::sync::Arc;

use dioxus::prelude::*;
use labrack_core::{Hub, Lan, Network, Store, Timing, Update};
use tokio::sync::broadcast::error::RecvError;

use crate::{Header, Page, Style};

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
                        Ok(_) | Err(RecvError::Lagged(_)) => changes += 1,
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    });
    changes();

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
                        hub.mute_all();
                    }
                },
            }
            main {}
        }
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
