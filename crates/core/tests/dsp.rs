//! DSP control against the IPD simulator and the fake IPX: state, safe writes, ceilings, coalescing.

mod common;

use std::time::{Duration, Instant};

use common::*;
use labgruppen::Value;
use labgruppen::amps::ipd::params::slot as ipd_slot;
use labgruppen::amps::ipx::fake::{Cmd, FakeAmp};
use labgruppen::amps::ipx::params as ipx_params;
use labgruppen::amps::ipx::protocol::{Env, Main, build_frame};
use labrack_core::{Ceilings, Error, Protocol, Status, Store, Timing, Update};
use tokio::time::{sleep, timeout};

// BEH-8, SEC-1: the state is read when the amp goes ONLINE, and nothing is written
#[tokio::test]
async fn the_state_is_read_on_connect_without_a_write() {
    let (hub, id, fake) = online_ipx(timing()).await;
    assert_eq!(hub.value(id, &param("out1.gain")), Some(f(-2.5)));
    assert_eq!(hub.value(id, &param("in1.gain")), Some(f(-21.0)));
    assert_eq!(hub.value(id, &param("out1.mute")), Some(Value::Bool(true)));
    assert_eq!(hub.value(id, &param("out1.eq.1.freq")), Some(Value::Int(920)));
    assert!(hub.values(id).len() > 250);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), 0);

    let (hub, id, sim) = online_ipd().await;
    assert!(hub.value(id, &param("in2.eq.4.freq")).is_some());
    assert!(hub.values(id).len() > 250);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(sim.writes(), []);
}

// the loaded state is announced once, and forgotten when the amp is not ONLINE
#[tokio::test]
async fn the_state_is_announced_and_forgotten_when_offline() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let mut rx = hub.subscribe();
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    let loaded = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Update::Loaded(i)) = rx.recv().await
                && i == id
            {
                return;
            }
        }
    })
    .await;
    assert!(loaded.is_ok());
    assert!(hub.loaded(id) && !hub.values(id).is_empty());

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(!hub.loaded(id) && hub.values(id).is_empty());
    assert_eq!(hub.value(id, &param("out1.gain")), None);
}

// BEH-27 to BEH-30: gain, mute, mixer, EQ, filters and delay reach the amp and the shared state
#[tokio::test]
async fn controls_are_written_to_the_ipx_and_kept_in_the_state() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let table = [
        ("out1.gain", f(3.25)),
        ("in2.gain", f(-10.0)),
        ("out2.mute", Value::Bool(true)),
        ("in1.mute", Value::Bool(false)),
        ("in1.mixer.aes2.level", f(-12.5)),
        ("in2.mixer.analog1.mute", Value::Bool(true)),
        ("out1.eq.3.freq", Value::Int(1000)),
        ("out1.eq.3.gain", f(-4.5)),
        ("out1.eq.3.q", f(2.5)),
        ("out1.eq.3.type", Value::Text("notch".into())),
        ("in1.eq.0.enabled", Value::Bool(true)),
        ("out2.xover.hp.freq", Value::Int(80)),
        ("out2.xover.lp.type", Value::Text("bessel_12".into())),
        ("out2.xover.lp.enabled", Value::Bool(true)),
        ("out1.delay", f(12.5)),
        ("in1.delay", f(0.5)),
    ];
    for (name, value) in &table {
        hub.set(id, param(name), value.clone()).unwrap();
        // one parameter at a time: a second value for the same one would wait for the interval
        wait_value(&hub, id, name, value.clone()).await;
        assert_eq!(&ipx_value(&fake, name), value, "{name} on the amp");
    }
    assert_eq!(fake.writes(), table.len());
}

#[tokio::test]
async fn controls_are_written_to_the_ipd_and_kept_in_the_state() {
    let (hub, id, sim) = online_ipd().await;
    let table = [
        ("out1.gain", f(3.25)),
        ("in2.gain", f(-10.0)),
        // the simulator starts muted: a different value, so waiting for it waits for the write
        ("out2.mute", Value::Bool(false)),
        ("in1.mixer.aes2.level", f(-12.5)),
        ("in1.eq.3.freq", Value::Int(1000)),
        ("out1.eq.3.type", Value::Text("notch".into())),
        ("out2.xover.hp.freq", Value::Int(80)),
        ("out1.delay", f(12.5)),
    ];
    for (name, value) in &table {
        hub.set(id, param(name), value.clone()).unwrap();
        wait_value(&hub, id, name, value.clone()).await;
        assert!(sim.writes().contains(&ipd_slot(&param(name)).unwrap().key), "{name} on the amp");
    }
}

