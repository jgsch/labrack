#![cfg(feature = "ipx")]

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use labgruppen::amps::ipx::client::ClientConfig;
use labgruppen::amps::ipx::fake::{Cmd, FakeAmp};
use labgruppen::amps::ipx::params::{self, Ty};
use labgruppen::amps::ipx::protocol::{
    Address, Ctrl, Env, FrameParser, Main, build_frame, ctrl_sub, xor_password,
};
use labgruppen::amps::ipx::structs::{
    DeviceState, Meters, StoredPreset, SystemAttributes, audio_dword, parse_preset_names, parse_valid_presets,
};
use labgruppen::amps::ipx::{Discovered, IpxAmp};
use labgruppen::{Amp, AmpEvent, Error, Param, Value};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn param(s: &str) -> Param {
    s.parse().unwrap()
}

fn f(v: f64) -> Value {
    Value::Float(v)
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// Read a parameter out of the audio config of a DEV_PRM_SYNC dump, through the same table the
/// driver uses.
fn read(st: &DeviceState, name: &str) -> Value {
    let slot = params::slot(&param(name)).unwrap();
    slot.decode(st.audio_bytes(slot.addr, slot.wire_len()).unwrap()).unwrap()
}

const GOLDEN: &str = "02aa01aa1000000001000200000000000d000001010000000f00000023011111";

#[test]
fn frame_roundtrip_and_resync() {
    let frame = build_frame(Main::SETPRM, [13, 0, 0, 1], &1u32.to_le_bytes()).unwrap();
    assert_eq!(hex(&frame), GOLDEN);

    let mut p = FrameParser::new();
    let mut input = b"junk".to_vec();
    input.extend_from_slice(&frame[..10]);
    let mut frames = p.feed(&input);
    let mut rest = frame[10..].to_vec();
    rest.extend_from_slice(&frame);
    frames.extend(p.feed(&rest));
    assert_eq!(frames.len(), 2);
    for fr in frames {
        assert_eq!(
            (fr.main, fr.address(), fr.payload.as_slice()),
            (1, Address::new(13, 0, 0, 1), &[1, 0, 0, 0][..])
        );
    }
}

#[test]
fn parser_rejects_bad_frames_and_byte_by_byte() {
    let good = build_frame(Main::GETPRM, [7, 1, 0, 0], &(-21f32).to_le_bytes()).unwrap();
    let mut bad_trailer = good.clone();
    *bad_trailer.last_mut().unwrap() ^= 1;
    let mut bad_sum = good.clone();
    bad_sum[good.len() - 8] ^= 0x55;
    let mut zero_sum = good.clone();
    zero_sum[good.len() - 8..good.len() - 4].fill(0);
    let mut huge = good[..8].to_vec();
    huge[4..8].copy_from_slice(&0x10000u32.to_le_bytes());

    let mut p = FrameParser::new();
    let mut stream = Vec::new();
    for chunk in [&bad_trailer, &bad_sum, &huge, &good, &zero_sum] {
        stream.extend_from_slice(chunk);
    }
    let mut frames = Vec::new();
    for b in &stream {
        frames.extend(p.feed(std::slice::from_ref(b)));
    }
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|f| f.payload == (-21f32).to_le_bytes() && f.address().channel == 1));
}

#[test]
fn oversized_frame_is_refused() {
    assert!(build_frame(Main::SETPRM, [0; 4], &vec![0; 0x2800]).is_err());
    assert!(build_frame(Main::SETPRM, [0; 4], &vec![0; 0x2800 - 12]).is_ok());
    assert_eq!(ctrl_sub(Ctrl::GETVALIDPRESETS), [19, 0, 0, 0]);
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn value_codec() {
    let gain = params::slot(&param("out1.gain")).unwrap();
    assert_eq!(gain.addr, Address::new(Env::OUTPUT_LEVEL, 0, 0, 0));
    assert_eq!(hex(&gain.encode(&f(-2.5)).unwrap()), "000020c0");
    assert_eq!(gain.decode(&[0, 0, 0x20, 0xc0]).unwrap(), f(-2.5));

    let freq = params::slot(&param("out1.eq.1.freq")).unwrap();
    assert_eq!(freq.addr, Address::new(Env::OUTPUT_EQ, 0, 1, 2));
    assert_eq!(freq.decode(&920i32.to_le_bytes()).unwrap(), Value::Int(920));
    assert_eq!(freq.encode(&f(920.4)).unwrap(), 920i32.to_le_bytes());

    let kind = params::slot(&param("in2.eq.0.type")).unwrap();
    assert_eq!(kind.encode(&text("Notch")).unwrap(), 1i32.to_le_bytes());
    assert_eq!(kind.decode(&7i32.to_le_bytes()).unwrap(), text("low_pass"));
    assert!(kind.encode(&text("nope")).is_err());
    assert_eq!(kind.decode(&99i32.to_le_bytes()).unwrap(), text("unknown(99)"), "as the IPD does");
    assert!(kind.encode(&text("unknown(99)")).is_err());

    let mute = params::slot(&param("in1.mute")).unwrap();
    assert_eq!(mute.encode(&Value::Bool(true)).unwrap(), 1i32.to_le_bytes());
    assert_eq!(mute.decode(&0i32.to_le_bytes()).unwrap(), Value::Bool(false));
    assert!(mute.decode(&[1, 2]).is_err());

    let name = params::slot(&param("out2.name")).unwrap();
    assert_eq!(name.decode(b"Tops\0garbage\0\0\0\0").unwrap(), text("Tops"));
    assert!(name.encode(&text("x")).is_err());
}

#[test]
fn parameter_table() {
    let all = params::all();
    assert_eq!(all.len(), 289, "288 per-channel settings and the device mode");
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), all.len());
    assert_eq!(all[0], param("in1.gain"));
    assert_eq!(all[71], param("in2.gain"));
    assert_eq!(all[142], param("out1.gain"));

    let mut addrs = HashSet::new();
    for p in all {
        let slot = params::slot(p).unwrap();
        assert!(addrs.insert(slot.addr), "{p} shares its address");
        assert_eq!(params::param_for(slot.addr), Some(p));
        // the spec is the slot's, plus a range for the basic controls and the output limiter
        // (ranges.rs checks the ranges)
        let (spec, plain) = (params::spec(p).unwrap(), slot.spec());
        assert_eq!((spec.kind, spec.unit, spec.writable), (plain.kind, plain.unit, plain.writable), "{p}");
        assert!(audio_dword(slot.addr).is_some() || slot.addr.env == Env::INPUT_MIXER, "{p}");
    }

    for missing in ["in1.polarity", "in1.xover.hp.freq", "out1.mixer.analog1.level", "in1.limiter.power"] {
        assert!(params::slot(&param(missing)).is_none(), "{missing}");
    }
    for read_only in ["out1.name", "in2.name"] {
        assert!(!params::spec(&param(read_only)).unwrap().writable, "{read_only}");
    }
    // plain stored values (docs/ipx-protocol.md): the amp derives nothing, so all four can be written
    for writable in ["out1.limiter.power", "out2.limiter.threshold_dbu", "out2.limiter.impedance"] {
        assert!(params::spec(&param(writable)).unwrap().writable, "{writable}");
    }
    assert!(params::slot(&param("out2.limiter.threshold")).unwrap().writable);
    assert_eq!(params::slot(&param("out1.limiter.threshold")).unwrap().ty, Ty::Float);
    assert_eq!(params::spec(&param("out1.limiter.threshold")).unwrap().unit, "Vpk");
}

