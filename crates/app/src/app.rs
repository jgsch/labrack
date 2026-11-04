use std::sync::Arc;

use dioxus::prelude::*;
use labrack_core::{Hub, Lan, Network, Store, Timing, Update};
use tokio::sync::broadcast::error::RecvError;

use crate::{Header, Page};

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
