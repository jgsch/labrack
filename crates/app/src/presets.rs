//! The preset bar of Mix & EQ (BEH-36, BEH-37, BEH-38): the slots of the amp, the name to store
//! under, LOAD, SAVE, OPEN FILE… and SAVE TO COMPUTER…, and the line that says what runs. What mutes
//! the outputs asks first (UX-24, SEC-6): [`Ask`] says what, and [`ConfirmDialog`] asks.

use std::path::PathBuf;

use dioxus::prelude::*;

/// What the bar shows for the amp.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PresetView {
    /// Every slot of the amp, with the name of the preset stored in it.
    pub slots: Vec<(u32, Option<String>)>,
    /// The preset the amp runs, as the app knows it (BEH-11).
    pub loaded: Option<(u32, String)>,
    pub modified: bool,
    /// Why the last action failed, said beside the running line until the next one.
    pub error: Option<String>,
    /// The amp is ONLINE: nothing can be done otherwise (BEH-10).
    pub online: bool,
}

/// A slot as the list and the messages write it: `03 · Show`.
pub fn slot_text(slot: u32, name: Option<&str>) -> String {
    format!("{slot:02} · {}", name.unwrap_or("Empty"))
}

impl PresetView {
    /// The entry of a slot in the list, with the mark of the loaded one.
    pub fn option_text(&self, slot: u32, name: Option<&str>) -> String {
        let text = slot_text(slot, name);
        match &self.loaded {
            Some((n, _)) if *n == slot && self.modified => format!("{text} (loaded, modified)"),
            Some((n, _)) if *n == slot => format!("{text} (loaded)"),
            _ => text,
        }
    }

    /// The line at the end of the bar: the preset that runs (UX-28).
    pub fn status(&self) -> String {
        match &self.loaded {
            Some((n, name)) if self.modified => format!("Running: {} · modified", slot_text(*n, Some(name))),
            Some((n, name)) => format!("Running: {}", slot_text(*n, Some(name))),
            None => "Running: no stored preset known".into(),
        }
    }
}

/// An action that mutes the outputs of an amp, waiting for the user's yes (UX-24).
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Recall { slot: u32, name: String },
    Store { slot: u32, name: String, replaces: Option<String> },
    Load { path: PathBuf },
    Link,
}

impl Ask {
    /// The question, naming the amplifier.
    pub fn title(&self, amp: &str) -> String {
        match self {
            Ask::Recall { slot, name } => format!("Recall preset {} on {amp}?", slot_text(*slot, Some(name))),
            Ask::Store { slot, name, .. } => {
                format!("Store the settings of {amp} in preset {}?", slot_text(*slot, Some(name)))
            }
            Ask::Load { path } => {
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                format!("Load the file {file} into {amp}?")
            }
            Ask::Link => format!("Link channels 1 and 2 of {amp}?"),
        }
    }

    /// What it does to the outputs: muted, and for recall, store and load, until the user unmutes
    /// them (SEC-6).
    pub fn text(&self, amp: &str) -> String {
        let muted = format!("Both outputs of {amp} are muted first and stay muted until you unmute them.");
        match self {
            Ask::Store { replaces: Some(old), .. } => format!("It replaces {old}. {muted}"),
            Ask::Link => format!(
                "Channel 2 takes every setting of channel 1, inputs and outputs. Both outputs of {amp} are \
                 muted while it is done, then put back as they were."
            ),
            _ => muted,
        }
    }

    /// The button that says yes.
    pub fn confirm(&self) -> &'static str {
        match self {
            Ask::Recall { .. } => "RECALL",
            Ask::Store { .. } => "STORE",
            Ask::Load { .. } => "LOAD",
            Ask::Link => "LINK",
        }
    }
}

/// The validation popup of a dangerous action (UX-24): the question, what happens to the outputs, and
/// the two answers. Escape is no.
#[component]
pub fn ConfirmDialog(
    amp: String,
    ask: Ask,
    #[props(default)] on_confirm: EventHandler<()>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let title = ask.title(&amp);
    rsx! {
        div {
            class: "modal-backdrop",
            onkeydown: move |event| {
                if event.key().to_string() == "Escape" {
                    on_cancel.call(());
                }
            },
            div { class: "modal", role: "alertdialog", "aria-modal": "true", "aria-label": "{title}",
                h3 { "{title}" }
                p { "{ask.text(&amp)}" }
                div { class: "modal-actions",
                    button { class: "button", autofocus: true, onclick: move |_| on_cancel.call(()), "CANCEL" }
                    button { class: "button confirm-yes", onclick: move |_| on_confirm.call(()), "{ask.confirm()}" }
                }
            }
        }
    }
}

