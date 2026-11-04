//! Output limiter in Vpk, Ohm, Watt and dBu on both families (BEH-5, BEH-28, SEC-3).

mod common;

use std::time::Duration;

use common::*;
use labgruppen::amps::ipd::params::slot as ipd_slot;
use labgruppen::amps::ipx::params as ipx_params;
use labgruppen::limiter as math;
use labrack_core::{AmpId, Ceilings, Error, Hub, Limiter, Protocol, Status, Store, Unit};
use tokio::time::sleep;

fn close(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a} is not within {tolerance} of {b}");
}

async fn wait_limiter(hub: &Hub, id: AmpId, output: u8, check: impl Fn(Limiter) -> bool) -> Limiter {
    let seen = async {
        loop {
            if let Some(l) = hub.limiter(id, output)
                && check(l)
            {
                return l;
            }
            sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), seen).await.expect("the limiter reaches the value")
}

fn limiter_params(output: u8) -> [String; 4] {
    ["threshold", "impedance", "power", "threshold_dbu"].map(|f| format!("out{}.limiter.{f}", output + 1))
}

// BEH-5: what the amps hold, shown in all four units
#[tokio::test]
async fn the_limiter_is_read_in_all_units_on_both_families() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let l = hub.limiter(id, 0).unwrap();
    // 69 Vpk into 4 ohm: the figures of docs/ipx-protocol.md
    close(l.vpk, 69.0, 1e-9);
    close(l.ohm, 4.0, 1e-9);
    close(l.watt, 595.125, 1e-6);
    close(l.dbu, 35.9851, 1e-3);
    assert!(hub.limiter(id, 2).is_none());

    let (hub, id, _sim) = online_ipd().await;
    let l = hub.limiter(id, 0).unwrap();
    close(l.dbu, 36.78, 1e-9);
    close(l.ohm, 4.0, 1e-9);
    close(l.vpk, math::vpk_from_dbu(36.78), 1e-9);
    close(l.watt, math::watt(l.vpk, 4.0), 1e-9);
}

// BEH-5: the same edit gives the same figures on both families
#[tokio::test(flavor = "multi_thread")]
async fn the_same_edit_gives_the_same_figures_on_both_families() {
    let (ipx, a, _fake) = online_ipx(timing()).await;
    let (ipd, b, _sim) = online_ipd().await;
    let edits = [(Unit::Dbu, 30.0), (Unit::Watt, 100.0), (Unit::Vpk, 40.0), (Unit::Dbu, 20.0)];
    for (unit, value) in edits {
        ipx.set_limiter(a, 0, unit, value).unwrap();
        ipd.set_limiter(b, 0, unit, value).unwrap();
        let near = |l: Limiter| (l.in_unit(unit) - value).abs() <= 0.003 * value.abs();
        let x = wait_limiter(&ipx, a, 0, near).await;
        let d = wait_limiter(&ipd, b, 0, near).await;
        // the IPD keeps the dBu to 0.01, which is 0.2 % of the watts
        for (x, d) in [(x.vpk, d.vpk), (x.watt, d.watt), (x.dbu, d.dbu)] {
            close(x, d, 0.003 * x.abs().max(1.0));
        }
    }
    // 100 W into 4 ohm is 28.24 dBu, whichever way it was asked for
    ipx.set_limiter(a, 1, Unit::Watt, 100.0).unwrap();
    let l = wait_limiter(&ipx, a, 1, |l| (l.watt - 100.0).abs() < 1e-3).await;
    close(l.dbu, 10.0 * (100.0f64 * 4.0 / 0.6).log10(), 1e-3);
}

