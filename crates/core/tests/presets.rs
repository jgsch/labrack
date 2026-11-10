//! Mute, bulk changes (recall, file load), presets and MODIFIED, against the IPD simulator and the
//! fake IPX. The point of most of these: no output is unmuted that was muted before (SEC-6, SEC-7).

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use common::*;
use labgruppen::amps::ipd::IpdAmp;
use labgruppen::amps::ipd::client::Options;
use labgruppen::amps::ipd::params::slot as ipd_slot;
use labgruppen::amps::ipd::protocol::{CMD_LOAD_PRESET, Key, M_COMMAND, M_MUTE, S_COMMAND, S_MUTE};
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::amps::ipx::fake::{Cmd, FakeAmp};
use labgruppen::amps::ipx::params as ipx_params;
use labgruppen::amps::ipx::protocol::{Address, Env, Main, build_frame};
use labgruppen::amps::ipx::structs::{AUDCFG_SIZE, DeviceState, audio_dword};
use labgruppen::{Snapshot, Value};
use labrack_core::{AmpId, Ceilings, Connector, Error, Hub, Preset, Protocol, Status, Store, Update};
use tokio::time::{sleep, timeout};

const TRUE: Value = Value::Bool(true);
const FALSE: Value = Value::Bool(false);

fn addr(name: &str) -> Address {
    ipx_params::slot(&param(name)).unwrap().addr
}

async fn set_and_wait(hub: &Hub, id: AmpId, name: &str, value: Value) {
    hub.set(id, param(name), value.clone()).unwrap();
    wait_value(hub, id, name, value).await;
}

/// Another controller sets a parameter of the simulated IPD.
async fn ipd_elsewhere_set(sim: &SimServer, name: &str, value: Value) {
    let options = Options { mk2: None, timeout: Duration::from_secs(5) };
    let amp = IpdAmp::connect_with("127.0.0.1", sim.addr.port(), options).await.unwrap();
    labgruppen::Amp::set(&amp, &param(name), value).await.unwrap();
    labgruppen::Amp::close(&amp).await;
}

/// Another controller stores the settings of the simulated IPD in a slot.
async fn ipd_elsewhere_store(sim: &SimServer, slot: u32, name: &str) {
    let options = Options { mk2: None, timeout: Duration::from_secs(5) };
    let amp = IpdAmp::connect_with("127.0.0.1", sim.addr.port(), options).await.unwrap();
    labgruppen::Amp::store_preset(&amp, slot, name).await.unwrap();
    labgruppen::Amp::close(&amp).await;
}

async fn wait_modified(hub: &Hub, id: AmpId, want: bool) {
    let seen = async {
        while hub.modified(id) != want {
            sleep(Duration::from_millis(10)).await;
        }
    };
    timeout(Duration::from_secs(10), seen).await.unwrap_or_else(|_| panic!("modified stayed {}", !want));
}

fn temp_file(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    (dir, path)
}

/// Edit the settings file: `edit` changes the parameter map.
fn edit_file(path: &Path, edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>)) {
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    edit(json["params"].as_object_mut().unwrap());
    std::fs::write(path, serde_json::to_string(&json).unwrap()).unwrap();
}

// ---- mute -------------------------------------------------------------------------------------

// BEH-24: per output and per amplifier
#[tokio::test]
async fn mute_per_output_and_per_amp() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;

    hub.set_mute(id, 1, true).unwrap();
    wait_value(&hub, id, "out2.mute", TRUE).await;
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (FALSE, TRUE));

    hub.mute_amp(id, true).unwrap();
    wait_value(&hub, id, "out1.mute", TRUE).await;
    assert!(hub.amp_muted(id));
    hub.mute_amp(id, false).unwrap();
    wait_value(&hub, id, "out1.mute", FALSE).await;
    wait_value(&hub, id, "out2.mute", FALSE).await;
    assert!(!hub.amp_muted(id));

    assert!(matches!(hub.set_mute(id, 2, true), Err(Error::NotControllable)));
}

#[tokio::test]
async fn mute_per_output_and_per_amp_on_the_ipd() {
    let (hub, id, sim) = online_ipd().await;
    assert!(hub.amp_muted(id), "the simulator starts muted");
    hub.mute_amp(id, false).unwrap();
    wait_value(&hub, id, "out1.mute", FALSE).await;
    wait_value(&hub, id, "out2.mute", FALSE).await;
    hub.set_mute(id, 0, true).unwrap();
    wait_value(&hub, id, "out1.mute", TRUE).await;
    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, FALSE));
}

// BEH-24, BEH-26: the label says what the button does
#[tokio::test]
async fn labels_follow_the_mute_state_of_the_amps_that_are_online() {
    let fake = FakeAmp::start().await;
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    local.route("10.0.0.2", sim.addr.port());
    assert!(!hub.all_muted(), "no amplifier at all");
    let a = hub.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = hub.add_manual("B", Protocol::Ipd, "10.0.0.2").unwrap();
    let off = hub.add_manual("C", Protocol::Ipx, "10.0.0.3").unwrap();
    wait_loaded(&hub, a).await;
    wait_loaded(&hub, b).await;
    wait_status(&hub, off, Status::Unreachable).await;

    // A: out1 muted (fixture), out2 as set; B: both muted
    set_and_wait(&hub, a, "out2.mute", FALSE).await;
    assert!(!hub.amp_muted(a) && hub.amp_muted(b));
    assert!(!hub.all_muted());

    set_and_wait(&hub, a, "out2.mute", TRUE).await;
    assert!(hub.amp_muted(a));
    assert!(hub.all_muted(), "an amp that is not online does not count");
    assert!(!hub.amp_muted(off));

    set_and_wait(&hub, b, "out1.mute", FALSE).await;
    assert!(!hub.all_muted());
}

