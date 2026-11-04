//! Channel link on both families (BEH-31 to BEH-35, SEC-6): channel 2 follows channel 1, the IPD
//! amp mirrors by itself and must never be written on channel 2, the IPX amp does not and is.

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::params::slot as ipd_slot;
use labgruppen::amps::ipd::protocol::{Key, S_MIXER, S_MUTE};
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::amps::ipx::fake::{Cmd, FakeAmp};
use labgruppen::amps::ipx::params as ipx_params;
use labgruppen::amps::ipx::protocol::{Address, Main, build_frame};
use labgruppen::amps::ipx::structs::{DeviceState, audio_dword};
use labgruppen::{Channel, Value};
use labrack_core::{AmpId, Ceilings, Error, Hub, Side, Unit};
use tokio::time::sleep;

const TRUE: Value = Value::Bool(true);
const FALSE: Value = Value::Bool(false);

fn addr(name: &str) -> Address {
    ipx_params::slot(&param(name)).unwrap().addr
}

async fn set_and_wait(hub: &Hub, id: AmpId, name: &str, value: Value) {
    hub.set(id, param(name), value.clone()).unwrap();
    wait_value(hub, id, name, value).await;
}

/// The settings a linked pair shares, as names for `out` (0 or 1).
fn shared_names(side: Side, channel: u8) -> Vec<String> {
    let c = match side {
        Side::Inputs => format!("in{}", channel + 1),
        Side::Outputs => format!("out{}", channel + 1),
    };
    let mut names = vec![format!("{c}.gain"), format!("{c}.delay")];
    for band in 0..10 {
        for field in ["enabled", "type", "freq", "gain", "q"] {
            names.push(format!("{c}.eq.{band}.{field}"));
        }
    }
    if side == Side::Outputs {
        for filter in ["hp", "lp"] {
            for field in ["enabled", "type", "freq"] {
                names.push(format!("{c}.xover.{filter}.{field}"));
            }
        }
    }
    names
}

fn limiter_names(channel: u8) -> Vec<String> {
    ["threshold", "impedance", "power", "threshold_dbu"]
        .iter()
        .map(|f| format!("out{}.limiter.{f}", channel + 1))
        .collect()
}

/// Make the two channels different, so that a copy has something to do.
async fn make_channels_differ(hub: &Hub, id: AmpId) {
    for (name, value) in [
        ("out1.gain", f(-10.0)),
        ("out2.gain", f(-20.0)),
        ("out1.delay", f(3.0)),
        ("out2.delay", f(9.0)),
        ("out1.eq.2.gain", f(4.0)),
        ("out2.eq.2.gain", f(-4.0)),
        ("out1.eq.4.freq", Value::Int(1500)),
        ("out2.eq.4.freq", Value::Int(2500)),
        ("out1.xover.hp.freq", Value::Int(80)),
        ("out2.xover.hp.freq", Value::Int(120)),
        ("in1.gain", f(-6.0)),
        ("in2.gain", f(-16.0)),
        ("in1.eq.1.q", f(2.0)),
        ("in2.eq.1.q", f(5.0)),
        ("in1.delay", f(1.0)),
        ("in2.delay", f(2.0)),
    ] {
        set_and_wait(hub, id, name, value).await;
    }
    hub.set_limiter(id, 0, Unit::Dbu, 30.0).unwrap();
    hub.set_limiter(id, 1, Unit::Dbu, 25.0).unwrap();
}

async fn ensure_unlinked(hub: &Hub, id: AmpId) {
    for side in [Side::Inputs, Side::Outputs] {
        hub.set_link(id, side, false).await.unwrap();
    }
}

/// An IPX with the channels different, unlinked, out1 muted and out2 not.
async fn ipx_ready() -> (Hub, AmpId, FakeAmp) {
    let (hub, id, fake) = online_ipx(timing()).await;
    ensure_unlinked(&hub, id).await;
    make_channels_differ(&hub, id).await;
    set_and_wait(&hub, id, "out1.mute", TRUE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    wait_limiter_dbu(&hub, id, 1, 25.0).await;
    (hub, id, fake)
}

async fn wait_limiter_dbu(hub: &Hub, id: AmpId, output: u8, dbu: f64) {
    let seen = async {
        while !hub.limiter(id, output).is_some_and(|l| (l.dbu - dbu).abs() < 0.01) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), seen).await.expect("the limiter is written");
}

