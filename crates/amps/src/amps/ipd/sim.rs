//! An IPD amplifier simulator (TCP) for developing and testing without hardware.
//!
//! It answers GET/SET fragments like the real amp and puts meters in every reply. MK1 mode copies
//! the behaviour seen on an IPD2400 (firmware 1.2.6): the probe reply carries no unit info, about
//! every other command gets a `DDN` NACK, and answers are pipelined over the following replies.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use super::params::{self, MODELS_BY_NUMBER};
use super::protocol::*;
use crate::error::Result;

fn xor(data: &[u8], period: usize) -> Vec<u8> {
    data.iter().enumerate().map(|(i, b)| b ^ CODE_TABLE[i % period]).collect()
}

pub fn build_reply(frags: &[(u8, Vec<u8>)], mk2: bool, pc_id: u8, hw_id: u8) -> Vec<u8> {
    let mut body = Vec::new();
    for (op, data) in frags {
        body.extend([*op, data.len() as u8 + 2]);
        body.extend(data);
    }
    let cs = body.iter().fold(0xD5, |a, b| a ^ b);
    body.extend([OP_CHECKSUM, cs]);
    if mk2 {
        let mut out = b"DDW".to_vec();
        out.extend([0, 0, 0, 0, pc_id, (body.len() >> 8) as u8, body.len() as u8]);
        out.extend(xor(&body, 34));
        out
    } else {
        body.resize(36, 0);
        let mut out = b"DDA".to_vec();
        out.extend(xor(&body, 32));
        out.extend([pc_id, hw_id, b'Q']);
        out
    }
}

pub fn build_announcement(mk2: bool) -> Vec<u8> {
    let mut a = vec![0u8; 42];
    a[0..3].copy_from_slice(b"DDB");
    a[5..22].copy_from_slice(b"00:0A:5F:12:34:56");
    a[22..25].copy_from_slice(&[1, 0, 0]);
    a[25..28].copy_from_slice(&[1, 2, 0]);
    a[28..31].copy_from_slice(&[1, 226, 64]);
    (a[31], a[32]) = (15, 6);
    a[33..36].copy_from_slice(&[27, 0, 1]);
    if mk2 {
        a[36..42].copy_from_slice(b"67890Q");
    } else {
        (a[36], a[37], a[39], a[40], a[41]) = (1, 1, 0, 1, b'Q');
    }
    a
}

struct Command {
    items: Vec<(u8, Key, Vec<u8>)>,
    pc_id: u8,
}

fn parse_command(data: &[u8]) -> Option<Command> {
    let end = if data.len() >= 9 && data[2..4] == *b"DJ" && data[..2] != *b"\x05D" {
        (data[0] as usize) << 8 | data[1] as usize
    } else if data.starts_with(b"\x05DDJ") && data.len() >= 9 {
        data.len() - 5
    } else {
        return None;
    };
    if end < 4 || end + 5 > data.len() {
        return None;
    }
    let frags = xor(&data[4..end], 32);
    let mut items = Vec::new();
    let mut pos = 0;
    while pos + 6 <= frags.len() {
        let (op, size) = (frags[pos], frags[pos + 1] as usize);
        if size < 6 || pos + size > frags.len() {
            break;
        }
        let key = Key::new(frags[pos + 2], frags[pos + 3], frags[pos + 4], frags[pos + 5]);
        let val = if op == OP_GET { vec![] } else { frags[pos + 6..pos + size].to_vec() };
        items.push((op, key, val));
        pos += size;
    }
    Some(Command { items, pc_id: data[end + 3] })
}