// BEH-25: MUTE ALL and UNMUTE ALL act on the amps that are ONLINE, and queue nothing for the others
#[tokio::test]
async fn mute_all_acts_on_the_amps_that_are_online() {
    let fake = FakeAmp::start().await;
    let other = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let a = hub.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = hub.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    wait_loaded(&hub, a).await;
    wait_status(&hub, b, Status::Unreachable).await;

    assert_eq!(hub.unmute_all(), 1);
    wait_value(&hub, a, "out1.mute", FALSE).await;
    wait_value(&hub, a, "out2.mute", FALSE).await;
    assert_eq!(hub.mute_all(), 1);
    wait_value(&hub, a, "out1.mute", TRUE).await;
    wait_value(&hub, a, "out2.mute", TRUE).await;
    assert!(hub.all_muted());

    // B shows up later: nothing waits for it
    local.route("10.0.0.2", other.port);
    wait_loaded(&hub, b).await;
    sleep(Duration::from_millis(300)).await;
    assert_eq!(other.writes(), 0);
}

// BEH-25: MUTE ALL and UNMUTE ALL reach the two outputs of an amp and nothing else: no input, no mixer
#[tokio::test(flavor = "multi_thread")]
async fn mute_all_writes_the_two_output_mutes_only() {
    let (hub, id, sim) = online_ipd().await;
    let before = sim.writes().len();
    assert_eq!(hub.unmute_all(), 1);
    wait_value(&hub, id, "out1.mute", FALSE).await;
    wait_value(&hub, id, "out2.mute", FALSE).await;
    assert_eq!(hub.mute_all(), 1);
    wait_value(&hub, id, "out1.mute", TRUE).await;
    wait_value(&hub, id, "out2.mute", TRUE).await;
    sleep(Duration::from_millis(200)).await;
    let mut written: Vec<_> =
        sim.writes()[before..].iter().map(|k| (k.struct_id, k.member_id, k.channel)).collect();
    written.sort();
    written.dedup();
    let mute = |channel| (S_MUTE, M_MUTE, channel);
    assert_eq!(
        written,
        [mute(0x80), mute(0x81)],
        "only the two output mutes, nothing of the inputs or the mixer"
    );

    let (hub, id, fake) = online_ipx(timing()).await;
    let before = fake.writes();
    assert_eq!(hub.unmute_all(), 1);
    wait_value(&hub, id, "out1.mute", FALSE).await;
    wait_value(&hub, id, "out2.mute", FALSE).await;
    assert_eq!(hub.mute_all(), 1);
    wait_value(&hub, id, "out1.mute", TRUE).await;
    wait_value(&hub, id, "out2.mute", TRUE).await;
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes() - before, 4, "two outputs, unmuted and muted");
    for name in ["in1.gain", "in2.gain"] {
        assert_eq!(hub.value(id, &param(name)), Some(ipx_value(&fake, name)), "{name} untouched");
    }
}

// BEH-26: a mute made elsewhere shows at once (IPX: the amp tells)
#[tokio::test]
async fn a_mute_from_the_ipx_front_panel_is_seen() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let push = build_frame(Main::GETPRM, [Env::OUTPUT_LEVEL, 1, 0, 1], &1i32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    wait_value(&hub, id, "out2.mute", TRUE).await;
}

// BEH-12, BEH-26: the IPD does not tell, so the output mutes are asked about every ~100 ms
#[tokio::test(flavor = "multi_thread")]
async fn a_mute_from_the_ipd_front_panel_is_seen() {
    let (hub, id, sim) = online_ipd().await;
    assert_eq!(hub.value(id, &param("out1.mute")), Some(TRUE));
    ipd_elsewhere_set(&sim, "out1.mute", FALSE).await;
    let started = std::time::Instant::now();
    wait_value(&hub, id, "out1.mute", FALSE).await;
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    ipd_elsewhere_set(&sim, "out2.mute", FALSE).await;
    wait_value(&hub, id, "out2.mute", FALSE).await;
    assert!(!hub.amp_muted(id));
}

// ---- recall, store and file load: the outputs end muted (SEC-6, SEC-7) -------------------------
//
// Whatever the preset or the file says, an output is unmuted by the user only.