// the amp rounds what the user typed to what the control takes
#[tokio::test]
async fn integers_are_rounded_like_the_amp_does() {
    let (hub, id, fake) = online_ipx(timing()).await;
    hub.set(id, param("out1.eq.2.freq"), f(1000.4)).unwrap();
    wait_value(&hub, id, "out1.eq.2.freq", Value::Int(1000)).await;
    assert_eq!(ipx_value(&fake, "out1.eq.2.freq"), Value::Int(1000));
}

// the amp tells about changes made elsewhere; the state follows
#[tokio::test]
async fn changes_from_the_amp_update_the_state() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let push = build_frame(Main::DEVPRMCHAGE, [Env::OUTPUT_LEVEL, 0, 0, 0], &5.5f32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    wait_value(&hub, id, "out1.gain", f(5.5)).await;
}

// BEH-10, SEC-1: an amp that is not ONLINE takes no intents, and nothing is queued for later
#[tokio::test]
async fn intents_for_an_amp_that_is_not_online_are_rejected_and_not_queued() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Unreachable).await;
    assert!(matches!(hub.set(id, param("out1.gain"), f(0.0)), Err(Error::NotOnline)));

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(matches!(hub.set(id, param("out1.gain"), f(0.0)), Err(Error::NotOnline)));

    hub.set_ip(id, "10.0.0.").unwrap();
    hub.connect(id).unwrap();
    wait_status(&hub, id, Status::CheckIp).await;
    assert!(matches!(hub.set(id, param("out1.mute"), Value::Bool(false)), Err(Error::NotOnline)));

    // the amp appears; none of the rejected intents shows up
    local.route("10.0.0.1", fake.port);
    hub.set_ip(id, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), 0);
}

#[tokio::test]
async fn intents_for_an_unknown_amp_are_rejected() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    hub.remove(id).unwrap();
    assert!(matches!(hub.set(id, param("out1.gain"), f(0.0)), Err(Error::UnknownAmp)));
}

// SEC-2: nothing outside the model range, no NaN or infinity, and no value of the wrong kind is sent
#[tokio::test]
async fn values_outside_the_range_are_never_sent() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let (hub2, id2, sim) = online_ipd().await;
    let bad = [
        ("out1.gain", f(12.01)),
        ("out1.gain", f(-48.01)),
        ("out1.gain", f(1e30)),
        ("out1.gain", f(f64::NAN)),
        ("out1.gain", f(f64::INFINITY)),
        ("out1.gain", f(f64::NEG_INFINITY)),
        ("out1.gain", Value::Bool(true)),
        ("out1.gain", Value::Text("loud".into())),
        ("in1.gain", Value::Int(13)),
        ("out2.delay", f(-0.1)),
        ("out2.delay", f(2000.1)),
        ("out1.eq.3.q", f(0.19)),
        ("out1.eq.3.q", f(25.1)),
        ("out1.eq.3.gain", f(12.1)),
        ("out1.eq.3.freq", Value::Int(19)),
        ("out1.eq.3.freq", Value::Int(20001)),
        ("out1.eq.3.type", Value::Text("wobble".into())),
        ("out1.xover.hp.freq", Value::Int(18)),
        ("out1.xover.lp.freq", Value::Int(20002)),
        ("in1.mixer.aes1.level", f(0.1)),
        ("out1.mute", f(0.0)),
        ("out1.mute", Value::Text("maybe".into())),
    ];
    for (name, value) in &bad {
        for (hub, id) in [(&hub, id), (&hub2, id2)] {
            let got = hub.set(id, param(name), value.clone());
            assert!(matches!(got, Err(Error::Invalid(_))), "{name} = {value:?}: {got:?}");
        }
    }
    sleep(Duration::from_millis(300)).await;
    assert_eq!((fake.writes(), sim.writes()), (0, vec![]));

    // the edges are fine
    for (name, value) in [("out1.gain", f(12.0)), ("in1.gain", f(-48.0)), ("in1.mixer.aes1.level", f(0.0))] {
        hub.set(id, param(name), value.clone()).unwrap();
        wait_value(&hub, id, name, value).await;
    }
}

// only the controls of this change can be set through it
#[tokio::test]
async fn other_parameters_are_not_controllable_yet() {
    let (hub, id, fake) = online_ipx(timing()).await;
    for name in [
        "out1.limiter.threshold",
        "out1.limiter.impedance",
        "in1.comp.ratio",
        "out1.polarity",
        "out1.link",
        "in1.link",
        "out1.name",
        "in1.xover.hp.freq",
        "out1.mixer.aes1.level",
    ] {
        let got = hub.set(id, param(name), f(1.0));
        assert!(matches!(got, Err(Error::NotControllable)), "{name}: {got:?}");
    }
    // a channel the amps do not have
    assert!(matches!(hub.set(id, param("out3.gain"), f(0.0)), Err(Error::NotControllable)));
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), 0);
}

