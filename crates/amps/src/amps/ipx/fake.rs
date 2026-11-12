//! A tiny in-process IPX amp (TCP on a free local port) for tests, built on the same fixtures as
//! the protocol tests. It serves one controller at a time, like the real amp, pushes meters every
//! 40 ms and records the writes it receives, so tests can prove that a connection only reads.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use super::IpxAmp;
use super::params;
use super::protocol::{Address, Ctrl, Env, Frame, FrameParser, Main, build_frame};
use super::structs::{AUDCFG_SIZE, DeviceState, StoredPreset, audio_bytes, audio_dword, cstr};

fn fixture(name: &str) -> Vec<u8> {
    match name {
        "dev_prm_sync.bin" => include_bytes!("../../../tests/fixtures/dev_prm_sync.bin").to_vec(),
        "gainslevel.bin" => include_bytes!("../../../tests/fixtures/gainslevel.bin").to_vec(),
        "getpreset_1.bin" => include_bytes!("../../../tests/fixtures/getpreset_1.bin").to_vec(),
        "preset_names.bin" => include_bytes!("../../../tests/fixtures/preset_names.bin").to_vec(),
        _ => panic!("no fixture {name}"),
    }
}

/// What the test can push at the connected controller.
pub enum Cmd {
    /// Raw bytes to send.
    Push(Vec<u8>),
    /// Close the connection.
    Drop,
}

#[derive(Default)]
pub struct State {
    pub sync: Vec<u8>,
    pub heartbeats: usize,
    pub recalled: Vec<u32>,
    pub reject_writes: HashSet<Address>,
    pub silent: bool,
    /// Send nothing on its own (no meters), as an amp that has vanished from the network.
    pub quiet: bool,
    /// Do not answer the next GETPRM of these addresses (once per entry).
    pub drop_reads: Vec<Address>,
    /// Do not answer the reading of the whole state, this many times, once it has been answered once
    /// (connecting reads it for the status).
    pub drop_syncs: usize,
    synced: bool,
    /// SETPRESET payloads as received.
    pub store_payloads: Vec<Vec<u8>>,
    /// Presets stored by SETPRESET: wire index -> name and audio config.
    pub stored: HashMap<u32, (String, Vec<u8>)>,
    /// Receive SETPRESET but do not store anything.
    pub ignore_store: bool,
    /// Address of every SETPRM received, oldest first.
    pub setprm: Vec<Address>,
    /// Another controller holds the amp: close every new connection at once.
    pub busy: bool,
    /// How many SETPRMs had arrived when each RECALLPRESET did: tells the order of writes.
    pub recall_marks: Vec<usize>,
    /// The same for each SETPRESET.
    pub store_marks: Vec<usize>,
}

pub struct FakeAmp {
    pub port: u16,
    state: Arc<Mutex<State>>,
    pub cmd: mpsc::UnboundedSender<Cmd>,
}

impl FakeAmp {
    pub async fn start() -> FakeAmp {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State { sync: fixture("dev_prm_sync.bin"), ..State::default() }));
        let (cmd, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(listener, state.clone(), rx));
        FakeAmp { port, state, cmd }
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Commands that change the amp so far: SETPRM, RECALLPRESET and SETPRESET. Reads and heartbeats
    /// are not writes.
    pub fn writes(&self) -> usize {
        let st = self.state();
        st.setprm.len() + st.recalled.len() + st.store_payloads.len()
    }

    pub async fn connect(&self) -> IpxAmp {
        IpxAmp::connect_with("127.0.0.1", self.port, Duration::from_secs(2)).await.unwrap()
    }
}

async fn run(listener: TcpListener, state: Arc<Mutex<State>>, mut cmd: mpsc::UnboundedReceiver<Cmd>) {
    while let Ok((sock, _)) = listener.accept().await {
        if state.lock().unwrap_or_else(|e| e.into_inner()).busy {
            drop(sock);
            continue;
        }
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
            _ = tick.tick() => match state.lock().unwrap_or_else(|e| e.into_inner()).quiet {
                true => vec![],
                false => vec![meters.clone()],
            },
        };
        for bytes in out {
            if sock.write_all(&bytes).await.is_err() {
                return;
            }
        }
    }
}

/// Where the 11 dwords of the device configuration start in the state (`docs/ipx-protocol.md`).
const DEVCFG_OFFSET: usize = 0x62C;

