//! Channel link on both families (BEH-31 to BEH-35, SEC-6): channel 2 follows channel 1, the IPD
//! amp mirrors by itself and must never be written on channel 2, the IPX amp does not and is.

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::params::slot as ipd_slot;
use labgruppen::amps::ipd::protocol::{Key, S_MUTE};
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

/// The settings a linked pair shares, as names for a channel (0 or 1): the gain; the limiter is
/// [`limiter_names`].
fn shared_names(side: Side, channel: u8) -> Vec<String> {
    vec![format!("{}.gain", channel_name(side, channel))]
}

fn channel_name(side: Side, channel: u8) -> String {
    match side {
        Side::Inputs => format!("in{}", channel + 1),
        Side::Outputs => format!("out{}", channel + 1),
    }
}

/// What a link leaves per channel: the delay, the EQ and the output filters.
fn per_channel_names(side: Side, channel: u8) -> Vec<String> {
    let c = channel_name(side, channel);
    let mut names = vec![format!("{c}.delay")];
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

// BEH-31, BEH-32: linking copies the level of channel 1 to channel 2 (the gain, and the whole limiter,
// of which the IPX copies the dBu only), and nothing else: the EQs are never merged
#[tokio::test]
async fn linking_copies_the_level_of_channel_1_to_channel_2_and_nothing_else_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    let eq_before = ipx_value(&fake, "out2.eq.2.gain");
    assert_ne!(eq_before, ipx_value(&fake, "out1.eq.2.gain"));
    for side in [Side::Outputs, Side::Inputs] {
        hub.set_link(id, side, true).await.unwrap();
        assert!(hub.linked(id, side));
        for (a, b) in shared_names(side, 0).iter().zip(shared_names(side, 1)) {
            assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} follows {a}");
        }
    }
    assert_eq!(ipx_value(&fake, "out2.eq.2.gain"), eq_before, "the EQ of channel 2 is untouched");
    assert_eq!(ipx_value(&fake, "out2.eq.4.freq"), Value::Int(2500));
    assert_eq!(ipx_value(&fake, "out2.xover.hp.freq"), Value::Int(120));
    assert_eq!(ipx_value(&fake, "in2.eq.1.q"), f(5.0));
    for (a, b) in limiter_names(0).iter().zip(limiter_names(1)) {
        assert_eq!(ipx_value(&fake, a), ipx_value(&fake, &b), "{b} follows {a}");
    }
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-10.0));
    assert_eq!(
        ipx_value(&fake, "out2.limiter.threshold_dbu").as_f64().map(|v| (v * 100.0).round()),
        Some(3000.0)
    );
    // the cache says so too, for the views
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-10.0)));
    assert_eq!(hub.value(id, &param("out2.eq.2.gain")), Some(eq_before));
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
    assert!((4..=8).contains(&copied), "{copied} settings copied: the gain and the limiter, no EQ");
    assert!(writes[..flag].iter().filter(|a| a.channel == 1).count() >= copied, "copied before the flag");
}

