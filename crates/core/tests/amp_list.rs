use labrack_core::{Error, Preset, Protocol, Source, Store};

fn names(store: &Store) -> Vec<&str> {
    store.amps().iter().map(|a| a.name.as_str()).collect()
}

// BEH-7
#[test]
fn amp_has_name_protocol_ip_source_and_no_preset_yet() {
    let mut store = Store::in_memory();
    let id = store.add_manual("Stage left", Protocol::Ipd, "10.0.0.5").unwrap();
    let amp = store.get(id).unwrap();
    assert_eq!(amp.name, "Stage left");
    assert_eq!(amp.protocol, Protocol::Ipd);
    assert_eq!(amp.ip, "10.0.0.5");
    assert_eq!(amp.source, Source::Manual);
    assert_eq!(amp.preset, None);
}

// BEH-7
#[test]
fn loaded_preset_can_be_set_and_cleared() {
    let mut store = Store::in_memory();
    let id = store.add_manual("", Protocol::Ipx, "10.0.0.5").unwrap();
    let preset = Preset { number: 3, name: "Walk-in".into() };
    store.set_preset(id, Some(preset.clone())).unwrap();
    assert_eq!(store.get(id).unwrap().preset, Some(preset));
    store.set_preset(id, None).unwrap();
    assert_eq!(store.get(id).unwrap().preset, None);
}

// BEH-7
#[test]
fn index_is_the_position_from_1_and_is_renumbered_on_remove() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = store.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    let c = store.add_manual("C", Protocol::Ipx, "10.0.0.3").unwrap();
    assert_eq!([a, b, c].map(|id| store.index(id)), [Some(1), Some(2), Some(3)]);
    store.remove(a).unwrap();
    assert_eq!([a, b, c].map(|id| store.index(id)), [None, Some(1), Some(2)]);
}

// BEH-20
#[test]
fn empty_name_defaults_to_amp_n() {
    let mut store = Store::in_memory();
    store.add_manual("", Protocol::Ipx, "10.0.0.1").unwrap();
    store.add_manual("   ", Protocol::Ipd, "10.0.0.2").unwrap();
    assert_eq!(names(&store), ["Amp 1", "Amp 2"]);
}

// BEH-20
#[test]
fn default_name_is_the_smallest_free_number() {
    let mut store = Store::in_memory();
    let first = store.add_manual("", Protocol::Ipx, "10.0.0.1").unwrap();
    store.add_manual("", Protocol::Ipx, "10.0.0.2").unwrap();
    store.remove(first).unwrap();
    store.add_manual("", Protocol::Ipx, "10.0.0.3").unwrap();
    assert_eq!(names(&store), ["Amp 2", "Amp 1"]);
    store.add_manual("", Protocol::Ipx, "10.0.0.4").unwrap();
    assert_eq!(names(&store), ["Amp 2", "Amp 1", "Amp 3"]);
}

// BEH-20
#[test]
fn given_name_is_trimmed() {
    let mut store = Store::in_memory();
    let id = store.add_manual("  Delay ring ", Protocol::Ipx, "10.0.0.1").unwrap();
    assert_eq!(store.get(id).unwrap().name, "Delay ring");
}

// BEH-21
#[test]
fn add_rejects_text_that_is_not_ipv4() {
    let mut store = Store::in_memory();
    for ip in ["", "abc", "10.0.0", "10.0.0.256", "10.0.0.1.2", "::1", "10.0.0.1:50168", "010.0.0.1"] {
        let err = store.add_manual("x", Protocol::Ipx, ip).unwrap_err();
        assert!(matches!(err, Error::InvalidIp), "{ip:?} gave {err:?}");
    }
    assert!(store.amps().is_empty());
}

// BEH-21
#[test]
fn add_rejects_an_ip_already_in_the_list_whatever_the_protocol() {
    let mut store = Store::in_memory();
    store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let err = store.add_manual("B", Protocol::Ipd, " 10.0.0.1 ").unwrap_err();
    assert!(matches!(err, Error::DuplicateIp), "{err:?}");
    assert_eq!(names(&store), ["A"]);
}

// BEH-21
#[test]
fn errors_have_text_for_the_ui() {
    assert_eq!(Error::InvalidIp.to_string(), "Not a valid IPv4 address");
    assert_eq!(Error::DuplicateIp.to_string(), "An amplifier with this IP is already in the list");
    assert_eq!(Error::EmptyName.to_string(), "The name cannot be empty");
}

// BEH-21
#[test]
fn a_rejected_add_does_not_use_up_a_default_name() {
    let mut store = Store::in_memory();
    store.add_manual("", Protocol::Ipx, "nope").unwrap_err();
    let id = store.add_manual("", Protocol::Ipx, "10.0.0.1").unwrap();
    assert_eq!(store.get(id).unwrap().name, "Amp 1");
}

// BEH-22
#[test]
fn manual_add_has_source_manual_and_starts_connected() {
    let mut store = Store::in_memory();
    let id = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let amp = store.get(id).unwrap();
    assert_eq!(amp.source, Source::Manual);
    assert!(amp.connected);
}

// BEH-19, BEH-20
#[test]
fn a_discovered_amp_is_added_as_auto_with_a_default_name() {
    let mut store = Store::in_memory();
    store.add_manual("Mine", Protocol::Ipx, "10.0.0.1").unwrap();
    let id = store.add_discovered(Protocol::Ipd, "10.0.0.5".parse().unwrap()).unwrap().unwrap();
    let amp = store.get(id).unwrap();
    assert_eq!((amp.name.as_str(), amp.protocol, amp.ip.as_str()), ("Amp 1", Protocol::Ipd, "10.0.0.5"));
    assert_eq!((amp.source, amp.connected), (Source::Auto, true));
    assert_eq!(store.index(id), Some(2));
}