#[test]
fn audio_offsets_match_the_documented_layout() {
    let a = |env, c, item, field| audio_dword(Address::new(env, c, item, field));
    assert_eq!(a(Env::INPUT_MIXER, 1, 3, 1), Some(0x02 + 0x18 + 18 + 1));
    assert_eq!(a(Env::INPUT_LEVEL, 1, 0, 3), Some(0x32 + 7 + 3));
    assert_eq!(a(Env::INPUT_EQ, 1, 9, 4), Some(0x40 + 0x32 + 45 + 4));
    assert_eq!(a(Env::INPUT_DELAY, 1, 0, 1), Some(0xA4 + 3));
    assert_eq!(a(Env::INPUT_COMPRESSOR, 1, 0, 5), Some(0xA8 + 11));
    assert_eq!(a(Env::INPUT_RMS_LIMITER, 1, 0, 1), Some(0xB4 + 3));
    assert_eq!(a(Env::OUTPUT_XOVER, 1, 1, 2), Some(0xB8 + 6 + 3 + 2));
    assert_eq!(a(Env::OUTPUT_LEVEL, 1, 0, 4), Some(0xC4 + 8 + 4));
    assert_eq!(a(Env::OUTPUT_EQ, 1, 9, 4), Some(0xD4 + 0x32 + 45 + 4));
    assert_eq!(a(Env::OUTPUT_DELAY, 1, 0, 1), Some(0x138 + 3));
    assert_eq!(a(Env::OUTPUT_COMPRESSOR, 1, 0, 5), Some(0x13C + 11));
    assert_eq!(a(Env::OUTPUT_LIMITER, 1, 0, 4), Some(0x148 + 9));
    assert_eq!(a(Env::OUTPUT_LIMITER, 2, 0, 0), None);
    assert_eq!(a(Env::OUTPUT_EQ, 0, 10, 0), None);
    assert_eq!(a(Env::GAINSLEVEL, 0, 0, 0), None);
}

#[test]
fn limiter_power_is_an_integer_in_single_replies() {
    // seen on an IPX 1200: GETPRM env 17 field 2 returns i32 595, the bulk blocks carry f32 595.0
    let slot = params::slot(&param("out1.limiter.power")).unwrap();
    assert_eq!(slot.decode_single(&595i32.to_le_bytes()).unwrap(), f(595.0));
    assert_eq!(slot.decode(&595f32.to_le_bytes()).unwrap(), f(595.0));
    let other = params::slot(&param("out1.limiter.threshold")).unwrap();
    assert_eq!(other.decode_single(&69f32.to_le_bytes()).unwrap(), f(69.0));
}

