//! Wire format of the IPD ("IntelliDrive") control protocol. Pure functions, no I/O.
//! See docs/ipd-protocol.md.

use crate::error::{Error, Result};

pub const TCP_PORT: u16 = 50675;
pub const ANNOUNCE_PORT: u16 = 50674;
pub const RESET_CONNECTION: &[u8] = b"DDRESETCONNECTIO";

pub const CODE_TABLE: [u8; 34] = [
    233, 45, 98, 215, 123, 42, 99, 23, 178, 132, 200, 150, 100, 50, 77, 43, 54, 90, 31, 43, 123, 234, 211,
    177, 65, 133, 3, 64, 23, 83, 251, 1, 54, 34,
];

// fragment opcodes
pub const OP_DATA_END: u8 = 0;
pub const OP_VU_IN: u8 = 1;
pub const OP_VU_OUT: u8 = 2;
pub const OP_GET: u8 = 3;
pub const OP_SET: u8 = 4;
pub const OP_RSP: u8 = 5;
pub const OP_ERR: u8 = 6;
pub const OP_GR_IN: u8 = 7;
pub const OP_GR_OUT: u8 = 8;
pub const OP_CHECKSUM: u8 = 9;
pub const OP_FACTORY: u8 = 10;
pub const OP_RAW_VU_IN: u8 = 11;
pub const OP_VU_READING: u8 = 12;
pub const OP_ACTIVITY_LOG: u8 = 15;

// struct ids
pub const S_GAIN: u8 = 0;
pub const S_MIXER: u8 = 1;
pub const S_DELAY: u8 = 2;
pub const S_LPF: u8 = 3;
pub const S_HPF: u8 = 4;
pub const S_PEQ: u8 = 5;
pub const S_LIMITER: u8 = 6;
pub const S_MUTE: u8 = 7;
pub const S_INVERT: u8 = 8;
pub const S_PRESET: u8 = 9;
pub const S_LINK: u8 = 10;
pub const S_COMMAND: u8 = 11;
pub const S_COMPRESSOR: u8 = 12;
pub const S_GLOBAL: u8 = 13;
pub const S_CHANNEL: u8 = 14;

// member ids
pub const M_GAIN: u8 = 0;
pub const M_FREQ: u8 = 1;
pub const M_Q: u8 = 2;
pub const M_TYPE: u8 = 3;
pub const M_THRESHOLD: u8 = 4;
pub const M_ATTACK: u8 = 5;
pub const M_RELEASE: u8 = 6;
pub const M_HOLD: u8 = 7;
pub const M_DELAY: u8 = 8;
pub const M_MUTE: u8 = 14;
pub const M_INVERT: u8 = 15;
pub const M_SHORT_NAME: u8 = 16;
pub const M_PRESET_NUMBER: u8 = 18;
pub const M_ON: u8 = 24;
pub const M_LINK: u8 = 25;
pub const M_COMMAND: u8 = 26;
pub const M_RATIO: u8 = 27;
pub const M_SET_PASSWORD: u8 = 28;
pub const M_CHECK_PASSWORD: u8 = 29;
pub const M_ACCESS_RIGHTS: u8 = 30;
pub const M_SW_VERSION: u8 = 33;
pub const M_STANDBY_DELAY: u8 = 34;
pub const M_HARDWARE_STATUS: u8 = 35;
pub const M_CONFIGURATION: u8 = 36;
pub const M_AMP_READINGS: u8 = 37;
pub const M_CURRENT_USER: u8 = 38;
pub const M_AMP_READINGS_2: u8 = 39;
pub const M_SUBNET_MASK: u8 = 40;
pub const M_STARTUP_PRESET: u8 = 43;
pub const M_AES_FS: u8 = 45;
pub const M_CONNECTOR_NAME: u8 = 46;
pub const M_MODEL_NUMBER: u8 = 48;
pub const M_MODEL_FIRMWARE: u8 = 49;
pub const M_MODEL_NAME: u8 = 50;
pub const M_PIN: u8 = 51;
pub const M_CHECK_PIN: u8 = 52;
pub const M_BUILD_NUMBER: u8 = 53;
pub const M_IP: u8 = 87;