// BEH-5: an edit of the threshold writes Vpk, ohm, watts and dBu together on the IPX, as the vendor
// app does, so whichever one the DSP uses holds the new level
#[tokio::test]
async fn a_threshold_edit_writes_all_four_values_on_the_ipx() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let [vpk, ohm, watt, dbu] = limiter_params(0);
    hub.set_limiter(id, 0, Unit::Dbu, 30.0).unwrap();
    wait_limiter(&hub, id, 0, |l| (l.dbu - 30.0).abs() < 1e-3).await;

    let expected = labrack_core::Limiter::from_dbu(30.0, 4.0);
    close(ipx_value(&fake, &vpk).as_f64().unwrap(), expected.vpk, 1e-3);
    close(ipx_value(&fake, &ohm).as_f64().unwrap(), 4.0, 1e-9);
    close(ipx_value(&fake, &watt).as_f64().unwrap(), expected.watt, 1e-2);
    close(ipx_value(&fake, &dbu).as_f64().unwrap(), 30.0, 1e-3);
    let mut written: Vec<_> = fake.state().setprm.clone();
    written.sort_by_key(|a| (a.env, a.channel, a.item, a.field));
    let mut want: Vec<_> =
        [&vpk, &ohm, &watt, &dbu].map(|n| ipx_params::slot(&param(n)).unwrap().addr).to_vec();
    want.sort_by_key(|a| (a.env, a.channel, a.item, a.field));
    assert_eq!(written, want, "exactly these four, once");
}

// BEH-5: a change of the load only moves the watts; the threshold, which the amp may be limiting
// on, is not touched (the vendor app's out2 stored a dBu that its Vpk does not give)
#[tokio::test]
async fn a_load_edit_writes_only_ohm_and_watts_on_the_ipx() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let [vpk, ohm, watt, dbu] = limiter_params(1);
    let (vpk_before, dbu_before) = (ipx_value(&fake, &vpk), ipx_value(&fake, &dbu));

    hub.set_load(id, 1, 8.0).unwrap();
    let l = wait_limiter(&hub, id, 1, |l| l.ohm == 8.0).await;

    close(l.vpk, 69.0, 1e-9);
    close(l.watt, 69.0 * 69.0 / 16.0, 1e-6);
    assert_eq!(ipx_value(&fake, &vpk), vpk_before);
    assert_eq!(ipx_value(&fake, &dbu), dbu_before, "the dBu field is as it was");
    close(ipx_value(&fake, &watt).as_f64().unwrap(), 69.0 * 69.0 / 16.0, 1e-2);
    assert_eq!(ipx_value(&fake, &ohm), f(8.0));
    assert_eq!(fake.state().setprm.len(), 2, "ohm and watts");
}