// BEH-31, BEH-32: while linked every gain change is also written to channel 2 on the IPX; the EQ, the
// filters and the delay are written where they are made
#[tokio::test]
async fn linked_gains_are_written_to_both_channels_and_the_eq_to_one_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    let before = fake.state().setprm.len();

    for (name, twin, value) in [("out1.gain", "out2.gain", f(-3.0)), ("in1.gain", "in2.gain", f(-2.0))] {
        hub.set(id, param(name), value.clone()).unwrap();
        wait_value(&hub, id, twin, value.clone()).await;
        assert_eq!(ipx_value(&fake, name), value, "{name}");
        assert_eq!(ipx_value(&fake, twin), value, "{twin}");
    }
    assert_eq!(fake.state().setprm.len() - before, 4, "each gain once per channel");

    let before = fake.state().setprm.len();
    for (name, twin, value) in [
        ("out1.delay", "out2.delay", f(7.0)),
        ("out1.eq.4.q", "out2.eq.4.q", f(3.5)),
        ("out1.xover.lp.freq", "out2.xover.lp.freq", Value::Int(9000)),
        ("in1.eq.0.freq", "in2.eq.0.freq", Value::Int(200)),
    ] {
        let other = ipx_value(&fake, twin);
        set_and_wait(&hub, id, name, value.clone()).await;
        assert_eq!(ipx_value(&fake, twin), other, "{twin} is not touched by {name}");
    }
    assert_eq!(fake.state().setprm.len() - before, 4, "each EQ change once");

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

    let got = hub.set(id, param("out2.gain"), f(0.0));
    assert!(matches!(got, Err(Error::Linked)), "out2.gain: {got:?}");
    for unit in [Unit::Dbu, Unit::Vpk, Unit::Watt, Unit::Ohm] {
        let got = hub.set_limiter(id, 1, unit, 10.0);
        assert!(matches!(got, Err(Error::Linked)), "{unit:?}: {got:?}");
    }
    assert!(matches!(hub.set_load(id, 1, 8.0), Err(Error::Linked)));
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), writes);

    // what is not part of the link stays per channel: the EQ, the filters, the delay, mutes, the other pair
    set_and_wait(&hub, id, "out2.eq.3.gain", f(1.0)).await;
    set_and_wait(&hub, id, "out2.delay", f(1.0)).await;
    set_and_wait(&hub, id, "out2.xover.hp.freq", Value::Int(100)).await;
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
    fake.state().reject_writes.insert(addr("out2.gain"));

    let got = hub.set_link(id, Side::Outputs, true).await;
    assert!(matches!(got, Err(Error::LeftMuted(_))), "{got:?}");
    assert_eq!(ipx_value(&fake, "out2.mute"), TRUE, "muted for the copy, not put back");
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(ipx_value(&fake, "out1.link"), FALSE);

    fake.state().reject_writes.clear();
    // the state is read again after a failure; an unmute needs it
    wait_loaded(&hub, id).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
}

// SEC-6: a bulk change that puts the outputs back "as they were" must not undo a MUTE the user asked
// for meanwhile: here it is asked for before the copy starts, which drops what waits to be written
#[tokio::test]
async fn a_mute_asked_for_around_a_copy_is_not_lost() {
    let (hub, id, fake) = ipx_ready().await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;

    let copy = {
        let hub = hub.clone();
        tokio::spawn(async move { hub.copy_eq(id, Channel::Output(1)).await })
    };
    // the copy is first in line for the amp: it takes the gate before the mute can be written
    hub.set_mute(id, 0, true).unwrap();
    copy.await.unwrap().unwrap();

    wait_value(&hub, id, "out1.mute", Value::Bool(true)).await;
    assert_eq!(ipx_value(&fake, "out1.mute"), TRUE, "the user's MUTE stands");
    assert_eq!(ipx_value(&fake, "out2.mute"), FALSE, "the other output is put back as it was");
}

// ---- IPD ---------------------------------------------------------------------------------------

// BEH-32: the link of an IPD is the app's: it never switches the amp's flag on (that would merge the EQs),
// writes the gain and the limiter to both channels, and the EQ where it is made
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_ipd_writes_the_level_to_both_channels_and_never_switches_its_flag_on() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    assert!(hub.linked(id, Side::Outputs) && hub.linked(id, Side::Inputs));
    assert!(!hub.amp_keeps_equal(id, Side::Outputs), "the amp's own link is off");
    let before = sim.writes().len();

    for (name, value) in [("out1.gain", f(-3.0)), ("in1.gain", f(-2.0))] {
        hub.set(id, param(name), value.clone()).unwrap();
        let twin = name.replacen('1', "2", 1);
        wait_value(&hub, id, &twin, value.clone()).await;
        assert_eq!(ipd_value(&sim, name).await, value, "{name} on the amp");
        assert_eq!(ipd_value(&sim, &twin).await, value, "{twin} on the amp");
    }
    hub.set_limiter(id, 0, Unit::Dbu, 28.0).unwrap();
    wait_limiter_dbu(&hub, id, 1, 28.0).await;
    hub.set_load(id, 0, 8.0).unwrap();
    assert_eq!(hub.amps()[0].load_ohms, [8.0, 8.0], "the load follows too");
    assert_eq!(
        ipd_value(&sim, "out1.limiter.threshold").await,
        ipd_value(&sim, "out2.limiter.threshold").await,
        "the limiter on both channels"
    );

    // the EQ is written where it is made, and channel 2 can be edited
    let other = ipd_value(&sim, "out2.eq.4.q").await;
    set_and_wait(&hub, id, "out1.eq.4.q", f(3.5)).await;
    assert_eq!(ipd_value(&sim, "out2.eq.4.q").await, other, "EQ 2 is not merged");
    set_and_wait(&hub, id, "out2.eq.4.q", f(6.0)).await;
    assert_eq!(ipd_value(&sim, "out1.eq.4.q").await, f(3.5));

    let flag = |k: &Key| k.struct_id == labgruppen::amps::ipd::protocol::S_LINK;
    assert!(!sim.writes().iter().any(flag), "the amp's link flag is never written");
    assert_eq!(sim.link_violations(), []);
    assert!(sim.writes().len() > before);

    // the gain of channel 2 cannot be edited directly
    assert!(matches!(hub.set(id, param("out2.gain"), f(0.0)), Err(Error::Linked)));
    assert!(matches!(hub.set_limiter(id, 1, Unit::Dbu, 20.0), Err(Error::Linked)));
}