pub struct SimAmp {
    mk2: bool,
    nack_every_other: bool,
    busy: bool,
    values: HashMap<Key, Raw>,
    queue: VecDeque<(u8, Vec<u8>)>,
    t0: Instant,
    hw_id: u8,
    noise: u32,
    /// Key of every SET / FACTORY item the amp processed (commands included), oldest first.
    writes: Vec<Key>,
    /// Settings saved by CMD_SAVE_PRESET, by slot; CMD_LOAD_PRESET applies them.
    presets: HashMap<i64, HashMap<Key, Raw>>,
    /// Writes to channel 2 of a linked pair.
    link_violations: Vec<Key>,
    /// A write is applied but echoed with the old value first, as an amp that is slow to apply it may do.
    stale_echo: bool,
    /// How long the amp takes to answer, as a real IPD does at times.
    latency: std::time::Duration,
    /// Key of every GET item the amp answered, oldest first.
    reads: Vec<Key>,
    /// The access rights a successful login gives.
    login_rights: u32,
    /// The amp never sends its model name along with the version, as a real IPD 2400 often does not.
    withhold_model_name: bool,
    /// The lock password leaves the level as it is.
    ignore_lock: bool,
    /// The network was told to restart (command 14): the connection is dropped after this answer.
    restart_network: bool,
}

/// The settings a linked pair shares (docs/ipd-protocol.md: gain and EQ seen on a real IPD). Mutes and
/// the mixer stay per channel here; what the real firmware does with them is not known.
fn is_shared(struct_id: u8) -> bool {
    matches!(struct_id, S_GAIN | S_DELAY | S_LPF | S_HPF | S_PEQ | S_LIMITER | S_COMPRESSOR)
}

#[allow(non_snake_case)]
fn GLOBAL(member: u8, ch: u8) -> Key {
    Key::new(S_GLOBAL, member, ch, 0)
}