/// Put a preset into the fake IPX as another tool (the vendor app) stores it, with the outputs unmuted:
/// the current settings with `edits` applied.
fn legacy_preset(fake: &FakeAmp, slot: u32, name: &str, edits: &[(&str, Value)]) {
    let mut st = fake.state();
    let from = DeviceState::AUDIO_OFFSET;
    let mut audio = st.sync[from..from + AUDCFG_SIZE].to_vec();
    for (edited, value) in edits {
        let slot = ipx_params::slot(&param(edited)).unwrap();
        let at = audio_dword(slot.addr).unwrap() * 4;
        let bytes = slot.encode(value).unwrap();
        audio[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    st.stored.insert(slot - 1, (name.to_string(), audio));
}

/// out1 muted by the user, out2 not; slot 7 holds a preset of another tool with both unmuted and
/// -7.5 dB on out1.
async fn ipx_with_a_preset_that_unmutes() -> (Hub, AmpId, FakeAmp) {
    let (hub, id, fake) = online_ipx(timing()).await;
    legacy_preset(&fake, 7, "Loud", &[("out1.mute", FALSE), ("out2.mute", FALSE), ("out1.gain", f(-7.5))]);
    set_and_wait(&hub, id, "out1.mute", TRUE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    set_and_wait(&hub, id, "out1.gain", f(0.0)).await;
    (hub, id, fake)
}

// SEC-6, SEC-7: after a recall every output is muted, even if the preset says unmuted
#[tokio::test]
async fn recall_leaves_every_output_muted_and_mutes_first_on_the_ipx() {
    let (hub, id, fake) = ipx_with_a_preset_that_unmutes().await;
    let before = fake.state().setprm.len();

    hub.recall_preset(id, 7).await.unwrap();

    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, TRUE));
    assert_eq!(ipx_value(&fake, "out1.gain"), f(-7.5), "the preset was applied");
    let st = fake.state();
    let mark = st.recall_marks[0];
    // before the recall: the one output that sounded gets muted; after it, where the preset unmuted
    // both: both are muted again
    assert_eq!(st.setprm[before..mark], [addr("out2.mute")]);
    assert_eq!(st.setprm[mark..], [addr("out1.mute"), addr("out2.mute")]);
    drop(st);
    // the shared state says so at once
    assert_eq!(
        (hub.value(id, &param("out1.mute")), hub.value(id, &param("out2.mute"))),
        (Some(TRUE), Some(TRUE))
    );
    assert!(hub.amp_muted(id));
}

// SEC-6: a preset the app stored has the outputs muted, so recalling it has no unmuted moment at all
#[tokio::test]
async fn recalling_a_preset_the_app_stored_never_unmutes_an_output() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    set_and_wait(&hub, id, "out1.gain", f(-7.5)).await;
    hub.store_preset(id, 8, "Quiet").await.unwrap(); // mutes the outputs first
    set_and_wait(&hub, id, "out1.mute", FALSE).await; // the user decides to unmute
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    set_and_wait(&hub, id, "out1.gain", f(0.0)).await;
    let before = fake.state().setprm.len();

    hub.recall_preset(id, 8).await.unwrap();

    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, TRUE));
    assert_eq!(ipx_value(&fake, "out1.gain"), f(-7.5));
    let st = fake.state();
    let mark = st.recall_marks[0];
    assert_eq!(st.setprm[before..mark], [addr("out1.mute"), addr("out2.mute")], "both muted first");
    assert_eq!(st.setprm.len(), mark, "the preset itself muted them: nothing to put right afterwards");
}

// SEC-6, SEC-7 on the IPD
#[tokio::test(flavor = "multi_thread")]
async fn recall_leaves_every_output_muted_and_mutes_first_on_the_ipd() {
    let (hub, id, sim) = online_ipd().await;
    // a preset another controller stored, with both outputs unmuted
    ipd_elsewhere_set(&sim, "out1.mute", FALSE).await;
    ipd_elsewhere_set(&sim, "out2.mute", FALSE).await;
    ipd_elsewhere_set(&sim, "out1.gain", f(-7.5)).await;
    ipd_elsewhere_store(&sim, 7, "Loud").await;
    ipd_elsewhere_set(&sim, "out1.mute", TRUE).await;
    ipd_elsewhere_set(&sim, "out1.gain", f(0.0)).await;
    let before = sim.writes().len();
    let mute_reads = |sim: &SimServer| {
        sim.reads()
            .iter()
            .filter(|k| [Key::new(S_MUTE, M_MUTE, 0x80, 0), Key::new(S_MUTE, M_MUTE, 0x81, 0)].contains(k))
            .count()
    };
    let reads_before = mute_reads(&sim);

    hub.recall_preset(id, 7).await.unwrap();

    // the mutes are asked for once before the change and once more by the reload of the state; afterwards
    // they are written without asking, because an IPD answers slowly and an unmuted output sounds
    // meanwhile (SEC-6)
    assert!(mute_reads(&sim) - reads_before <= 4, "no read of the mutes between the change and their write");
    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, TRUE));
    assert_eq!(ipd_value(&sim, "out1.gain").await, f(-7.5), "the preset was applied");
    let writes = sim.writes()[before..].to_vec();
    let key = |name: &str| ipd_slot(&param(name)).unwrap().key;
    let load = Key::new(S_COMMAND, M_COMMAND, 0, CMD_LOAD_PRESET);
    let at = writes.iter().position(|k| *k == load).expect("the recall command");
    assert_eq!(writes[0], key("out2.mute"), "the output that sounded is muted before anything else");
    assert!(writes[..at].iter().all(|k| *k != key("out1.mute")), "the muted output is not touched before");
    assert_eq!(writes[at + 1..], [key("out1.mute"), key("out2.mute")], "and both muted again at the end");
    assert_eq!(
        (hub.value(id, &param("out1.mute")), hub.value(id, &param("out2.mute"))),
        (Some(TRUE), Some(TRUE))
    );
}

// SEC-6: storing a preset mutes the outputs first, so the preset carries muted outputs, and leaves them
#[tokio::test]
async fn store_mutes_the_outputs_first_stores_them_muted_and_leaves_them_muted_on_the_ipx() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let before = fake.state().setprm.len();

    hub.store_preset(id, 12, "Church").await.unwrap();

    let st = fake.state();
    assert_eq!(st.setprm[before..st.store_marks[0]], [addr("out1.mute"), addr("out2.mute")], "muted first");
    assert_eq!(st.setprm.len(), st.store_marks[0], "and not put back");
    let (name, audio) = &st.stored[&11];
    assert_eq!(name, "Church");
    for output in ["out1.mute", "out2.mute"] {
        let at = audio_dword(addr(output)).unwrap() * 4;
        assert_eq!(audio[at..at + 4], 1i32.to_le_bytes(), "{output} is muted in the stored preset");
    }
    drop(st);
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, TRUE));
    assert!(hub.amp_muted(id));
    assert_eq!(hub.amps()[0].preset, Some(Preset { number: 12, name: "Church".into() }));
}

