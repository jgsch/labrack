//! Wire format of the IPX binary control protocol (TCP 50168, UDP 52350). No I/O in here.
//!
//! Frame, little-endian: `sync(4) len(4) main(2) src(2) reserved(4) | sub(4) payload | checksum(4)
//! trailer(4)`. `len` counts everything after the 16-byte header. The checksum is the byte sum of
//! `sub + payload` modulo 256; a zero checksum is accepted without checking.

use crate::error::{Error, Result};

pub const SYNC: u32 = 0xAA01AA02;
pub const TRAILER: u32 = 0x11110123;
pub const HEADER_LEN: usize = 16;
pub const MAX_LEN: usize = 0x2800;

pub const TCP_PORT: u16 = 50168;
pub const DISCOVERY_PORT: u16 = 52350;

/// Fixed XOR key the app applies to 16-byte password fields.
pub const PASSWORD_KEY: [u8; 16] =
    [0x3a, 0x7f, 0x92, 0x4b, 0x1d, 0xe8, 0x6c, 0x55, 0xa3, 0x0f, 0x78, 0x9b, 0x2e, 0xc1, 0x4d, 0x8a];

macro_rules! consts {
    ($(#[$m:meta])* $name:ident($ty:ty) { $($c:ident = $v:expr),+ $(,)? }) => {
        $(#[$m])*
        pub struct $name;
        impl $name {
            $(pub const $c: $ty = $v;)+

            /// Symbolic name of a value, for logs and errors.
            pub fn name(value: $ty) -> Option<&'static str> {
                match value { $(v if v == Self::$c => Some(stringify!($c)),)+ _ => None }
            }
        }
    };
}

consts! {
    /// Frame kind (the `main` field of the header).
    Main(u16) {
        NONE = 0, SETPRM = 1, GETPRM = 2, SYSCTRL = 3, DEVBROADCAST = 4, DEVPRMCHAGE = 5,
    }
}

consts! {
    /// Parameter blocks: first byte of the SETPRM/GETPRM address.
    Env(u8) {
        NONE = 0, SYSATTR = 1, USER = 2, NETWORK = 3, AUDCFG = 4, MODEAES = 5, INPUT_MIXER = 6,
        INPUT_LEVEL = 7, INPUT_EQ = 8, INPUT_DELAY = 9, INPUT_COMPRESSOR = 10, INPUT_RMS_LIMITER = 11,
        OUTPUT_XOVER = 12, OUTPUT_LEVEL = 13, OUTPUT_EQ = 14, OUTPUT_DELAY = 15, OUTPUT_COMPRESSOR = 16,
        OUTPUT_LIMITER = 17, ACK = 18, DEVCFG = 19, GAINSLEVEL = 20, PRESET_PRMCFG = 21, TEMPERATURE = 22,
        SWITCH_USER = 23, USERPASSWORD = 24, DEV_PRM_SYNC = 25, PRESET_NOTIFY = 26, DEVICE_NAME = 27,
        ALL_TEMPERATURES = 28, FIR_MODE = 29, STATUS = 31,
    }
}

consts! {
    /// SYSCTRL commands (first u16 of the sub-header).
    Ctrl(u16) {
        NONE = 0, HEARTBEAT = 1, SETPRESET = 2, GETPRESET = 3, RECALLPRESET = 4, DELETEPRESET = 5,
        QUERYSTATUS = 6, DEVICE_REBOOT = 7, SETDEV_FACTORYINFO = 8, UPGRADE_REQUEST = 9, UPGRADE_READY = 10,
        UPGRADE_PACKAGE = 11, UPGRADE_SUCCESS = 12, USER_LOGIN = 13, DEL_ALLUSERPRESET = 14,
        DEL_ALLFACTPRESET = 15, ACK = 16, FPLEDFLASH = 17, GETALLPRESETNAME = 18, GETVALIDPRESETS = 19,
        STOREONEPRESET_PKG = 20, PC_STORE_PRESET = 21, PC_RECALL_PRESET = 22, SET_HARDWARE_CFG = 23,
        GET_HARDWARE_CFG = 24, FORCE_FAN_ON = 25, INSTALL_KEY = 26, GET_KEY = 27, DEL_KEY = 28,
        PC_RECALL_PRESET_VLOCK = 29, FIR_SET_DATA = 30, FIR_GET_DATA = 31, STORE_FIR_DATA = 32,
        LOAD_FIR_DATA = 33, GET_LOG = 34,
    }
}

/// A single 4-byte parameter: block, channel, item (band / source / filter), field (dword index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Address {
    pub env: u8,
    pub channel: u8,
    pub item: u8,
    pub field: u8,
}

