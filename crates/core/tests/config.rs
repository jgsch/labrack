use std::collections::BTreeSet;
use std::fs;

use labrack_core::{Error, Protocol, Source, Store, Switches};

const FULL: &str = r#"
[switches]
ipd = true
ipx = false

[[amp]]
name = "Main L"
protocol = "ipd"
ip = "10.0.0.5"
source = "auto"
connected = false
load_ohms = [8.0, 2.5]

[amp.ceilings]
max_output_gain_db = -6.0
max_limiter_dbu = 30.0

[[amp]]
name = "Sub"
protocol = "ipx"
ip = "10.0.0.6"
source = "manual"
connected = true
"#;

// BEH-40
#[test]
fn missing_file_is_an_empty_list_with_both_switches_on() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("labrack/config.toml")).unwrap();
    assert!(store.amps().is_empty());
    assert_eq!(store.switches(), Switches { ipd: true, ipx: true });
}

// BEH-40
#[test]
fn every_change_is_saved_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("labrack/config.toml");
    {
        let mut store = Store::open(&path).unwrap();
        let a = store.add_manual("", Protocol::Ipx, "10.0.0.1").unwrap();
        let b = store.add_manual("Rear", Protocol::Ipd, "10.0.0.2").unwrap();
        store.rename(a, "Front").unwrap();
        store.set_connected(b, false).unwrap();
        store.set_ip(b, "10.0.0.9").unwrap();
        store.set_switches(Switches { ipd: false, ipx: true }).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let amps = store.amps();
    assert_eq!(amps.len(), 2);
    assert_eq!(
        (amps[0].name.as_str(), amps[0].protocol, amps[0].ip.as_str()),
        ("Front", Protocol::Ipx, "10.0.0.1")
    );
    assert_eq!((amps[0].source, amps[0].connected), (Source::Manual, true));
    assert_eq!(
        (amps[1].name.as_str(), amps[1].protocol, amps[1].ip.as_str()),
        ("Rear", Protocol::Ipd, "10.0.0.9")
    );
    assert_eq!((amps[1].source, amps[1].connected), (Source::Manual, false));
    assert_eq!(store.switches(), Switches { ipd: false, ipx: true });
    assert_eq!(store.index(amps[1].id), Some(2));
}

// BEH-40
#[test]
fn remove_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut store = Store::open(&path).unwrap();
    let a = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    store.remove(a).unwrap();
    let again = Store::open(&path).unwrap();
    assert_eq!(again.amps().len(), 1);
    assert_eq!(again.amps()[0].name, "B");
}

// BEH-40
#[test]
fn a_rejected_intent_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut store = Store::open(&path).unwrap();
    store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let before = fs::read_to_string(&path).unwrap();
    store.add_manual("B", Protocol::Ipx, "10.0.0.1").unwrap_err();
    store.add_manual("B", Protocol::Ipx, "nope").unwrap_err();
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

// BEH-40
#[test]
fn load_per_output_and_ceilings_are_read_and_kept_when_the_list_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, FULL).unwrap();

    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.switches(), Switches { ipd: true, ipx: false });
    let main = &store.amps()[0];
    assert_eq!(main.name, "Main L");
    assert_eq!(main.source, Source::Auto);
    assert!(!main.connected);
    assert_eq!(main.load_ohms, [8.0, 2.5]);
    assert_eq!(main.ceilings.max_output_gain_db, Some(-6.0));
    assert_eq!(main.ceilings.max_limiter_dbu, Some(30.0));

    let sub = &store.amps()[1];
    assert_eq!(sub.load_ohms, [4.0, 4.0], "default load is 4 ohm");
    assert_eq!(sub.ceilings.max_output_gain_db, None);
    assert_eq!(sub.ceilings.max_limiter_dbu, None);

    let id = store.amps()[0].id;
    store.rename(id, "Main left").unwrap();
    let again = Store::open(&path).unwrap();
    assert_eq!(again.amps()[0].name, "Main left");
    assert_eq!(again.amps()[0].load_ohms, [8.0, 2.5]);
    assert_eq!(again.amps()[0].ceilings.max_limiter_dbu, Some(30.0));
    assert_eq!(again.amps()[1].load_ohms, [4.0, 4.0]);
}