#[tokio::test(flavor = "multi_thread")]
async fn store_mutes_the_outputs_first_and_leaves_them_muted_on_the_ipd() {
    let (hub, id, sim) = online_ipd().await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let before = sim.writes().len();

    hub.store_preset(id, 12, "Church").await.unwrap();

    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, TRUE));
    let key = |name: &str| ipd_slot(&param(name)).unwrap().key;
    let writes = sim.writes()[before..].to_vec();
    assert_eq!(writes[..2], [key("out1.mute"), key("out2.mute")], "muted before the save");
    assert!(
        !writes[2..].iter().any(|k| *k == key("out1.mute") || *k == key("out2.mute")),
        "and not put back"
    );
}

// a store the amp refuses (bad name, protected slot) changes nothing, so the app takes back the mute it
// added for it and nothing stays muted
#[tokio::test]
async fn a_refused_store_does_not_leave_the_outputs_muted() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;

    // a name the amp cannot hold is refused before anything is muted
    let writes = fake.writes();
    for (slot, name) in [(5, ""), (5, "Empty"), (5, "this name is far too long")] {
        let got = hub.store_preset(id, slot, name).await;
        assert!(matches!(got, Err(Error::Amp(labgruppen::Error::InvalidValue(_)))), "{name:?}: {got:?}");
    }
    assert_eq!(fake.writes(), writes, "nothing was muted for them");
    let got = hub.store_preset(id, 1, "Protected").await;
    assert!(matches!(got, Err(Error::Amp(labgruppen::Error::Denied(_)))), "{got:?}");

    assert!(fake.state().store_payloads.is_empty());
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (FALSE, FALSE));
    assert_eq!(
        (hub.value(id, &param("out1.mute")), hub.value(id, &param("out2.mute"))),
        (Some(FALSE), Some(FALSE))
    );
}

// SEC-6: if the outputs cannot be muted, nothing is stored
#[tokio::test]
async fn nothing_is_stored_when_the_outputs_cannot_be_muted() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    fake.state().reject_writes.insert(addr("out2.mute"));

    let got = hub.store_preset(id, 12, "Church").await;
    assert!(matches!(got, Err(Error::LeftMuted(_))), "{got:?}");
    assert!(fake.state().store_payloads.is_empty());
    assert_eq!(hub.amps()[0].preset.as_ref().map(|p| p.number), Some(2), "the loaded preset is as it was");
}

// BEH-11, BEH-38: the loaded preset, MODIFIED and the state after a recall
#[tokio::test]
async fn recall_sets_the_loaded_preset_clears_modified_and_reads_the_state_again() {
    let (hub, id, _fake) = ipx_with_a_preset_that_unmutes().await;
    assert!(hub.modified(id), "the app changed things");
    let mut rx = hub.subscribe();

    hub.recall_preset(id, 7).await.unwrap();
    assert!(!hub.modified(id));
    assert_eq!(hub.amps()[0].preset, Some(Preset { number: 7, name: "Loud".into() }));
    wait_loaded(&hub, id).await;
    assert_eq!(hub.value(id, &param("out1.gain")), Some(f(-7.5)));

    let mut seen = Vec::new();
    while let Ok(update) = rx.try_recv() {
        match update {
            Update::Modified(i, m) if i == id => seen.push(format!("modified {m}")),
            Update::Preset(i) if i == id => seen.push("preset".into()),
            _ => {}
        }
    }
    assert!(seen.contains(&"preset".to_string()) && seen.contains(&"modified false".to_string()), "{seen:?}");
}

// BEH-36: a store tells the views by a Preset update, which is what makes the app read the list of presets
// again (it does so once, on that update, and not a second time itself)
#[tokio::test]
async fn a_store_is_followed_by_a_preset_update() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let mut rx = hub.subscribe();

    hub.store_preset(id, 12, "Church").await.unwrap();

    let mut presets = 0;
    while let Ok(update) = rx.try_recv() {
        presets += usize::from(matches!(update, Update::Preset(i) if i == id));
    }
    assert_eq!(presets, 1, "one update, so one read of the list");
}

// SEC-6: if the recall cannot be done, the outputs that were muted for it stay muted
#[tokio::test]
async fn when_muting_fails_nothing_is_recalled() {
    let (hub, id, fake) = ipx_with_a_preset_that_unmutes().await;
    fake.state().reject_writes.insert(addr("out2.mute"));

    let got = hub.recall_preset(id, 7).await;
    assert!(matches!(got, Err(Error::LeftMuted(_))), "{got:?}");
    assert!(fake.state().recalled.is_empty(), "no recall after a mute that did not hold");
    assert_eq!(ipx_value(&fake, "out1.mute"), TRUE);

    // the amp is usable again
    fake.state().reject_writes.clear();
    set_and_wait(&hub, id, "out2.delay", f(1.0)).await;
}

#[tokio::test]
async fn a_recall_needs_a_valid_slot_and_an_online_amp() {
    let (hub, id, fake) = online_ipx(timing()).await;
    for slot in [0, 51, 1000] {
        let got = hub.recall_preset(id, slot).await;
        assert!(matches!(got, Err(Error::Invalid(_))), "{slot}: {got:?}");
    }
    assert_eq!(fake.writes(), 0, "not even a mute");

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(matches!(hub.recall_preset(id, 7).await, Err(Error::NotOnline)));
    hub.remove(id).unwrap();
    assert!(matches!(hub.recall_preset(id, 7).await, Err(Error::UnknownAmp)));
}

// BEH-6, BEH-36: the slots the views list are the amp's own, 1 to 50; none when it is not ONLINE
#[tokio::test]
async fn the_preset_slots_are_the_amp_s() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    assert_eq!(hub.preset_slots(id), Some(1..=50));
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert_eq!(hub.preset_slots(id), None);
}