// UX, backlog: the range the controls show is the model's, so the IPD mixer tops out at 0 dB
#[tokio::test]
async fn the_range_of_a_control_is_the_models() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let r = hub.range(id, &param("in1.mixer.aes1.level")).unwrap();
    assert_eq!((r.min, r.max, r.unit), (Some(-48.0), Some(0.0), "dB"));
    let r = hub.range(id, &param("out1.gain")).unwrap();
    assert_eq!((r.min, r.max, r.unit), (Some(-48.0), Some(12.0), "dB"));
    let r = hub.range(id, &param("out1.eq.0.freq")).unwrap();
    assert_eq!((r.min, r.max, r.unit), (Some(20.0), Some(20000.0), "Hz"));
    assert_eq!(hub.range(id, &param("out1.mute")).map(|r| r.max), Some(None));
    assert!(hub.range(id, &param("out1.limiter.threshold")).is_none(), "not controllable yet");

    let (hub, id, _sim) = online_ipd().await;
    let r = hub.range(id, &param("in2.mixer.analog1.level")).unwrap();
    assert_eq!((r.min, r.max), (Some(-48.0), Some(0.0)));

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(hub.range(id, &param("out1.gain")).is_none());
}

// SEC-3: an output gain above the ceiling is refused, and the control's range ends at the ceiling
#[tokio::test]
async fn the_output_gain_ceiling_holds() {
    let (hub, id, fake) = online_ipx(timing()).await;
    hub.set_ceilings(id, Ceilings { max_output_gain_db: Some(-6.0), max_limiter_dbu: None }).unwrap();

    assert_eq!(hub.range(id, &param("out1.gain")).unwrap().max, Some(-6.0));
    assert_eq!(hub.range(id, &param("out2.gain")).unwrap().max, Some(-6.0));
    assert_eq!(hub.range(id, &param("in1.gain")).unwrap().max, Some(12.0), "inputs have no ceiling");

    for gain in [-5.99, 0.0, 12.0] {
        let got = hub.set(id, param("out2.gain"), f(gain));
        assert!(matches!(got, Err(Error::AboveCeiling { .. })), "{gain}: {got:?}");
    }
    assert_eq!(fake.writes(), 0);

    hub.set(id, param("out2.gain"), f(-6.0)).unwrap();
    wait_value(&hub, id, "out2.gain", f(-6.0)).await;
    hub.set(id, param("in1.gain"), f(12.0)).unwrap();
    wait_value(&hub, id, "in1.gain", f(12.0)).await;

    hub.set_ceilings(id, Ceilings::default()).unwrap();
    hub.set(id, param("out2.gain"), f(0.0)).unwrap();
    wait_value(&hub, id, "out2.gain", f(0.0)).await;
}

#[test]
fn the_ceiling_text_names_the_limit() {
    let e = Error::AboveCeiling { ceiling: -6.0, unit: "dB" };
    assert_eq!(e.to_string(), "Above the ceiling of -6 dB");
}

// SEC-3, BEH-40: ceilings are saved; lowering one writes nothing to the amp
#[tokio::test]
async fn ceilings_are_saved_and_lowering_one_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let fake = FakeAmp::start().await;
    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;

    let ceilings = Ceilings { max_output_gain_db: Some(-30.0), max_limiter_dbu: Some(30.0) };
    hub.set_ceilings(id, ceilings).unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), 0, "out1.gain is -2.5, above the new ceiling, and stays");
    assert_eq!(hub.value(id, &param("out1.gain")), Some(f(-2.5)));
    assert_eq!(Store::open(&path).unwrap().amps()[0].ceilings, ceilings);

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let c = Ceilings { max_output_gain_db: Some(bad), max_limiter_dbu: None };
        assert!(matches!(hub.set_ceilings(id, c), Err(Error::Invalid(_))));
        let c = Ceilings { max_output_gain_db: None, max_limiter_dbu: Some(bad) };
        assert!(matches!(hub.set_ceilings(id, c), Err(Error::Invalid(_))));
    }
    assert_eq!(hub.amps()[0].ceilings, ceilings);
}