async fn ipd_ready() -> (Hub, AmpId, SimServer) {
    let (hub, id, sim) = online_ipd().await;
    ensure_unlinked(&hub, id).await;
    make_channels_differ(&hub, id).await;
    set_and_wait(&hub, id, "out1.mute", TRUE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    wait_limiter_dbu(&hub, id, 1, 25.0).await;
    (hub, id, sim)
}

// BEH-33, BEH-34: a linked pair is one channel to the views
#[tokio::test]
async fn a_linked_pair_is_one_channel_in_the_view_model() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    ensure_unlinked(&hub, id).await;
    assert!(!hub.linked(id, Side::Inputs) && !hub.linked(id, Side::Outputs));
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0), Channel::Output(1)]);

    hub.set_link(id, Side::Outputs, true).await.unwrap();
    assert!(hub.linked(id, Side::Outputs) && !hub.linked(id, Side::Inputs));
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0)]);
    assert_eq!(
        hub.channels(id, Side::Inputs),
        [Channel::Input(0), Channel::Input(1)],
        "each pair on its own"
    );

    hub.set_link(id, Side::Inputs, true).await.unwrap();
    assert_eq!(hub.channels(id, Side::Inputs), [Channel::Input(0)]);
    hub.set_link(id, Side::Outputs, false).await.unwrap();
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0), Channel::Output(1)]);
}

// BEH-32, BEH-35: the link flag is the first channel's; an amp that is not ONLINE takes no link
#[tokio::test]
async fn link_needs_an_online_amp() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, labrack_core::Status::Disconnected).await;
    assert!(matches!(hub.set_link(id, Side::Outputs, true).await, Err(Error::NotOnline)));
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(hub.channels(id, Side::Outputs).len(), 2);
}

// BEH-31, BEH-32: linking copies every shared setting of channel 1 to channel 2, also what the IPX
// does not copy by itself (it copies the gain and the limiter dBu only)
#[tokio::test]
async fn linking_copies_channel_1_to_channel_2_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    for side in [Side::Outputs, Side::Inputs] {
        hub.set_link(id, side, true).await.unwrap();
        assert!(hub.linked(id, side));
        for (a, b) in shared_names(side, 0).iter().zip(shared_names(side, 1)) {
            assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} follows {a}");
        }
    }
    for (a, b) in limiter_names(0).iter().zip(limiter_names(1)) {
        assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} follows {a}");
    }
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-10.0));
    assert_eq!(
        ipx_value(&fake, "out2.limiter.threshold_dbu").as_f64().map(|v| (v * 100.0).round()),
        Some(3000.0)
    );
    // the cache says so too, for the views
    assert_eq!(hub.value(id, &param("out2.eq.2.gain")), Some(f(4.0)));
    assert_eq!(
        hub.value(id, &param("out2.link")).unwrap_or(FALSE),
        FALSE,
        "only the first channel carries the flag"
    );
}

// SEC-6, SEC-7: the copy mutes the outputs first, and no output ends up unmuted that was muted
#[tokio::test]
async fn the_link_copy_mutes_first_and_leaves_the_mutes_as_they_were_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    let before = fake.state().setprm.len();

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, FALSE));
    let writes = fake.state().setprm[before..].to_vec();
    assert_eq!(writes.first(), Some(&addr("out2.mute")), "the output that sounds is muted first");
    assert_eq!(writes.last(), Some(&addr("out2.mute")), "and put back last");
    assert!(!writes.contains(&addr("out1.mute")), "the muted one is not touched");
    let flag = writes.iter().position(|a| *a == addr("out1.link")).expect("the flag is written");
    let copied = writes.iter().filter(|a| a.channel == 1 && a.env != addr("out2.mute").env).count();
    assert!(copied >= 60, "{copied} settings copied");
    assert!(writes[..flag].iter().filter(|a| a.channel == 1).count() >= copied, "copied before the flag");
}