// the fader value waiting for its turn is not sent after the recall over the recalled settings
#[tokio::test]
async fn a_value_waiting_for_its_turn_does_not_outlive_a_recall() {
    let slow = labrack_core::Timing { write_interval: Duration::from_millis(700), ..timing() };
    let (hub, id, fake) = online_ipx(slow).await;
    set_and_wait(&hub, id, "out1.gain", f(-7.5)).await;
    hub.store_preset(id, 7, "Loud").await.unwrap();
    set_and_wait(&hub, id, "out1.gain", f(-20.0)).await;
    hub.set(id, param("out1.gain"), f(-30.0)).unwrap(); // waits for its turn

    hub.recall_preset(id, 7).await.unwrap();
    sleep(Duration::from_millis(1200)).await;
    assert_eq!(ipx_value(&fake, "out1.gain"), f(-7.5), "the stale -30 was dropped");
}

// ---- store, list, files -----------------------------------------------------------------------

// BEH-36, BEH-37
#[tokio::test]
async fn store_preset_names_the_slot_and_makes_it_the_loaded_one() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.gain", f(-3.0)).await;
    assert!(hub.modified(id));

    hub.store_preset(id, 12, "  Church  ").await.unwrap();
    assert_eq!(hub.amps()[0].preset, Some(Preset { number: 12, name: "Church".into() }));
    assert!(!hub.modified(id));
    assert_eq!(fake.state().store_payloads.len(), 1);

    for (slot, name) in [(0, "x"), (51, "x"), (5, ""), (5, "Empty"), (5, "this name is far too long")] {
        let got = hub.store_preset(id, slot, name).await;
        assert!(matches!(got, Err(Error::Invalid(_)) | Err(Error::Amp(_))), "{slot} {name:?}: {got:?}");
    }
    assert_eq!(fake.state().store_payloads.len(), 1);
    // a protected slot is the amp's to refuse
    let got = hub.store_preset(id, 1, "x").await;
    assert!(matches!(got, Err(Error::Amp(labgruppen::Error::Denied(_)))), "{got:?}");
    assert_eq!(hub.amps()[0].preset.as_ref().map(|p| p.number), Some(12), "unchanged");
}

#[tokio::test(flavor = "multi_thread")]
async fn presets_lists_the_slots() {
    let (hub, id, _sim) = online_ipd().await;
    let list = hub.presets(id).await.unwrap();
    let list: Vec<_> = list.iter().map(|p| (p.number, p.name.as_str())).collect();
    assert_eq!(list, [(1, "Default"), (2, "Sub"), (3, "Top")]);

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(matches!(hub.presets(id).await, Err(Error::NotOnline)));
}

// BEH-37: the file is the library's snapshot
#[tokio::test]
async fn save_file_writes_the_snapshot() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let snap: Snapshot = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!((snap.format, snap.driver.as_str()), (Snapshot::FORMAT, "ipx"));
    assert!(snap.params.len() > 250);
    assert_eq!(snap.params["out1.gain"], f(-2.5));

    let missing = Path::new("/nonexistent-dir/x.json");
    assert!(matches!(hub.save_file(id, missing).await, Err(Error::Io(_))));
}

// BEH-37, SEC-6, SEC-7: a file brings back the settings, never an output mute, and every output ends
// muted: the user unmutes
#[tokio::test]
async fn load_file_restores_settings_and_leaves_every_output_muted() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out1.mute", TRUE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    edit_file(&path, |p| {
        p.insert("out1.mute".into(), serde_json::json!(false)); // hostile: would unmute
        p.insert("out2.mute".into(), serde_json::json!(true));
    });

    let eq_freq = ipx_value(&fake, "in1.eq.2.freq");
    set_and_wait(&hub, id, "out1.gain", f(-20.0)).await;
    set_and_wait(&hub, id, "in1.eq.2.freq", Value::Int(4000)).await;
    wait_modified(&hub, id, true).await;
    let before = fake.state().setprm.len();

    let report = hub.load_file(id, &path).await.unwrap();

    assert_eq!(ipx_value(&fake, "out1.gain"), f(-2.5));
    assert_eq!(ipx_value(&fake, "in1.eq.2.freq"), eq_freq);
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, TRUE));
    let writes = fake.state().setprm[before..].to_vec();
    assert_eq!(writes.first(), Some(&addr("out2.mute")), "the output that sounded is muted first");
    assert_eq!(writes.iter().filter(|a| **a == addr("out2.mute")).count(), 1, "and not put back");
    assert!(!writes.contains(&addr("out1.mute")), "the muted output is not touched, whatever the file says");
    assert!(report.applied > 100, "{}", report.applied);
    assert!(report.skipped.iter().any(|s| s.starts_with("out1.mute")), "{:?}", report.skipped);
    assert!(report.skipped.iter().any(|s| s.starts_with("out2.mute")));
    assert!(hub.modified(id), "the file is not the loaded preset");
    wait_loaded(&hub, id).await;
    assert_eq!(hub.value(id, &param("out1.gain")), Some(f(-2.5)));
}

