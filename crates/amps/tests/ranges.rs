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

/// The IPX reports no limiter range. The range is what the vendor Windows app allows on an IPX 1200:
/// 19.22 dBu (10 Vpk) to 35.99 dBu (69 Vpk). Other models are not measured and take the same.
#[tokio::test]
async fn the_ipx_limiter_has_a_hard_range_and_all_four_values_can_be_written() {
    use labgruppen::limiter;
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let (lo, hi) = (limiter::vpk_from_dbu(19.22), limiter::vpk_from_dbu(35.99));

    for (name, value) in [
        ("out1.limiter.threshold", lo * 0.99),
        ("out1.limiter.threshold", hi * 1.01),
        ("out1.limiter.threshold", 1e9),
        ("out1.limiter.impedance", 0.0),
        ("out1.limiter.impedance", 0.4),
        ("out1.limiter.impedance", 101.0),
        ("out2.limiter.power", -1.0),
        ("out2.limiter.power", 13_000.0),
        ("out2.limiter.threshold_dbu", 19.21),
        ("out2.limiter.threshold_dbu", 35.995),
        ("out2.limiter.threshold_dbu", 39.0),
        ("out2.limiter.threshold_dbu", 16.7),
        ("out2.limiter.threshold_dbu", f64::NAN),
    ] {
        let set = amp.set(&param(name), Value::Float(value)).await;
        assert!(matches!(set, Err(Error::InvalidValue(_))), "{name} = {value}: {set:?}");
    }
    assert_eq!(fake.writes(), 0);

    // the edges, and the figures a real IPX 1200 holds, are fine; so is a power with decimals, which
    // the amp reads back as an integer
    for (name, value) in [
        ("out1.limiter.threshold", lo),
        ("out1.limiter.threshold", hi),
        ("out1.limiter.threshold", 69.0),
        ("out1.limiter.impedance", 0.5),
        ("out1.limiter.impedance", 100.0),
        ("out1.limiter.power", 595.125),
        ("out1.limiter.power", 12_000.0),
        ("out1.limiter.threshold_dbu", 19.22),
        ("out1.limiter.threshold_dbu", 35.99),
        ("out1.limiter.threshold_dbu", 35.9851),
    ] {
        amp.set(&param(name), Value::Float(value)).await.unwrap_or_else(|e| panic!("{name} = {value}: {e}"));
    }
    assert_eq!(fake.writes(), 10);
    // 69 Vpk, what the real amp holds, is inside
    assert!(hi > 69.0 && limiter::vpk_from_dbu(19.22) < 10.1);
    assert!(!ipx::spec(&param("out1.name")).unwrap().writable, "names stay read-only");
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