// BEH-32: while linked every channel 1 change is also written to channel 2 on the IPX
#[tokio::test]
async fn linked_edits_are_written_to_both_channels_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    let before = fake.state().setprm.len();

    for (name, twin, value) in [
        ("out1.gain", "out2.gain", f(-3.0)),
        ("out1.delay", "out2.delay", f(7.0)),
        ("out1.eq.4.q", "out2.eq.4.q", f(3.5)),
        ("out1.xover.lp.freq", "out2.xover.lp.freq", Value::Int(9000)),
        ("in1.gain", "in2.gain", f(-2.0)),
        ("in1.eq.0.freq", "in2.eq.0.freq", Value::Int(200)),
    ] {
        hub.set(id, param(name), value.clone()).unwrap();
        wait_value(&hub, id, twin, value.clone()).await;
        assert_eq!(ipx_value(&fake, name), value, "{name}");
        assert_eq!(ipx_value(&fake, twin), value, "{twin}");
    }
    assert_eq!(fake.state().setprm.len() - before, 12, "each change once per channel");

    // the limiter, all four values, on both
    hub.set_limiter(id, 0, Unit::Dbu, 28.0).unwrap();
    wait_limiter_dbu(&hub, id, 1, 28.0).await;
    for (a, b) in limiter_names(0).iter().zip(limiter_names(1)) {
        assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} follows {a}");
    }
    hub.set_load(id, 0, 8.0).unwrap();
    wait_value(&hub, id, "out2.limiter.impedance", f(8.0)).await;
    assert_eq!(ipx_value(&fake, "out1.limiter.impedance"), f(8.0));
}

// BEH-31: channel 1 is the master; a channel 2 edit of a linked pair is refused
#[tokio::test]
async fn channel_2_of_a_linked_pair_cannot_be_edited_directly() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let writes = fake.writes();

    for (name, value) in [
        ("out2.gain", f(0.0)),
        ("out2.delay", f(1.0)),
        ("out2.eq.3.gain", f(1.0)),
        ("out2.xover.hp.freq", Value::Int(100)),
    ] {
        let got = hub.set(id, param(name), value);
        assert!(matches!(got, Err(Error::Linked)), "{name}: {got:?}");
    }
    for unit in [Unit::Dbu, Unit::Vpk, Unit::Watt, Unit::Ohm] {
        let got = hub.set_limiter(id, 1, unit, 10.0);
        assert!(matches!(got, Err(Error::Linked)), "{unit:?}: {got:?}");
    }
    assert!(matches!(hub.set_load(id, 1, 8.0), Err(Error::Linked)));
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), writes);

    // what is not part of the link stays per channel: mutes, and the other pair
    set_and_wait(&hub, id, "out2.mute", TRUE).await;
    assert_eq!(ipx_value(&fake, "out1.mute"), TRUE, "out1 was muted already");
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    assert_eq!(ipx_value(&fake, "out2.mute"), TRUE, "a mute is not mirrored");
    set_and_wait(&hub, id, "in2.gain", f(-4.0)).await;
    set_and_wait(&hub, id, "in2.mixer.aes1.level", f(-6.0)).await;
}

// BEH-35: unlinking keeps the values and the channels are independent again
#[tokio::test]
async fn unlinking_keeps_the_values_and_makes_the_channels_independent_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(-3.0)).await;
    wait_value(&hub, id, "out2.gain", f(-3.0)).await;
    let before = fake.state().setprm.len();

    hub.set_link(id, Side::Outputs, false).await.unwrap();
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(fake.state().setprm[before..], [addr("out1.link")], "only the flag");
    for (a, b) in shared_names(Side::Outputs, 0).iter().zip(shared_names(Side::Outputs, 1)) {
        assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} keeps the value of {a}");
    }

    set_and_wait(&hub, id, "out2.gain", f(-30.0)).await;
    assert_eq!(ipx_value(&fake, "out1.gain"), f(-3.0));
    set_and_wait(&hub, id, "out1.gain", f(-1.0)).await;
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-30.0));
}