// SEC-2, SEC-3: nothing of an invalid file is written, not even the mutes
#[tokio::test]
async fn load_file_refuses_a_file_with_invalid_values_before_writing_anything() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let writes = fake.writes();

    for (name, value) in [
        ("out1.gain", serde_json::json!(99.0)),
        ("out1.gain", serde_json::json!(1e300)),
        ("in1.mixer.aes1.level", serde_json::json!(6.0)),
        ("out1.eq.0.type", serde_json::json!("wobble")),
        ("out1.delay", serde_json::json!("long")),
    ] {
        let fresh = temp_file("bad.json");
        std::fs::copy(&path, &fresh.1).unwrap();
        edit_file(&fresh.1, |p| {
            p.insert(name.into(), value.clone());
        });
        let got = hub.load_file(id, &fresh.1).await;
        assert!(matches!(got, Err(Error::Invalid(_))), "{name}: {got:?}");
        assert_eq!(fake.writes(), writes, "{name}: nothing was written");
        assert_eq!(ipx_value(&fake, "out2.mute"), FALSE, "{name}: not even muted for it");
    }
}

// what the app cannot set yet is skipped and reported, not written and not an error
#[tokio::test]
async fn load_file_skips_what_it_cannot_set() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    edit_file(&path, |p| {
        p.insert("out1.polarity".into(), serde_json::json!(true));
        p.insert("in1.comp.ratio".into(), serde_json::json!(99.0));
        p.insert("out9.gain".into(), serde_json::json!(1.0));
        p.insert("no.such.thing".into(), serde_json::json!(1.0));
    });
    let ratio = ipx_value(&fake, "in1.comp.ratio");
    let polarity = ipx_value(&fake, "out1.polarity");

    let report = hub.load_file(id, &path).await.unwrap();
    for name in ["out1.polarity", "in1.comp.ratio", "out9.gain", "no.such.thing"] {
        assert!(report.skipped.iter().any(|s| s.starts_with(name)), "{name}: {:?}", report.skipped);
    }
    // what was skipped is as it was (the restored limiter moves by the 4 decimals the file keeps)
    assert_eq!(ipx_value(&fake, "in1.comp.ratio"), ratio);
    assert_eq!(ipx_value(&fake, "out1.polarity"), polarity);
}

// BEH-5, BEH-37, SEC-2: the limiter threshold is dBV in an IPD's file and Vpk in an IPX's: a file of the
// other family leaves the limiter alone (32.56 dBV read as 32.56 Vpk is a threshold far too low); what the
// families share is loaded as usual
#[tokio::test]
async fn load_file_of_the_other_family_leaves_the_limiter_alone() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["driver"] = "ipd".into();
    json["params"]["out1.limiter.threshold"] = serde_json::json!(32.56);
    json["params"]["out2.limiter.threshold"] = serde_json::json!(32.56);
    json["params"]["out1.gain"] = serde_json::json!(-10.0);
    std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();
    let limiter: Vec<_> = ["threshold", "threshold_dbu", "power", "impedance"]
        .map(|field| ipx_value(&fake, &format!("out1.limiter.{field}")))
        .into();
    let writes = fake.state().setprm.len();

    let report = hub.load_file(id, &path).await.unwrap();

    let after: Vec<_> = ["threshold", "threshold_dbu", "power", "impedance"]
        .map(|field| ipx_value(&fake, &format!("out1.limiter.{field}")))
        .into();
    assert_eq!(after, limiter, "the limiter is as it was");
    let touched = fake.state().setprm[writes..].to_vec();
    assert!(!touched.iter().any(|a| a.env == 17), "no limiter address was written: {touched:?}");
    assert!(
        report.skipped.iter().any(|s| s.starts_with("out1.limiter.threshold:")),
        "the limiter is reported as skipped: {:?}",
        report.skipped
    );
    assert_eq!(ipx_value(&fake, "out1.gain"), f(-10.0), "the shared settings are loaded");
}

// BEH-37, BEH-5: the amp stores a hair more than its range (36.02 dBu for the 35.99 dBu of 69 Vpk): its own
// file loads, with the nearest value in range
#[tokio::test]
async fn load_file_takes_the_limiter_a_hair_above_the_range() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    edit_file(&path, |p| {
        p.insert("out1.limiter.threshold_dbu".into(), serde_json::json!(36.02));
        p.insert("out1.limiter.threshold".into(), serde_json::json!(69.24));
    });

    let report = hub.load_file(id, &path).await.unwrap();

    assert!(report.skipped.iter().all(|s| !s.starts_with("out1.limiter.th")), "{:?}", report.skipped);
    let Value::Float(dbu) = ipx_value(&fake, "out1.limiter.threshold_dbu") else { panic!() };
    assert!((dbu - 35.99).abs() < 1e-3, "the nearest value in range: {dbu}");
}

#[tokio::test]
async fn load_file_refuses_files_that_are_not_settings() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let writes = fake.writes();
    let (_dir, path) = temp_file("x.json");

    std::fs::write(&path, "not json").unwrap();
    assert!(matches!(hub.load_file(id, &path).await, Err(Error::Invalid(_))));
    std::fs::write(&path, r#"{"format": 99, "driver": "ipx", "model": "x", "params": {}}"#).unwrap();
    assert!(matches!(hub.load_file(id, &path).await, Err(Error::Invalid(_))));
    std::fs::write(&path, r#"{"hello": 1}"#).unwrap();
    assert!(matches!(hub.load_file(id, &path).await, Err(Error::Invalid(_))));
    assert!(matches!(hub.load_file(id, &path.with_file_name("missing.json")).await, Err(Error::Io(_))));
    assert_eq!(fake.writes(), writes);
}

// SEC-3: a file cannot get past the ceiling either
#[tokio::test]
async fn load_file_honours_the_ceilings() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap(); // out1.gain is -2.5
    hub.set_ceilings(id, Ceilings { max_output_gain_db: Some(-6.0), max_limiter_dbu: None }).unwrap();
    let writes = fake.writes();

    let got = hub.load_file(id, &path).await;
    assert!(matches!(&got, Err(Error::Invalid(why)) if why.contains("ceiling")), "{got:?}");
    assert_eq!(fake.writes(), writes);
}

