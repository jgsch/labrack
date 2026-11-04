//! Discovery: which protocols are searched, and what is done with what is found.

mod common;

use std::time::Duration;

use common::*;
use labrack_core::{Lan, Protocol, Scan, Source, Status, Store, Switches, Update};
use tokio::time::sleep;

const OFF: Switches = Switches { ipd: false, ipx: false };

/// A hub whose switches are `switches`, saved before it starts, so that starting scans nothing.
fn hub_at(switches: Switches, heard: &std::sync::Arc<Heard>) -> labrack_core::Hub {
    let mut store = Store::in_memory();
    store.set_switches(switches).unwrap();
    hub_finding(store, heard.clone())
}

// BEH-17, BEH-18: the app does not search by itself when it starts
#[tokio::test]
async fn starting_the_app_searches_nothing() {
    let heard = Heard::nothing();
    let hub = hub_finding(Store::in_memory(), heard.clone());
    sleep(Duration::from_millis(200)).await;
    assert_eq!((heard.searched(), hub.scanning()), (vec![], false));
}

// BEH-18: switching a protocol on starts a scan of that protocol only
#[tokio::test]
async fn switching_a_protocol_on_scans_that_protocol() {
    let heard = Heard::nothing();
    let hub = hub_at(OFF, &heard);
    let mut rx = hub.subscribe();

    hub.set_switches(Switches { ipd: false, ipx: true }).unwrap();
    scan_finished(&mut rx).await;
    assert_eq!(heard.searched(), [Protocol::Ipx]);

    hub.set_switches(Switches { ipd: true, ipx: true }).unwrap();
    scan_finished(&mut rx).await;
    assert_eq!(heard.searched(), [Protocol::Ipx, Protocol::Ipd], "the IPX is not searched again");
}

// BEH-17: switching a protocol off, or changing nothing, scans nothing
#[tokio::test]
async fn switching_off_or_not_changing_scans_nothing() {
    let heard = Heard::nothing();
    let hub = hub_at(Switches { ipd: true, ipx: true }, &heard);
    hub.set_switches(Switches { ipd: true, ipx: false }).unwrap();
    hub.set_switches(Switches { ipd: true, ipx: false }).unwrap();
    hub.set_switches(OFF).unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!(heard.searched(), []);
    assert_eq!(hub.switches(), OFF);
}

// BEH-17: with both switches off scanning is disabled and no discovery traffic is sent
#[tokio::test]
async fn with_both_off_scan_now_is_disabled() {
    let heard = Heard::nothing();
    let hub = hub_at(OFF, &heard);
    let mut rx = hub.subscribe();
    assert!(!hub.scan_now());
    sleep(Duration::from_millis(200)).await;
    assert_eq!((heard.searched(), hub.scanning()), (vec![], false));
    assert!(rx.try_recv().is_err(), "not even a scan started");
}

// BEH-17, BEH-18: SCAN NOW searches the protocols that are on, and only those
#[tokio::test]
async fn scan_now_searches_every_protocol_that_is_on() {
    for (switches, want) in [
        (Switches { ipd: true, ipx: true }, 2),
        (Switches { ipd: true, ipx: false }, 1),
        (Switches { ipd: false, ipx: true }, 1),
    ] {
        let heard = Heard::nothing();
        let hub = hub_at(switches, &heard);
        let mut rx = hub.subscribe();
        assert!(hub.scan_now());
        scan_finished(&mut rx).await;
        let searched = heard.searched();
        assert_eq!(searched.len(), want, "{switches:?}");
        assert_eq!(searched.contains(&Protocol::Ipd), switches.ipd);
        assert_eq!(searched.contains(&Protocol::Ipx), switches.ipx);
    }
}

// BEH-18: a scan runs for a while; SCAN NOW does nothing while one runs
#[tokio::test]
async fn a_scan_has_a_state_and_scan_now_waits_for_it() {
    let heard = Heard::nothing();
    *heard.delay.lock().unwrap() = Duration::from_millis(300);
    let hub = hub_at(Switches { ipd: false, ipx: true }, &heard);
    let mut rx = hub.subscribe();

    assert!(hub.scan_now());
    assert!(hub.scanning());
    assert!(matches!(rx.recv().await.unwrap(), Update::Scan(Scan::Started)));
    assert!(!hub.scan_now(), "a scan is running");
    scan_finished(&mut rx).await;
    assert!(!hub.scanning());
    assert_eq!(heard.searched(), [Protocol::Ipx]);

    assert!(hub.scan_now(), "SCAN NOW works again");
}