// BEH-31: linking copies the level of channel 1 over channel 2 on the IPD too, and not the EQ
#[tokio::test(flavor = "multi_thread")]
async fn linking_copies_the_level_of_channel_1_to_channel_2_and_nothing_else_on_the_ipd() {
    let (hub, id, sim) = ipd_ready().await;
    let mut before = Vec::new();
    for name in per_channel_names(Side::Outputs, 1) {
        before.push((name.clone(), ipd_value(&sim, &name).await));
    }
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    for (a, b) in shared_names(Side::Outputs, 0).iter().zip(shared_names(Side::Outputs, 1)) {
        assert_eq!(ipd_value(&sim, a).await, ipd_value(&sim, &b).await, "{b} follows {a}");
    }
    let (a, b) =
        (ipd_value(&sim, "out1.limiter.threshold").await, ipd_value(&sim, "out2.limiter.threshold").await);
    assert_eq!(a, b);
    for (name, value) in before {
        assert_eq!(ipd_value(&sim, &name).await, value, "{name} stays what it was");
    }
    assert_eq!(hub.amps()[0].load_ohms[1], hub.amps()[0].load_ohms[0]);
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-10.0)));
    // the inputs were not linked: their channels differ still
    assert_ne!(ipd_value(&sim, "in1.gain").await, ipd_value(&sim, "in2.gain").await);
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
    assert!(writes.contains(&key("out2.gain")), "the gain is copied");
    assert!(!writes.contains(&key("out1.link")), "and the amp's flag is not written");
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

// BEH-37 with a link: a file gives channel 2 of a linked pair the level of channel 1
#[tokio::test(flavor = "multi_thread")]
async fn a_file_load_into_a_linked_pair_gives_channel_2_the_level_of_channel_1_on_the_ipd() {
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

// BEH-32: the app shows what is stored. A pair that the amp flags linked while its channels differ is
// shown as two unlinked channels with their stored values, and nothing is written to the amp
#[tokio::test]
async fn a_pair_found_out_of_step_is_shown_as_two_unlinked_channels() {
    let (fake, local) = drifted_ipx().await;
    let hub = labrack_core::Hub::new(labrack_core::Store::in_memory(), local, Heard::nothing(), timing());
    let mut rx = hub.subscribe();
    let id = hub.add_manual("A", labrack_core::Protocol::Ipx, "10.0.0.1").unwrap();

    let side = saw(&mut rx, |u| match u {
        labrack_core::Update::LinkChanged(i, side) if i == id => Some(side),
        _ => None,
    })
    .await;
    assert_eq!(side, Side::Outputs);
    assert!(!hub.linked(id, Side::Outputs), "shown unlinked");
    assert!(hub.link_diverged(id, Side::Outputs), "and said to be linked in the amp");
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0), Channel::Output(1)]);
    assert_eq!(
        (hub.value(id, &param("out1.gain")), hub.value(id, &param("out2.gain"))),
        (Some(f(-10.0)), Some(f(-20.0)))
    );
    // the gain set up above, and the limiter that the fixture's two outputs really differ in
    let drift = hub.link_drift(id, Side::Outputs);
    assert!(
        drift.contains(&param("out1.gain")) && drift.contains(&param("out1.limiter.threshold_dbu")),
        "{drift:?}"
    );
    assert!(!hub.link_diverged(id, Side::Inputs), "the other pair is as it was");
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), 0, "connecting writes nothing (SEC-1)");

    // channels that are shown separately are edited separately; the flag on the amp is not touched
    hub.set(id, param("out2.gain"), f(-15.0)).unwrap();
    wait_value(&hub, id, "out2.gain", f(-15.0)).await;
    hub.set(id, param("out1.gain"), f(-12.0)).unwrap();
    wait_value(&hub, id, "out1.gain", f(-12.0)).await;
    assert_eq!(fake.state().setprm, [addr("out2.gain"), addr("out1.gain")], "each only where it was made");
    assert_eq!(ipx_value(&fake, "out1.link"), TRUE);
}