// SEC-4: while a fader moves only the latest value is sent, at most every 50 ms per parameter
#[tokio::test]
async fn a_moving_fader_is_written_at_most_every_50_ms() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let started = Instant::now();
    for step in 0..60 {
        hub.set(id, param("out1.gain"), f(-40.0 + f64::from(step) * 0.5)).unwrap();
        hub.set(id, param("in1.gain"), f(-40.0 + f64::from(step) * 0.5)).unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    let moved = started.elapsed();
    let last = f(-40.0 + 59.0 * 0.5);
    wait_value(&hub, id, "out1.gain", last.clone()).await;
    wait_value(&hub, id, "in1.gain", last.clone()).await;

    // per parameter: the first write at once, then one per 50 ms at most (and one more for the end)
    let limit = (moved.as_millis() / 50) as usize + 2;
    let setprm = fake.state().setprm.clone();
    for name in ["out1.gain", "in1.gain"] {
        let addr = ipx_params::slot(&param(name)).unwrap().addr;
        let n = setprm.iter().filter(|a| **a == addr).count();
        assert!((2..=limit).contains(&n), "{name}: {n} writes in {moved:?}, at most {limit}");
        assert_eq!(ipx_value(&fake, name), last, "the latest value is the one that stays");
    }
}

// SEC-4: the interval is a setting of the hub; the first value of a parameter is not delayed
#[tokio::test]
async fn the_first_write_goes_out_at_once_the_next_waits() {
    let slow = Timing { write_interval: Duration::from_millis(600), ..timing() };
    let (hub, id, fake) = online_ipx(slow).await;
    hub.set(id, param("out1.gain"), f(1.0)).unwrap();
    wait_value(&hub, id, "out1.gain", f(1.0)).await;
    let first = Instant::now();
    hub.set(id, param("out1.gain"), f(2.0)).unwrap();
    hub.set(id, param("out1.gain"), f(3.0)).unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!(ipx_value(&fake, "out1.gain"), f(1.0), "held back");
    wait_value(&hub, id, "out1.gain", f(3.0)).await;
    assert!(first.elapsed() >= Duration::from_millis(500), "{:?}", first.elapsed());
    assert_eq!(fake.writes(), 2, "2.0 was never sent");
}

// SEC-1: a value waiting for its turn is dropped when the connection ends, not sent after a reconnect
#[tokio::test]
async fn a_waiting_value_is_dropped_when_the_connection_is_lost() {
    let slow = Timing { write_interval: Duration::from_millis(800), ..timing() };
    let (hub, id, fake) = online_ipx(slow).await;
    hub.set(id, param("out1.gain"), f(1.0)).unwrap();
    wait_value(&hub, id, "out1.gain", f(1.0)).await;
    hub.set(id, param("out1.gain"), f(2.0)).unwrap(); // waits for its turn

    let mut rx = hub.subscribe();
    fake.cmd.send(Cmd::Drop).unwrap();
    next_status(&mut rx, id, Status::Reconnecting).await;
    next_status(&mut rx, id, Status::Online).await;
    sleep(Duration::from_millis(1200)).await;

    assert_eq!(ipx_value(&fake, "out1.gain"), f(1.0));
    assert_eq!(fake.writes(), 1);
}

// a write the amp does not take is reported, and the state keeps what the amp has
#[tokio::test]
async fn a_failed_write_is_reported() {
    let (hub, id, fake) = online_ipx(timing()).await;
    fake.state().reject_writes.insert(ipx_params::slot(&param("out2.gain")).unwrap().addr);
    let mut rx = hub.subscribe();
    hub.set(id, param("out2.gain"), f(-6.0)).unwrap();

    let failed = timeout(Duration::from_secs(10), async {
        loop {
            if let Update::WriteFailed(i, p, why) = rx.recv().await.unwrap()
                && i == id
            {
                return (p, why);
            }
        }
    })
    .await
    .expect("the failure is reported");
    assert_eq!(failed.0, param("out2.gain"));
    assert!(failed.1.contains("not confirmed"), "{}", failed.1);
    assert_eq!(hub.value(id, &param("out2.gain")), Some(ipx_value(&fake, "out2.gain")));
}

// the views hear about confirmed writes, from this app or from elsewhere, the same way
#[tokio::test]
async fn confirmed_writes_are_announced_as_changes() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let mut rx = hub.subscribe();
    hub.set(id, param("out1.delay"), f(4.0)).unwrap();
    let got = timeout(Duration::from_secs(10), async {
        loop {
            if let Update::Changed(i, p, v) = rx.recv().await.unwrap()
                && i == id
                && p == param("out1.delay")
            {
                return v;
            }
        }
    })
    .await
    .expect("the change is announced");
    assert_eq!(got, f(4.0));
}