// SEC-3: the copy cannot carry a value past a ceiling, and then nothing at all is written
#[tokio::test]
async fn the_link_copy_is_refused_above_a_ceiling() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_ceilings(id, Ceilings { max_output_gain_db: Some(-15.0), max_limiter_dbu: None }).unwrap();
    let writes = fake.writes();

    // out1.gain is -10: copying it to out2 (-20) would raise out2 past the ceiling
    let got = hub.set_link(id, Side::Outputs, true).await;
    assert!(matches!(&got, Err(Error::Invalid(why)) if why.contains("ceiling")), "{got:?}");
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(fake.writes(), writes, "not even a mute");

    hub.set_ceilings(id, Ceilings { max_output_gain_db: None, max_limiter_dbu: Some(27.0) }).unwrap();
    let got = hub.set_link(id, Side::Outputs, true).await;
    assert!(matches!(&got, Err(Error::Invalid(why)) if why.contains("ceiling")), "{got:?}");
    assert_eq!(fake.writes(), writes);
}

// SEC-6: if the copy fails, the outputs stay muted, the pair is not linked, and the amp is usable
#[tokio::test]
async fn a_failed_link_copy_leaves_the_outputs_muted() {
    let (hub, id, fake) = ipx_ready().await;
    fake.state().reject_writes.insert(addr("out2.eq.2.gain"));

    let got = hub.set_link(id, Side::Outputs, true).await;
    assert!(matches!(got, Err(Error::LeftMuted(_))), "{got:?}");
    assert_eq!(ipx_value(&fake, "out2.mute"), TRUE, "muted for the copy, not put back");
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(ipx_value(&fake, "out1.link"), FALSE);

    fake.state().reject_writes.clear();
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
}

// ---- IPD ---------------------------------------------------------------------------------------

// BEH-32: nothing is ever written to channel 2 of a linked IPD: the amp mirrors by itself
#[tokio::test(flavor = "multi_thread")]
async fn no_write_reaches_channel_2_of_a_linked_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    assert_eq!(sim.link_violations(), []);
    let before = sim.writes().len();

    for (name, value) in [
        ("out1.gain", f(-3.0)),
        ("out1.delay", f(7.0)),
        ("out1.eq.4.q", f(3.5)),
        ("out1.xover.lp.freq", Value::Int(9000)),
        ("in1.gain", f(-2.0)),
        ("in1.eq.0.freq", Value::Int(200)),
    ] {
        hub.set(id, param(name), value.clone()).unwrap();
        wait_value(&hub, id, name, value.clone()).await;
        // the amp mirrored it; so does the shared state, for the views
        let twin = name.replacen('1', "2", 1);
        assert_eq!(ipd_value(&sim, &twin).await, value, "{twin} on the amp");
        wait_value(&hub, id, &twin, value).await;
    }
    hub.set_limiter(id, 0, Unit::Dbu, 28.0).unwrap();
    wait_limiter_dbu(&hub, id, 1, 28.0).await;
    hub.set_load(id, 0, 8.0).unwrap();
    assert_eq!(hub.amps()[0].load_ohms, [8.0, 8.0], "the load follows too");

    // nothing was written to a channel 2 key since the link, and the amp never saw one it ignores
    let second = |k: &Key| k.channel & 0x7F == 1 && k.struct_id != S_MUTE && k.struct_id != S_MIXER;
    let writes = sim.writes()[before..].to_vec();
    assert!(!writes.iter().any(second), "{:?}", writes.iter().filter(|k| second(k)).collect::<Vec<_>>());
    assert!(!writes.is_empty());
    assert_eq!(sim.link_violations(), []);

    // channel 2 cannot be edited directly
    assert!(matches!(hub.set(id, param("out2.gain"), f(0.0)), Err(Error::Linked)));
    assert!(matches!(hub.set_limiter(id, 1, Unit::Dbu, 20.0), Err(Error::Linked)));
    sleep(Duration::from_millis(200)).await;
    assert_eq!(sim.link_violations(), []);
}