// commands: SET Key(S_COMMAND, M_COMMAND, 0, cmd) = 1
pub const CMD_LOAD_PRESET: u8 = 1;
pub const CMD_SAVE_PRESET: u8 = 2;
pub const CMD_GOTO_STANDBY: u8 = 4;
pub const CMD_EXIT_STANDBY: u8 = 5;
pub const CMD_LOCATE: u8 = 6;

pub const USERS: [&str; 6] = ["AllDSP", "Factory", "Developer", "Admin", "User", "Locked"];

/// The user level that is locked: the last of [`USERS`].
pub const USER_LOCKED: i64 = 5;

pub use crate::model::AccessRights;

/// Hardware status flags (Global HARDWARE_STATUS), bit 1 excluded (it means "init done").
pub const STATUS_FLAGS: &[(u32, &str)] = &[
    (2, "memory test failed"),
    (4, "standby"),
    (8, "codec / internal bus failure"),
    (16, "DSP not synced"),
    (32, "protect"),
    (64, "dim display"),
    (128, "display off"),
    (1024, "AES backup"),
    (32768, "telnet enabled"),
    (524288, "amplifier protection/standby"),
];

/// PEQ filter types as sent on the wire, in the order of `model::EQ_TYPES`.
pub const PEQ_CODES: [u8; 8] = [128, 129, 130, 131, 132, 133, 134, 136];
/// Crossover shapes as sent on the wire, in the order of `model::XOVER_TYPES`.
pub const XOVER_CODES: [u8; 10] = [0x00, 0x10, 0x01, 0x11, 0x21, 0x02, 0x12, 0x03, 0x13, 0x23];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub struct_id: u8,
    pub member_id: u8,
    pub channel: u8,
    pub num: u8,
}

impl Key {
    pub const fn new(struct_id: u8, member_id: u8, channel: u8, num: u8) -> Key {
        Key { struct_id, member_id, channel, num }
    }
    pub fn bytes(self) -> [u8; 4] {
        [self.struct_id, self.member_id, self.channel, self.num]
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {}, {})", self.struct_id, self.member_id, self.channel, self.num)
    }
}

/// A value as it is on the wire: integer or text.
#[derive(Clone, Debug, PartialEq)]
pub enum Raw {
    Int(i64),
    Text(String),
}

