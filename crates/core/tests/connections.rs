//! Connections against the IPD simulator and the fake IPX (ARCH-6).

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::amps::ipx::fake::{Cmd, FakeAmp};
use labgruppen::amps::ipx::protocol::{Env, Main, build_frame};
use labgruppen::{Meters, Param, Value};
use labrack_core::{Preset, Protocol, Status, Store, Update};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout};

async fn sim(mk2: bool) -> SimServer {
    SimServer::spawn("127.0.0.1:0".parse().unwrap(), mk2, 0).await.unwrap()
}

// BEH-8, BEH-20: a new amp is CONNECTING… at once, then ONLINE; BEH-7, BEH-11: the loaded preset is
// read; SEC-1: connecting writes nothing
#[tokio::test]
async fn an_ipx_goes_online_without_any_write() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    assert_eq!(hub.status(id), Some(Status::Connecting));

    wait_status(&hub, id, Status::Online).await;
    sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.writes(), 0);
    // wire index 1 is slot 2 (BEH-6)
    assert_eq!(hub.amps()[0].preset, Some(Preset { number: 2, name: "base".into() }));
}

// BEH-8, SEC-1
#[tokio::test(flavor = "multi_thread")]
async fn an_ipd_goes_online_without_any_write() {
    for mk2 in [true, false] {
        let sim = sim(mk2).await;
        let (hub, local) = hub();
        local.route("10.0.0.1", sim.addr.port());
        let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();

        wait_status(&hub, id, Status::Online).await;
        sleep(Duration::from_millis(500)).await;
        assert_eq!(sim.writes(), [], "mk2: {mk2}");
        assert_eq!(hub.amps()[0].preset, Some(Preset { number: 1, name: "Default".into() }), "mk2: {mk2}");
    }
}

// BEH-1, BEH-39: both families side by side, one list
#[tokio::test(flavor = "multi_thread")]
async fn ipx_and_ipd_are_online_side_by_side() {
    let fake = FakeAmp::start().await;
    let sim = sim(true).await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    local.route("10.0.0.2", sim.addr.port());
    let a = hub.add_manual("", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = hub.add_manual("", Protocol::Ipd, "10.0.0.2").unwrap();

    wait_status(&hub, a, Status::Online).await;
    wait_status(&hub, b, Status::Online).await;
    assert_eq!((fake.writes(), sim.writes()), (0, vec![]));
}

// BEH-8, BEH-42, SEC-1: the app starts with the amps of the config and only reads
#[tokio::test]
async fn at_start_every_connected_amp_connects_and_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let fake = FakeAmp::start().await;
    {
        let mut store = Store::open(&path).unwrap();
        store.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    }
    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", fake.port);
    let id = hub.amps()[0].id;
    wait_status(&hub, id, Status::Online).await;
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), 0);
}

// BEH-8: BUSY while another controller holds the IPX; the app keeps trying
#[tokio::test]
async fn busy_amp_shows_busy_and_connects_when_the_other_controller_leaves() {
    let fake = FakeAmp::start().await;
    fake.state().busy = true;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();

    wait_status(&hub, id, Status::Busy).await;
    let tries = local.attempts();
    sleep(Duration::from_millis(300)).await;
    assert!(local.attempts() > tries, "the app keeps retrying");

    fake.state().busy = false;
    wait_status(&hub, id, Status::Online).await;
    assert_eq!(fake.writes(), 0);
}

// BEH-8: UNREACHABLE when nothing answers; the app keeps trying
#[tokio::test]
async fn unreachable_amp_is_retried_until_it_answers() {
    let (hub, local) = hub();
    local.route("10.0.0.1", closed_port().await);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Unreachable).await;

    let fake = FakeAmp::start().await;
    local.route("10.0.0.1", fake.port);
    wait_status(&hub, id, Status::Online).await;
}

// BEH-8: a port that accepts and says nothing is UNREACHABLE too
#[tokio::test]
async fn a_silent_amp_is_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = listener.accept().await {
            held.push(sock);
        }
    });
    let (hub, local) = hub();
    local.route("10.0.0.1", port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Unreachable).await;
}

// BEH-8, BEH-10: no route for the IP is UNREACHABLE, not a crash
#[tokio::test]
async fn an_ip_without_a_route_is_unreachable() {
    let (hub, _local) = hub();
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Unreachable).await;
}