// BEH-31: linking copies the settings of channel 1 over channel 2 on the IPD too
#[tokio::test(flavor = "multi_thread")]
async fn linking_copies_channel_1_to_channel_2_on_the_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    for (a, b) in shared_names(Side::Outputs, 0).iter().zip(shared_names(Side::Outputs, 1)) {
        assert_eq!(ipd_value(&sim, a).await, ipd_value(&sim, &b).await, "{b} follows {a}");
    }
    let (a, b) =
        (ipd_value(&sim, "out1.limiter.threshold").await, ipd_value(&sim, "out2.limiter.threshold").await);
    assert_eq!(a, b);
    assert_eq!(sim.link_violations(), [], "the copy came before the flag");
    // the inputs were not linked: their channels differ still
    assert_ne!(ipd_value(&sim, "in1.gain").await, ipd_value(&sim, "in2.gain").await);
    // the state for the views
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-10.0)));
}

// SEC-6, SEC-7 on the IPD
#[tokio::test(flavor = "multi_thread")]
async fn the_link_copy_mutes_first_and_leaves_the_mutes_as_they_were_on_the_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    let before = sim.writes().len();

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, FALSE));
    let writes = sim.writes()[before..].to_vec();
    let key = |name: &str| ipd_slot(&param(name)).unwrap().key;
    assert_eq!(writes.first(), Some(&key("out2.mute")));
    assert_eq!(writes.last(), Some(&key("out2.mute")));
    assert!(!writes.contains(&key("out1.mute")));
    let flag = writes.iter().position(|k| *k == key("out1.link")).expect("the flag is written");
    assert!(writes[..flag].contains(&key("out2.gain")), "copied before the flag");
    assert!(!writes[flag..].iter().any(|k| k.channel == 0x81 && k.struct_id != S_MUTE), "nothing after it");
}

// BEH-35 on the IPD: the values stay, the channels are independent again
#[tokio::test(flavor = "multi_thread")]
async fn unlinking_keeps_the_values_on_the_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(-3.0)).await;
    wait_value(&hub, id, "out2.gain", f(-3.0)).await;

    hub.set_link(id, Side::Outputs, false).await.unwrap();
    assert_eq!(hub.channels(id, Side::Outputs).len(), 2);
    assert_eq!(ipd_value(&sim, "out2.gain").await, f(-3.0));
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-3.0)));

    set_and_wait(&hub, id, "out2.gain", f(-30.0)).await;
    assert_eq!(ipd_value(&sim, "out1.gain").await, f(-3.0));
    assert_eq!(sim.link_violations(), [], "unlinked, channel 2 may be written");
}

// BEH-37 with a link: a file never writes channel 2 of a linked pair
#[tokio::test(flavor = "multi_thread")]
async fn a_file_load_into_a_linked_pair_writes_channel_1_only_on_the_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["params"]["out1.gain"] = serde_json::json!(-4.0);
    json["params"]["out2.gain"] = serde_json::json!(9.0);
    std::fs::write(&path, json.to_string()).unwrap();

    let report = hub.load_file(id, &path).await.unwrap();

    assert_eq!(sim.link_violations(), []);
    assert_eq!(ipd_value(&sim, "out1.gain").await, f(-4.0));
    assert_eq!(ipd_value(&sim, "out2.gain").await, f(-4.0), "channel 2 follows channel 1");
    assert!(
        report.skipped.iter().any(|s| s.starts_with("out2.gain") && s.contains("follows channel 1")),
        "{:?}",
        report.skipped
    );
}

#[tokio::test]
async fn a_file_load_into_a_linked_pair_makes_channel_2_follow_channel_1_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["params"]["out1.gain"] = serde_json::json!(-4.0);
    json["params"]["out2.gain"] = serde_json::json!(9.0);
    std::fs::write(&path, json.to_string()).unwrap();

    hub.load_file(id, &path).await.unwrap();

    assert_eq!(ipx_value(&fake, "out1.gain"), f(-4.0));
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-4.0), "the pair stays together");
    assert_eq!(
        (ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")),
        (TRUE, TRUE),
        "a load leaves the outputs muted"
    );
}