#[test]
fn password_xor() {
    let raw = xor_password(&hex_bytes("6a1ee1386a871e31a30f789b2ec14d8a"));
    assert_eq!(&raw[..8], b"Password");
    assert!(raw[8..].iter().all(|b| *b == 0));
    assert_eq!(xor_password(&raw), hex_bytes("6a1ee1386a871e31a30f789b2ec14d8a")[..]);
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn device_state() {
    let st = DeviceState::parse(&fixture("dev_prm_sync.bin")).unwrap();
    assert_eq!((st.preset_name.as_str(), st.preset_index), ("base", 1));
    assert_eq!(st.versions.mcu, "2.3.4.0");
    assert_eq!(st.network.port, 50168);

    assert_eq!(read(&st, "in1.gain"), f(-21.0));
    assert_eq!(read(&st, "out1.gain"), f(-2.5));
    assert_eq!(read(&st, "out1.mute"), Value::Bool(true));
    assert_eq!(read(&st, "in1.mixer.analog1.level"), f(-4.0));
    assert_eq!(read(&st, "in1.mixer.aes2.level"), f(-48.0));

    assert_eq!(read(&st, "out1.eq.1.freq"), Value::Int(920));
    assert_eq!(read(&st, "out1.eq.1.gain"), f(-4.5));
    assert_eq!(read(&st, "out1.eq.1.q"), f(2.5));
    assert_eq!(read(&st, "out1.eq.0.type"), text("bell"));
    assert_eq!(read(&st, "out1.eq.0.enabled"), Value::Bool(true));

    assert_eq!(read(&st, "out1.xover.hp.enabled"), Value::Bool(true));
    assert_eq!(read(&st, "out1.xover.hp.type"), text("linkwitz_riley_24"));
    assert_eq!(read(&st, "out1.xover.hp.freq"), Value::Int(370));

    assert_eq!(read(&st, "in1.comp.threshold"), f(24.0));
    assert_eq!(read(&st, "in1.comp.release"), Value::Int(50));

    assert_eq!(read(&st, "out1.limiter.threshold"), f(69.0));
    assert_eq!(read(&st, "out1.limiter.impedance"), f(4.0));
    assert_eq!(read(&st, "out1.limiter.power").as_f64().unwrap().round(), 595.0);
    let dbu = read(&st, "out2.limiter.threshold_dbu").as_f64().unwrap();
    assert_eq!((dbu * 100.0).round() / 100.0, 34.77);

    assert!(DeviceState::parse(&st.raw[..100]).is_err());
}

#[test]
fn stored_preset() {
    let p = StoredPreset::parse(&fixture("getpreset_1.bin")).unwrap();
    assert_eq!((p.name.as_str(), p.index), ("base", 1));
    for b in 0..10 {
        for field in ["enabled", "type", "freq", "gain", "q"] {
            let slot = params::slot(&param(&format!("out1.eq.{b}.{field}"))).unwrap();
            slot.decode(p.audio_bytes(slot.addr, 4).unwrap()).unwrap();
        }
    }
    let slot = params::slot(&param("out1.eq.1.freq")).unwrap();
    assert_eq!(slot.decode(p.audio_bytes(slot.addr, 4).unwrap()).unwrap(), Value::Int(920));
    assert!(StoredPreset::parse(&p.raw[..200]).is_err());
}

#[test]
fn every_param_decodes_from_the_fixtures() {
    let st = DeviceState::parse(&fixture("dev_prm_sync.bin")).unwrap();
    for p in params::all() {
        let slot = params::slot(p).unwrap();
        let bytes = st.audio_bytes(slot.addr, slot.wire_len()).unwrap();
        slot.decode(bytes).unwrap_or_else(|e| panic!("{p}: {e}"));
    }
}

#[test]
fn meters_and_names() {
    let m = Meters::parse(&fixture("gainslevel.bin")).unwrap();
    assert_eq!(m.output, [-150.0, -150.0]);
    assert_eq!((m.input[0] - m.post_input[0]).round(), 25.0);
    assert!(Meters::parse(&[0; 47]).is_none());

    let common: labgruppen::Meters = (&m).into();
    assert_eq!(common.output, m.amp.to_vec(), "the output level is the amp pair (indices 6-7)");
    assert_eq!(common.extra["output_signal"], m.output.to_vec());
    assert_eq!(common.input, m.post_input.to_vec(), "IN 1 and 2 are the channels after the mixer");
    assert_eq!(common.extra["raw_input"], m.input.to_vec(), "ANALOG 1 and 2 for the mixer");
    assert_eq!(common.extra.len(), 4);

    let names = parse_preset_names(&fixture("preset_names.bin"));
    assert_eq!(names.len(), 50);
    assert_eq!(names[1..4], ["base", "alegra-ext", "alegra-int"]);
    assert_eq!(parse_valid_presets(&[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0]), [true, false, true]);
}

#[test]
fn system_attributes_and_discovery_payload() {
    let mut raw = vec![0u8; 48];
    raw[..8].copy_from_slice(b"IPX 2400");
    raw[16..21].copy_from_slice(b"SN001");
    let a = SystemAttributes::parse(&raw);
    assert_eq!((a.model.as_str(), a.serial.as_str()), ("IPX 2400", "SN001"));

    let mut p = vec![0u8; 0x48];
    p[..3].copy_from_slice(b"abc");
    p[16..21].copy_from_slice(b"Rack1");
    p[48..59].copy_from_slice(b"192.168.1.9");
    p[0x40..0x42].copy_from_slice(&50168u16.to_le_bytes());
    p[0x42..0x44].copy_from_slice(&2u16.to_le_bytes());
    p[0x44] = 1;
    let d = Discovered::from_payload("192.168.1.9".into(), &p).unwrap();
    assert_eq!((d.id.as_str(), d.name.as_str(), d.ip.as_str()), ("abc", "Rack1", "192.168.1.9"));
    assert_eq!((d.port, d.model, d.flag), (50168, 2, 1));
    assert!(serde_json::to_string(&d).unwrap().contains("\"port\":50168"));
    assert!(Discovered::from_payload("x".into(), &p[..0x40]).is_none());
}

fn padded(s: &str, n: usize) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.resize(n, 0);
    v
}

#[tokio::test]
async fn end_to_end_get_set_verify() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    assert_eq!((amp.info().driver, amp.info().model.as_str()), ("ipx", "IPX 1200"));

    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(-2.5));
    assert_eq!(amp.get(&param("out1.eq.0.type")).await.unwrap(), text("bell"));
    assert_eq!(amp.get(&param("in1.mixer.aes2.level")).await.unwrap(), f(-48.0));

    amp.set(&param("out1.gain"), f(3.25)).await.unwrap();
    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(3.25));
    amp.set(&param("out1.eq.1.type"), text("notch")).await.unwrap();
    assert_eq!(amp.get(&param("out1.eq.1.type")).await.unwrap(), text("notch"));
    amp.set(&param("out1.eq.1.freq"), f(1000.4)).await.unwrap();
    assert_eq!(amp.get(&param("out1.eq.1.freq")).await.unwrap(), Value::Int(1000));
    amp.set(&param("in2.mute"), Value::Bool(true)).await.unwrap();
    assert_eq!(amp.get(&param("in2.mute")).await.unwrap(), Value::Bool(true));
    amp.set(&param("out1.xover.lp.type"), text("bessel_12")).await.unwrap();

    let wrote = DeviceState::parse(&fake.state().sync).unwrap();
    assert_eq!(read(&wrote, "out1.gain"), f(3.25));
    assert_eq!(read(&wrote, "out1.xover.lp.type"), text("bessel_12"));

    assert!(matches!(amp.set(&param("out1.name"), text("x")).await, Err(Error::Unsupported(_))));
    assert!(matches!(amp.set(&param("out1.eq.0.type"), text("bogus")).await, Err(Error::InvalidValue(_))));
    assert!(matches!(amp.get(&param("in1.polarity")).await, Err(Error::Unsupported(_))));
    assert!(matches!(amp.set(&param("in1.polarity"), Value::Bool(true)).await, Err(Error::Unsupported(_))));

    let raw = amp.get_raw(Address::new(Env::OUTPUT_LEVEL, 0, 0, 0)).await.unwrap();
    assert_eq!(raw, 3.25f32.to_le_bytes());
    amp.close().await;
}