// BEH-32: a change made on the IPX itself to one channel of a linked pair is shown as it is stored, and the
// pair is shown unlinked; nothing is written, nothing is corrected
#[tokio::test]
async fn a_change_on_the_ipx_itself_shows_the_pair_unlinked_with_the_stored_values() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    hub.set_link(id, Side::Inputs, true).await.unwrap();
    let writes = fake.writes();
    let mut rx = hub.subscribe();

    front_panel(&fake, "out1.gain", f(-3.0));
    let side = saw(&mut rx, |u| match u {
        labrack_core::Update::LinkChanged(i, side) if i == id => Some(side),
        _ => None,
    })
    .await;
    assert_eq!(side, Side::Outputs);
    assert!(!hub.linked(id, Side::Outputs) && hub.link_diverged(id, Side::Outputs));
    assert_eq!(
        (hub.value(id, &param("out1.gain")), hub.value(id, &param("out2.gain"))),
        (Some(f(-3.0)), Some(f(-10.0)))
    );
    assert!(hub.linked(id, Side::Inputs), "the other pair is untouched");
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), writes, "nothing was written");

    // the same thing done to the other channel on the amp makes them equal again: linked again
    front_panel(&fake, "out2.gain", f(-3.0));
    saw(&mut rx, |u| matches!(u, labrack_core::Update::LinkChanged(i, _) if i == id).then_some(())).await;
    assert!(hub.linked(id, Side::Outputs) && !hub.link_diverged(id, Side::Outputs));
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0)]);
}

