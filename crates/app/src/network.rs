//! The NETWORK dialog of a Search row (UX-34, BEH-50): AUTOMATIC, or a fixed address and mask, then APPLY.

use dioxus::prelude::*;
use labgruppen::Network;

/// What the boxes ask for: automatic keeps the mask the amp has (it does not use it); a fixed address is checked
/// as the core will check it.
pub fn network_from(automatic: bool, ip: &str, mask: &str, current: Network) -> Result<Network, String> {
    if automatic {
        return Ok(Network { ip: None, mask: current.mask });
    }
    let ip =
        ip.trim().parse().map_err(|_| "The address is not an IPv4 address like 192.168.1.50".to_string())?;
    let mask = mask.trim().parse().map_err(|_| "The mask is not like 255.255.255.0".to_string())?;
    labrack_core::check_network(ip, mask).map_err(|e| e.to_string())?;
    Ok(Network { ip: Some(ip), mask })
}

#[component]
pub fn NetworkDialog(
    amp: String,
    /// What the amp is set to now.
    current: Network,
    #[props(default)] on_apply: EventHandler<Network>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let mut automatic = use_signal(|| current.ip.is_none());
    let mut ip = use_signal(|| current.ip.map(|ip| ip.to_string()).unwrap_or_default());
    let mut mask = use_signal(|| current.mask.to_string());
    let asked = network_from(automatic(), &ip.read(), &mask.read(), current);
    let error = asked.as_ref().err().cloned();
    let fixed = !automatic();
    rsx! {
        div {
            class: "modal-backdrop",
            onkeydown: move |event| {
                if event.key().to_string() == "Escape" {
                    on_cancel.call(());
                }
            },
            div { class: "modal", role: "dialog", "aria-modal": "true", "aria-label": "Network of {amp}",
                h3 { "Network of {amp}" }
                label { class: "modal-check",
                    input {
                        r#type: "checkbox",
                        checked: automatic(),
                        "aria-label": "Automatic address",
                        onchange: move |event| automatic.set(event.checked()),
                    }
                    "AUTOMATIC (DHCP)"
                }
                label { class: "field",
                    "IP ADDRESS"
                    input {
                        r#type: "text",
                        inputmode: "decimal",
                        autocomplete: "off",
                        disabled: !fixed,
                        "aria-label": "Address",
                        value: "{ip}",
                        oninput: move |event| ip.set(event.value()),
                    }
                }
                label { class: "field",
                    "MASK"
                    input {
                        r#type: "text",
                        inputmode: "decimal",
                        autocomplete: "off",
                        disabled: !fixed,
                        "aria-label": "Mask",
                        value: "{mask}",
                        oninput: move |event| mask.set(event.value()),
                    }
                }
                p { "The amp restarts its network. LAB.RACK follows it to a fixed address; find an automatic one with SCAN NOW." }
                if let Some(error) = error {
                    p { class: "error-text", role: "alert", "{error}" }
                }
                div { class: "modal-actions",
                    button { class: "button", onclick: move |_| on_cancel.call(()), "CANCEL" }
                    button {
                        class: "button confirm-yes",
                        disabled: asked.is_err(),
                        onclick: move |_| {
                            if let Ok(network) = asked {
                                on_apply.call(network);
                            }
                        },
                        "APPLY"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(ip: Option<&str>, mask: &str) -> Network {
        Network { ip: ip.map(|ip| ip.parse().unwrap()), mask: mask.parse().unwrap() }
    }

    // BEH-50: automatic keeps the amp's mask; a fixed address is checked as the core checks it
    #[test]
    fn the_boxes_make_a_network() {
        let current = net(Some("192.168.1.183"), "255.255.255.0");
        assert_eq!(network_from(true, "", "", current), Ok(net(None, "255.255.255.0")));
        assert_eq!(
            network_from(false, " 192.168.1.50 ", "255.255.0.0", current),
            Ok(net(Some("192.168.1.50"), "255.255.0.0"))
        );
        for (ip, mask) in
            [("192.168.1.50", "0.0.0.0"), ("192.168.1.255", "255.255.255.0"), ("x", "255.0.0.0")]
        {
            assert!(network_from(false, ip, mask, current).is_err(), "{ip}/{mask}");
        }
    }

    // UX-34: the dialog is filled with what the amp has; APPLY waits for a usable address
    #[test]
    fn the_dialog_shows_the_amp_settings() {
        let mut dom = VirtualDom::new(|| {
            rsx! { NetworkDialog { amp: "FOH", current: net(Some("192.168.1.183"), "255.255.255.0") } }
        });
        dom.rebuild_in_place();
        let out = dioxus_ssr::render(&dom);
        assert!(out.contains("Network of FOH"), "{out}");
        assert!(out.contains("value=\"192.168.1.183\"") && out.contains("value=\"255.255.255.0\""), "{out}");
        assert!(out.contains("AUTOMATIC (DHCP)") && out.contains("restarts its network"), "{out}");
        let apply = &out[out.rfind("<button").unwrap()..];
        assert!(apply.contains(">APPLY<") && !apply.contains("disabled"), "{apply}");

        let mut dom = VirtualDom::new(|| {
            rsx! { NetworkDialog { amp: "FOH", current: net(Some("192.168.1.183"), "0.0.0.0") } }
        });
        dom.rebuild_in_place();
        let out = dioxus_ssr::render(&dom);
        let apply = &out[out.rfind("<button").unwrap()..];
        assert!(apply.contains("disabled"), "the amp's own mask of 0.0.0.0 is not usable: {apply}");
        assert!(out.contains("role=\"alert\""), "{out}");
    }
}
