//! IPD user levels and login (BEH-13 to BEH-16, SEC-8).

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::IpdAmp;
use labgruppen::amps::ipd::client::Options;
use labgruppen::amps::ipd::protocol::{M_CHECK_PASSWORD, M_CHECK_PIN, M_PIN, M_SET_PASSWORD};
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::{Amp, Value};
use labrack_core::{AmpId, Error, Hub, Protocol, Side, Status, Store, Unit, Update};
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

const INPUT: u32 = 0x1;
const OUTPUT: u32 = 0x2;
const LOAD: u32 = 0x4;
const SAVE: u32 = 0x8;
const ALL: u32 = 0xFFFF_FFFF;
const USER: i64 = 4;
const LOCKED: i64 = 5;

/// A simulated IPD at `level` with `rights`, its outputs unmuted, and a hub ONLINE with it.
async fn ipd_at(level: i64, rights: u32) -> (Hub, AmpId, SimServer) {
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    unmute(&sim).await;
    sim.set_access(level, rights);
    let (hub, local) = hub();
    local.route("10.0.0.1", sim.addr.port());
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    (hub, id, sim)
}

/// The amp is muted when it starts; another controller unmutes it, as a person at the amp could.
async fn unmute(sim: &SimServer) {
    let options = Options { mk2: None, timeout: Duration::from_secs(5) };
    let amp = IpdAmp::connect_with("127.0.0.1", sim.addr.port(), options).await.unwrap();
    for output in ["out1.mute", "out2.mute"] {
        amp.set(&param(output), Value::Bool(false)).await.unwrap();
    }
    amp.close().await;
}

/// The next `Update::Access` of `id`, within a second.
async fn next_access(rx: &mut broadcast::Receiver<Update>, id: AmpId) -> Option<()> {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(Update::Access(i)) if i == id => return,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("{e}"),
            }
        }
    };
    timeout(Duration::from_secs(1), wait).await.ok()
}

fn user(hub: &Hub, id: AmpId) -> String {
    hub.access(id).expect("the access is known while ONLINE").user
}

// BEH-13: the level is read when the amp goes ONLINE and is gone with the connection
#[tokio::test(flavor = "multi_thread")]
async fn the_user_level_is_shown_while_online() {
    let (hub, id, sim) = ipd_at(LOCKED, 0).await;
    let a = hub.access(id).unwrap();
    assert_eq!((a.user.as_str(), a.locked, a.needs_login()), ("Locked", true, true));
    assert_eq!(a.rights.raw, 0);

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert_eq!(hub.access(id), None, "nothing is known of an amp that is not ONLINE");

    // what the amp has when it is reached again, not what it had
    sim.set_access(USER, ALL);
    hub.connect(id).unwrap();
    wait_loaded(&hub, id).await;
    let a = hub.access(id).unwrap();
    assert_eq!((a.user.as_str(), a.locked, a.needs_login()), ("User", false, false));
    let gone = hub.add_manual("Gone", Protocol::Ipd, "10.0.0.9").unwrap();
    hub.remove(gone).unwrap();
    assert_eq!(hub.access(gone), None);
}

// BEH-13: an IPX has no user levels: nothing to show, nothing refused, no login
#[tokio::test(flavor = "multi_thread")]
async fn an_ipx_has_no_levels() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    assert_eq!(hub.access(id), None);
    hub.set(id, param("out1.gain"), f(-3.0)).unwrap();
    assert!(matches!(hub.login(id, "Password").await, Err(Error::Amp(labgruppen::Error::Unsupported(_)))));
}

// BEH-14: a control the rights do not allow is refused, and nothing is sent for it
#[tokio::test(flavor = "multi_thread")]
async fn controls_the_rights_do_not_allow_are_refused() {
    let (hub, id, sim) = ipd_at(USER, INPUT).await;
    let sent = sim.writes().len();
    let denied = |got: Result<(), Error>| matches!(got, Err(Error::AccessDenied(who)) if who == "User");

    // the input channels are allowed, the outputs are not
    hub.set(id, param("in1.gain"), f(-6.0)).unwrap();
    wait_value(&hub, id, "in1.gain", f(-6.0)).await;
    for name in ["out1.gain", "out2.delay", "out1.eq.0.gain", "out1.xover.hp.freq", "out2.eq.1.freq"] {
        assert!(denied(hub.set(id, param(name), f(0.0))), "{name}");
    }
    assert!(denied(hub.set_limiter(id, 0, Unit::Watt, 100.0)), "output limiter");
    assert!(denied(hub.set_link(id, Side::Outputs, true).await), "output link");
    assert!(denied(hub.set_mute(id, 0, false)), "unmuting an output");

    // no right to load or save presets
    assert!(denied(hub.recall_preset(id, 2).await), "recall");
    assert!(denied(hub.store_preset(id, 5, "Stage").await), "store");
    let (_dir, path) = temp_file("settings.json");
    hub.save_file(id, &path).await.unwrap();
    assert!(denied(hub.load_file(id, &path).await.map(|_| ())), "file load");

    sleep(Duration::from_millis(300)).await;
    let writes = sim.writes()[sent..].to_vec();
    assert_eq!(writes.len(), 1, "only the allowed gain: {writes:?}");
    assert_eq!(hub.value(id, &param("out1.mute")), Some(Value::Bool(false)));
}