impl Raw {
    pub fn int(&self) -> Option<i64> {
        if let Raw::Int(i) = self { Some(*i) } else { None }
    }
    pub fn text(&self) -> Option<&str> {
        if let Raw::Text(t) = self { Some(t) } else { None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    I16,
    U16,
    U8,
    U32,
    I32,
    Str(usize),
}

impl Fmt {
    pub fn size(self) -> usize {
        match self {
            Fmt::U8 => 1,
            Fmt::I16 | Fmt::U16 => 2,
            Fmt::U32 | Fmt::I32 => 4,
            Fmt::Str(n) => n,
        }
    }
}

/// Wire format of a member (big-endian); unknown members are signed 32-bit.
pub fn member_format(member: u8) -> Fmt {
    match member {
        M_GAIN | M_THRESHOLD | M_ATTACK | M_RELEASE | M_HOLD | M_RATIO => Fmt::I16,
        M_FREQ | M_Q => Fmt::U16,
        M_TYPE | M_MUTE | M_INVERT | M_ON | M_LINK | M_COMMAND | 9 => Fmt::U8,
        M_DELAY | M_PRESET_NUMBER | M_ACCESS_RIGHTS | 31 | M_SW_VERSION | M_STANDBY_DELAY
        | M_HARDWARE_STATUS | M_CONFIGURATION | M_AMP_READINGS | M_AMP_READINGS_2 | M_CURRENT_USER | M_IP
        | M_SUBNET_MASK | M_STARTUP_PRESET | 44 | M_AES_FS | M_MODEL_NUMBER | M_MODEL_FIRMWARE | M_PIN
        | M_CHECK_PIN | M_BUILD_NUMBER => Fmt::U32,
        M_SHORT_NAME | M_CONNECTOR_NAME | M_MODEL_NAME => Fmt::Str(16),
        M_SET_PASSWORD | M_CHECK_PASSWORD => Fmt::Str(8),
        64 => Fmt::Str(8),
        _ => Fmt::I32,
    }
}

/// raw = value * scale
pub fn member_scale(member: u8) -> f64 {
    match member {
        M_GAIN | M_THRESHOLD => 100.0,
        M_Q | M_RATIO => 10.0,
        M_DELAY => 1000.0,
        M_STANDBY_DELAY => 60000.0,
        M_AES_FS => 100.0,
        _ => 1.0,
    }
}

pub fn to_raw(member: u8, value: f64) -> i64 {
    (value * member_scale(member)).round() as i64
}

pub fn from_raw(member: u8, raw: i64) -> f64 {
    raw as f64 / member_scale(member)
}

pub fn encode_raw(member: u8, raw: &Raw) -> Result<Vec<u8>> {
    let fmt = member_format(member);
    let range = |lo: i64, hi: i64, v: i64| {
        if (lo..=hi).contains(&v) {
            Ok(v)
        } else {
            Err(Error::InvalidValue(format!("{v} does not fit member {member} ({fmt:?})")))
        }
    };
    if let Fmt::Str(n) = fmt {
        let text = raw.text().ok_or_else(|| Error::InvalidValue(format!("member {member} takes text")))?;
        let mut b: Vec<u8> = text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
        b.resize(n, b' ');
        return Ok(b);
    }
    let v = raw.int().ok_or_else(|| Error::InvalidValue(format!("member {member} takes a number")))?;
    Ok(match fmt {
        Fmt::U8 => vec![range(0, 255, v)? as u8],
        Fmt::I16 => (range(i16::MIN as i64, i16::MAX as i64, v)? as i16).to_be_bytes().to_vec(),
        Fmt::U16 => (range(0, u16::MAX as i64, v)? as u16).to_be_bytes().to_vec(),
        Fmt::U32 => (range(0, u32::MAX as i64, v)? as u32).to_be_bytes().to_vec(),
        Fmt::I32 => (range(i32::MIN as i64, i32::MAX as i64, v)? as i32).to_be_bytes().to_vec(),
        Fmt::Str(_) => unreachable!(),
    })
}

pub fn decode_raw(member: u8, data: &[u8]) -> Option<Raw> {
    let fmt = member_format(member);
    let d = data.get(..fmt.size())?;
    Some(match fmt {
        Fmt::U8 => Raw::Int(d[0] as i64),
        Fmt::I16 => Raw::Int(i16::from_be_bytes([d[0], d[1]]) as i64),
        Fmt::U16 => Raw::Int(u16::from_be_bytes([d[0], d[1]]) as i64),
        Fmt::U32 => Raw::Int(u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as i64),
        Fmt::I32 => Raw::Int(i32::from_be_bytes([d[0], d[1], d[2], d[3]]) as i64),
        Fmt::Str(_) => {
            let end = d.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
            Raw::Text(d[..end].iter().map(|&b| b as char).collect())
        }
    })
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0xAA, |a, b| a ^ b)
}

/// Build a "J" command. Each item is a GET (`None`) or a SET.
pub fn build_command(items: &[(Key, Option<Raw>)], mk2: bool, hw_id: u8, my_id: u8) -> Result<Vec<u8>> {
    let mut frags = Vec::new();
    for (key, raw) in items {
        match raw {
            None => {
                frags.extend([OP_GET, 7]);
                frags.extend(key.bytes());
                frags.push(b'j');
            }
            Some(raw) => {
                let val = encode_raw(key.member_id, raw)?;
                frags.extend([OP_SET, 6 + val.len() as u8]);
                frags.extend(key.bytes());
                frags.extend(val);
            }
        }
    }
    let plen = 4 + frags.len();
    let mut cmd =
        if mk2 { vec![(plen >> 8) as u8, plen as u8, b'D', b'J'] } else { vec![5, b'D', b'D', b'J'] };
    cmd.extend(frags.iter().enumerate().map(|(i, b)| b ^ CODE_TABLE[i % 32]));
    cmd.extend([0xFD, hw_id, 0xFF, my_id]);
    cmd.push(checksum(&cmd));
    Ok(cmd)
}

/// Sent to every address of the subnet when looking for amps.
pub fn build_probe() -> Vec<u8> {
    let mut cmd = b"4DPP\xfd\x00\xff\x00".to_vec();
    cmd.push(checksum(&cmd));
    cmd
}

/// Bytes of fragments one command may carry.
pub fn fragment_budget(mk2: bool) -> usize {
    if mk2 { 500 } else { 8 }
}

// ---- amp -> PC ----

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Announcement {
    pub mac: String,
    pub mk2: bool,
    pub unit_type: String,
    pub hw_version: String,
    pub pic_version: String,
    pub serial: u32,
    pub production_date: String,
    pub dsp_state: u8,
    pub pc_packet_id: u8,
    pub hw_packet_id: u8,
}

pub fn parse_announcement(d: &[u8]) -> Option<Announcement> {
    if d.len() < 42 || d[41] != b'Q' || &d[..3] != b"DDB" {
        return None;
    }
    Some(Announcement {
        mac: d[5..22].iter().map(|&b| b as char).collect::<String>().trim_matches(['\0', ' ']).to_string(),
        mk2: &d[36..42] == b"67890Q",
        unit_type: format!("{}_{}_{}", d[33], d[34], d[35]),
        hw_version: format!("{}.{}.{}", d[22], d[23], d[24]),
        pic_version: format!("{}.{}.{}", d[25], d[26], d[27]),
        serial: (d[28] as u32) << 16 | (d[29] as u32) << 8 | d[30] as u32,
        production_date: format!("{}-{}-{}", d[36], d[32], 2000 + d[31] as u32),
        dsp_state: d[37],
        pc_packet_id: d[39],
        hw_packet_id: d[40],
    })
}

/// Reply to [`build_probe`]. IPD2400 MK1 firmware 1.2.6 sends a 42-byte `DDP`/`DDC` packet that has
/// no unit info; then only the generation and the packet ids are known.
pub fn parse_probe_reply(data: &[u8]) -> Option<Announcement> {
    let i = data.windows(2).position(|w| w == b"DD")?;
    let d = &data[i..];
    if let Some(a) = parse_announcement(d) {
        return Some(a);
    }
    if d.len() < 42 || d[41] != b'Q' {
        return None;
    }
    Some(Announcement {
        mac: String::new(),
        mk2: &d[36..42] == b"67890Q",
        unit_type: String::new(),
        hw_version: String::new(),
        pic_version: String::new(),
        serial: 0,
        production_date: String::new(),
        dsp_state: 0,
        pc_packet_id: d[39],
        hw_packet_id: d[40],
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyKind {
    Data,
    Announce,
    /// `DDN`/`DDM`/`DDC`: the amp is busy or could not process the command; resend.
    Nack,
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub kind: ReplyKind,
    pub pc_id: u8,
    pub hw_id: u8,
    pub mk2: bool,
    pub fragments: Vec<(u8, Vec<u8>)>,
    pub checksum_ok: bool,
    pub announcement: Option<Announcement>,
}

impl Reply {
    fn bare(kind: ReplyKind, pc_id: u8, hw_id: u8) -> Reply {
        Reply { kind, pc_id, hw_id, mk2: false, fragments: vec![], checksum_ok: false, announcement: None }
    }
}

/// Port of the app's `parse_data_packet`. `data` is the whole decoded packet: `DDA` + 36 bytes + ids +
/// `Q` (MK1, fragments from offset 3) or the 10-byte `DDW` header + body + `Q` (MK2, from offset 10).
fn fragments(mut data: Vec<u8>, mk2: bool) -> Option<Vec<(u8, Vec<u8>)>> {
    let start = if mk2 { 10 } else { 3 };
    if !mk2 && data.get(3) == Some(&233) {
        // firmware quirk the app compensates: the first opcode is sent as 233 ^ opcode
        let mut op = if data.get(4) == Some(&213) { 9 } else { 5 };
        if op == 5
            && let Some(&next) = data.get(data[4] as usize + 3)
        {
            op = match next {
                2 => 1,
                8 => 7,
                _ => op,
            };
        }
        data[3] = op;
    }
    let (mut frags, mut pos, mut ok) = (Vec::new(), start, false);
    while pos + 2 < data.len() {
        let op = data[pos];
        pos += 1;
        match op {
            OP_DATA_END => break,
            OP_CHECKSUM => {
                let cs = data[start..pos - 1].iter().fold(0xD5, |a, b| a ^ b);
                if cs != data[pos] {
                    return None;
                }
                pos += 1;
                ok = true;
            }
            OP_ACTIVITY_LOG => {
                frags.push((op, data[pos.min(data.len())..(pos + 513).min(data.len())].to_vec()));
                pos += 513;
            }
            _ => {
                let size = data[pos].checked_sub(2)? as usize;
                pos += 1;
                if size < 1 || pos + size >= data.len() {
                    return None;
                }
                frags.push((op, data[pos..pos + size].to_vec()));
                pos += size;
            }
        }
    }
    ok.then_some(frags)
}

fn xor_table(data: &[u8], period: usize) -> Vec<u8> {
    data.iter().enumerate().map(|(i, b)| b ^ CODE_TABLE[i % period]).collect()
}

/// Splits the TCP byte stream into replies.
#[derive(Default)]
pub struct ReplyParser {
    buf: Vec<u8>,
}

impl ReplyParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) -> Vec<Reply> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            let Some(i) = self.buf.windows(2).position(|w| w == b"DD") else {
                let keep = self.buf.len().min(1);
                self.buf.drain(..self.buf.len() - keep);
                return out;
            };
            self.buf.drain(..i);
            if self.buf.len() < 10 {
                return out;
            }
            let kind = self.buf[2];
            if kind == b'W' {
                let n = 10 + ((self.buf[8] as usize) << 8 | self.buf[9] as usize);
                if self.buf.len() < n {
                    return out;
                }
                let pkt: Vec<u8> = self.buf.drain(..n).collect();
                let mut dec = pkt[..10].to_vec();
                dec.extend(xor_table(&pkt[10..], 34));
                dec.push(b'Q');
                let frags = fragments(dec, true);
                out.push(Reply {
                    mk2: true,
                    checksum_ok: frags.is_some(),
                    fragments: frags.unwrap_or_default(),
                    ..Reply::bare(ReplyKind::Data, pkt[7], pkt[8])
                });
                continue;
            }
            if kind == b'N' && (self.buf.len() < 42 || self.buf[41] != b'Q') {
                self.buf.drain(..10); // short NACK with an error code
                out.push(Reply::bare(ReplyKind::Nack, 0, 0));
                continue;
            }
            if self.buf.len() < 42 {
                return out;
            }
            let pkt: Vec<u8> = self.buf.drain(..42).collect();
            if pkt[41] != b'Q' {
                continue;
            }
            match kind {
                b'B' => {
                    let a = parse_announcement(&pkt);
                    out.push(Reply {
                        mk2: a.as_ref().is_some_and(|a| a.mk2),
                        announcement: a,
                        ..Reply::bare(ReplyKind::Announce, pkt[39], pkt[40])
                    });
                }
                b'A' => {
                    let body = if pkt[3..5] == [0, 0] { vec![0; 36] } else { xor_table(&pkt[3..39], 32) };
                    let mut dec = b"DDA".to_vec();
                    dec.extend(body);
                    dec.extend(&pkt[39..42]);
                    let frags = fragments(dec, false);
                    out.push(Reply {
                        checksum_ok: frags.is_some(),
                        fragments: frags.unwrap_or_default(),
                        ..Reply::bare(ReplyKind::Data, pkt[39], pkt[40])
                    });
                }
                b'N' | b'M' | b'C' => out.push(Reply::bare(ReplyKind::Nack, pkt[39], pkt[40])),
                _ => {}
            }
        }
    }
}

// ---- fragment payloads ----

pub fn signed(data: &[u8]) -> Vec<i8> {
    data.iter().map(|&b| b as i8).collect()
}

/// RSP fragment: key + value.
pub fn decode_rsp(data: &[u8]) -> Option<(Key, Raw)> {
    if data.len() < 5 {
        return None;
    }
    let key = Key::new(data[0], data[1], data[2], data[3]);
    Some((key, decode_raw(key.member_id, &data[4..])?))
}

/// Global AMP_READINGS: load impedance per output (0.1 ohm steps, 255 = open) and temperature.
pub fn decode_amp_readings(raw: u32) -> ([f64; 3], u8) {
    let z = |shift: u32| ((raw >> shift) & 0xFF) as f64 / 10.0;
    ([z(24), z(16), z(8)], (raw & 0xFF) as u8)
}

pub fn decode_status(flags: u32) -> Vec<&'static str> {
    let mut out = Vec::new();
    if flags & 1 == 0 {
        out.push("initialization running");
    }
    out.extend(STATUS_FLAGS.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, n)| *n));
    out
}

pub fn decode_version(v: u32) -> String {
    format!("{}.{}.{}", v >> 16, v >> 8 & 0xFF, v & 0xFF)
}