// BEH-32: the link of an IPD is the app's, so it is kept in the config (no link in an old file)
#[test]
fn the_links_the_app_keeps_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, FULL).unwrap();
    let mut store = Store::open(&path).unwrap();
    let id = store.amps()[0].id;
    assert_eq!(store.amps()[0].links, [false, false], "none in a file without them");

    store.set_link(id, 1, true).unwrap();

    assert_eq!(Store::open(&path).unwrap().amps()[0].links, [false, true]);
    store.set_link(id, 1, false).unwrap();
    assert_eq!(Store::open(&path).unwrap().amps()[0].links, [false, false]);
}

// BEH-40, BEH-7
#[test]
fn the_loaded_preset_is_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut store = Store::open(&path).unwrap();
    let id = store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    store.set_preset(id, Some(labrack_core::Preset { number: 4, name: "Secret".into() })).unwrap();
    store.rename(id, "B").unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains("Secret"));
    assert_eq!(Store::open(&path).unwrap().amps()[0].preset, None);
}

// BEH-40, BEH-8
#[test]
fn an_invalid_ip_in_the_file_is_kept_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[[amp]]\nname = \"A\"\nprotocol = \"ipx\"\nip = \"10.0.\"\nsource = \"manual\"\nconnected = true\n",
    )
    .unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.amps()[0].ip, "10.0.");
    assert!(!store.amps()[0].ip_is_valid());
}

// BEH-40
#[test]
fn an_unreadable_file_is_an_error_and_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "this is [not toml").unwrap();
    let err = Store::open(&path).err().expect("open must fail");
    assert!(matches!(err, Error::Parse(_)), "{err:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "this is [not toml");
}

// BEH-40
#[test]
fn a_file_with_an_unknown_protocol_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[amp]]\nname = \"A\"\nprotocol = \"dante\"\nip = \"10.0.0.1\"\nsource = \"auto\"\nconnected = true\n")
        .unwrap();
    assert!(matches!(Store::open(&path).err(), Some(Error::Parse(_))));
}

// BEH-40
#[test]
fn saving_leaves_no_temporary_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/config.toml");
    let mut store = Store::open(&path).unwrap();
    store.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let files: Vec<_> =
        fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files, ["config.toml"]);
}

// BEH-40
#[test]
fn the_default_path_is_labrack_config_toml_in_the_config_dir() {
    if let Some(path) = Store::default_path() {
        assert!(path.ends_with("labrack/config.toml"), "{path:?}");
    }
}

fn keys(value: &toml::Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                out.insert(path.clone());
                keys(v, &path, out);
            }
        }
        toml::Value::Array(items) => items.iter().for_each(|v| keys(v, prefix, out)),
        _ => {}
    }
}

// BEH-40, BEH-16, SEC-8
#[test]
fn the_file_holds_no_password_and_no_dsp_setting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, FULL).unwrap();
    let mut store = Store::open(&path).unwrap();
    let id = store.amps()[0].id;
    store.set_preset(id, Some(labrack_core::Preset { number: 1, name: "P".into() })).unwrap();
    store.rename(id, "Main L").unwrap();

    let saved: toml::Table = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut found = BTreeSet::new();
    keys(&toml::Value::Table(saved), "", &mut found);
    let allowed: BTreeSet<String> = [
        "switches",
        "switches.ipd",
        "switches.ipx",
        "amp",
        "amp.name",
        "amp.protocol",
        "amp.ip",
        "amp.source",
        "amp.connected",
        "amp.load_ohms",
        "amp.links",
        "amp.ceilings",
        "amp.ceilings.max_output_gain_db",
        "amp.ceilings.max_limiter_dbu",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let extra: Vec<_> = found.difference(&allowed).collect();
    assert!(extra.is_empty(), "keys outside the allowed list: {extra:?}");
    assert!(found.contains("amp.ceilings.max_limiter_dbu"));
}