// a pair that is not linked is loaded as it is
#[tokio::test]
async fn a_file_load_into_unlinked_channels_writes_both() {
    let (hub, id, fake) = ipx_ready().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.json");
    hub.save_file(id, &path).await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(0.0)).await;
    set_and_wait(&hub, id, "out2.gain", f(0.0)).await;

    hub.load_file(id, &path).await.unwrap();
    assert_eq!((ipx_value(&fake, "out1.gain"), ipx_value(&fake, "out2.gain")), (f(-10.0), f(-20.0)));
}

// ---- an already linked pair, and a pair that drifted -----------------------------------------------

/// Someone changes `name` on the amp itself (front panel): the amp holds it and says so.
fn front_panel(fake: &FakeAmp, name: &str, value: Value) {
    let slot = ipx_params::slot(&param(name)).unwrap();
    let bytes = slot.encode(&value).unwrap();
    let at = DeviceState::AUDIO_OFFSET + audio_dword(slot.addr).unwrap() * 4;
    fake.state().sync[at..at + bytes.len()].copy_from_slice(&bytes);
    let frame = build_frame(Main::DEVPRMCHAGE, slot.addr.pack(), &bytes).unwrap();
    fake.cmd.send(Cmd::Push(frame)).unwrap();
}

// BEH-32: an IPD keeps a linked pair equal itself and channel 2 is never written: linking a pair that is
// linked already is nothing to do (it used to copy over channel 2, which a linked IPD must not get)
#[tokio::test(flavor = "multi_thread")]
async fn linking_a_linked_ipd_again_writes_nothing() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let writes = sim.writes().len();

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    assert_eq!(sim.writes().len(), writes);
    assert_eq!(sim.link_violations(), []);
    assert!(hub.linked(id, Side::Outputs));
}

// a pair that is not linked has no drift, and neither has a pair the app just linked
#[tokio::test]
async fn a_pair_that_is_not_linked_or_just_linked_has_no_drift() {
    let (hub, id, _fake) = ipx_ready().await;
    assert!(hub.link_drift(id, Side::Outputs).is_empty(), "not linked: the channels are meant to differ");
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    assert!(hub.link_drift(id, Side::Outputs).is_empty());
    assert!(hub.link_drift(id, Side::Inputs).is_empty());
}

/// Read updates until one that `pick` takes.
async fn saw<T>(
    rx: &mut tokio::sync::broadcast::Receiver<labrack_core::Update>,
    mut pick: impl FnMut(labrack_core::Update) -> Option<T>,
) -> T {
    let wait = async {
        loop {
            if let Some(t) = pick(rx.recv().await.unwrap()) {
                return t;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait).await.expect("the update arrives")
}

// BEH-32: the IPX does not keep a linked pair equal, so when a change made on the amp itself reaches the
// app, the app sets channel 2 to channel 1's value and says so
#[tokio::test]
async fn a_change_on_the_ipx_itself_is_followed_by_channel_2_with_a_warning() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    let mut rx = hub.subscribe();

    front_panel(&fake, "out1.gain", f(-3.0));
    let corrected = saw(&mut rx, |u| match u {
        labrack_core::Update::LinkCorrected(i, side, p) if i == id => Some((side, p)),
        _ => None,
    })
    .await;
    assert_eq!(corrected, (Side::Outputs, param("out1.gain")));
    wait_value(&hub, id, "out2.gain", f(-3.0)).await;
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-3.0));

    // an EQ band, and the other pair
    front_panel(&fake, "out1.eq.2.freq", Value::Int(3000));
    front_panel(&fake, "in1.gain", f(-5.0));
    wait_value(&hub, id, "out2.eq.2.freq", Value::Int(3000)).await;
    wait_value(&hub, id, "in2.gain", f(-5.0)).await;
    assert_eq!(
        (ipx_value(&fake, "out2.eq.2.freq"), ipx_value(&fake, "in2.gain")),
        (Value::Int(3000), f(-5.0))
    );
    assert!(hub.link_drift(id, Side::Outputs).is_empty() && hub.link_drift(id, Side::Inputs).is_empty());
    assert!(hub.linked(id, Side::Outputs));
}