// BEH-8, BEH-10: an invalid IP is CHECK IP and nothing is tried; fixing it connects
#[tokio::test]
async fn an_invalid_ip_is_check_ip_and_releases_the_amp() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    hub.set_ip(id, "10.0.0.").unwrap();
    next_status(&mut rx, id, Status::CheckIp).await;
    assert!(another_controller_can_connect(fake.port).await, "the old connection is closed");

    let tries = local.attempts();
    sleep(Duration::from_millis(300)).await;
    assert_eq!(local.attempts(), tries, "nothing is tried while the IP is invalid");

    hub.set_ip(id, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;
}

// BEH-8: an amp added with a duplicate or invalid IP is refused before any task starts
#[tokio::test]
async fn refused_adds_start_no_connection() {
    let (hub, local) = hub();
    hub.add_manual("Left", Protocol::Ipx, "nope").unwrap_err();
    sleep(Duration::from_millis(100)).await;
    assert_eq!((hub.amps().len(), local.attempts()), (0, 0));
}

// BEH-10: editing the IP reconnects to the new address and releases the old one
#[tokio::test]
async fn editing_the_ip_reconnects() {
    let (a, b) = (FakeAmp::start().await, FakeAmp::start().await);
    let (hub, local) = hub();
    local.route("10.0.0.1", a.port);
    local.route("10.0.0.2", b.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    hub.set_ip(id, "10.0.0.2").unwrap();
    next_status(&mut rx, id, Status::Connecting).await;
    next_status(&mut rx, id, Status::Online).await;
    assert_eq!(local.attempts(), 2);
    assert!(another_controller_can_connect(a.port).await, "the old amp is released");
    assert_eq!((a.writes(), b.writes()), (0, 0));
}

// the name is not part of the connection
#[tokio::test]
async fn renaming_does_not_reconnect() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    hub.rename(id, "Front").unwrap();
    sleep(Duration::from_millis(300)).await;
    assert_eq!((local.attempts(), hub.status(id)), (1, Some(Status::Online)));
}

// BEH-9: DISCONNECT releases an IPX so another controller can connect; CONNECT takes it back
#[tokio::test]
async fn disconnect_releases_the_ipx_and_connect_takes_it_back() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    hub.disconnect(id).unwrap();
    next_status(&mut rx, id, Status::Disconnected).await;
    assert!(!hub.amps()[0].connected);
    assert!(another_controller_can_connect(fake.port).await);

    let tries = local.attempts();
    sleep(Duration::from_millis(300)).await;
    assert_eq!(local.attempts(), tries, "a disconnected amp is left alone");

    hub.connect(id).unwrap();
    next_status(&mut rx, id, Status::Online).await;
    assert!(hub.amps()[0].connected);
    assert_eq!(fake.writes(), 0);
}

// BEH-9, BEH-40: the choice is kept after a restart, and the restarted app does not connect
#[tokio::test]
async fn disconnect_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let fake = FakeAmp::start().await;
    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    drop(hub);

    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", fake.port);
    let id = hub.amps()[0].id;
    assert_eq!(hub.status(id), Some(Status::Disconnected));
    sleep(Duration::from_millis(300)).await;
    assert_eq!((hub.status(id), local.attempts()), (Some(Status::Disconnected), 0));

    hub.connect(id).unwrap();
    wait_status(&hub, id, Status::Online).await;
}

// BEH-9: editing the IP of a disconnected amp does not connect it, and CONNECT with an invalid IP is
// CHECK IP
#[tokio::test]
async fn a_disconnected_amp_stays_disconnected_when_its_ip_is_edited() {
    let (hub, local) = hub();
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    let tries = local.attempts();

    hub.set_ip(id, "10.0.0.").unwrap();
    hub.set_ip(id, "10.0.0.7").unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!((hub.status(id), local.attempts()), (Some(Status::Disconnected), tries));

    hub.set_ip(id, "10.0.0.").unwrap();
    hub.connect(id).unwrap();
    wait_status(&hub, id, Status::CheckIp).await;
}

// BEH-23: removing an amp closes its connection and forgets it
#[tokio::test]
async fn removing_an_amp_closes_its_connection() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    hub.remove(id).unwrap();
    assert_eq!((hub.status(id), hub.meters(id)), (None, None));
    assert!(hub.amps().is_empty());
    assert!(another_controller_can_connect(fake.port).await);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(local.attempts(), 1, "a removed amp is not retried");
}

