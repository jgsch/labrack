#![cfg(feature = "ipx")]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use labgruppen::amps::ipx::client::ClientConfig;
use labgruppen::amps::ipx::params::{self, Ty};
use labgruppen::amps::ipx::protocol::{
    Address, Ctrl, Env, FrameParser, Main, build_frame, ctrl_sub, xor_password,
};
use labgruppen::amps::ipx::structs::{
    DeviceState, Meters, StoredPreset, SystemAttributes, audio_bytes, audio_dword, parse_preset_names,
    parse_valid_presets,
};
use labgruppen::amps::ipx::{Discovered, IpxAmp};
use labgruppen::{Amp, AmpEvent, Error, Param, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};

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
    assert!(kind.decode(&99i32.to_le_bytes()).is_err());

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
    assert_eq!(all.len(), 288);
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), all.len());
    assert_eq!(all[0], param("in1.gain"));
    assert_eq!(all[71], param("in2.gain"));
    assert_eq!(all[142], param("out1.gain"));

    let mut addrs = HashSet::new();
    for p in all {
        let slot = params::slot(p).unwrap();
        assert!(addrs.insert(slot.addr), "{p} shares its address");
        assert_eq!(params::param_for(slot.addr), Some(p));
        assert_eq!(params::spec(p), Some(slot.spec()));
        assert!(audio_dword(slot.addr).is_some() || slot.addr.env == Env::INPUT_MIXER, "{p}");
    }

    for missing in ["in1.polarity", "in1.xover.hp.freq", "out1.mixer.analog1.level", "in1.limiter.power"] {
        assert!(params::slot(&param(missing)).is_none(), "{missing}");
    }
    for read_only in ["out1.name", "in2.name", "out1.limiter.power", "out2.limiter.threshold_dbu"] {
        assert!(!params::spec(&param(read_only)).unwrap().writable, "{read_only}");
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
    assert_eq!(common.output, vec![-150.0, -150.0]);
    assert_eq!(common.extra["post_input"], m.post_input.to_vec());
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

// ---- a tiny in-process IPX ----

enum Cmd {
    Push(Vec<u8>),
    Drop,
}

#[derive(Default)]
struct State {
    sync: Vec<u8>,
    heartbeats: usize,
    recalled: Vec<u32>,
    reject_writes: HashSet<Address>,
    silent: bool,
    /// Do not answer the next GETPRM of these addresses (once per entry).
    drop_reads: Vec<Address>,
}

struct FakeAmp {
    port: u16,
    state: Arc<Mutex<State>>,
    cmd: mpsc::UnboundedSender<Cmd>,
}

impl FakeAmp {
    async fn start() -> FakeAmp {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State { sync: fixture("dev_prm_sync.bin"), ..State::default() }));
        let (cmd, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(listener, state.clone(), rx));
        FakeAmp { port, state, cmd }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    async fn connect(&self) -> IpxAmp {
        IpxAmp::connect_with("127.0.0.1", self.port, Duration::from_secs(2)).await.unwrap()
    }
}

async fn run(listener: TcpListener, state: Arc<Mutex<State>>, mut cmd: mpsc::UnboundedReceiver<Cmd>) {
    while let Ok((sock, _)) = listener.accept().await {
        serve(sock, &state, &mut cmd).await;
    }
}

async fn serve(mut sock: TcpStream, state: &Mutex<State>, cmd: &mut mpsc::UnboundedReceiver<Cmd>) {
    let meters = build_frame(Main::GETPRM, [Env::GAINSLEVEL, 0, 0, 0], &fixture("gainslevel.bin")).unwrap();
    let mut parser = FrameParser::new();
    let mut buf = [0u8; 4096];
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    loop {
        let out: Vec<Vec<u8>> = tokio::select! {
            n = sock.read(&mut buf) => match n {
                Ok(0) | Err(_) => return,
                Ok(n) => parser.feed(&buf[..n]).into_iter().flat_map(|fr| answer(state, fr)).collect(),
            },
            c = cmd.recv() => match c {
                Some(Cmd::Push(bytes)) => vec![bytes],
                Some(Cmd::Drop) | None => return,
            },
            _ = tick.tick() => vec![meters.clone()],
        };
        for bytes in out {
            if sock.write_all(&bytes).await.is_err() {
                return;
            }
        }
    }
}

fn padded(s: &str, n: usize) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.resize(n, 0);
    v
}

