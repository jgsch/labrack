//! The order of the known amplifiers (BEH-45): the hub changes it and tells the views.

mod common;

use common::*;
use labrack_core::{Error, Protocol, Update};

// BEH-45: the order of Hub::amps, which the Amplifiers page and Mix & EQ show, follows the change, and
// the views are told
#[tokio::test(flavor = "multi_thread")]
async fn moving_an_amp_changes_the_order_every_view_reads_and_says_so() {
    let (hub, _local) = hub();
    let a = hub.add_manual("A", Protocol::Ipx, "10.0.0.1").unwrap();
    let b = hub.add_manual("B", Protocol::Ipx, "10.0.0.2").unwrap();
    let mut rx = hub.subscribe();

    hub.move_amp(b, 0).unwrap();

    let names: Vec<_> = hub.amps().into_iter().map(|amp| amp.name).collect();
    assert_eq!(names, ["B", "A"]);
    let mut told = false;
    while let Ok(update) = rx.try_recv() {
        told |= matches!(update, Update::Listed);
    }
    assert!(told, "the views must be told");

    hub.remove(a).unwrap();
    assert!(matches!(hub.move_amp(a, 0), Err(Error::UnknownAmp)));
}