// BEH-23: removing an amp that is still connecting stops the attempts
#[tokio::test]
async fn removing_an_unreachable_amp_stops_the_retries() {
    let (hub, local) = hub();
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Unreachable).await;
    hub.remove(id).unwrap();
    sleep(Duration::from_millis(100)).await;
    let tries = local.attempts();
    sleep(Duration::from_millis(300)).await;
    assert_eq!(local.attempts(), tries);
}

// BEH-8, BEH-42, SEC-1, SEC-7: a lost IPX connection is RECONNECTING…, comes back ONLINE, and the
// reconnect writes nothing (so it cannot unmute anything)
#[tokio::test]
async fn a_lost_ipx_reconnects_without_writing() {
    let fake = FakeAmp::start().await;
    let slow = labrack_core::Timing { retry_first: Duration::from_millis(300), ..timing() };
    let (hub, local) = hub_with(Store::in_memory(), slow);
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;
    let state = fake.state().sync.clone();

    let mut rx = hub.subscribe();
    fake.cmd.send(Cmd::Drop).unwrap();
    next_status(&mut rx, id, Status::Reconnecting).await;
    next_status(&mut rx, id, Status::Online).await;

    assert_eq!(fake.writes(), 0);
    assert_eq!(fake.state().sync, state);
}

// BEH-8, BEH-42, SEC-1, SEC-7: the same for an IPD whose connection is lost and comes back
#[tokio::test(flavor = "multi_thread")]
async fn a_lost_ipd_reconnects_without_writing() {
    let first = sim(true).await;
    let addr = first.addr;
    let (hub, local) = hub();
    local.route("10.0.0.1", addr.port());
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    drop(first);
    next_status(&mut rx, id, Status::Reconnecting).await;

    let second = timeout(Duration::from_secs(5), async {
        loop {
            match SimServer::spawn(addr, true, 0).await {
                Ok(s) => return s,
                Err(_) => sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("the simulator's port came free again");
    next_status(&mut rx, id, Status::Online).await;
    sleep(Duration::from_millis(300)).await;
    assert_eq!(second.writes(), []);
}

// BEH-12, PERF-2: meters reach the UI about every 100 ms, even when the amp sends them faster
#[tokio::test]
async fn meters_are_forwarded_at_about_10_hz() {
    let fake = FakeAmp::start().await; // pushes a meter frame every 40 ms
    let (hub, local) = hub_with(Store::in_memory(), labrack_core::Timing::default());
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    let mut seen: Vec<Meters> = Vec::new();
    let _ = timeout(Duration::from_secs(2), async {
        while let Ok(update) = rx.recv().await {
            if let Update::Meters(i, m) = update
                && i == id
            {
                seen.push(m);
            }
        }
    })
    .await;
    assert!((14..=22).contains(&seen.len()), "{} updates in 2 s", seen.len());
    assert_eq!(seen[0].output, [-150.0, -150.0]);
    assert_eq!(hub.meters(id).map(|m| m.output), Some(vec![-150.0, -150.0]));
}

// BEH-10: an amp that is not ONLINE shows no signal and no preset
#[tokio::test]
async fn an_amp_that_is_not_online_has_no_signal_and_no_preset() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub_with(Store::in_memory(), labrack_core::Timing::default());
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    assert_eq!(hub.meters(id), Some(Meters::default()));
    wait_status(&hub, id, Status::Online).await;
    timeout(Duration::from_secs(3), async {
        while hub.meters(id).is_some_and(|m| m.output.is_empty()) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("meters arrive");
    assert!(hub.amps()[0].preset.is_some());

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert_eq!(hub.meters(id), Some(Meters::default()));
    assert_eq!(hub.amps()[0].preset, None);
}

// BEH-8: changes made elsewhere (front panel, another controller) are passed on
#[tokio::test]
async fn changes_from_the_amp_are_forwarded() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;

    let mut rx = hub.subscribe();
    let push = build_frame(Main::DEVPRMCHAGE, [Env::OUTPUT_LEVEL, 0, 0, 0], &5.5f32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    let got = timeout(Duration::from_secs(3), async {
        loop {
            if let Update::Changed(i, p, v) = rx.recv().await.unwrap()
                && i == id
            {
                return (p, v);
            }
        }
    })
    .await
    .expect("the change arrives");
    assert_eq!(got, ("out1.gain".parse::<Param>().unwrap(), Value::Float(5.5)));
}

// the last Hub handle going away closes every connection
#[tokio::test]
async fn dropping_the_hub_closes_the_connections() {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub();
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_status(&hub, id, Status::Online).await;
    drop(hub);
    assert!(another_controller_can_connect(fake.port).await);
}