fn temp_file(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    (dir, path)
}

// BEH-14: each right opens what it names, and only that
#[tokio::test(flavor = "multi_thread")]
async fn each_right_opens_its_own_controls() {
    // outputs only
    let (hub, id, _sim) = ipd_at(USER, OUTPUT).await;
    hub.set(id, param("out1.gain"), f(-4.0)).unwrap();
    wait_value(&hub, id, "out1.gain", f(-4.0)).await;
    hub.set_limiter(id, 0, Unit::Watt, 100.0).unwrap();
    assert!(matches!(hub.set(id, param("in1.gain"), f(0.0)), Err(Error::AccessDenied(_))));
    assert!(matches!(hub.set_link(id, Side::Inputs, true).await, Err(Error::AccessDenied(_))));
    drop((hub, _sim));

    // presets
    let (hub, id, sim) = ipd_at(USER, LOAD | SAVE).await;
    assert!(matches!(hub.set(id, param("in1.gain"), f(0.0)), Err(Error::AccessDenied(_))));
    hub.recall_preset(id, 2).await.unwrap();
    wait_loaded(&hub, id).await; // the state is read again after a recall
    hub.store_preset(id, 5, "Stage").await.unwrap();
    assert_eq!(ipd_value(&sim, "out1.mute").await, Value::Bool(true), "and they leave the outputs muted");

    // a file writes both kinds of channel, so it needs both rights
    let (_dir, path) = temp_file("settings.json");
    hub.save_file(id, &path).await.unwrap();
    assert!(matches!(hub.load_file(id, &path).await, Err(Error::AccessDenied(_))));
    let (hub, id, _sim) = ipd_at(USER, INPUT | OUTPUT).await;
    hub.load_file(id, &path).await.unwrap();
}

// SEC-6: a muted output is never the dangerous state, so MUTE is not held back by the rights; the amp
// has the last word. UNMUTE is.
#[tokio::test(flavor = "multi_thread")]
async fn muting_is_always_tried_and_unmuting_needs_the_right() {
    let (hub, id, sim) = ipd_at(LOCKED, 0).await;
    wait_value(&hub, id, "out1.mute", Value::Bool(false)).await;

    hub.set_mute(id, 0, true).unwrap();
    wait_value(&hub, id, "out1.mute", Value::Bool(true)).await;
    assert!(matches!(hub.set_mute(id, 0, false), Err(Error::AccessDenied(who)) if who == "Locked"));
    assert_eq!(hub.mute_all(), 1);
    wait_value(&hub, id, "out2.mute", Value::Bool(true)).await;
    // UNMUTE ALL reaches the amp as nothing: both outputs refuse
    assert_eq!(hub.unmute_all(), 1);

    sleep(Duration::from_millis(300)).await;
    for output in ["out1.mute", "out2.mute"] {
        assert_eq!(hub.value(id, &param(output)), Some(Value::Bool(true)), "{output}");
        assert_eq!(ipd_value(&sim, output).await, Value::Bool(true), "{output} on the amp");
    }
}

// BEH-15: a login takes the level of the password and the rights come with it
#[tokio::test(flavor = "multi_thread")]
async fn a_login_opens_what_the_level_may_do() {
    let (hub, id, sim) = ipd_at(LOCKED, 0).await;
    let mut updates = hub.subscribe();
    assert!(matches!(hub.set(id, param("in1.gain"), f(-2.0)), Err(Error::AccessDenied(_))));

    let a = hub.login(id, "Password").await.unwrap();
    assert_eq!((a.user.as_str(), a.needs_login()), ("User", false));
    assert_eq!(hub.access(id), Some(a));
    assert!(next_access(&mut updates, id).await.is_some(), "the views are told");

    hub.set(id, param("in1.gain"), f(-2.0)).unwrap();
    wait_value(&hub, id, "in1.gain", f(-2.0)).await;
    assert_eq!(
        sim.writes().iter().filter(|k| k.member_id == M_CHECK_PASSWORD).count(),
        1,
        "the password, sent once"
    );
}