// BEH-5: the IPD stores the dBu threshold only; the load is the app's (BEH-40)
#[tokio::test(flavor = "multi_thread")]
async fn a_threshold_edit_writes_the_dbu_only_on_the_ipd() {
    let (hub, id, sim) = online_ipd().await;
    let before = sim.writes().len();
    hub.set_limiter(id, 1, Unit::Watt, 150.0).unwrap();
    // the IPD keeps the dBu to 0.01
    let l = wait_limiter(&hub, id, 1, |l| (l.watt - 150.0).abs() < 0.5).await;
    close(l.dbu, 10.0 * (150.0f64 * 4.0 / 0.6).log10(), 0.01);

    let writes = sim.writes()[before..].to_vec();
    assert_eq!(writes.first(), Some(&ipd_slot(&param("out2.limiter.threshold")).unwrap().key));
    assert!(writes.iter().all(|k| *k == writes[0]), "nothing but the threshold: {writes:?}");
    let truth = ipd_value(&sim, "out2.limiter.threshold").await;
    close(truth.as_f64().unwrap(), l.dbu, 1e-2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_load_edit_on_the_ipd_is_the_apps_and_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let sim =
        labgruppen::amps::ipd::sim::SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let (hub, local) = hub_with(Store::open(&path).unwrap(), timing());
    local.route("10.0.0.1", sim.addr.port());
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    let before = (sim.writes().len(), hub.limiter(id, 0).unwrap());

    hub.set_load(id, 0, 8.0).unwrap();
    let l = hub.limiter(id, 0).unwrap();
    assert_eq!(l.ohm, 8.0);
    close(l.dbu, before.1.dbu, 1e-12);
    close(l.vpk, before.1.vpk, 1e-12);
    close(l.watt, before.1.watt / 2.0, 1e-9);
    assert_eq!(hub.limiter(id, 1).unwrap().ohm, 4.0, "per output");
    assert_eq!(sim.writes().len(), before.0, "nothing is sent");
    sleep(Duration::from_millis(200)).await;
    assert_eq!(sim.writes().len(), before.0);
    assert_eq!(Store::open(&path).unwrap().amps()[0].load_ohms, [8.0, 4.0]);
}

// SEC-2: the hard range, in every unit
#[tokio::test(flavor = "multi_thread")]
async fn values_outside_the_range_are_never_sent() {
    let (ipx, a, fake) = online_ipx(timing()).await;
    let (ipd, b, sim) = online_ipd().await;
    let bad = [
        (Unit::Dbu, 16.7),
        (Unit::Dbu, 39.9),
        (Unit::Dbu, f64::NAN),
        (Unit::Dbu, f64::INFINITY),
        (Unit::Dbu, f64::NEG_INFINITY),
        (Unit::Vpk, 0.0),
        (Unit::Vpk, 7.0),
        (Unit::Vpk, 200.0),
        (Unit::Vpk, -40.0),
        (Unit::Vpk, 1e300),
        (Unit::Watt, 0.0),
        (Unit::Watt, 5.0),
        (Unit::Watt, 1e6),
        (Unit::Watt, -100.0),
        (Unit::Ohm, 0.0),
        (Unit::Ohm, 0.49),
        (Unit::Ohm, 100.1),
        (Unit::Ohm, -4.0),
        (Unit::Ohm, f64::NAN),
        (Unit::Ohm, f64::INFINITY),
    ];
    for (unit, value) in bad {
        for (hub, id) in [(&ipx, a), (&ipd, b)] {
            let got = hub.set_limiter(id, 0, unit, value);
            assert!(matches!(got, Err(Error::Invalid(_))), "{unit:?} {value}: {got:?}");
        }
    }
    // the IPD 2400 tops out at 36.78 dBu (its library range), the IPX 1200 at 35.99 dBu (69 Vpk)
    assert!(matches!(ipd.set_limiter(b, 0, Unit::Dbu, 36.79), Err(Error::Invalid(_))));
    assert!(matches!(ipx.set_limiter(a, 0, Unit::Dbu, 36.0), Err(Error::Invalid(_))));
    assert!(matches!(ipx.set_limiter(a, 0, Unit::Dbu, 19.2), Err(Error::Invalid(_))));
    assert!(matches!(ipx.set_limiter(a, 0, Unit::Vpk, 70.0), Err(Error::Invalid(_))));
    assert!(matches!(ipx.set_limiter(a, 0, Unit::Vpk, 9.9), Err(Error::Invalid(_))));
    assert!(ipx.set_limiter(a, 0, Unit::Dbu, 35.99).is_ok());
    assert!(matches!(ipx.set_limiter(a, 7, Unit::Dbu, 30.0), Err(Error::NotControllable)));
    sleep(Duration::from_millis(300)).await;
    assert_eq!(sim.writes(), []);
    assert_eq!(fake.writes(), 4, "only the one good edit: four values");
}

// the generic setter does not take the limiter: its figures must be written together
#[tokio::test]
async fn the_limiter_is_not_set_one_value_at_a_time() {
    let (hub, id, fake) = online_ipx(timing()).await;
    for name in limiter_params(0) {
        assert!(matches!(hub.set(id, param(&name), f(30.0)), Err(Error::NotControllable)), "{name}");
    }
    sleep(Duration::from_millis(100)).await;
    assert_eq!(fake.writes(), 0);
}

// the control shows the range in its unit
#[tokio::test(flavor = "multi_thread")]
async fn the_range_of_a_limiter_control() {
    let (hub, id, _fake) = online_ipx(timing()).await;
    let r = hub.limiter_range(id, 0, Unit::Dbu).unwrap();
    assert_eq!((r.min, r.max, r.unit), (Some(19.22), Some(35.99), "dBu"), "an IPX 1200, as its Windows app");
    let r = hub.limiter_range(id, 0, Unit::Vpk).unwrap();
    close(r.min.unwrap(), 10.0, 0.1); // the Windows app shows whole volts
    close(r.max.unwrap(), 69.0, 0.1);
    assert_eq!(r.unit, "Vpk");
    let r = hub.limiter_range(id, 0, Unit::Watt).unwrap();
    close(r.max.unwrap(), math::watt(math::vpk_from_dbu(35.99), 4.0), 1e-6);
    assert_eq!(r.unit, "W");
    let r = hub.limiter_range(id, 0, Unit::Ohm).unwrap();
    assert_eq!((r.min, r.max, r.unit), (Some(0.5), Some(100.0), "Ohm"));
    assert!(hub.limiter_range(id, 2, Unit::Dbu).is_none());

    let (hub, id, _sim) = online_ipd().await;
    let r = hub.limiter_range(id, 0, Unit::Dbu).unwrap();
    assert_eq!((r.min, r.max), (Some(16.78), Some(36.78)), "the IPD 2400");
    hub.disconnect(id).unwrap();
    wait_status(&hub, id, Status::Disconnected).await;
    assert!(hub.limiter_range(id, 0, Unit::Dbu).is_none());
    assert!(hub.limiter(id, 0).is_none());
    assert!(matches!(hub.set_limiter(id, 0, Unit::Dbu, 30.0), Err(Error::NotOnline)));
}

// SEC-3: the ceiling holds in every unit, and the range ends at it
#[tokio::test(flavor = "multi_thread")]
async fn the_limiter_ceiling_holds_in_every_unit() {
    for ipx in [true, false] {
        let (hub, id, writes): (Hub, AmpId, Box<dyn Fn() -> usize>) = if ipx {
            let (hub, id, fake) = online_ipx(timing()).await;
            (hub, id, Box::new(move || fake.writes()))
        } else {
            let (hub, id, sim) = online_ipd().await;
            (hub, id, Box::new(move || sim.writes().len()))
        };
        hub.set_ceilings(id, Ceilings { max_output_gain_db: None, max_limiter_dbu: Some(30.0) }).unwrap();
        let ohm = hub.limiter(id, 0).unwrap().ohm;
        let at_30 = Limiter::from_dbu(30.0, ohm);

        assert_eq!(hub.limiter_range(id, 0, Unit::Dbu).unwrap().max, Some(30.0));
        close(hub.limiter_range(id, 0, Unit::Vpk).unwrap().max.unwrap(), at_30.vpk, 1e-9);
        close(hub.limiter_range(id, 0, Unit::Watt).unwrap().max.unwrap(), at_30.watt, 1e-9);

        let before = writes();
        for (unit, over, label) in [
            (Unit::Dbu, 30.01, "dBu"),
            (Unit::Vpk, at_30.vpk * 1.001, "Vpk"),
            (Unit::Watt, at_30.watt * 1.01, "W"),
        ] {
            let got = hub.set_limiter(id, 0, unit, over);
            match got {
                Err(Error::AboveCeiling { ceiling, unit: u }) => {
                    assert_eq!(u, label);
                    close(ceiling, at_30.in_unit(unit), 1e-9);
                }
                other => panic!("ipx {ipx}, {unit:?} {over}: {other:?}"),
            }
        }
        assert_eq!(writes(), before, "nothing was sent");

        // exactly the ceiling is fine, in any unit
        for (unit, value) in [(Unit::Dbu, 30.0), (Unit::Vpk, at_30.vpk), (Unit::Watt, at_30.watt)] {
            hub.set_limiter(id, 0, unit, value).unwrap_or_else(|e| panic!("ipx {ipx}, {unit:?}: {e}"));
        }
        // the load does not move the threshold: allowed whatever the ceiling
        hub.set_load(id, 0, 2.0).unwrap();
        // below it is fine, and so is no ceiling
        hub.set_limiter(id, 0, Unit::Dbu, 25.0).unwrap();
        hub.set_ceilings(id, Ceilings::default()).unwrap();
        hub.set_limiter(id, 0, Unit::Dbu, 33.0).unwrap();
    }
}

// the load edit is allowed even when the ceiling is below the current threshold
#[tokio::test]
async fn lowering_the_ceiling_changes_nothing_on_the_amp() {
    let (hub, id, fake) = online_ipx(timing()).await;
    hub.set_ceilings(id, Ceilings { max_output_gain_db: None, max_limiter_dbu: Some(20.0) }).unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.writes(), 0);
    close(hub.limiter(id, 0).unwrap().dbu, 35.9851, 1e-3);
    hub.set_load(id, 0, 8.0).unwrap();
    wait_limiter(&hub, id, 0, |l| l.ohm == 8.0).await;
}