impl SimAmp {
    pub fn new(model: usize, mk2: bool) -> SimAmp {
        let mut values = HashMap::new();
        for p in params::all_params() {
            if let Some(slot) = params::slot(&p) {
                let default = if matches!(member_format(slot.key.member_id), Fmt::Str(_)) {
                    Raw::Text(String::new())
                } else {
                    Raw::Int(0)
                };
                values.insert(slot.key, default);
            }
        }
        let mut set = |key: Key, raw: Raw| {
            values.insert(key, raw);
        };
        for ch in [0u8, 1, 0x80, 0x81] {
            for band in 0..10u8 {
                let freq = [32, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000][band as usize];
                set(Key::new(S_PEQ, M_TYPE, ch, band), Raw::Int(128));
                set(Key::new(S_PEQ, M_FREQ, ch, band), Raw::Int(freq));
                set(Key::new(S_PEQ, M_Q, ch, band), Raw::Int(7));
            }
            set(Key::new(S_LPF, M_FREQ, ch, 0), Raw::Int(20001));
            set(Key::new(S_HPF, M_FREQ, ch, 0), Raw::Int(19));
            set(Key::new(S_COMPRESSOR, M_RATIO, ch, 0), Raw::Int(40));
            set(Key::new(S_COMPRESSOR, M_ATTACK, ch, 0), Raw::Int(10));
            set(Key::new(S_COMPRESSOR, M_HOLD, ch, 0), Raw::Int(10));
            set(Key::new(S_COMPRESSOR, M_RELEASE, ch, 0), Raw::Int(100));
            set(Key::new(S_LIMITER, M_RELEASE, ch, 0), Raw::Int(100));
        }
        for ch in [0x80u8, 0x81] {
            set(Key::new(S_LIMITER, M_THRESHOLD, ch, 0), Raw::Int(3678));
            set(Key::new(S_COMPRESSOR, M_THRESHOLD, ch, 0), Raw::Int(3678));
            set(Key::new(S_MUTE, M_MUTE, ch, 0), Raw::Int(1));
        }
        let name = MODELS_BY_NUMBER[model.min(2)];
        set(GLOBAL(M_SHORT_NAME, 0), Raw::Text("SIM IPD".into()));
        set(GLOBAL(M_SW_VERSION, 0), Raw::Int(0x01020B));
        set(GLOBAL(M_MODEL_NUMBER, 0), Raw::Int(model as i64));
        set(GLOBAL(M_MODEL_NAME, 0), Raw::Text(name.into()));
        set(GLOBAL(M_HARDWARE_STATUS, 0), Raw::Int(1));
        set(GLOBAL(M_CURRENT_USER, 0), Raw::Int(4));
        set(GLOBAL(M_ACCESS_RIGHTS, 0), Raw::Int(0xFFFF_FFFF));
        set(GLOBAL(M_AMP_READINGS, 0), Raw::Int((80 << 24) | (82 << 16) | (255 << 8) | 41));
        set(GLOBAL(M_SET_PASSWORD, 4), Raw::Text("Password".into()));
        set(GLOBAL(M_IP, 0), Raw::Int(0xC0A8_01B7));
        set(GLOBAL(M_SUBNET_MASK, 0), Raw::Int(0xFFFF_FF00));
        for (i, n) in ["Default", "Sub", "Top"].iter().enumerate() {
            set(Key::new(S_PRESET, M_SHORT_NAME, i as u8 + 1, 0), Raw::Text((*n).into()));
        }
        set(Key::new(S_PRESET, M_SHORT_NAME, 0, 0), Raw::Text("Default".into()));
        set(Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0), Raw::Int(1));
        // stereo, with other bits set as on a real IPD2400 (35)
        set(Key::new(S_PRESET, M_DEVICE_MODE, 0, 0), Raw::Int(35));
        SimAmp {
            mk2,
            nack_every_other: !mk2,
            busy: false,
            values,
            queue: VecDeque::new(),
            t0: Instant::now(),
            hw_id: 1,
            noise: 12345,
            writes: Vec::new(),
            presets: HashMap::new(),
            link_violations: Vec::new(),
            stale_echo: false,
            latency: std::time::Duration::ZERO,
            reads: Vec::new(),
            login_rights: 0xFFFF_FFFF,
            withhold_model_name: false,
            ignore_lock: false,
            restart_network: false,
        }
    }

    fn next_hw_id(&mut self) -> u8 {
        self.hw_id = (self.hw_id % 254) + 1;
        self.hw_id
    }

    fn meters(&mut self) -> Vec<(u8, Vec<u8>)> {
        self.noise = self.noise.wrapping_mul(1664525).wrapping_add(1013904223);
        let jitter = (self.noise >> 16) as f64 / 65536.0 * 4.0 - 2.0;
        let lvl = (-20.0 + 10.0 * (self.t0.elapsed().as_secs_f64() * 2.0).sin() + jitter) as i8;
        let out = [0x80u8, 0x81].map(|ch| {
            if self.values.get(&Key::new(S_MUTE, M_MUTE, ch, 0)) == Some(&Raw::Int(1)) {
                -100i8
            } else {
                lvl + 10
            }
        });
        vec![
            (OP_VU_IN, vec![lvl as u8, (lvl - 3) as u8]),
            (OP_VU_OUT, vec![out[0] as u8, out[1] as u8]),
            (OP_GR_IN, vec![0, 0]),
            (OP_GR_OUT, vec![0, 0]),
        ]
    }

    fn rsp(&self, key: Key) -> (u8, Vec<u8>) {
        let default = if matches!(member_format(key.member_id), Fmt::Str(_)) {
            Raw::Text(String::new())
        } else {
            Raw::Int(0)
        };
        let raw = self.values.get(&key).unwrap_or(&default);
        let mut data = key.bytes().to_vec();
        data.extend(encode_raw(key.member_id, raw).unwrap_or_default());
        (OP_RSP, data)
    }

    /// Whether the pair `channel` belongs to (inputs 0 / 1, outputs 0x80 / 0x81) is linked.
    fn linked(&self, channel: u8) -> bool {
        self.values.get(&Key::new(S_LINK, M_LINK, channel & 0x80, 2)) == Some(&Raw::Int(1))
    }

    fn command(&mut self, cmd: u8) {
        let hw = GLOBAL(M_HARDWARE_STATUS, 0);
        let status = self.values.get(&hw).and_then(Raw::int).unwrap_or(0);
        match cmd {
            CMD_LOAD_PRESET => {
                let n = self
                    .values
                    .get(&Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0))
                    .and_then(Raw::int)
                    .unwrap_or(0);
                let name = self
                    .values
                    .get(&Key::new(S_PRESET, M_SHORT_NAME, n as u8, 0))
                    .cloned()
                    .unwrap_or(Raw::Text(String::new()));
                self.values.insert(Key::new(S_PRESET, M_SHORT_NAME, 0, 0), name);
                if let Some(saved) = self.presets.get(&n) {
                    self.values.extend(saved.iter().map(|(k, v)| (*k, v.clone())));
                }
            }
            CMD_SAVE_PRESET => {
                let n = self
                    .values
                    .get(&Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0))
                    .and_then(Raw::int)
                    .unwrap_or(0);
                let name = self
                    .values
                    .get(&Key::new(S_PRESET, M_SHORT_NAME, 0, 0))
                    .cloned()
                    .unwrap_or(Raw::Text(String::new()));
                self.values.insert(Key::new(S_PRESET, M_SHORT_NAME, n as u8, 0), name);
                let audio = |k: &Key| !matches!(k.struct_id, S_GLOBAL | S_PRESET | S_COMMAND);
                let settings = self.values.iter().filter(|(k, _)| audio(k)).map(|(k, v)| (*k, v.clone()));
                self.presets.insert(n, settings.collect());
            }
            CMD_GOTO_STANDBY => {
                self.values.insert(hw, Raw::Int(status | 4));
            }
            CMD_EXIT_STANDBY => {
                self.values.insert(hw, Raw::Int(status & !4));
            }
            CMD_ACTIVATE_NETWORK => self.restart_network = true,
            _ => {}
        }
    }

    /// Handle one request; `None` = no answer.
    pub fn handle(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        let Some(cmd) = parse_command(data) else {
            if data.starts_with(b"4DPP") {
                if self.mk2 {
                    return Some(build_announcement(true));
                }
                let mut r = b"DDP".to_vec();
                for _ in 0..36 {
                    self.noise = self.noise.wrapping_mul(1664525).wrapping_add(1013904223);
                    r.push((self.noise >> 24) as u8);
                }
                r.extend([0, self.hw_id, b'Q']);
                return Some(r);
            }
            return None;
        };
        if self.nack_every_other {
            self.busy = !self.busy;
            if self.busy {
                let hw = self.next_hw_id();
                let mut r = b"DDN".to_vec();
                r.extend([0u8; 36]);
                r.extend([cmd.pc_id, hw, b'Q']);
                return Some(r);
            }
        }
        let mut rsps: Vec<(u8, Vec<u8>)> = Vec::new();
        for (op, key, val) in &cmd.items {
            let mut echo = None;
            if matches!(*op, OP_SET | OP_FACTORY)
                && let Some(v) = decode_raw(key.member_id, val)
            {
                self.writes.push(*key);
                if key.struct_id == S_COMMAND {
                    self.command(key.num);
                } else if key.member_id == M_CHECK_PASSWORD {
                    // the echo is masked, as on the real amp: the password is not kept
                    self.values.insert(*key, Raw::Text("********".into()));
                    if v.text().is_some_and(|t| t == LOCK_PASSWORD) && !self.ignore_lock {
                        self.values.insert(GLOBAL(M_CURRENT_USER, 0), Raw::Int(USER_LOCKED));
                        self.values.insert(GLOBAL(M_ACCESS_RIGHTS, 0), Raw::Int(0));
                    }
                    for level in 1..=5u8 {
                        let stored = self.values.get(&GLOBAL(M_SET_PASSWORD, level));
                        if stored
                            .zip(v.text())
                            .is_some_and(|(s, t)| s.text().is_some_and(|s| s.trim_end() == t.trim_end()))
                        {
                            self.values.insert(GLOBAL(M_CURRENT_USER, 0), Raw::Int(level as i64));
                            self.values
                                .insert(GLOBAL(M_ACCESS_RIGHTS, 0), Raw::Int(self.login_rights as i64));
                        }
                    }
                } else if *key == GLOBAL(M_HARDWARE_STATUS, 0) {
                    // the amp keeps its own bits, and the top byte moves on its own (seen on an IPD 2400)
                    let old = self.values.get(key).and_then(Raw::int).unwrap_or(0);
                    let top = ((old >> 24) + 1) & 0xFF;
                    let kept = old & STATUS_READ_ONLY & 0x00FF_FFFF;
                    let new = (top << 24) | kept | (v.int().unwrap_or(0) & !STATUS_READ_ONLY);
                    self.values.insert(*key, Raw::Int(new));
                } else if is_shared(key.struct_id) && key.channel & 0x7F == 1 && self.linked(key.channel) {
                    // like the real firmware: echoed, but channel 2 keeps channel 1's value
                    self.link_violations.push(*key);
                    echo = Some(v);
                } else {
                    if is_shared(key.struct_id) && key.channel & 0x7F == 0 && self.linked(key.channel) {
                        let second = Key::new(key.struct_id, key.member_id, key.channel + 1, key.num);
                        self.values.insert(second, v.clone());
                    }
                    if self.stale_echo && !self.linked(key.channel) {
                        echo = self.values.get(key).cloned();
                    }
                    self.values.insert(*key, v);
                }
            }
            if *op == OP_GET {
                self.reads.push(*key);
            }
            match echo {
                Some(value) => {
                    let mut data = key.bytes().to_vec();
                    data.extend(encode_raw(key.member_id, &value).unwrap_or_default());
                    rsps.push((OP_RSP, data));
                }
                None => rsps.push(self.rsp(*key)),
            }
            // companions the real firmware sends along
            let companion = match (key.struct_id, key.member_id) {
                (S_GLOBAL, M_CURRENT_USER) => Some(M_AMP_READINGS),
                (S_GLOBAL, M_ACCESS_RIGHTS) => Some(M_MODEL_NUMBER),
                (S_GLOBAL, M_SW_VERSION) => Some(M_MODEL_NAME),
                _ => None,
            };
            if let Some(m) = companion.filter(|m| !(self.withhold_model_name && *m == M_MODEL_NAME)) {
                rsps.push(self.rsp(GLOBAL(m, 0)));
            }
        }
        let hw = self.next_hw_id();
        let frags = if self.mk2 {
            let mut f = self.meters();
            f.extend(rsps);
            f
        } else {
            self.queue.extend(rsps);
            match self.queue.pop_front() {
                Some(r) => vec![r],
                None => self.meters().into_iter().take(2).collect(),
            }
        };
        Some(build_reply(&frags, self.mk2, cmd.pc_id, hw))
    }
}