#[tokio::test]
async fn unconfirmed_write_is_reported() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let gain = params::slot(&param("out2.gain")).unwrap();
    fake.state().reject_writes.insert(gain.addr);

    let err = amp.set(&param("out2.gain"), f(-6.0)).await.unwrap_err();
    assert!(matches!(err, Error::NotConfirmed(_)), "{err}");
    amp.set_unverified(&param("out2.gain"), f(-6.0)).await.unwrap();
    amp.set(&param("out1.gain"), f(-6.0)).await.unwrap();
}

#[tokio::test]
async fn lost_read_back_is_retried() {
    // seen on an IPX 1200 over Wi-Fi: during a restore, an occasional read-back got no reply
    let fake = FakeAmp::start().await;
    let config =
        ClientConfig { port: fake.port, timeout: Duration::from_millis(300), ..ClientConfig::default() };
    let amp = IpxAmp::connect_with_config("127.0.0.1", &config).await.unwrap();
    let gain = params::slot(&param("out2.gain")).unwrap();
    fake.state().drop_reads.push(gain.addr);
    amp.set(&param("out2.gain"), f(-6.0)).await.unwrap();
    assert!(fake.state().drop_reads.is_empty());
    assert_eq!(amp.get(&param("out2.gain")).await.unwrap(), f(-6.0));
}

#[tokio::test]
async fn concurrent_requests() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let p = ["in1.gain", "out1.gain", "out1.eq.1.freq", "in1.comp.release", "out1.xover.hp.type"].map(param);
    let (a, b, c, d, e) =
        tokio::try_join!(amp.get(&p[0]), amp.get(&p[1]), amp.get(&p[2]), amp.get(&p[3]), amp.get(&p[4]))
            .unwrap();
    assert_eq!(
        [a, b, c, d, e],
        [f(-21.0), f(-2.5), Value::Int(920), Value::Int(50), text("linkwitz_riley_24")]
    );

    // The same address asked twice at once gets two answers.
    let g = param("in1.gain");
    let (x, y) = tokio::join!(amp.get(&g), amp.get(&g));
    assert_eq!((x.unwrap(), y.unwrap()), (f(-21.0), f(-21.0)));
}

#[tokio::test]
async fn snapshot_matches_single_reads_and_restores() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let snap = amp.snapshot().await.unwrap();
    assert_eq!((snap.driver.as_str(), snap.model.as_str()), ("ipx", "IPX 1200"));
    assert_eq!(snap.params.len(), 289);
    for p in amp.params() {
        assert_eq!(snap.params[&p.to_string()], amp.get(&p).await.unwrap(), "{p}");
    }
    assert_eq!(snap.params["out1.gain"], f(-2.5));

    amp.set(&param("out1.gain"), f(-10.0)).await.unwrap();
    amp.set(&param("in1.eq.3.freq"), Value::Int(4000)).await.unwrap();
    amp.set(&param("out2.xover.lp.enabled"), Value::Bool(false)).await.unwrap();

    let dry = amp.restore(&snap, true).await.unwrap();
    assert!(dry.failed.is_empty(), "{:?}", dry.failed);
    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(-10.0));

    let report = amp.restore(&snap, false).await.unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    // everything but the four names and the device mode: the limiter's watts and dBu are written too (BEH-5)
    assert_eq!(report.applied, 284);
    assert_eq!(report.skipped.len(), 5);
    assert_eq!(amp.snapshot().await.unwrap().params, snap.params);
}

/// BEH-13, BEH-47: an IPX has a lock, not levels: `Unlocked` with every right, or `Locked` with none; the amp is
/// locked and unlocked with a write of the lock flag (DEVCFG field 1), read back; it has no login.
#[tokio::test]
async fn an_ipx_is_locked_and_unlocked_without_a_password() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let lock_dword = || {
        let o = 0x62C + 4;
        i32::from_le_bytes(fake.state().sync[o..o + 4].try_into().unwrap())
    };
    let a = amp.access().await.unwrap().expect("an IPX says its lock");
    assert_eq!((a.user.as_str(), a.locked, a.needs_login()), ("Unlocked", false, false));
    assert!(a.rights.complete());

    let a = amp.set_locked(true).await.unwrap();
    assert_eq!((lock_dword(), a.user.as_str(), a.locked), (1, "Locked", true));
    assert!(
        !(a.rights.input_channel || a.rights.output_channel || a.rights.load_preset || a.rights.save_preset)
    );
    assert_eq!(amp.access().await.unwrap().unwrap(), a);

    // a lock set at the front panel is seen
    fake.state().sync[0x62C + 4..0x62C + 8].copy_from_slice(&0i32.to_le_bytes());
    assert!(!amp.access().await.unwrap().unwrap().locked);
    let a = amp.set_locked(false).await.unwrap();
    assert_eq!((lock_dword(), a.locked), (0, false));

    // no password, no login
    assert!(matches!(amp.login("Password").await, Err(Error::Unsupported(_))));
    assert!(matches!(amp.change_password("Password").await, Err(Error::Unsupported(_))));
}