impl Address {
    pub const fn new(env: u8, channel: u8, item: u8, field: u8) -> Self {
        Address { env, channel, item, field }
    }

    /// Address of a whole block without channel/item/field (SYSATTR, DEV_PRM_SYNC, ...).
    pub const fn block(env: u8) -> Self {
        Address::new(env, 0, 0, 0)
    }

    pub const fn pack(self) -> [u8; 4] {
        [self.env, self.channel, self.item, self.field]
    }

    pub const fn unpack(sub: [u8; 4]) -> Self {
        Address::new(sub[0], sub[1], sub[2], sub[3])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub main: u16,
    pub sub: [u8; 4],
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn address(&self) -> Address {
        Address::unpack(self.sub)
    }

    pub fn ctrl(&self) -> u16 {
        u16::from_le_bytes([self.sub[0], self.sub[1]])
    }
}

fn byte_sum(body: &[u8]) -> u32 {
    body.iter().fold(0u32, |a, b| a.wrapping_add(*b as u32)) & 0xFF
}

pub fn build_frame(main: u16, sub: [u8; 4], payload: &[u8]) -> Result<Vec<u8>> {
    let len = 4 + payload.len() + 8;
    if len > MAX_LEN {
        return Err(Error::Protocol(format!("frame too large ({len} bytes)")));
    }
    let mut out = Vec::with_capacity(HEADER_LEN + len);
    out.extend_from_slice(&SYNC.to_le_bytes());
    out.extend_from_slice(&(len as u32).to_le_bytes());
    out.extend_from_slice(&main.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&sub);
    out.extend_from_slice(payload);
    let checksum = byte_sum(&out[HEADER_LEN..]);
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&TRAILER.to_le_bytes());
    Ok(out)
}

pub fn ctrl_sub(cmd: u16) -> [u8; 4] {
    let c = cmd.to_le_bytes();
    [c[0], c[1], 0, 0]
}

/// Incremental parser: [`feed`](FrameParser::feed) bytes, get back complete frames. Skips junk
/// before a sync word and drops frames with a bad trailer or checksum.
#[derive(Debug, Default)]
pub struct FrameParser {
    buf: Vec<u8>,
}

impl FrameParser {
    const SYNC_BYTES: [u8; 4] = SYNC.to_le_bytes();

    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, data: &[u8]) -> Vec<Frame> {
        let buf = &mut self.buf;
        buf.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            let Some(i) = buf.windows(4).position(|w| w == Self::SYNC_BYTES) else {
                let keep = buf.len().saturating_sub(3);
                buf.drain(..keep);
                break;
            };
            buf.drain(..i);
            if buf.len() < HEADER_LEN {
                break;
            }
            let length = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            let main = u16::from_le_bytes([buf[8], buf[9]]);
            if !(12..=MAX_LEN).contains(&length) {
                buf.drain(..4);
                continue;
            }
            let end = HEADER_LEN + length;
            if buf.len() < end {
                break;
            }
            let word = |o: usize| u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
            let (checksum, trailer) = (word(end - 8), word(end - 4));
            let body = buf[HEADER_LEN..end - 8].to_vec();
            buf.drain(..end);
            if trailer != TRAILER || (checksum != 0 && checksum != byte_sum(&body)) {
                continue;
            }
            frames.push(Frame {
                main,
                sub: [body[0], body[1], body[2], body[3]],
                payload: body[4..].to_vec(),
            });
        }
        frames
    }
}

/// Encode or decode a 16-byte password field (the operation is its own inverse).
pub fn xor_password(raw: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = raw.get(i).copied().unwrap_or(0) ^ PASSWORD_KEY[i];
    }
    out
}