// BEH-19, BEH-20: what is found is added with source AUTO and a default name, in address order
#[tokio::test]
async fn found_amps_are_added_as_auto() {
    let heard = Heard::nothing();
    heard.hear(Protocol::Ipx, &["10.0.0.9", "10.0.0.2"]);
    heard.hear(Protocol::Ipd, &["10.0.0.5"]);
    let hub = hub_at(Switches { ipd: true, ipx: true }, &heard);
    let mut rx = hub.subscribe();

    assert!(hub.scan_now());
    let (added, errors) = scan_finished(&mut rx).await;
    assert!(errors.is_empty(), "{errors:?}");

    let amps = hub.amps();
    let summary: Vec<_> =
        amps.iter().map(|a| (a.name.as_str(), a.protocol, a.ip.as_str(), a.source)).collect();
    assert_eq!(
        summary,
        [
            ("Amp 1", Protocol::Ipx, "10.0.0.2", Source::Auto),
            ("Amp 2", Protocol::Ipd, "10.0.0.5", Source::Auto),
            ("Amp 3", Protocol::Ipx, "10.0.0.9", Source::Auto),
        ]
    );
    assert_eq!(added, amps.iter().map(|a| a.id).collect::<Vec<_>>());
    // they join the list like any amp: the app connects to them
    for amp in &amps {
        assert!(hub.status(amp.id).is_some_and(|s| s != Status::Disconnected));
    }
}

// BEH-19: an IP that is already known is never added twice
#[tokio::test]
async fn a_known_ip_is_not_added_again() {
    let heard = Heard::nothing();
    heard.hear(Protocol::Ipx, &["10.0.0.1", "10.0.0.2"]);
    heard.hear(Protocol::Ipd, &["10.0.0.2", "10.0.0.3"]);
    let hub = hub_at(Switches { ipd: true, ipx: true }, &heard);
    hub.add_manual("Mine", Protocol::Ipx, "10.0.0.1").unwrap();
    let mut rx = hub.subscribe();

    hub.scan_now();
    let (added, _) = scan_finished(&mut rx).await;
    assert_eq!(added.len(), 2);
    assert_eq!(hub.amps().len(), 3);
    assert_eq!(hub.amps()[0].source, Source::Manual, "the amp the user added stays MANUAL");

    hub.scan_now();
    let (added, _) = scan_finished(&mut rx).await;
    assert_eq!((added.len(), hub.amps().len()), (0, 3));
}

// BEH-17: what a protocol finds after its switch went off is dropped
#[tokio::test]
async fn results_of_a_protocol_switched_off_meanwhile_are_dropped() {
    let heard = Heard::nothing();
    heard.hear(Protocol::Ipx, &["10.0.0.1"]);
    heard.hear(Protocol::Ipd, &["10.0.0.2"]);
    *heard.delay.lock().unwrap() = Duration::from_millis(300);
    let hub = hub_at(Switches { ipd: true, ipx: true }, &heard);
    let mut rx = hub.subscribe();

    hub.scan_now();
    sleep(Duration::from_millis(50)).await;
    hub.set_switches(Switches { ipd: true, ipx: false }).unwrap();
    scan_finished(&mut rx).await;
    let ips: Vec<_> = hub.amps().into_iter().map(|a| a.ip).collect();
    assert_eq!(ips, ["10.0.0.2"]);
}

// BEH-19: one protocol failing does not lose what the other found; the failure is reported
#[tokio::test]
async fn a_failed_search_is_reported_and_the_other_still_counts() {
    let heard = Heard::nothing();
    heard.fail(Protocol::Ipx, "port 52350 is taken");
    heard.hear(Protocol::Ipd, &["10.0.0.5"]);
    let hub = hub_at(Switches { ipd: true, ipx: true }, &heard);
    let mut rx = hub.subscribe();

    hub.scan_now();
    let (added, errors) = scan_finished(&mut rx).await;
    assert_eq!(added.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("IPX: ") && errors[0].contains("port 52350 is taken"), "{errors:?}");
}

// BEH-40, BEH-17: the switches are saved with the config
#[tokio::test]
async fn the_switches_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let hub = hub_finding(Store::open(&path).unwrap(), Heard::nothing());
    assert_eq!(hub.switches(), Switches { ipd: true, ipx: true });
    hub.set_switches(Switches { ipd: true, ipx: false }).unwrap();
    drop(hub);

    let hub = hub_finding(Store::open(&path).unwrap(), Heard::nothing());
    assert_eq!(hub.switches(), Switches { ipd: true, ipx: false });
}

// BEH-17, BEH-18, BEH-19: the real IPX search hears the amp's UDP broadcast
#[tokio::test]
async fn the_lan_finder_hears_an_ipx_broadcast() {
    use labgruppen::amps::ipx::protocol::{Main, build_frame};
    use labrack_core::Finder;

    let finder = tokio::spawn(async { Lan.find(Protocol::Ipx, Duration::from_millis(800)).await });
    sleep(Duration::from_millis(200)).await;

    let mut payload = vec![0u8; 0x48];
    payload[..4].copy_from_slice(b"ipx1");
    payload[16..22].copy_from_slice(b"Rack 1");
    payload[48..57].copy_from_slice(b"10.1.2.30");
    payload[0x40..0x42].copy_from_slice(&50168u16.to_le_bytes());
    let datagram = build_frame(Main::DEVBROADCAST, [0; 4], &payload).unwrap();
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for _ in 0..2 {
        sender.send_to(&datagram, "127.0.0.1:52350").await.unwrap();
    }

    match finder.await.unwrap() {
        Ok(found) => assert_eq!(found, ["10.1.2.30".parse::<std::net::Ipv4Addr>().unwrap()]),
        // Port 52350 taken by something else on this machine.
        Err(labgruppen::Error::Io(e)) => eprintln!("skipping the broadcast test: {e}"),
        Err(e) => panic!("{e}"),
    }
}