/// BEH-47: a lock the amp does not take is an error, not a success
#[tokio::test]
async fn a_lock_the_ipx_does_not_take_is_an_error() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    fake.state().reject_writes.insert(Address::new(Env::DEVCFG, 0, 0, 1));
    assert!(matches!(amp.set_locked(true).await, Err(Error::NotConfirmed(_))));
}

#[tokio::test]
async fn status_presets_and_recall() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;

    let st = amp.status().await.unwrap();
    assert_eq!((st.driver.as_str(), st.model.as_str(), st.name.as_str()), ("ipx", "IPX 1200", "Stage Left"));
    assert_eq!(st.firmware, "2.3.4.0");
    let preset = st.preset.unwrap();
    // BEH-6: slots are the wire index + 1, as in the vendor app ("base" is wire index 1)
    assert_eq!((preset.index, preset.name.as_str()), (2, "base"));
    assert_eq!(st.temperatures, [35, 40, 38]);
    assert_eq!(st.extra["port"], "50168");
    assert_eq!(st.extra["serial"], "SN-0042");

    assert_eq!(amp.preset_slots(), 1..=50);
    let presets = amp.presets().await.unwrap();
    let got: Vec<(u32, &str)> = presets.iter().map(|p| (p.index, p.name.as_str())).collect();
    assert_eq!(&got[..3], [(2, "base"), (3, "alegra-ext"), (4, "alegra-int")]);
    assert_eq!(got.len(), 4);
    assert_eq!(got[3].0, 6);

    amp.recall_preset(3).await.unwrap();
    amp.get(&param("in1.gain")).await.unwrap();
    assert_eq!(fake.state().recalled, [2]);

    // BEH-6, BEH-36: slots 1-50 map to wire 0-49; anything else is refused before sending
    amp.recall_preset(50).await.unwrap();
    for bad in [0, 51] {
        assert!(matches!(amp.recall_preset(bad).await, Err(Error::InvalidValue(_))), "slot {bad}");
    }
    amp.get(&param("in1.gain")).await.unwrap();
    assert_eq!(fake.state().recalled, [2, 49]);

    assert_eq!(amp.temperatures().await.unwrap(), [35, 40, 38]);
    assert_eq!(amp.device_name().await.unwrap(), "Stage Left");
    assert_eq!(amp.stored_preset(2).await.unwrap().name, "base");
    assert!(matches!(amp.stored_preset(0).await, Err(Error::InvalidValue(_))));
}

/// BEH-37. SETPRESET payload (verified on an IPX 1200): char[16] name + u32 wire index, no reply.
#[tokio::test]
async fn store_preset() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;

    amp.store_preset(50, "Stage").await.unwrap();
    let mut want = padded("Stage", 16);
    want.extend(49u32.to_le_bytes());
    assert_eq!(fake.state().store_payloads, [want]);
    let p = amp.stored_preset(50).await.unwrap();
    assert_eq!((p.name.as_str(), p.index), ("Stage", 49));
    assert_eq!(amp.stored_preset(2).await.unwrap().name, "base");

    amp.store_preset(5, "fifteen chars!!").await.unwrap();
    assert_eq!(fake.state().store_payloads.len(), 2);

    // invalid slot or name: nothing is sent
    for slot in [0, 51] {
        assert!(matches!(amp.store_preset(slot, "x").await, Err(Error::InvalidValue(_))), "slot {slot}");
    }
    for name in ["", "   ", "Empty", "empty", "sixteen chars!!!", "tab\there", "ünï"] {
        assert!(matches!(amp.store_preset(6, name).await, Err(Error::InvalidValue(_))), "{name:?}");
    }
    assert_eq!(fake.state().store_payloads.len(), 2);

    // slots up to the read-only range (DEVCFG field 6) are refused
    let o = DeviceState::parse(&fake.state().sync).unwrap().device.read_only_preset_range;
    let range_at = 0x62C + 6 * 4;
    assert_eq!(o, 1, "fixture is from an amp with range 1");
    assert!(matches!(amp.store_preset(1, "x").await, Err(Error::Denied(_))));
    fake.state().sync[range_at..range_at + 4].copy_from_slice(&3u32.to_le_bytes());
    assert!(matches!(amp.store_preset(3, "x").await, Err(Error::Denied(_))));
    amp.store_preset(4, "Free").await.unwrap();
    assert_eq!(fake.state().store_payloads.len(), 3);

    // the amp did not take it
    fake.state().ignore_store = true;
    assert!(matches!(amp.store_preset(10, "Lost").await, Err(Error::NotConfirmed(_))));

    // same name as before: the name proves nothing, the stored settings must be the current ones
    fake.state().ignore_store = false;
    amp.set(&param("out1.gain"), f(-6.0)).await.unwrap();
    amp.store_preset(4, "Free").await.unwrap();
    fake.state().ignore_store = true;
    amp.set(&param("out1.gain"), f(-3.0)).await.unwrap();
    assert!(matches!(amp.store_preset(4, "Free").await, Err(Error::NotConfirmed(_))));
}

/// BEH-36: a current preset index outside the slots is no current preset, not a panic.
#[tokio::test]
async fn status_with_preset_index_out_of_range() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    for index in [u32::MAX, 50] {
        fake.state().sync[16..20].copy_from_slice(&index.to_le_bytes());
        assert_eq!(amp.status().await.unwrap().preset, None, "wire index {index}");
    }
}