/// A running simulator.
pub struct SimServer {
    pub addr: SocketAddr,
    stop: watch::Sender<bool>,
    amp: Arc<Mutex<SimAmp>>,
}

impl SimServer {
    /// Listen on `addr` (port 0 picks a free one). `model`: 0 IPD2400, 1 IPD1200, 2 IPD4800.
    pub async fn spawn(addr: SocketAddr, mk2: bool, model: usize) -> Result<SimServer> {
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let (stop, mut stopped) = watch::channel(false);
        let amp = Arc::new(Mutex::new(SimAmp::new(model, mk2)));
        let shared = amp.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((conn, _)) = accepted else { continue };
                        tokio::spawn(serve(shared.clone(), conn, stopped.clone()));
                    }
                }
            }
        });
        Ok(SimServer { addr, stop, amp })
    }

    /// Writes to channel 2 of a linked pair so far: the real amp ignores them and may stop answering.
    pub fn link_violations(&self) -> Vec<Key> {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).link_violations.clone()
    }

    /// Key of every SET the simulated amp processed so far, oldest first. Reads are not listed.
    pub fn writes(&self) -> Vec<Key> {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).writes.clone()
    }

    /// Key of every GET the simulated amp answered so far, oldest first.
    pub fn reads(&self) -> Vec<Key> {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).reads.clone()
    }

    /// What the amp says about the preset it runs: its number (0: none known, as an IPD can say) and
    /// its name.
    pub fn set_current_preset(&self, index: i64, name: &str) {
        let mut amp = self.amp.lock().unwrap_or_else(|e| e.into_inner());
        amp.values.insert(Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0), Raw::Int(index));
        amp.values.insert(Key::new(S_PRESET, M_SHORT_NAME, 0, 0), Raw::Text(name.into()));
    }

    /// The number the amp holds for its device mode (bit 0x1000 set = two ways), as a change on the front
    /// panel would leave it.
    pub fn set_device_mode_number(&self, number: i64) {
        let mut amp = self.amp.lock().unwrap_or_else(|e| e.into_inner());
        amp.values.insert(Key::new(S_PRESET, M_DEVICE_MODE, 0, 0), Raw::Int(number));
    }

    /// The temperature the amp reports, as the amp leaves it in the readings.
    pub fn set_temperature(&self, percent: u8) {
        let mut amp = self.amp.lock().unwrap_or_else(|e| e.into_inner());
        let key = GLOBAL(M_AMP_READINGS, 0);
        let old = match amp.values.get(&key) {
            Some(Raw::Int(r)) => *r,
            _ => 0,
        };
        amp.values.insert(key, Raw::Int((old & !0xFF) | i64::from(percent)));
    }

    /// From now on the model name is no longer sent along with the version.
    pub fn withhold_model_name(&self) {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).withhold_model_name = true;
    }

    /// From now on every answer takes `latency`.
    pub fn set_latency(&self, latency: std::time::Duration) {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).latency = latency;
    }

    /// From now on a write is echoed with the value it had before, though the amp applies it.
    pub fn echo_old_values(&self) {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).stale_echo = true;
    }

    /// Put the amp at user level `level` (index in `USERS`) with these access rights, as if someone
    /// had logged in or out at the front panel.
    pub fn set_access(&self, level: i64, rights: u32) {
        let mut amp = self.amp.lock().unwrap_or_else(|e| e.into_inner());
        amp.values.insert(GLOBAL(M_CURRENT_USER, 0), Raw::Int(level));
        amp.values.insert(GLOBAL(M_ACCESS_RIGHTS, 0), Raw::Int(rights as i64));
    }

    /// The value the amp holds for `key`.
    pub fn value(&self, key: Key) -> Option<Raw> {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).values.get(&key).cloned()
    }

    /// The hardware status as the amp would report it (display bits included).
    pub fn set_hardware_status(&self, status: i64) {
        let mut amp = self.amp.lock().unwrap_or_else(|e| e.into_inner());
        amp.values.insert(GLOBAL(M_HARDWARE_STATUS, 0), Raw::Int(status));
    }

    /// From now on the lock password leaves the level as it is.
    pub fn ignore_lock(&self) {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).ignore_lock = true;
    }

    /// The access rights a successful login gives (all of them unless set).
    pub fn set_login_rights(&self, rights: u32) {
        self.amp.lock().unwrap_or_else(|e| e.into_inner()).login_rights = rights;
    }

    pub fn shutdown(&self) {
        let _ = self.stop.send(true);
    }
}

impl Drop for SimServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn serve(amp: Arc<Mutex<SimAmp>>, mut conn: TcpStream, mut stopped: watch::Receiver<bool>) {
    let mut buf = vec![0u8; 4096];
    loop {
        let n = tokio::select! {
            _ = stopped.changed() => return,
            n = conn.read(&mut buf) => match n { Ok(0) | Err(_) => return, Ok(n) => n },
        };
        if buf[..n].starts_with(b"DDRESET") {
            return;
        }
        let (reply, latency, restart) = {
            let mut amp = amp.lock().unwrap_or_else(|e| e.into_inner());
            let reply = amp.handle(&buf[..n]);
            (reply, amp.latency, std::mem::take(&mut amp.restart_network))
        };
        if restart {
            // as the real amp: no answer, the connection goes
            return;
        }
        if !latency.is_zero() {
            tokio::time::sleep(latency).await;
        }
        if let Some(r) = reply
            && conn.write_all(&r).await.is_err()
        {
            return;
        }
    }
}
