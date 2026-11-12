//! The login dialog of an IPD (UX-31, BEH-15, BEH-16): one password box, LOGIN and CANCEL. The password
//! lives in this component only, is handed on once and cleared.

use dioxus::prelude::*;

#[component]
pub fn LoginDialog(
    amp: String,
    #[props(default)] on_login: EventHandler<String>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let mut password = use_signal(String::new);
    let ready = !password.read().is_empty();
    let mut submit = move || {
        let typed = std::mem::take(&mut *password.write());
        if !typed.is_empty() {
            on_login.call(typed);
        }
    };
    rsx! {
        div {
            class: "modal-backdrop",
            onkeydown: move |event| {
                if event.key().to_string() == "Escape" {
                    password.write().clear();
                    on_cancel.call(());
                }
            },
            div { class: "modal", role: "dialog", "aria-modal": "true", "aria-label": "Log in to {amp}",
                h3 { "Log in to {amp}" }
                p { "Type the password of the user level you need. The app does not keep it." }
                input {
                    class: "password",
                    r#type: "password",
                    maxlength: "8",
                    autocomplete: "off",
                    autofocus: true,
                    "aria-label": "Password of {amp}",
                    value: "{password}",
                    oninput: move |event| password.set(event.value()),
                    onkeydown: move |event| {
                        if event.key().to_string() == "Enter" {
                            submit();
                        }
                    },
                }
                div { class: "modal-actions",
                    button {
                        class: "button",
                        onclick: move |_| {
                            password.write().clear();
                            on_cancel.call(());
                        },
                        "CANCEL"
                    }
                    button { class: "button confirm-yes", disabled: !ready, onclick: move |_| submit(), "LOGIN" }
                }
            }
        }
    }
}

/// The dialog that changes the password an IPD unlocks with (UX-33, BEH-48): the new password and the same again,
/// CHANGE and CANCEL. CHANGE waits until both are filled and equal; nothing is kept once it is pressed or the
/// dialog closes.
#[component]
pub fn PasswordDialog(
    amp: String,
    #[props(default)] on_change: EventHandler<String>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let mut first = use_signal(String::new);
    let mut again = use_signal(String::new);
    let ready = !first.read().is_empty() && *first.read() == *again.read();
    let mismatch = !again.read().is_empty() && *first.read() != *again.read();
    let mut clear = move || {
        first.write().clear();
        again.write().clear();
    };
    let mut submit = move || {
        let ok = !first.read().is_empty() && *first.read() == *again.read();
        let typed = std::mem::take(&mut *first.write());
        again.write().clear();
        if ok {
            on_change.call(typed);
        }
    };
    rsx! {
        div {
            class: "modal-backdrop",
            onkeydown: move |event| {
                if event.key().to_string() == "Escape" {
                    clear();
                    on_cancel.call(());
                }
            },
            div { class: "modal", role: "dialog", "aria-modal": "true", "aria-label": "New password of {amp}",
                h3 { "New password of {amp}" }
                p { "1 to 8 characters. It unlocks the amp from the front panel and in LAB.RACK. The app does not keep it." }
                input {
                    class: "password",
                    r#type: "password",
                    maxlength: "8",
                    autocomplete: "off",
                    autofocus: true,
                    "aria-label": "New password",
                    value: "{first}",
                    oninput: move |event| first.set(event.value()),
                }
                input {
                    class: "password",
                    r#type: "password",
                    maxlength: "8",
                    autocomplete: "off",
                    "aria-label": "New password again",
                    value: "{again}",
                    oninput: move |event| again.set(event.value()),
                    onkeydown: move |event| {
                        if event.key().to_string() == "Enter" {
                            submit();
                        }
                    },
                }
                if mismatch {
                    p { class: "error-text", role: "alert", "The two passwords are not the same" }
                }
                div { class: "modal-actions",
                    button {
                        class: "button",
                        onclick: move |_| {
                            clear();
                            on_cancel.call(());
                        },
                        "CANCEL"
                    }
                    button { class: "button confirm-yes", disabled: !ready, onclick: move |_| submit(), "CHANGE" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(page: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(page);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    // UX-33, BEH-48: two hidden boxes, CHANGE waits for both
    #[test]
    fn the_password_dialog_asks_twice() {
        let out = html(|| rsx! { PasswordDialog { amp: "FOH" } });
        assert!(out.contains("New password of FOH"), "{out}");
        assert_eq!(out.matches("type=\"password\"").count(), 2, "{out}");
        assert_eq!(out.matches("maxlength=\"8\"").count(), 2, "{out}");
        let change = out.split(">CANCEL<").nth(1).unwrap();
        assert!(
            out.contains("disabled") && change.contains(">CHANGE<"),
            "CHANGE is disabled while empty: {out}"
        );
        assert!(!out.contains("not the same"), "{out}");
    }

    // UX-31, BEH-16: a hidden password of at most 8 characters, no suggestions; LOGIN waits for a character
    #[test]
    fn the_dialog_asks_for_a_hidden_password() {
        let out = html(|| rsx! { LoginDialog { amp: "FOH" } });
        assert!(out.contains("Log in to FOH"), "{out}");
        assert!(out.contains("type=\"password\"") && out.contains("maxlength=\"8\""), "{out}");
        assert!(out.contains("autocomplete=\"off\""), "{out}");
        let login = out.split(">CANCEL<").nth(1).unwrap();
        assert!(
            out.contains("disabled") && login.contains(">LOGIN<"),
            "LOGIN is disabled while empty: {out}"
        );
    }
}
