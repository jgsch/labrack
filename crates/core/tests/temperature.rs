//! The temperature of an amp (BEH-46): read every few seconds once the state is read, kept when an answer is
//! missing, gone when the amp is not ONLINE.

mod common;

use std::time::Duration;

use common::*;
use labrack_core::{Status, Timing, Update};
use tokio::time::{sleep, timeout};

fn quick() -> Timing {
    Timing { temperature_interval: Duration::from_millis(50), ..timing() }
}

async fn wait_temperature(hub: &labrack_core::Hub, id: labrack_core::AmpId, want: Option<i32>) {
    let seen = async {
        while hub.temperature(id) != want {
            sleep(Duration::from_millis(10)).await;
        }
    };
    timeout(Duration::from_secs(10), seen)
        .await
        .unwrap_or_else(|_| panic!("temperature stayed {:?}, wanted {want:?}", hub.temperature(id)));
}

// BEH-46: an IPX shows the first of its readings
#[tokio::test(flavor = "multi_thread")]
async fn an_ipx_shows_its_first_temperature() {
    let (hub, id, _fake) = online_ipx(quick()).await;
    wait_temperature(&hub, id, Some(35)).await;
}

// BEH-46: an IPD shows its reading, a change on the amp is seen, and the view is told
#[tokio::test(flavor = "multi_thread")]
async fn an_ipd_shows_its_temperature_and_a_change() {
    let sim =
        labgruppen::amps::ipd::sim::SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let (hub, local) = hub_with(labrack_core::Store::in_memory(), quick());
    local.route("10.0.0.1", sim.addr.port());
    let mut updates = hub.subscribe();
    let id = hub.add_manual("Right", labrack_core::Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    wait_temperature(&hub, id, Some(41)).await;

    sim.set_temperature(55);
    wait_temperature(&hub, id, Some(55)).await;
    let told = async {
        loop {
            if let Ok(Update::Temperature(told)) = updates.recv().await
                && told == id
            {
                return;
            }
        }
    };
    timeout(Duration::from_secs(5), told).await.expect("Update::Temperature");
}

// BEH-46: nothing is shown once the amp is not ONLINE
#[tokio::test(flavor = "multi_thread")]
async fn no_temperature_when_the_amp_is_not_online() {
    let (hub, id, _fake) = online_ipx(quick()).await;
    wait_temperature(&hub, id, Some(35)).await;
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert_eq!(hub.temperature(id), None);
}
