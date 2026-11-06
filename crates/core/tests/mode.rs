//! The device mode of an IPD (BEH-44): shown as the amp holds it, changed with the outputs protected.

mod common;

use std::time::Duration;

use common::*;
use labgruppen::Value;
use labgruppen::amps::ipd::protocol::{Key, S_MUTE, S_PRESET};
use labrack_core::{Error, Update};
use tokio::time::{sleep, timeout};

fn mode_key() -> Key {
    Key::new(S_PRESET, 36, 0, 0)
}

async fn set_and_wait(hub: &labrack_core::Hub, id: labrack_core::AmpId, name: &str, value: Value) {
    hub.set(id, param(name), value.clone()).unwrap();
    wait_value(hub, id, name, value).await;
}

async fn ipd_mode(sim: &labgruppen::amps::ipd::sim::SimServer) -> i64 {
    use labgruppen::amps::ipd::IpdAmp;
    use labgruppen::amps::ipd::client::Options;
    let other = IpdAmp::connect_with(
        "127.0.0.1",
        sim.addr.port(),
        Options { mk2: None, timeout: Duration::from_secs(5) },
    )
    .await
    .unwrap();
    let raw = other.client().get_raw(mode_key(), None).await.unwrap().int().unwrap();
    labgruppen::Amp::close(&other).await;
    raw
}

// BEH-44: the mode the amp holds is shown once the state is read, on an IPD and on an IPX
#[tokio::test(flavor = "multi_thread")]
async fn the_device_mode_is_shown() {
    let (hub, id, _sim) = online_ipd().await;
    assert_eq!(hub.device_mode(id).as_deref(), Some("stereo"));

    let (hub, id, _fake) = online_ipx(timing()).await;
    assert_eq!(hub.device_mode(id).as_deref(), Some("stereo"));
}

// BEH-44: an IPX changes its mode like an IPD does, and a change on its front panel is seen
#[tokio::test(flavor = "multi_thread")]
async fn an_ipx_changes_its_mode_and_shows_a_change_on_the_front_panel() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let mode_dword = labgruppen::amps::ipx::structs::DeviceState::AUDIO_OFFSET;
    let held = || i32::from_le_bytes(fake.state().sync[mode_dword..mode_dword + 4].try_into().unwrap());

    hub.set_device_mode(id, "two_way").await.unwrap();
    assert_eq!((held(), hub.device_mode(id).as_deref()), (0, Some("two_way")));
    hub.set_device_mode(id, "stereo").await.unwrap();
    assert_eq!((held(), hub.device_mode(id).as_deref()), (1, Some("stereo")));

    fake.state().sync[mode_dword..mode_dword + 4].copy_from_slice(&0i32.to_le_bytes());
    let mut seen = None;
    for _ in 0..30 {
        seen = hub.device_mode(id);
        if seen.as_deref() == Some("two_way") {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(seen.as_deref(), Some("two_way"), "the hub asks for the mode now and then");
}

// BEH-44, SEC-6, SEC-7: changing the mode mutes the outputs around it and puts them back as they were; the
// other bits of the number the amp holds stay
#[tokio::test(flavor = "multi_thread")]
async fn changing_the_mode_protects_the_outputs_and_keeps_the_other_bits() {
    let (hub, id, sim) = online_ipd().await;
    set_and_wait(&hub, id, "out1.mute", Value::Bool(true)).await;
    set_and_wait(&hub, id, "out2.mute", Value::Bool(false)).await;
    let before = sim.writes().len();

    hub.set_device_mode(id, "two_way").await.unwrap();

    assert_eq!(hub.device_mode(id).as_deref(), Some("two_way"));
    assert_eq!(ipd_mode(&sim).await, 4131, "bit 0x1000 only");
    let writes = sim.writes()[before..].to_vec();
    let mute = |channel: u8| Key::new(S_MUTE, 14, channel, 0);
    assert_eq!(writes.first(), Some(&mute(0x81)), "the output that sounds is muted first");
    assert_eq!(writes.last(), Some(&mute(0x81)), "and put back last");
    assert!(writes.contains(&mode_key()) && !writes.contains(&mute(0x80)), "{writes:?}");
    assert_eq!(hub.value(id, &param("out2.mute")), Some(Value::Bool(false)));

    hub.set_device_mode(id, "stereo").await.unwrap();
    assert_eq!(ipd_mode(&sim).await, 35);
}

// nothing is written when the amp holds that mode already, and a label that is no mode is refused
#[tokio::test(flavor = "multi_thread")]
async fn the_same_mode_writes_nothing_and_a_bad_one_is_refused() {
    let (hub, id, sim) = online_ipd().await;
    let before = sim.writes().len();
    hub.set_device_mode(id, "stereo").await.unwrap();
    assert_eq!(sim.writes().len(), before);
    assert!(matches!(hub.set_device_mode(id, "bridge").await, Err(Error::Invalid(_))));
    assert_eq!(sim.writes().len(), before);
}

// BEH-44: a change on the front panel is seen within a couple of seconds
#[tokio::test(flavor = "multi_thread")]
async fn a_change_on_the_front_panel_is_seen() {
    let (hub, id, sim) = online_ipd().await;
    let mut rx = hub.subscribe();

    sim.set_device_mode_number(4131);

    let seen = async {
        loop {
            if let Update::Changed(i, labgruppen::Param::Mode, Value::Text(mode)) = rx.recv().await.unwrap()
                && i == id
            {
                return mode;
            }
        }
    };
    assert_eq!(timeout(Duration::from_secs(5), seen).await.expect("the change is seen"), "two_way");
    assert_eq!(hub.device_mode(id).as_deref(), Some("two_way"));
    sleep(Duration::from_millis(100)).await;
}