// BEH-15: a login that leaves a right missing still needs one
#[tokio::test(flavor = "multi_thread")]
async fn a_level_with_a_right_missing_still_offers_a_login() {
    let (hub, id, sim) = ipd_at(LOCKED, 0).await;
    sim.set_login_rights(INPUT | OUTPUT);
    let a = hub.login(id, "Password").await.unwrap();
    assert_eq!((a.user.as_str(), a.locked, a.needs_login()), ("User", false, true));
    assert!(matches!(hub.recall_preset(id, 2).await, Err(Error::AccessDenied(who)) if who == "User"));
    hub.set(id, param("out1.gain"), f(-2.0)).unwrap();
}

// BEH-15: the amp does not say a password was wrong; the level stays and nothing is announced
#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_password_changes_nothing() {
    let (hub, id, _sim) = ipd_at(LOCKED, 0).await;
    let mut updates = hub.subscribe();
    let a = hub.login(id, "Wrong123").await.unwrap();
    assert_eq!((a.user.as_str(), a.locked), ("Locked", true));
    assert_eq!(user(&hub, id), "Locked");
    assert!(next_access(&mut updates, id).await.is_none());
    assert!(matches!(hub.set(id, param("in1.gain"), f(-2.0)), Err(Error::AccessDenied(_))));
}

// BEH-15: an 8-character password; anything else is refused before anything is sent
#[tokio::test(flavor = "multi_thread")]
async fn a_password_has_one_to_eight_characters() {
    let (hub, id, sim) = ipd_at(LOCKED, 0).await;
    for bad in ["", "123456789", "abcdefghijkl", "pässword"] {
        assert!(matches!(hub.login(id, bad).await, Err(Error::Invalid(_))), "{bad:?}");
    }
    assert_eq!(sim.writes().iter().filter(|k| k.member_id == M_CHECK_PASSWORD).count(), 0);
    // not ONLINE, or not known
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(matches!(hub.login(id, "Password").await, Err(Error::NotOnline)));
    let gone = hub.add_manual("Gone", Protocol::Ipd, "10.0.0.9").unwrap();
    hub.remove(gone).unwrap();
    assert!(matches!(hub.login(gone, "Password").await, Err(Error::UnknownAmp)));
}

// BEH-16, SEC-8: the app never asks the amp for a password, and keeps none: not in the config file, not
// in anything it tells the views, not in an error
#[tokio::test(flavor = "multi_thread")]
async fn passwords_are_never_read_and_never_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    sim.set_access(LOCKED, 0);
    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", sim.addr.port());
    let mut updates = hub.subscribe();
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;

    let mut seen = Vec::new();
    seen.push(format!("{:?}", hub.login(id, "Wrong123").await));
    seen.push(format!("{:?}", hub.login(id, "Password").await));
    seen.push(format!("{:?}", hub.login(id, "123456789").await));
    seen.push(format!("{:?}", hub.login(id, "").await));
    hub.rename(id, "Main").unwrap();
    hub.save_file(id, dir.path().join("settings.json")).await.unwrap();
    sleep(Duration::from_millis(300)).await;
    while let Ok(update) = updates.try_recv() {
        seen.push(format!("{update:?}"));
    }
    seen.push(format!("{:?} {:?}", hub.amps(), hub.access(id)));
    seen.push(format!("{:?}", hub.values(id)));
    seen.push(std::fs::read_to_string(&path).unwrap());
    seen.push(std::fs::read_to_string(dir.path().join("settings.json")).unwrap());

    for text in &seen {
        for secret in ["Wrong123", "Password", "123456789"] {
            assert!(!text.contains(secret), "{secret} found in {text}");
        }
    }
    let secret = |k: &labgruppen::amps::ipd::protocol::Key| {
        matches!(k.member_id, M_SET_PASSWORD | M_CHECK_PASSWORD | M_PIN | M_CHECK_PIN)
    };
    let read: Vec<_> = sim.reads().into_iter().filter(secret).collect();
    assert_eq!(read, [], "the app never reads a password or a PIN from the amp");
}