/// The preset bar (BEH-37). The slot picked and the name typed are the bar's own; the actions go out
/// with them.
#[component]
pub fn PresetBar(
    view: PresetView,
    #[props(default)] on_recall: EventHandler<(u32, String)>,
    #[props(default)] on_store: EventHandler<(u32, String)>,
    #[props(default)] on_open: EventHandler<()>,
    #[props(default)] on_save_file: EventHandler<()>,
) -> Element {
    let first = view.loaded.as_ref().map(|(n, _)| *n).or(view.slots.first().map(|(n, _)| *n)).unwrap_or(1);
    let name_of = {
        let slots = view.slots.clone();
        move |slot: u32| slots.iter().find(|(n, _)| *n == slot).and_then(|(_, name)| name.clone())
    };
    let mut slot = use_signal(|| first);
    let mut draft = use_signal({
        let name_of = name_of.clone();
        move || name_of(first).unwrap_or_default()
    });
    let stored = name_of(slot());
    let online = view.online;
    let can_store = online && !draft().trim().is_empty();
    rsx! {
        section { class: "preset-bar", "aria-label": "Presets",
            label { class: "preset-title", r#for: "preset-slot", "PRESET" }
            select {
                id: "preset-slot",
                value: "{slot}",
                disabled: !online,
                onchange: {
                    let name_of = name_of.clone();
                    move |e: Event<FormData>| {
                        if let Ok(n) = e.value().parse::<u32>() {
                            slot.set(n);
                            draft.set(name_of(n).unwrap_or_default());
                        }
                    }
                },
                for (n, name) in view.slots.iter() {
                    option { value: "{n}", selected: *n == slot(), "{view.option_text(*n, name.as_deref())}" }
                }
            }
            input {
                class: "preset-name",
                r#type: "text",
                maxlength: "24",
                autocomplete: "off",
                placeholder: "Slot name",
                "aria-label": "Name to store the preset under",
                disabled: !online,
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
            }
            button {
                class: "button preset-load",
                disabled: !online || stored.is_none(),
                onclick: {
                    let stored = stored.clone();
                    move |_| {
                        if let Some(name) = stored.clone() {
                            on_recall.call((slot(), name));
                        }
                    }
                },
                "LOAD"
            }
            button {
                class: "button",
                disabled: !can_store,
                onclick: move |_| on_store.call((slot(), draft().trim().to_string())),
                "SAVE"
            }
            button { class: "button", disabled: !online, onclick: move |_| on_open.call(()), "OPEN FILE…" }
            button { class: "button", disabled: !online, onclick: move |_| on_save_file.call(()), "SAVE TO COMPUTER…" }
            span { class: "preset-status", role: "status", "aria-live": "polite",
                if let Some(error) = &view.error {
                    span { class: "preset-error", role: "alert", "{error}" }
                }
                "{view.status()}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> PresetView {
        PresetView {
            slots: vec![(1, Some("Factory default".into())), (2, None), (3, Some("Show".into()))],
            loaded: Some((1, "Factory default".into())),
            modified: false,
            error: None,
            online: true,
        }
    }

    // BEH-36, BEH-11, BEH-38: the slots by number and name, the loaded one marked, MODIFIED said
    #[test]
    fn the_list_and_the_running_line() {
        let mut v = view();
        assert_eq!(v.option_text(1, Some("Factory default")), "01 · Factory default (loaded)");
        assert_eq!(v.option_text(2, None), "02 · Empty");
        assert_eq!(v.status(), "Running: 01 · Factory default");
        v.modified = true;
        assert_eq!(v.option_text(1, Some("Factory default")), "01 · Factory default (loaded, modified)");
        assert_eq!(v.status(), "Running: 01 · Factory default · modified");
        v.error = Some("Not ONLINE".into());
        assert_eq!(v.status(), "Running: 01 · Factory default · modified", "the running preset stays");
        assert_eq!(PresetView::default().status(), "Running: no stored preset known");
    }

    // UX-24, SEC-6: every question names the amp and says the outputs are muted; recall, store and load
    // say they stay muted, linking that they are put back
    #[test]
    fn the_questions_name_the_amp_and_the_mutes() {
        let asks = [
            Ask::Recall { slot: 3, name: "Show".into() },
            Ask::Store { slot: 3, name: "Show".into(), replaces: Some("03 · Old".into()) },
            Ask::Load { path: PathBuf::from("/home/me/show.json") },
            Ask::Link,
        ];
        for ask in &asks {
            assert!(ask.title("FOH").contains("FOH"), "{ask:?}");
            assert!(ask.text("FOH").contains("outputs of FOH are muted"), "{ask:?}");
        }
        for ask in &asks[..3] {
            assert!(ask.text("FOH").contains("stay muted until you unmute them"), "{ask:?}");
        }
        assert!(asks[3].text("FOH").contains("put back as they were"));
        assert_eq!(asks[0].title("FOH"), "Recall preset 03 · Show on FOH?");
        assert!(asks[1].text("FOH").starts_with("It replaces 03 · Old."));
        assert_eq!(asks[2].title("FOH"), "Load the file show.json into FOH?");
        assert_eq!(asks.map(|a| a.confirm()), ["RECALL", "STORE", "LOAD", "LINK"]);
    }

    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // BEH-37: the bar has its actions; LOAD needs a stored preset, and nothing works offline (BEH-10)
    #[test]
    fn the_bar_and_its_actions() {
        let out = html(|| rsx! { PresetBar { view: view() } });
        for text in
            ["PRESET", "LOAD", "SAVE", "OPEN FILE…", "SAVE TO COMPUTER…", "Running: 01 · Factory default"]
        {
            assert!(out.contains(text), "{text}: {out}");
        }
        assert!(out.contains("01 · Factory default (loaded)") && out.contains("02 · Empty"));
        assert!(out.contains("value=\"Factory default\""), "the name of the slot to store under: {out}");
        assert!(!out.contains("disabled=true") && !out.contains("disabled=\"true\""), "{out}");

        let failed =
            html(|| rsx! { PresetBar { view: PresetView { error: Some("No answer".into()), ..view() } } });
        assert!(failed.contains("No answer") && failed.contains("Running: 01 · Factory default"), "{failed}");

        let off = html(|| rsx! { PresetBar { view: PresetView { online: false, ..view() } } });
        assert!(off.matches("disabled").count() >= 6, "{off}");
    }

    // UX-24: the popup is an alert dialog with the question and both answers
    #[test]
    fn the_popup_asks() {
        let out =
            html(|| rsx! { ConfirmDialog { amp: "FOH", ask: Ask::Recall { slot: 3, name: "Show".into() } } });
        assert!(out.contains("role=\"alertdialog\""), "{out}");
        assert!(out.contains("Recall preset 03 · Show on FOH?"));
        assert!(out.contains(">CANCEL<") && out.contains(">RECALL<"));
    }
}