async fn next_event<T>(
    rx: &mut broadcast::Receiver<AmpEvent>,
    mut pick: impl FnMut(AmpEvent) -> Option<T>,
) -> T {
    let wait = async {
        loop {
            if let Some(t) = pick(rx.recv().await.unwrap()) {
                return t;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(3), wait).await.expect("expected event did not arrive")
}

#[tokio::test]
async fn meters_and_events() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let mut events = amp.subscribe();

    let meters = next_event(&mut events, |e| if let AmpEvent::Meters(m) = e { Some(m) } else { None }).await;
    assert_eq!(meters.output, [-94.99, -94.99]);
    assert_eq!(meters.input.len(), 2);
    let latest = amp.meters();
    assert_eq!(latest, meters);
    assert_eq!(
        latest.extra.keys().map(String::as_str).collect::<Vec<_>>(),
        ["gain_reduction_a", "gain_reduction_b", "output_signal", "raw_input"]
    );

    let push = build_frame(Main::DEVPRMCHAGE, [Env::OUTPUT_LEVEL, 0, 0, 0], &5.5f32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    let push = build_frame(Main::GETPRM, [Env::OUTPUT_LEVEL, 1, 0, 1], &1i32.to_le_bytes()).unwrap();
    fake.cmd.send(Cmd::Push(push)).unwrap();
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let change = |e| if let AmpEvent::Changed(p, v) = e { Some((p.to_string(), v)) } else { None };
        seen.push(next_event(&mut events, change).await);
    }
    assert_eq!(seen, [("out1.gain".to_string(), f(5.5)), ("out2.mute".to_string(), Value::Bool(true))]);
}

#[tokio::test]
async fn heartbeat_is_sent() {
    let fake = FakeAmp::start().await;
    let config = ClientConfig {
        port: fake.port,
        heartbeat: Some(Duration::from_millis(60)),
        ..ClientConfig::default()
    };
    let amp = IpxAmp::connect_with_config("127.0.0.1", &config).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(fake.state().heartbeats >= 3, "{}", fake.state().heartbeats);
    amp.close().await;
    let after = fake.state().heartbeats;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fake.state().heartbeats, after);
}

#[tokio::test]
async fn connection_loss_fails_pending_requests() {
    let fake = FakeAmp::start().await;
    let amp = Arc::new(fake.connect().await);
    let mut events = amp.subscribe();
    fake.state().silent = true;

    let pending = {
        let amp = amp.clone();
        tokio::spawn(async move { amp.get(&param("in1.gain")).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    fake.cmd.send(Cmd::Drop).unwrap();

    let err = pending.await.unwrap().unwrap_err();
    assert!(matches!(err, Error::Disconnected(_)), "{err}");
    next_event(&mut events, |e| matches!(e, AmpEvent::Disconnected(Some(_))).then_some(())).await;
    assert!(matches!(amp.get(&param("in1.gain")).await, Err(Error::Disconnected(_))));
    assert!(matches!(amp.set(&param("in1.gain"), f(0.0)).await, Err(Error::Disconnected(_))));
    assert!(!amp.client().is_connected());
}

/// BEH-8, BEH-42: an amp that vanishes without closing the connection (power off, cable pulled) sends
/// nothing any more, not even meters: the connection is lost after a few heartbeats, and released.
#[tokio::test]
async fn an_amp_that_goes_quiet_is_a_lost_connection() {
    let fake = FakeAmp::start().await;
    let config = ClientConfig {
        port: fake.port,
        heartbeat: Some(Duration::from_millis(100)),
        ..ClientConfig::default()
    };
    let amp = IpxAmp::connect_with_config("127.0.0.1", &config).await.unwrap();
    let mut events = amp.subscribe();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(amp.client().is_connected(), "meters keep it alive");

    fake.state().quiet = true;
    let reason = tokio::time::timeout(
        Duration::from_secs(5),
        next_event(&mut events, |e| if let AmpEvent::Disconnected(r) = e { Some(r) } else { None }),
    )
    .await
    .expect("a quiet amp is a lost connection");
    assert!(reason.as_deref().is_some_and(|r| r.contains("no data")), "{reason:?}");
    assert!(!amp.client().is_connected());
    assert!(matches!(amp.get(&param("in1.gain")).await, Err(Error::Disconnected(_))));
}

#[tokio::test]
async fn local_close_is_clean() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let mut events = amp.subscribe();
    amp.close().await;
    amp.close().await;
    let reason =
        next_event(&mut events, |e| if let AmpEvent::Disconnected(r) = e { Some(r) } else { None }).await;
    assert_eq!(reason, None);
    assert!(matches!(amp.get(&param("in1.gain")).await, Err(Error::Disconnected(_))));
    // The fake accepts a new controller once the first one is gone.
    let again = fake.connect().await;
    assert_eq!(again.get(&param("out1.gain")).await.unwrap(), f(-2.5));
}

/// ARCH-6: the fake counts what changes the amp (SETPRM, RECALLPRESET, SETPRESET), so tests can prove
/// that a connection only reads (SEC-1).
#[tokio::test]
async fn fake_counts_writes_and_not_reads() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    amp.status().await.unwrap();
    amp.presets().await.unwrap();
    amp.snapshot().await.unwrap();
    amp.get(&param("out1.gain")).await.unwrap();
    assert_eq!(fake.writes(), 0);

    amp.set(&param("out1.gain"), f(1.0)).await.unwrap();
    assert_eq!(fake.state().setprm, [Address::new(Env::OUTPUT_LEVEL, 0, 0, 0)]);
    amp.recall_preset(3).await.unwrap();
    amp.store_preset(5, "Walk").await.unwrap();
    assert_eq!(fake.writes(), 3);
}

/// docs/ipx-protocol.md: setting the link flag copies channel 1's gain and limiter dBu to channel 2
/// once; after that the channels are independent. Nothing else is copied.
#[tokio::test]
async fn fake_link_copies_once_and_then_the_channels_are_independent() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let get = |name: &'static str| {
        let amp = &amp;
        async move { amp.get(&param(name)).await.unwrap() }
    };
    amp.set(&param("out1.gain"), f(-10.0)).await.unwrap();
    amp.set(&param("out2.gain"), f(-20.0)).await.unwrap();
    amp.set(&param("out1.limiter.threshold_dbu"), f(30.0)).await.unwrap();
    amp.set(&param("out2.limiter.threshold_dbu"), f(25.0)).await.unwrap();
    amp.set(&param("out1.eq.2.gain"), f(3.0)).await.unwrap();
    amp.set(&param("out2.eq.2.gain"), f(0.0)).await.unwrap();
    amp.set(&param("out2.mute"), Value::Bool(true)).await.unwrap();
    amp.set(&param("out1.mute"), Value::Bool(false)).await.unwrap();

    amp.set(&param("out1.link"), Value::Bool(true)).await.unwrap();
    assert_eq!(get("out2.gain").await, f(-10.0));
    assert_eq!(get("out2.limiter.threshold_dbu").await, f(30.0));
    assert_eq!(get("out2.eq.2.gain").await, f(0.0), "the EQ is not copied");
    assert_eq!(get("out2.mute").await, Value::Bool(true), "nor the mute");

    // linked, and still independent
    amp.set(&param("out1.gain"), f(-5.0)).await.unwrap();
    assert_eq!(get("out2.gain").await, f(-10.0));
    amp.set(&param("out2.gain"), f(-1.0)).await.unwrap();
    assert_eq!(get("out1.gain").await, f(-5.0));

    // inputs the same
    amp.set(&param("in1.gain"), f(-8.0)).await.unwrap();
    amp.set(&param("in2.gain"), f(-9.0)).await.unwrap();
    amp.set(&param("in1.link"), Value::Bool(true)).await.unwrap();
    assert_eq!(get("in2.gain").await, f(-8.0));
    // unlinking copies nothing
    amp.set(&param("in1.gain"), f(0.0)).await.unwrap();
    amp.set(&param("in1.link"), Value::Bool(false)).await.unwrap();
    assert_eq!(get("in2.gain").await, f(-8.0));
}