// a mute is not part of the link: a mute made on the amp is not carried to the other output
#[tokio::test]
async fn a_mute_made_on_the_amp_is_not_followed() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let writes = fake.writes();
    let mute = ipx_params::slot(&param("out1.mute")).unwrap();
    let frame = build_frame(Main::DEVPRMCHAGE, mute.addr.pack(), &0i32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(frame)).unwrap(); // out1 unmuted on the front panel
    wait_value(&hub, id, "out1.mute", FALSE).await;
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), writes, "nothing was written");
    assert_eq!(ipx_value(&fake, "out2.mute"), FALSE, "as it was");
}

// SEC-3: a correction is checked like any value; one past a ceiling is not made, and the pair is reported
#[tokio::test]
async fn a_correction_past_a_ceiling_is_not_made() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_ceilings(id, Ceilings { max_output_gain_db: Some(-12.0), max_limiter_dbu: None }).unwrap();
    let mut rx = hub.subscribe();
    let writes = fake.writes();

    front_panel(&fake, "out1.gain", f(-5.0)); // above the ceiling, and channel 2 is at -10
    let side = saw(&mut rx, |u| match u {
        labrack_core::Update::LinkOutOfSync(i, side) if i == id => Some(side),
        _ => None,
    })
    .await;
    assert_eq!(side, Side::Outputs);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), writes);
    assert_eq!(hub.link_drift(id, Side::Outputs), [param("out1.gain")]);
}

/// A fake IPX whose outputs are flagged linked while channel 2 differs, as an amp can be at connect.
async fn drifted_ipx() -> (FakeAmp, std::sync::Arc<Local>) {
    let fake = FakeAmp::start().await;
    let from = DeviceState::AUDIO_OFFSET;
    for (name, value) in [("out1.link", TRUE), ("out1.gain", f(-10.0)), ("out2.gain", f(-20.0))] {
        let slot = ipx_params::slot(&param(name)).unwrap();
        let bytes = slot.encode(&value).unwrap();
        let at = from + audio_dword(slot.addr).unwrap() * 4;
        fake.state().sync[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    let local = Local::new();
    local.route("10.0.0.1", fake.port);
    (fake, local)
}

// SEC-1: a drift that is already there when the app connects is reported, not corrected: connecting
// writes nothing
#[tokio::test]
async fn a_drift_found_at_connect_is_reported_and_nothing_is_written() {
    let (fake, local) = drifted_ipx().await;
    let hub = labrack_core::Hub::new(labrack_core::Store::in_memory(), local, Heard::nothing(), timing());
    let mut rx = hub.subscribe();
    let id = hub.add_manual("A", labrack_core::Protocol::Ipx, "10.0.0.1").unwrap();

    let side = saw(&mut rx, |u| match u {
        labrack_core::Update::LinkOutOfSync(i, side) if i == id => Some(side),
        _ => None,
    })
    .await;
    assert_eq!(side, Side::Outputs);
    // the gain set up above, and the limiter that the fixture's two outputs really differ in
    let drift = hub.link_drift(id, Side::Outputs);
    assert!(
        drift.contains(&param("out1.gain")) && drift.contains(&param("out1.limiter.threshold_dbu")),
        "{drift:?}"
    );
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), 0);
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-20.0));
}

// there is no sync: linking a pair that is linked already is nothing to do, on both families
#[tokio::test]
async fn linking_a_linked_pair_again_writes_nothing_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let writes = fake.writes();
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    assert_eq!(fake.writes(), writes);
}

// an IPD cannot drift: the amp mirrors, and so does the state
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_ipd_never_drifts() {
    let (hub, id, _sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(-3.0)).await;
    wait_value(&hub, id, "out2.gain", f(-3.0)).await;
    assert!(hub.link_drift(id, Side::Outputs).is_empty());
}
