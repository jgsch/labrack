#![cfg(all(feature = "ipx", feature = "ipd"))]

//! SEC-2: both families refuse values outside the range of a shared control. The IPD ranges come from
//! the vendor app; the IPX has none of its own (docs/ipx-protocol.md), so it takes the same.

use labgruppen::amps::ipd::params as ipd;
use labgruppen::amps::ipx::fake::FakeAmp;
use labgruppen::amps::ipx::params as ipx;
use labgruppen::{Amp, Error, Param, Value, shared_range};

fn param(s: &str) -> Param {
    s.parse().unwrap()
}

#[test]
fn shared_ranges_are_the_ipd_ranges_and_the_ipx_has_them_too() {
    let mut checked = 0;
    for p in ipx::all() {
        let Some((min, max)) = shared_range(p) else { continue };
        let spec = ipx::spec(p).unwrap();
        assert_eq!((spec.min, spec.max), (Some(min), Some(max)), "ipx {p}");
        let spec = ipd::spec(p, "IPD2400").unwrap();
        assert_eq!((spec.min, spec.max), (Some(min), Some(max)), "ipd {p}");
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn every_ranged_ipd_control_of_the_basic_set_is_shared() {
    for p in ipd::all_params() {
        let basic = matches!(
            p,
            Param::Gain(_) | Param::Delay(_) | Param::Eq { .. } | Param::Xover { .. } | Param::Mixer { .. }
        );
        let ranged = ipd::spec(&p, "IPD2400").unwrap().min.is_some();
        assert_eq!(shared_range(&p).is_some(), basic && ranged, "{p}");
    }
}

#[test]
fn controls_without_a_shared_range_stay_without() {
    for p in ["out1.mute", "out1.eq.0.type", "out1.xover.hp.type", "out1.limiter.threshold", "in1.comp.ratio"]
    {
        assert_eq!(shared_range(&param(p)), None, "{p}");
    }
}

#[tokio::test]
async fn the_ipx_refuses_values_outside_the_range_and_sends_nothing() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    for (name, value) in [
        ("out1.gain", 13.0),
        ("out1.gain", -49.0),
        ("in1.gain", 1e30),
        ("out2.delay", -1.0),
        ("out2.delay", 2000.5),
        ("out1.eq.3.gain", 12.5),
        ("out1.eq.3.q", 0.0),
        ("out1.eq.3.freq", 19.0),
        ("out1.xover.hp.freq", 18.0),
        ("in1.mixer.aes1.level", 0.5),
    ] {
        let p = param(name);
        for set in [amp.set(&p, Value::Float(value)).await, amp.set_unverified(&p, Value::Float(value)).await]
        {
            assert!(matches!(set, Err(Error::InvalidValue(_))), "{name} = {value}: {set:?}");
        }
    }
    assert_eq!(fake.writes(), 0);

    // the edges of the ranges are fine
    amp.set(&param("out1.gain"), Value::Float(12.0)).await.unwrap();
    amp.set(&param("out1.gain"), Value::Float(-48.0)).await.unwrap();
    amp.set(&param("in1.mixer.aes1.level"), Value::Float(0.0)).await.unwrap();
    assert_eq!(fake.writes(), 3);
}