// SEC-6: a load that fails halfway leaves the outputs muted, and says so
#[tokio::test]
async fn a_load_that_fails_leaves_the_outputs_muted() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    let (_dir, path) = temp_file("amp.json");
    hub.save_file(id, &path).await.unwrap();
    set_and_wait(&hub, id, "out2.gain", f(-30.0)).await;
    fake.state().reject_writes.insert(addr("out2.gain"));

    let got = hub.load_file(id, &path).await;
    assert!(matches!(got, Err(Error::LeftMuted(_))), "{got:?}");
    assert_eq!(ipx_value(&fake, "out2.mute"), TRUE, "muted for the load, not put back");
    assert_eq!(ipx_value(&fake, "out1.mute"), TRUE);

    // the amp is usable again, and the user can unmute on purpose
    fake.state().reject_writes.clear();
    wait_loaded(&hub, id).await; // the state is read again after a failure; an unmute needs it
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
}

// ---- MODIFIED (BEH-38) --------------------------------------------------------------------------

#[tokio::test]
async fn modified_follows_changes_from_the_app_and_from_the_amp() {
    let (hub, id, fake) = online_ipx(timing()).await;
    assert!(!hub.modified(id), "reading the state is not a change");
    let mut rx = hub.subscribe();

    // the same value is not a change
    hub.set(id, param("out1.gain"), f(-2.5)).unwrap();
    sleep(Duration::from_millis(300)).await;
    assert!(!hub.modified(id));

    hub.set(id, param("out1.gain"), f(-3.0)).unwrap();
    wait_modified(&hub, id, true).await;
    let got = timeout(Duration::from_secs(5), async {
        loop {
            if let Update::Modified(i, m) = rx.recv().await.unwrap()
                && i == id
            {
                return m;
            }
        }
    })
    .await
    .unwrap();
    assert!(got);

    // clear it by recalling, then a change from the amp sets it again
    hub.store_preset(id, 4, "x").await.unwrap();
    assert!(!hub.modified(id));
    let push = build_frame(Main::DEVPRMCHAGE, [Env::OUTPUT_LEVEL, 0, 0, 0], &5.5f32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    wait_modified(&hub, id, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_the_state_of_an_ipd_is_not_a_modification() {
    let (hub, id, sim) = online_ipd().await;
    sleep(Duration::from_millis(500)).await;
    assert!(!hub.modified(id));
    // the IPD tells nothing but the mutes, which are asked for
    ipd_elsewhere_set(&sim, "out2.mute", FALSE).await;
    wait_modified(&hub, id, true).await;
}

// BEH-38, SEC-7: after a reconnect the flag is cleared, the mutes are as they were, nothing is written
#[tokio::test]
async fn a_reconnect_clears_modified_and_changes_no_mute() {
    let (hub, id, fake) = online_ipx(timing()).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;
    set_and_wait(&hub, id, "out1.gain", f(-9.0)).await;
    wait_modified(&hub, id, true).await;
    let (writes, state) = (fake.writes(), fake.state().sync.clone());

    let mut rx = hub.subscribe();
    fake.cmd.send(Cmd::Drop).unwrap();
    next_status(&mut rx, id, Status::Reconnecting).await;
    next_status(&mut rx, id, Status::Online).await;
    wait_loaded(&hub, id).await;

    assert!(!hub.modified(id));
    assert_eq!(fake.writes(), writes);
    assert_eq!(fake.state().sync, state);
    assert_eq!(hub.value(id, &param("out1.mute")), Some(TRUE));
    assert_eq!(hub.value(id, &param("out2.mute")), Some(FALSE));
}

// ---- a bulk change that fails after the amp may have applied it ------------------------------------

/// An IPX whose recall is sent, and so applied, but reported as not confirmed: the echo of an IPD's
/// command that comes too late looks the same.
struct Unconfirmed(Arc<dyn labgruppen::Amp>);

#[async_trait]
impl labgruppen::Amp for Unconfirmed {
    fn info(&self) -> &labgruppen::AmpInfo {
        self.0.info()
    }
    fn params(&self) -> Vec<labgruppen::Param> {
        self.0.params()
    }
    fn spec(&self, p: &labgruppen::Param) -> Option<labgruppen::ParamSpec> {
        self.0.spec(p)
    }
    async fn get(&self, p: &labgruppen::Param) -> labgruppen::Result<Value> {
        self.0.get(p).await
    }
    async fn set(&self, p: &labgruppen::Param, v: Value) -> labgruppen::Result<()> {
        self.0.set(p, v).await
    }
    async fn status(&self) -> labgruppen::Result<labgruppen::Status> {
        self.0.status().await
    }
    fn preset_slots(&self) -> std::ops::RangeInclusive<u32> {
        self.0.preset_slots()
    }
    async fn presets(&self) -> labgruppen::Result<Vec<labgruppen::PresetInfo>> {
        self.0.presets().await
    }
    async fn recall_preset(&self, index: u32) -> labgruppen::Result<()> {
        self.0.recall_preset(index).await?;
        Err(labgruppen::Error::NotConfirmed("SET key (11, 26, 0, 1)".into()))
    }
    async fn store_preset(&self, slot: u32, name: &str) -> labgruppen::Result<()> {
        self.0.store_preset(slot, name).await
    }
    fn meters(&self) -> labgruppen::Meters {
        self.0.meters()
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<labgruppen::AmpEvent> {
        self.0.subscribe()
    }
    async fn close(&self) {
        self.0.close().await
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    async fn snapshot(&self) -> labgruppen::Result<labgruppen::Snapshot> {
        self.0.snapshot().await
    }
}

struct UnconfirmedRecall(u16);

#[async_trait]
impl Connector for UnconfirmedRecall {
    async fn connect(&self, _: Protocol, _: &str) -> labgruppen::Result<Arc<dyn labgruppen::Amp>> {
        let amp =
            labgruppen::amps::ipx::IpxAmp::connect_with("127.0.0.1", self.0, Duration::from_secs(1)).await?;
        Ok(Arc::new(Unconfirmed(Arc::new(amp))))
    }
}

// SEC-6, SEC-7: a recall the amp applied but did not confirm must not leave the outputs unmuted while
// the app says they are muted: the outputs are muted again after the failure, and the state is read again
#[tokio::test]
async fn a_recall_that_fails_after_the_amp_applied_it_still_ends_muted() {
    let fake = FakeAmp::start().await;
    legacy_preset(&fake, 7, "Loud", &[("out1.mute", FALSE), ("out2.mute", FALSE), ("out1.gain", f(-7.5))]);
    let hub =
        Hub::new(Store::in_memory(), Arc::new(UnconfirmedRecall(fake.port)), Heard::nothing(), timing());
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
    set_and_wait(&hub, id, "out2.mute", FALSE).await;

    let done = hub.recall_preset(id, 7).await;

    assert!(matches!(done, Err(Error::LeftMuted(_))), "{done:?}");
    let settled = async {
        while ipx_value(&fake, "out1.gain") != f(-7.5) || !hub.loaded(id) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    timeout(Duration::from_secs(5), settled).await.expect("the recall was applied and the state read again");
    assert_eq!((ipx_value(&fake, "out1.mute"), ipx_value(&fake, "out2.mute")), (TRUE, TRUE), "the amp");
    assert_eq!(
        (hub.value(id, &param("out1.mute")), hub.value(id, &param("out2.mute"))),
        (Some(TRUE), Some(TRUE)),
        "what the app shows is what the amp holds"
    );
    assert_eq!(hub.value(id, &param("out1.gain")), Some(f(-7.5)), "the state was read again");
}

// SEC-6, SEC-7: an unmute asked for while a bulk change runs is refused, not queued: it would be written
// after the change and undo "leave the outputs muted". Muting stays possible.
#[tokio::test(flavor = "multi_thread")]
async fn an_unmute_asked_for_during_a_recall_is_refused() {
    let (hub, id, sim) = online_ipd().await;
    hub.store_preset(id, 7, "Scene").await.unwrap();
    sim.set_latency(Duration::from_millis(40));
    let recall = tokio::spawn({
        let hub = hub.clone();
        async move { hub.recall_preset(id, 7).await }
    });
    sleep(Duration::from_millis(150)).await;
    assert!(!recall.is_finished(), "the recall is still running");

    let got = hub.set_mute(id, 0, false);
    assert!(matches!(got, Err(Error::Busy)), "{got:?}");
    hub.unmute_all(); // refused for this amp, and says nothing
    assert!(hub.set_mute(id, 1, true).is_ok(), "muting is never held back");

    recall.await.unwrap().unwrap();
    sim.set_latency(Duration::ZERO);
    wait_loaded(&hub, id).await;
    assert_eq!((ipd_value(&sim, "out1.mute").await, ipd_value(&sim, "out2.mute").await), (TRUE, TRUE));
    // and when it is over, the user can unmute on purpose
    set_and_wait(&hub, id, "out1.mute", FALSE).await;
}

// SEC-6: a setting asked for while a bulk change runs is refused, not queued: it would be written over the
// recalled settings. Muting stays possible.
#[tokio::test(flavor = "multi_thread")]
async fn a_setting_asked_for_during_a_recall_is_refused() {
    let (hub, id, sim) = online_ipd().await;
    ipd_elsewhere_set(&sim, "out1.gain", f(-20.0)).await;
    ipd_elsewhere_store(&sim, 7, "Quiet").await;
    ipd_elsewhere_set(&sim, "out1.gain", f(0.0)).await;
    sim.set_latency(Duration::from_millis(40));
    let recall = tokio::spawn({
        let hub = hub.clone();
        async move { hub.recall_preset(id, 7).await }
    });
    sleep(Duration::from_millis(150)).await;
    assert!(!recall.is_finished(), "the recall is still running");

    assert!(matches!(hub.set(id, param("out1.gain"), f(3.0)), Err(Error::Busy)));
    assert!(matches!(hub.set(id, param("in1.eq.0.gain"), f(3.0)), Err(Error::Busy)));
    assert!(matches!(hub.set_limiter(id, 0, labrack_core::Unit::Dbu, 30.0), Err(Error::Busy)));
    assert!(hub.set_mute(id, 1, true).is_ok(), "muting is never held back");

    recall.await.unwrap().unwrap();
    sim.set_latency(Duration::ZERO);
    wait_loaded(&hub, id).await;
    assert_eq!(ipd_value(&sim, "out1.gain").await, f(-20.0), "the recalled gain was not overwritten");
}

// SEC-6: an unmute that waits for its turn when a store starts is dropped, not written after the store
#[tokio::test]
async fn an_unmute_waiting_when_a_store_starts_is_dropped() {
    let slow = labrack_core::Timing { write_interval: Duration::from_millis(700), ..timing() };
    let (hub, id, fake) = online_ipx(slow).await;
    set_and_wait(&hub, id, "out1.mute", TRUE).await;
    hub.set(id, param("out1.mute"), FALSE).unwrap(); // waits for its turn

    hub.store_preset(id, 7, "Probe").await.unwrap();
    sleep(Duration::from_millis(1500)).await;
    assert_eq!(ipx_value(&fake, "out1.mute"), TRUE, "the output was unmuted after the store");
}