/// A recall applies the settings stored in the slot, mutes included, and makes it the current preset
/// (what SEC-6 has to survive). `recall_marks` says how many SETPRMs came before each recall.
#[tokio::test]
async fn fake_recall_applies_the_stored_settings() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    amp.set(&param("out1.mute"), Value::Bool(false)).await.unwrap();
    amp.set(&param("out1.gain"), f(-7.5)).await.unwrap();
    amp.store_preset(7, "Loud").await.unwrap();
    assert_eq!(fake.state().store_marks, [2], "two SETPRMs had arrived when the preset was stored");

    amp.set(&param("out1.mute"), Value::Bool(true)).await.unwrap();
    amp.set(&param("out1.gain"), f(0.0)).await.unwrap();
    amp.recall_preset(7).await.unwrap();
    assert_eq!(amp.get(&param("out1.mute")).await.unwrap(), Value::Bool(false));
    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(-7.5));
    let preset = amp.status().await.unwrap().preset.unwrap();
    assert_eq!((preset.index, preset.name.as_str()), (7, "Loud"));
    assert_eq!(fake.state().recall_marks, [4]);

    // a slot nobody stored changes no setting
    amp.set(&param("out1.gain"), f(1.0)).await.unwrap();
    amp.recall_preset(20).await.unwrap();
    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(1.0));
}

/// ARCH-6: a busy fake accepts the connection and closes it at once, like an amp that has another
/// controller; it serves again when the other controller is gone.
#[tokio::test]
async fn busy_fake_refuses_a_second_controller() {
    let fake = FakeAmp::start().await;
    fake.state().busy = true;
    let err = IpxAmp::connect_with("127.0.0.1", fake.port, Duration::from_secs(1)).await.err().unwrap();
    assert!(matches!(err, Error::Busy(_)), "{err}");
    fake.state().busy = false;
    assert_eq!(fake.connect().await.get(&param("out1.gain")).await.unwrap(), f(-2.5));
}

#[tokio::test]
async fn busy_amp_is_detected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            drop(sock);
        }
    });
    let err = IpxAmp::connect_with("127.0.0.1", port, Duration::from_secs(1)).await.err().unwrap();
    assert!(matches!(err, Error::Busy(_)), "{err}");
}

#[tokio::test]
async fn silent_amp_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = listener.accept().await {
            held.push(sock);
        }
    });
    let err = IpxAmp::connect_with("127.0.0.1", port, Duration::from_millis(200)).await.err().unwrap();
    assert!(matches!(err, Error::Timeout(_)), "{err}");
}

#[tokio::test]
async fn refused_connection_is_an_io_error() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let err = IpxAmp::connect_with("127.0.0.1", port, Duration::from_millis(200)).await.err().unwrap();
    assert!(matches!(err, Error::Io(_)), "{err}");
}

#[tokio::test]
async fn discovery_listener_sees_a_broadcast() {
    let listener = tokio::spawn(labgruppen::amps::ipx::discover(Duration::from_millis(800)));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut payload = vec![0u8; 0x48];
    payload[..4].copy_from_slice(b"ipx1");
    payload[16..22].copy_from_slice(b"Rack 1");
    payload[48..57].copy_from_slice(b"10.1.2.30");
    payload[0x40..0x42].copy_from_slice(&50168u16.to_le_bytes());
    let datagram = build_frame(Main::DEVBROADCAST, [0; 4], &payload).unwrap();
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender.send_to(&datagram, "127.0.0.1:52350").await.unwrap();
    sender.send_to(&datagram, "127.0.0.1:52350").await.unwrap();

    match listener.await.unwrap() {
        Ok(found) => {
            assert_eq!(found.len(), 1);
            assert_eq!((found[0].source.as_str(), found[0].name.as_str()), ("127.0.0.1", "Rack 1"));
            assert_eq!((found[0].ip.as_str(), found[0].port), ("10.1.2.30", 50168));
        }
        // Port 52350 taken by something else on this machine.
        Err(Error::Io(e)) => eprintln!("skipping discovery test: {e}"),
        Err(e) => panic!("{e}"),
    }
}