// BEH-19
#[test]
fn a_known_ip_is_never_added_twice() {
    let mut store = Store::in_memory();
    store.add_manual("Mine", Protocol::Ipx, "10.0.0.1").unwrap();
    let ip = "10.0.0.1".parse().unwrap();
    assert_eq!(store.add_discovered(Protocol::Ipx, ip).unwrap(), None);
    assert_eq!(store.add_discovered(Protocol::Ipd, ip).unwrap(), None, "whatever the protocol");

    let other = "10.0.0.2".parse().unwrap();
    assert!(store.add_discovered(Protocol::Ipx, other).unwrap().is_some());
    assert_eq!(store.add_discovered(Protocol::Ipx, other).unwrap(), None);
    assert_eq!(store.amps().len(), 2);
}

// BEH-19: an amp the user edited to another address does not make the old address "known"
#[test]
fn an_ip_that_is_no_longer_in_the_list_can_be_discovered() {
    let mut store = Store::in_memory();
    let id = store.add_manual("Mine", Protocol::Ipx, "10.0.0.1").unwrap();
    store.set_ip(id, "10.0.0.9").unwrap();
    assert!(store.add_discovered(Protocol::Ipx, "10.0.0.1".parse().unwrap()).unwrap().is_some());
}

// BEH-17
#[test]
fn switches_say_which_protocols_are_searched() {
    use labrack_core::Switches;
    let on = |ipd, ipx| Switches { ipd, ipx };
    assert!(on(true, false).allows(Protocol::Ipd) && !on(true, false).allows(Protocol::Ipx));
    assert!(!on(false, true).allows(Protocol::Ipd) && on(false, true).allows(Protocol::Ipx));
    assert!(on(false, false).none() && !on(true, false).none());
}

// BEH-23
#[test]
fn rename_changes_the_name() {
    let mut store = Store::in_memory();
    let id = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.rename(id, " Main L ").unwrap();
    assert_eq!(store.get(id).unwrap().name, "Main L");
}

// BEH-23
#[test]
fn rename_to_empty_is_rejected_and_the_old_name_stays() {
    let mut store = Store::in_memory();
    let id = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    for name in ["", "  \t"] {
        assert!(matches!(store.rename(id, name).unwrap_err(), Error::EmptyName));
    }
    assert_eq!(store.get(id).unwrap().name, "A");
}

// BEH-23
#[test]
fn remove_drops_the_amp_and_keeps_the_order_of_the_others() {
    let mut store = Store::in_memory();
    store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = store.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    store.add_manual("C", Protocol::Ipx, "10.0.0.3").unwrap();
    store.remove(b).unwrap();
    assert_eq!(names(&store), ["A", "C"]);
    assert!(store.get(b).is_none());
}

// BEH-23
#[test]
fn removed_ip_can_be_added_again() {
    let mut store = Store::in_memory();
    let id = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.remove(id).unwrap();
    store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
}

// BEH-23
#[test]
fn intents_on_a_removed_amp_fail_without_touching_the_others() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.remove(a).unwrap();
    assert!(matches!(store.rename(a, "x").unwrap_err(), Error::UnknownAmp));
    assert!(matches!(store.remove(a).unwrap_err(), Error::UnknownAmp));
    assert!(matches!(store.set_ip(a, "10.0.0.9").unwrap_err(), Error::UnknownAmp));
    assert!(matches!(store.set_connected(a, false).unwrap_err(), Error::UnknownAmp));
    assert!(store.amps().is_empty());
}

// BEH-10, BEH-21
#[test]
fn edited_ip_may_be_invalid_text_but_not_a_duplicate() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();

    store.set_ip(a, "10.0.0.7").unwrap();
    assert_eq!(store.get(a).unwrap().ip, "10.0.0.7");

    store.set_ip(a, "10.0.0.").unwrap();
    assert_eq!(store.get(a).unwrap().ip, "10.0.0.");
    assert!(!store.get(a).unwrap().ip_is_valid());

    assert!(matches!(store.set_ip(a, "10.0.0.2").unwrap_err(), Error::DuplicateIp));
    assert_eq!(store.get(a).unwrap().ip, "10.0.0.", "old IP stays");
}

// BEH-10, BEH-21
#[test]
fn setting_an_amps_own_ip_again_is_not_a_duplicate() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.set_ip(a, "10.0.0.1").unwrap();
}

// BEH-9
#[test]
fn connected_flag_can_be_cleared_and_set() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.set_connected(a, false).unwrap();
    assert!(!store.get(a).unwrap().connected);
    store.set_connected(a, true).unwrap();
    assert!(store.get(a).unwrap().connected);
}

// BEH-39
#[test]
fn there_is_one_list_every_view_reads() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = store.add_manual("B", Protocol::Ipd, "10.0.0.2").unwrap();
    store.rename(a, "Front").unwrap();
    store.remove(b).unwrap();
    store.add_manual("C", Protocol::Ipd, "10.0.0.3").unwrap();
    // Amplifiers, Search and Mix & EQ all read `amps()`; a change shows in it at once.
    assert_eq!(names(&store), ["Front", "C"]);
    assert_eq!(store.index(a), Some(1));
}

// BEH-7, BEH-39
#[test]
fn amp_ids_are_not_reused() {
    let mut store = Store::in_memory();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.remove(a).unwrap();
    let b = store.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    assert_ne!(a, b);
}

// BEH-41
#[test]
fn a_new_store_has_no_amps() {
    assert!(Store::in_memory().amps().is_empty());
}