// the app's own edits of a linked pair write both channels, and the pair never looks out of step meanwhile
#[tokio::test]
async fn editing_a_linked_pair_never_shows_it_out_of_step() {
    let (hub, id, _fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let mut rx = hub.subscribe();
    let watcher = {
        let hub = hub.clone();
        tokio::spawn(async move {
            let mut seen_unlinked = false;
            for _ in 0..400 {
                seen_unlinked |= !hub.linked(id, Side::Outputs);
                sleep(Duration::from_millis(1)).await;
            }
            seen_unlinked
        })
    };
    for step in 0..30 {
        hub.set(id, param("out1.gain"), f(-20.0 + f64::from(step) * 0.5)).unwrap();
        hub.set(id, param("out1.eq.3.gain"), f(-6.0 + f64::from(step) * 0.2)).unwrap();
        hub.set(id, param("out1.delay"), f(f64::from(step) * 0.1)).unwrap();
        sleep(Duration::from_millis(12)).await;
    }
    assert!(!watcher.await.unwrap(), "the pair looked unlinked while the app was writing it");
    assert!(hub.linked(id, Side::Outputs));
    // the state can be so short-lived that sampling misses it; the event does not
    sleep(Duration::from_millis(200)).await;
    while let Ok(update) = rx.try_recv() {
        assert!(
            !matches!(update, labrack_core::Update::LinkChanged(..)),
            "the pair changed state: {update:?}"
        );
    }
}

// LINK on a pair shown unlinked makes channel 2 equal to channel 1, as for any pair
#[tokio::test]
async fn linking_a_pair_that_was_found_out_of_step_makes_channel_2_equal() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    front_panel(&fake, "out1.gain", f(-3.0));
    let mut rx = hub.subscribe();
    wait_value(&hub, id, "out1.gain", f(-3.0)).await;
    saw(&mut rx, |u| {
        matches!(u, labrack_core::Update::LinkChanged(i, Side::Outputs) if i == id).then_some(())
    })
    .await;
    assert!(!hub.linked(id, Side::Outputs));

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    assert!(hub.linked(id, Side::Outputs) && !hub.link_diverged(id, Side::Outputs));
    assert_eq!(ipx_value(&fake, "out2.gain"), f(-3.0));
    assert_eq!(
        (ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")),
        (TRUE, FALSE),
        "mutes as they were"
    );
}

// UNLINK on such a pair clears the flag in the amp
#[tokio::test]
async fn unlinking_a_pair_that_was_found_out_of_step_clears_the_flag() {
    let (fake, local) = drifted_ipx().await;
    let hub = labrack_core::Hub::new(labrack_core::Store::in_memory(), local, Heard::nothing(), timing());
    let id = hub.add_manual("A", labrack_core::Protocol::Ipx, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    assert!(hub.link_diverged(id, Side::Outputs));

    hub.set_link(id, Side::Outputs, false).await.unwrap();

    assert_eq!(ipx_value(&fake, "out1.link"), FALSE);
    assert!(!hub.linked(id, Side::Outputs) && !hub.link_diverged(id, Side::Outputs));
}

// linking a pair that is linked already is nothing to do, on both families
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

// BEH-32: an IPD cannot differ. A pair it already flags linked while channel 2 still holds other values
// (linked on the amp, or by another tool) is shown linked with channel 1's values, and LINK writes
// nothing: writing channel 2 of a linked IPD can leave it unanswering for 3 s
#[tokio::test(flavor = "multi_thread")]
async fn an_ipd_found_linked_is_shown_linked_and_never_written_on_channel_2() {
    use labgruppen::amps::ipd::IpdAmp;
    use labgruppen::amps::ipd::client::Options;
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let other = IpdAmp::connect_with(
        "127.0.0.1",
        sim.addr.port(),
        Options { mk2: None, timeout: Duration::from_secs(5) },
    )
    .await
    .unwrap();
    for (name, value) in [("out1.gain", f(-3.0)), ("out2.gain", f(-10.0)), ("out1.link", TRUE)] {
        labgruppen::Amp::set(&other, &param(name), value).await.unwrap();
    }
    labgruppen::Amp::close(&other).await;
    let writes = sim.writes().len();

    let (hub, local) = hub();
    local.route("10.0.0.1", sim.addr.port());
    let id = hub.add_manual("Right", labrack_core::Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;

    assert!(hub.linked(id, Side::Outputs), "shown linked");
    assert!(!hub.link_diverged(id, Side::Outputs));
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-3.0)), "channel 2 shows channel 1");

    hub.set_link(id, Side::Outputs, true).await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(-4.0)).await;
    sleep(Duration::from_millis(300)).await;
    assert!(hub.linked(id, Side::Outputs) && !hub.link_diverged(id, Side::Outputs), "still linked");
    assert_eq!(sim.link_violations(), []);
    assert!(
        sim.writes()[writes..].iter().all(|k| k.channel & 0x7F == 0 || k.struct_id == S_MUTE),
        "{:?}",
        &sim.writes()[writes..]
    );
}

// BEH-31: channel 1 may hold a value a hair outside the range (the vendor app stores 36.02 dBu for 69 Vpk,
// whose 35.99 is the top of the range): linking copies the nearest value in range instead of failing (the
// amp then copies channel 1 itself when the link goes on, so what it holds is not what is checked)
#[tokio::test(flavor = "multi_thread")]
async fn linking_does_not_fail_on_a_value_just_outside_the_range() {
    let (hub, id, fake) = ipx_ready().await;
    front_panel(&fake, "out1.limiter.threshold_dbu", f(36.02));
    wait_value(&hub, id, "out1.limiter.threshold_dbu", f(36.02)).await;

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    assert!(hub.linked(id, Side::Outputs));
}

// the ceilings still refuse: a clamp to the range never gets past the user's ceiling (SEC-3)
#[tokio::test(flavor = "multi_thread")]
async fn a_value_outside_the_range_is_still_refused_above_a_ceiling() {
    let (hub, id, fake) = ipx_ready().await;
    hub.set_ceilings(id, Ceilings { max_limiter_dbu: Some(30.0), ..Ceilings::default() }).unwrap();
    front_panel(&fake, "out1.limiter.threshold_dbu", f(36.02));
    wait_value(&hub, id, "out1.limiter.threshold_dbu", f(36.02)).await;

    assert!(hub.set_link(id, Side::Outputs, true).await.is_err());
    assert!(!hub.linked(id, Side::Outputs));
}

// ---- the link an IPD keeps for itself, an IPD that links a pair itself, and the EQ copy -------------------