// BEH-37, BEH-5: a settings file brings the limiter back, all four values on the IPX
#[tokio::test]
async fn a_file_restores_the_limiter() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let names: Vec<_> = (0..2).flat_map(limiter_params).collect();
    let before: Vec<_> = names.iter().map(|n| ipx_value(&fake, n)).collect();

    hub.set_limiter(id, 0, Unit::Dbu, 25.0).unwrap();
    hub.set_limiter(id, 1, Unit::Vpk, 30.0).unwrap();
    wait_limiter(&hub, id, 1, |l| (l.vpk - 30.0).abs() < 1e-3).await;
    wait_limiter(&hub, id, 0, |l| (l.dbu - 25.0).abs() < 1e-3).await;

    let report = hub.load_file(id, &path).await.unwrap();
    let after: Vec<_> = names.iter().map(|n| ipx_value(&fake, n)).collect();
    for (b, a) in before.iter().zip(&after) {
        close(a.as_f64().unwrap(), b.as_f64().unwrap(), 1e-2);
    }
    let output_limiter = |s: &String| s.starts_with("out") && names.iter().any(|n| s.starts_with(n.as_str()));
    assert!(!report.skipped.iter().any(output_limiter), "{:?}", report.skipped);
}

// SEC-2, SEC-3: the file cannot carry a limiter value past the range or the ceiling
#[tokio::test]
async fn a_file_cannot_carry_a_limiter_past_the_range_or_the_ceiling() {
    let (hub, id, fake) = online_ipx(timing()).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amp.json");
    hub.save_file(id, &path).await.unwrap();
    let writes = fake.writes();

    for (name, value) in [
        ("out1.limiter.threshold", serde_json::json!(1e9)),
        ("out1.limiter.threshold", serde_json::json!(2.0)),
        ("out2.limiter.threshold_dbu", serde_json::json!(39.9)),
        ("out1.limiter.impedance", serde_json::json!(0.0)),
        ("out1.limiter.power", serde_json::json!(-5.0)),
        ("out1.limiter.threshold_dbu", serde_json::json!("loud")),
    ] {
        let mut json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        json["params"][name] = value.clone();
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, json.to_string()).unwrap();
        let got = hub.load_file(id, &bad).await;
        assert!(matches!(got, Err(Error::Invalid(_))), "{name} = {value}: {got:?}");
        assert_eq!(fake.writes(), writes, "{name}: nothing written");
    }

    // the amp holds 35.98 dBu on out1: a ceiling of 30 refuses the file that restores it
    hub.set_ceilings(id, Ceilings { max_output_gain_db: None, max_limiter_dbu: Some(30.0) }).unwrap();
    let got = hub.load_file(id, &path).await;
    assert!(matches!(&got, Err(Error::Invalid(why)) if why.contains("ceiling")), "{got:?}");
    assert_eq!(fake.writes(), writes);
}