fn padded(s: &str, n: usize) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.resize(n, 0);
    v
}

fn answer(state: &Mutex<State>, fr: Frame) -> Vec<Vec<u8>> {
    const AUDIO: usize = DeviceState::AUDIO_OFFSET;
    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
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
                // the device configuration: 11 dwords in the state, from DEVCFG_OFFSET
                Env::DEVCFG => {
                    let o = DEVCFG_OFFSET + usize::from(a.field) * 4;
                    st.sync[o..o + 4].to_vec()
                }
                Env::DEV_PRM_SYNC if st.synced && st.drop_syncs > 0 => {
                    st.drop_syncs -= 1;
                    return vec![];
                }
                Env::DEV_PRM_SYNC => {
                    st.synced = true;
                    st.sync.clone()
                }
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
            st.setprm.push(a);
            if a.env == Env::DEVCFG && !st.reject_writes.contains(&a) {
                let o = DEVCFG_OFFSET + usize::from(a.field) * 4;
                st.sync[o..o + 4].copy_from_slice(&fr.payload[..4]);
            }
            if let Some(d) = audio_dword(a).filter(|_| !st.reject_writes.contains(&a)) {
                let o = AUDIO + d * 4;
                st.sync[o..o + fr.payload.len()].copy_from_slice(&fr.payload);
                // docs/ipx-protocol.md: switching a link on copies channel 1's gain and limiter dBu to
                // channel 2, once; nothing else is copied and nothing follows afterwards
                let on = fr.payload.get(..4) == Some(&1i32.to_le_bytes());
                let link = (a.env == Env::OUTPUT_LEVEL && a.field == 3)
                    || (a.env == Env::INPUT_LEVEL && a.field == 2);
                if link && on && a.channel == 0 {
                    let mut copy = |env: u8, field: u8| {
                        let from = audio_dword(Address::new(env, 0, 0, field));
                        let to = audio_dword(Address::new(env, 1, 0, field));
                        if let (Some(from), Some(to)) = (from, to) {
                            let (from, to) = (AUDIO + from * 4, AUDIO + to * 4);
                            st.sync.copy_within(from..from + 4, to);
                        }
                    };
                    copy(a.env, 0);
                    if a.env == Env::OUTPUT_LEVEL {
                        copy(Env::OUTPUT_LIMITER, 3);
                    }
                }
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
            Ctrl::GETPRESET => {
                let mut p = fixture("getpreset_1.bin");
                let index = u32::from_le_bytes(fr.payload[..4].try_into().unwrap());
                if let Some((name, audio)) = st.stored.get(&index) {
                    p[4..20].copy_from_slice(&padded(name, 16));
                    p[20..24].copy_from_slice(&index.to_le_bytes());
                    p[StoredPreset::AUDIO_OFFSET..StoredPreset::AUDIO_OFFSET + AUDCFG_SIZE]
                        .copy_from_slice(audio);
                }
                reply(Main::SYSCTRL, &p)
            }
            Ctrl::SETPRESET => {
                st.store_payloads.push(fr.payload.clone());
                let marked = st.setprm.len();
                st.store_marks.push(marked);
                if !st.ignore_store && fr.payload.len() == 20 {
                    let index = u32::from_le_bytes(fr.payload[16..20].try_into().unwrap());
                    let audio = st.sync[AUDIO..AUDIO + AUDCFG_SIZE].to_vec();
                    st.stored.insert(index, (cstr(&fr.payload[..16]), audio));
                }
                vec![]
            }
            Ctrl::RECALLPRESET => {
                let index = u32::from_le_bytes(fr.payload[..4].try_into().unwrap());
                st.recalled.push(index);
                let marked = st.setprm.len();
                st.recall_marks.push(marked);
                // a stored preset replaces the settings, mutes included, and becomes the current one
                if let Some((name, audio)) = st.stored.get(&index).cloned() {
                    st.sync[AUDIO..AUDIO + AUDCFG_SIZE].copy_from_slice(&audio);
                    st.sync[0..16].copy_from_slice(&padded(&name, 16));
                    st.sync[16..20].copy_from_slice(&index.to_le_bytes());
                }
                vec![]
            }
            _ => vec![],
        },
        _ => vec![],
    }
}