/// A simulated IPD whose own link flag is on for the outputs, set by another tool before the app comes.
async fn ipd_linked_by_the_amp() -> (Hub, AmpId, SimServer) {
    use labgruppen::amps::ipd::IpdAmp;
    use labgruppen::amps::ipd::client::Options;
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let other = IpdAmp::connect_with(
        "127.0.0.1",
        sim.addr.port(),
        Options { mk2: None, timeout: Duration::from_secs(5) },
    )
    .await
    .unwrap();
    for (name, value) in [("out1.eq.4.q", f(2.0)), ("out2.eq.4.q", f(5.0)), ("out1.link", TRUE)] {
        labgruppen::Amp::set(&other, &param(name), value).await.unwrap();
    }
    labgruppen::Amp::close(&other).await;
    let (hub, local) = hub();
    local.route("10.0.0.1", sim.addr.port());
    let id = hub.add_manual("Right", labrack_core::Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    (hub, id, sim)
}

// BEH-32: a pair the IPD itself has linked keeps channel 2 equal in everything, so nothing of channel 2 can be
// edited, and UNLINK switches the amp's flag off: then the EQs are independent
#[tokio::test(flavor = "multi_thread")]
async fn an_ipd_that_links_a_pair_itself_refuses_channel_2_until_unlinked() {
    let (hub, id, sim) = ipd_linked_by_the_amp().await;
    assert!(hub.linked(id, Side::Outputs) && hub.amp_keeps_equal(id, Side::Outputs));
    let writes = sim.writes().len();

    for (name, value) in [
        ("out2.eq.4.q", f(1.0)),
        ("out2.gain", f(0.0)),
        ("out2.delay", f(1.0)),
        ("out2.xover.hp.freq", Value::Int(100)),
    ] {
        let got = hub.set(id, param(name), value);
        assert!(matches!(got, Err(Error::Linked)), "{name}: {got:?}");
    }
    assert!(matches!(hub.copy_eq(id, Channel::Output(1)).await, Err(Error::Linked)));
    set_and_wait(&hub, id, "out1.eq.4.q", f(3.0)).await;
    wait_value(&hub, id, "out2.eq.4.q", f(3.0)).await;
    assert_eq!(sim.link_violations(), []);
    assert!(sim.writes()[writes..].iter().all(|k| k.channel & 0x7F == 0), "channel 1 only");

    hub.set_link(id, Side::Outputs, false).await.unwrap();

    assert!(!hub.linked(id, Side::Outputs) && !hub.amp_keeps_equal(id, Side::Outputs));
    assert_eq!(ipd_value(&sim, "out1.link").await, FALSE, "the amp's own flag is off");
    set_and_wait(&hub, id, "out2.eq.4.q", f(6.0)).await;
    assert_eq!(ipd_value(&sim, "out1.eq.4.q").await, f(3.0), "the EQs are independent");
}

// BEH-32: the link of an IPD is kept in the config, so it is still there after a reconnect; a link whose
// channels no longer match when the state is read (changed by another tool meanwhile) is dropped
#[tokio::test(flavor = "multi_thread")]
async fn the_link_the_app_keeps_for_an_ipd_survives_a_reconnect_unless_the_channels_differ() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    assert_eq!(hub.amps()[0].links, [false, true]);

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, labrack_core::Status::Disconnected).await;
    assert!(!hub.linked(id, Side::Outputs), "nothing is shown while the amp is not online");
    hub.connect(id).unwrap();
    wait_loaded(&hub, id).await;
    assert!(hub.linked(id, Side::Outputs), "still linked");
    assert_eq!(hub.channels(id, Side::Outputs), [Channel::Output(0)]);

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, labrack_core::Status::Disconnected).await;
    {
        use labgruppen::amps::ipd::IpdAmp;
        use labgruppen::amps::ipd::client::Options;
        let other = IpdAmp::connect_with(
            "127.0.0.1",
            sim.addr.port(),
            Options { mk2: None, timeout: Duration::from_secs(5) },
        )
        .await
        .unwrap();
        labgruppen::Amp::set(&other, &param("out2.gain"), f(-30.0)).await.unwrap();
        labgruppen::Amp::close(&other).await;
    }
    hub.connect(id).unwrap();
    wait_loaded(&hub, id).await;

    assert!(!hub.linked(id, Side::Outputs), "dropped: the channels differ");
    assert_eq!(hub.channels(id, Side::Outputs).len(), 2);
    assert_eq!(hub.amps()[0].links, [false, false]);
    assert_eq!(hub.value(id, &param("out2.gain")), Some(f(-30.0)), "nothing was corrected (SEC-1)");
}