// BEH-18: an IPX that does not broadcast is found by asking every address of the subnet; what listens
// on the port but does not speak IPX is not taken for one
#[tokio::test]
async fn a_scan_finds_an_ipx_on_its_port_and_nothing_else() {
    let fake = FakeAmp::start().await;
    let found = labgruppen::amps::ipx::scan_port("127.0.0", fake.port, Duration::from_secs(3)).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "127.0.0.1");
    assert!(!found[0].1.is_empty(), "the model");
    assert_eq!(fake.writes(), 0, "a scan only reads");

    let mute = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = mute.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = mute.accept().await {
            held.push(sock); // accepts and says nothing
        }
    });
    let found = labgruppen::amps::ipx::scan_port("127.0.0", port, Duration::from_secs(2)).await;
    assert!(found.is_empty(), "{found:?}");
}

/// BEH-44: the device mode of an IPX is env 5 field 0 (1 = stereo, 0 = 2 ways, seen on an IPX 1200); a
/// settings file never sets it.
#[tokio::test]
async fn the_device_mode_is_read_and_written() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let mode = Address::new(Env::MODEAES, 0, 0, 0);
    amp.set(&Param::Mode, text("stereo")).await.unwrap();
    assert_eq!(amp.get_raw(mode).await.unwrap(), 1i32.to_le_bytes());
    assert_eq!(amp.get(&Param::Mode).await.unwrap(), text("stereo"));

    amp.set(&Param::Mode, text("two_way")).await.unwrap();
    assert_eq!(amp.get_raw(mode).await.unwrap(), 0i32.to_le_bytes());
    assert_eq!(amp.get(&Param::Mode).await.unwrap(), text("two_way"));
    assert_eq!(amp.snapshot().await.unwrap().params["mode"], text("two_way"));
    assert!(amp.set(&Param::Mode, text("bridge")).await.is_err());

    let mut snap = amp.snapshot().await.unwrap();
    snap.params.insert("mode".into(), text("stereo"));
    let report = amp.restore(&snap, false).await.unwrap();
    assert!(report.skipped.contains(&"mode".to_string()), "{:?}", report.skipped);
    assert_eq!(amp.get(&Param::Mode).await.unwrap(), text("two_way"));
}

/// BEH-44: a device mode the table does not know (bridge, parallel?) is shown as `unknown(n)`, and does not
/// fail the snapshot of the whole amp
#[tokio::test]
async fn an_unlisted_device_mode_is_unknown_not_an_error() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    amp.set_raw(Address::new(Env::MODEAES, 0, 0, 0), &2i32.to_le_bytes()).await.unwrap();
    assert_eq!(amp.get(&Param::Mode).await.unwrap(), text("unknown(2)"));
    assert_eq!(amp.snapshot().await.unwrap().params["mode"], text("unknown(2)"));
}

/// SEC-8: the passwords the amp hands out are not decoded into memory
#[test]
fn a_password_in_the_state_is_not_decoded() {
    let mut data = vec![0u8; DeviceState::SIZE];
    for k in 0..3 {
        let at = 0x54 + k * 0x14;
        let mut plain = [0u8; 16];
        plain[..7].copy_from_slice(b"hunter2");
        data[at..at + 16].copy_from_slice(&xor_password(&plain));
    }
    let state = DeviceState::parse(&data).unwrap();
    assert!(!format!("{state:?}").contains("hunter2"), "the password was decoded");
}

/// BEH-37, BEH-5: the amp stores a hair more than its limiter range (36.02 dBu for the 35.99 dBu of 69 Vpk),
/// so its own dump restores, with the nearest value in range; a value further out still fails
#[tokio::test]
async fn restore_takes_the_limiter_a_hair_above_the_range() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let mut snap = amp.snapshot().await.unwrap();
    snap.params.insert("out1.limiter.threshold_dbu".into(), f(36.02));
    snap.params.insert("out2.limiter.threshold_dbu".into(), f(39.9));

    let report = amp.restore(&snap, false).await.unwrap();

    let failed: Vec<_> = report.failed.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(failed, ["out2.limiter.threshold_dbu"], "{:?}", report.failed);
    let Value::Float(dbu) = amp.get(&param("out1.limiter.threshold_dbu")).await.unwrap() else { panic!() };
    assert!((dbu - 35.99).abs() < 1e-3, "the nearest value in range: {dbu}");
}

/// BEH-37, SEC-2: the limiter threshold is dBV in an IPD's dump and Vpk in an IPX's: a dump of the other
/// family leaves the limiter alone, and the rest is restored as usual
#[tokio::test]
async fn restore_of_the_other_family_leaves_the_limiter_alone() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;
    let mut snap = amp.snapshot().await.unwrap();
    snap.driver = "ipd".into();
    snap.params.insert("out1.limiter.threshold".into(), Value::Float(32.56));
    snap.params.insert("out1.gain".into(), f(-10.0));
    let before = amp.get(&param("out1.limiter.threshold")).await.unwrap();

    let dry = amp.restore(&snap, true).await.unwrap();
    assert!(dry.skipped.contains(&"out1.limiter.threshold".to_string()), "{:?}", dry.skipped);
    let report = amp.restore(&snap, false).await.unwrap();

    assert!(report.skipped.contains(&"out1.limiter.threshold".to_string()), "{:?}", report.skipped);
    assert_eq!(amp.get(&param("out1.limiter.threshold")).await.unwrap(), before);
    assert_eq!(amp.get(&param("out1.gain")).await.unwrap(), f(-10.0));
}
