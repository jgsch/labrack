//! Settings of the amp itself: the front-panel display (BEH-49) and the network of an IPD (BEH-50).

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::{DisplayMode, Network};
use labrack_core::{AmpId, Error, Hub, Protocol, Status, Update};
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

async fn wait_display(hub: &Hub, id: AmpId, want: Option<DisplayMode>) {
    let seen = async {
        while hub.display(id) != want {
            sleep(Duration::from_millis(10)).await;
        }
    };
    timeout(Duration::from_secs(5), seen).await.unwrap_or_else(|_| panic!("display {:?}", hub.display(id)));
}

/// The next `Update::Display` of `id`, within a second.
async fn next_display(rx: &mut broadcast::Receiver<Update>, id: AmpId) -> Option<()> {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(Update::Display(i)) if i == id => return,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("{e}"),
            }
        }
    };
    timeout(Duration::from_secs(1), wait).await.ok()
}

fn net(ip: Option<&str>, mask: &str) -> Network {
    Network { ip: ip.map(|ip| ip.parse().unwrap()), mask: mask.parse().unwrap() }
}

// BEH-49: the display of an ONLINE amp is read, set from the app and the views are told; it is gone when the amp
// is not ONLINE
#[tokio::test(flavor = "multi_thread")]
async fn the_display_is_read_and_set() {
    let (hub, id, _sim) = online_ipd().await;
    wait_display(&hub, id, Some(DisplayMode::On)).await;
    let mut updates = hub.subscribe();
    hub.set_display(id, DisplayMode::Dimmed).await.unwrap();
    assert_eq!(hub.display(id), Some(DisplayMode::Dimmed));
    assert!(next_display(&mut updates, id).await.is_some(), "the views are told");

    let (hub, id, fake) = online_ipx(timing()).await;
    wait_display(&hub, id, Some(DisplayMode::On)).await;
    hub.set_display(id, DisplayMode::DelayedOff).await.unwrap();
    assert_eq!(hub.display(id), Some(DisplayMode::DelayedOff));
    assert_eq!(fake.state().sync[0x62C], 1);

    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert_eq!(hub.display(id), None);
}

// BEH-49: a locked amp's display is not changed; a display the amp does not take is an error and the app shows
// what the amp has
#[tokio::test(flavor = "multi_thread")]
async fn the_display_is_held_back_while_locked_and_read_again_after_a_failure() {
    let (hub, id, fake) = online_ipx(timing()).await;
    wait_display(&hub, id, Some(DisplayMode::On)).await;
    hub.set_locked(id, true).await.unwrap();
    assert!(matches!(hub.set_display(id, DisplayMode::Dimmed).await, Err(Error::AccessDenied(_))));
    hub.set_locked(id, false).await.unwrap();

    let field0 =
        labgruppen::amps::ipx::protocol::Address::new(labgruppen::amps::ipx::protocol::Env::DEVCFG, 0, 0, 0);
    fake.state().reject_writes.insert(field0);
    assert!(hub.set_display(id, DisplayMode::Dimmed).await.is_err());
    assert_eq!(hub.display(id), Some(DisplayMode::On));
}

/// An IPD at 10.0.0.1 and a second amp at 10.0.0.9, both ONLINE; 10.0.0.2 leads to the IPD too.
async fn ipd_and_another() -> (Hub, AmpId, SimServer) {
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let other = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let (hub, local) = hub();
    for ip in ["10.0.0.1", "10.0.0.2"] {
        local.route(ip, sim.addr.port());
    }
    local.route("10.0.0.9", other.addr.port());
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    hub.add_manual("Other", Protocol::Ipd, "10.0.0.9").unwrap();
    wait_loaded(&hub, id).await;
    std::mem::forget(other);
    (hub, id, sim)
}

fn ip_of(hub: &Hub, id: AmpId) -> String {
    hub.amps().into_iter().find(|a| a.id == id).unwrap().ip
}

// BEH-50: the network of an IPD is read; a fixed address moves the amp's row there and the app connects to it
#[tokio::test(flavor = "multi_thread")]
async fn an_ipd_is_moved_to_a_fixed_address_and_followed() {
    let (hub, id, _sim) = ipd_and_another().await;
    assert_eq!(hub.network(id).await.unwrap(), net(Some("192.168.1.183"), "255.255.255.0"));

    let mut updates = hub.subscribe();
    hub.set_network(id, net(Some("10.0.0.2"), "255.255.255.0")).await.unwrap();
    assert_eq!(ip_of(&hub, id), "10.0.0.2");
    // the old connection can still look ONLINE for a moment: wait for the new one
    next_status(&mut updates, id, Status::Online).await;
    wait_loaded(&hub, id).await;
    assert_eq!(hub.network(id).await.unwrap(), net(Some("10.0.0.2"), "255.255.255.0"));
}

// BEH-50: automatic keeps the row's address; the amp is found with a scan
#[tokio::test(flavor = "multi_thread")]
async fn automatic_keeps_the_row_address() {
    let (hub, id, sim) = ipd_and_another().await;
    hub.set_network(id, net(None, "255.255.255.0")).await.unwrap();
    assert_eq!(ip_of(&hub, id), "10.0.0.1");
    let ip = labgruppen::amps::ipd::protocol::Key::new(13, 87, 0, 0);
    assert_eq!(sim.value(ip), Some(labgruppen::amps::ipd::protocol::Raw::Int(0)));
}

// BEH-50: an address or mask that is not usable, or the address of another amp, is refused before anything is
// written; a locked IPD and an IPX have no network settings to change
#[tokio::test(flavor = "multi_thread")]
async fn a_bad_network_is_refused_before_it_is_written() {
    let (hub, id, sim) = ipd_and_another().await;
    let writes = sim.writes().len();
    for (ip, mask) in [
        ("10.0.0.2", "0.0.0.0"),
        ("10.0.0.2", "255.0.255.0"),
        ("10.0.0.2", "255.255.255.254"),
        ("10.0.0.0", "255.255.255.0"),
        ("10.0.0.255", "255.255.255.0"),
        ("0.0.0.5", "255.255.255.0"),
        ("127.0.0.5", "255.0.0.0"),
        ("224.0.0.5", "255.255.255.0"),
        ("10.0.0.9", "255.255.255.0"),
    ] {
        let refused = hub.set_network(id, net(Some(ip), mask)).await;
        assert!(matches!(refused, Err(Error::Invalid(_) | Error::DuplicateIp)), "{ip}/{mask}: {refused:?}");
    }
    assert_eq!(sim.writes().len(), writes, "nothing was written");
    assert_eq!(ip_of(&hub, id), "10.0.0.1");

    sim.set_access(5, 0);
    let locked = async {
        while !hub.access(id).is_some_and(|a| a.locked) {
            sleep(Duration::from_millis(20)).await;
        }
    };
    timeout(Duration::from_secs(10), locked).await.unwrap();
    let refused = hub.set_network(id, net(Some("10.0.0.2"), "255.255.255.0")).await;
    assert!(matches!(refused, Err(Error::AccessDenied(_))), "{refused:?}");

    let (hub, id, _fake) = online_ipx(timing()).await;
    let refused = hub.set_network(id, net(Some("10.0.0.2"), "255.255.255.0")).await;
    assert!(matches!(refused, Err(Error::Amp(labgruppen::Error::Unsupported(_)))), "{refused:?}");
}