fn answer(state: &Mutex<State>, fr: labgruppen::amps::ipx::protocol::Frame) -> Vec<Vec<u8>> {
    const AUDIO: usize = DeviceState::AUDIO_OFFSET;
    let mut st = state.lock().unwrap();
    let reply = |main, payload: &[u8]| vec![build_frame(main, fr.sub, payload).unwrap()];
    match fr.main {
        Main::GETPRM => {
            let a = fr.address();
            let payload = match a.env {
                Env::SYSATTR => {
                    let mut p = padded("IPX 1200", 16);
                    p.extend(padded("SN-0042", 32));
                    p
                }
                _ if st.silent => return vec![],
                _ if st.drop_reads.contains(&a) => {
                    st.drop_reads.retain(|x| *x != a);
                    return vec![];
                }
                Env::DEV_PRM_SYNC => st.sync.clone(),
                Env::DEVICE_NAME => padded("Stage Left", 32),
                Env::ALL_TEMPERATURES => [35i32, 40, 38].iter().flat_map(|t| t.to_le_bytes()).collect(),
                _ => {
                    let len = params::param_for(a).and_then(params::slot).map_or(4, |s| s.wire_len());
                    match audio_bytes(&st.sync, AUDIO, a, len) {
                        // like the real amp: the derived limiter power is an integer in a single GETPRM
                        Some(b) if a.env == Env::OUTPUT_LIMITER && a.field == 2 => {
                            (f32::from_le_bytes(b.try_into().unwrap()).round() as i32).to_le_bytes().to_vec()
                        }
                        Some(b) => b.to_vec(),
                        None => return vec![],
                    }
                }
            };
            reply(Main::GETPRM, &payload)
        }
        Main::SETPRM => {
            let a = fr.address();
            if let Some(d) = audio_dword(a).filter(|_| !st.reject_writes.contains(&a)) {
                let o = AUDIO + d * 4;
                st.sync[o..o + fr.payload.len()].copy_from_slice(&fr.payload);
            }
            vec![]
        }
        Main::SYSCTRL => match fr.ctrl() {
            Ctrl::HEARTBEAT => {
                st.heartbeats += 1;
                vec![]
            }
            Ctrl::GETALLPRESETNAME => reply(Main::SYSCTRL, &fixture("preset_names.bin")),
            Ctrl::GETVALIDPRESETS => {
                let valid: Vec<u8> =
                    (0..50u32).flat_map(|i| u32::from(matches!(i, 1 | 2 | 3 | 5)).to_le_bytes()).collect();
                reply(Main::SYSCTRL, &valid)
            }
            Ctrl::GETPRESET => reply(Main::SYSCTRL, &fixture("getpreset_1.bin")),
            Ctrl::RECALLPRESET => {
                st.recalled.push(u32::from_le_bytes(fr.payload[..4].try_into().unwrap()));
                vec![]
            }
            _ => vec![],
        },
        _ => vec![],
    }
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
    assert!(matches!(amp.set(&param("out1.limiter.power"), f(1.0)).await, Err(Error::Unsupported(_))));
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
    assert_eq!(snap.params.len(), 288);
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
    assert_eq!(report.applied, 280);
    assert_eq!(report.skipped.len(), 8);
    assert_eq!(amp.snapshot().await.unwrap().params, snap.params);
}

#[tokio::test]
async fn status_presets_and_recall() {
    let fake = FakeAmp::start().await;
    let amp = fake.connect().await;

    let st = amp.status().await.unwrap();
    assert_eq!((st.driver.as_str(), st.model.as_str(), st.name.as_str()), ("ipx", "IPX 1200", "Stage Left"));
    assert_eq!(st.firmware, "2.3.4.0");
    let preset = st.preset.unwrap();
    assert_eq!((preset.index, preset.name.as_str()), (1, "base"));
    assert_eq!(st.temperatures, [35, 40, 38]);
    assert_eq!(st.extra["port"], "50168");
    assert_eq!(st.extra["serial"], "SN-0042");

    let presets = amp.presets().await.unwrap();
    let got: Vec<(u32, &str)> = presets.iter().map(|p| (p.index, p.name.as_str())).collect();
    assert_eq!(&got[..3], [(1, "base"), (2, "alegra-ext"), (3, "alegra-int")]);
    assert_eq!(got.len(), 4);
    assert_eq!(got[3].0, 5);

    amp.recall_preset(2).await.unwrap();
    amp.get(&param("in1.gain")).await.unwrap();
    assert_eq!(fake.state().recalled, [2]);

    assert_eq!(amp.temperatures().await.unwrap(), [35, 40, 38]);
    assert_eq!(amp.device_name().await.unwrap(), "Stage Left");
    assert_eq!(amp.stored_preset(1).await.unwrap().name, "base");
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
    assert_eq!(meters.output, [-150.0, -150.0]);
    assert_eq!(meters.input.len(), 2);
    let latest = amp.meters();
    assert_eq!(latest, meters);
    assert_eq!(
        latest.extra.keys().map(String::as_str).collect::<Vec<_>>(),
        ["amp", "gain_reduction_a", "gain_reduction_b", "post_input"]
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