// BEH-33: COPY takes the EQ of the other channel: the bands and the output filters, not the delay; the mutes
// end as they were
#[tokio::test]
async fn copying_an_eq_takes_the_bands_and_the_filters_but_not_the_delay_on_the_ipx() {
    let (hub, id, fake) = ipx_ready().await;
    assert_ne!(ipx_value(&fake, "out1.eq.2.gain"), ipx_value(&fake, "out2.eq.2.gain"));
    let delay = ipx_value(&fake, "out2.delay");

    hub.copy_eq(id, Channel::Output(1)).await.unwrap();

    assert_eq!(ipx_value(&fake, "out2.eq.2.gain"), f(4.0));
    assert_eq!(ipx_value(&fake, "out2.eq.4.freq"), Value::Int(1500));
    assert_eq!(ipx_value(&fake, "out2.xover.hp.freq"), Value::Int(80));
    assert_eq!(ipx_value(&fake, "out2.delay"), delay, "the delay is not copied");
    assert_eq!(ipx_value(&fake, "out1.eq.2.gain"), f(4.0), "the source is untouched");
    assert_eq!(hub.value(id, &param("out2.eq.2.gain")), Some(f(4.0)), "and the views see it");
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, FALSE));
    assert!(!hub.linked(id, Side::Outputs), "copying does not link");

    // and the other way round, on the inputs
    hub.copy_eq(id, Channel::Input(0)).await.unwrap();
    assert_eq!(ipx_value(&fake, "in1.eq.1.q"), f(5.0));
}

#[tokio::test(flavor = "multi_thread")]
async fn copying_an_eq_works_on_the_ipd_too() {
    let (hub, id, sim) = ipd_ready().await;

    hub.copy_eq(id, Channel::Output(1)).await.unwrap();

    assert_eq!(ipd_value(&sim, "out2.eq.2.gain").await, f(4.0));
    assert_eq!(ipd_value(&sim, "out2.xover.hp.freq").await, Value::Int(80));
    assert_ne!(ipd_value(&sim, "out2.delay").await, ipd_value(&sim, "out1.delay").await);
    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, FALSE));
    assert_eq!(sim.link_violations(), []);
}

// SEC-1: nothing to copy from before the state is read, and an amp that is not online takes nothing
#[tokio::test]
async fn copying_an_eq_needs_an_online_amp() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, labrack_core::Status::Disconnected).await;
    assert!(matches!(hub.copy_eq(id, Channel::Output(1)).await, Err(Error::NotOnline)));
}

// BEH-12: an IPD answers a read in seconds at times, so linking and the mutes around it do not read what the
// app holds already: channel 1 comes from the state, and the mutes are asked for once, not twice
#[tokio::test(flavor = "multi_thread")]
async fn linking_an_ipd_reads_little_from_the_amp() {
    let (hub, id, sim) = ipd_ready().await;
    sleep(Duration::from_millis(300)).await;
    let before = sim.reads().len();

    hub.set_link(id, Side::Outputs, true).await.unwrap();

    let reads = sim.reads()[before..].to_vec();
    let polled = reads.iter().filter(|k| k.struct_id == S_MUTE).count();
    assert!(reads.len() == polled, "only mutes are read: {reads:?}");
    assert!(polled <= 4, "{polled} reads of the mutes (one asking before, the rest the polling)");
}

// BEH-35: unlinking the app's own link of an IPD writes nothing and does not wait for the amp
#[tokio::test(flavor = "multi_thread")]
async fn unlinking_an_ipd_pair_does_not_wait_for_the_amp() {
    let (hub, id, sim) = ipd_ready().await;
    hub.set_link(id, Side::Outputs, true).await.unwrap();
    let writes = sim.writes().len();
    sim.set_latency(Duration::from_millis(500));

    let started = std::time::Instant::now();
    hub.set_link(id, Side::Outputs, false).await.unwrap();

    assert!(started.elapsed() < Duration::from_millis(200), "{:?}", started.elapsed());
    assert!(!hub.linked(id, Side::Outputs));
    assert_eq!(sim.writes().len(), writes, "nothing written");
}
