//! Decoders for the binary structures the amp sends (little-endian, 4-byte fields).
//!
//! The audio configuration (0x548 bytes, in DEV_PRM_SYNC at 0xE4 and in a GETPRESET reply at 0x48)
//! is not decoded into structs. Every 4-byte parameter has an [`Address`], and [`audio_dword`] maps
//! that address to the dword index inside the audio config, so one table drives both single reads and
//! bulk reads.

use std::collections::BTreeMap;

use super::protocol::{Address, Env, xor_password};
use crate::error::{Error, Result};

pub const AUDCFG_SIZE: usize = 0x548;

/// Latin-1 text up to the first NUL.
pub fn cstr(b: &[u8]) -> String {
    b.iter().take_while(|c| **c != 0).map(|c| *c as char).collect()
}

fn u32_at(data: &[u8], o: usize) -> Option<u32> {
    data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(data: &[u8], o: usize) -> Option<f32> {
    u32_at(data, o).map(f32::from_bits)
}

fn str_at(data: &[u8], o: usize, n: usize) -> String {
    data.get(o..o + n).map(cstr).unwrap_or_default()
}

fn round(v: f64, decimals: i32) -> f64 {
    let k = 10f64.powi(decimals);
    (v * k).round() / k
}

/// Dword index inside the audio config of the parameter at `addr`; `None` when the address is not
/// part of it. For text fields (names) this is the first of four dwords.
pub fn audio_dword(addr: Address) -> Option<usize> {
    let (c, item, f) = (addr.channel as usize, addr.item as usize, addr.field as usize);
    if c >= 2 {
        return None;
    }
    // (base of channel 0, stride per channel, items, stride per item, fields per item)
    let (base, chan, items, item_stride, fields) = match addr.env {
        Env::INPUT_MIXER => (0x02, 0x18, 4, 6, 6),
        Env::INPUT_LEVEL => (0x32, 7, 1, 0, 7),
        Env::INPUT_EQ => (0x40, 0x32, 10, 5, 5),
        Env::INPUT_DELAY => (0xA4, 2, 1, 0, 2),
        Env::INPUT_COMPRESSOR => (0xA8, 6, 1, 0, 6),
        Env::INPUT_RMS_LIMITER => (0xB4, 2, 1, 0, 2),
        Env::OUTPUT_XOVER => (0xB8, 6, 2, 3, 3),
        Env::OUTPUT_LEVEL => (0xC4, 8, 1, 0, 8),
        Env::OUTPUT_EQ => (0xD4, 0x32, 10, 5, 5),
        Env::OUTPUT_DELAY => (0x138, 2, 1, 0, 2),
        Env::OUTPUT_COMPRESSOR => (0x13C, 6, 1, 0, 6),
        Env::OUTPUT_LIMITER => (0x148, 5, 1, 0, 5),
        _ => return None,
    };
    (item < items && f < fields).then_some(base + c * chan + item * item_stride + f)
}

/// `len` bytes at the parameter `addr` of an audio config that starts at `base` in `raw`.
pub fn audio_bytes(raw: &[u8], base: usize, addr: Address, len: usize) -> Option<&[u8]> {
    let o = base + audio_dword(addr)? * 4;
    raw.get(o..o + len)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    pub dhcp: bool,
    pub port: u32,
    pub ip: String,
    pub netmask: String,
    pub gateway: String,
    pub mac: String,
}

impl NetworkConfig {
    fn parse(data: &[u8], o: usize) -> Self {
        NetworkConfig {
            dhcp: u32_at(data, o).unwrap_or(0) != 0,
            port: u32_at(data, o + 4).unwrap_or(0),
            ip: str_at(data, o + 8, 16),
            netmask: str_at(data, o + 24, 16),
            gateway: str_at(data, o + 40, 16),
            mac: str_at(data, o + 56, 32),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceConfig {
    pub display_mode: u32,
    pub lock_unit: bool,
    pub power_off: bool,
    /// 255 = last used.
    pub power_on_preset: u32,
    pub read_only_preset_range: u32,
    pub auto_power_down: u32,
    pub locked_access_rights: u32,
    pub user_access_rights: u32,
    pub admin_access_rights: u32,
}

impl DeviceConfig {
    fn parse(data: &[u8], o: usize) -> Self {
        let v = |i: usize| u32_at(data, o + i * 4).unwrap_or(0);
        DeviceConfig {
            display_mode: v(0),
            lock_unit: v(1) != 0,
            power_off: v(2) != 0,
            power_on_preset: v(5),
            read_only_preset_range: v(6),
            auto_power_down: v(7),
            locked_access_rights: v(8),
            user_access_rights: v(9),
            admin_access_rights: v(10),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    pub build: String,
    pub mcu: String,
    pub dsp: String,
}

/// Everything returned by GETPRM DEV_PRM_SYNC (what the app requests right after connecting).
#[derive(Clone, Debug)]
pub struct DeviceState {
    pub preset_name: String,
    pub preset_index: u32,
    pub current_user_mode: u32,
    /// Access level -> password (decoded).
    pub user_passwords: BTreeMap<u32, String>,
    pub network: NetworkConfig,
    pub device: DeviceConfig,
    pub versions: Versions,
    pub device_id: String,
    pub vendor_locked: bool,
    pub raw: Vec<u8>,
}

impl DeviceState {
    pub const SIZE: usize = 0x6B0;
    pub const AUDIO_OFFSET: usize = 0xE4;

    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::Protocol(format!("DEV_PRM_SYNC payload too short ({} bytes)", data.len())));
        }
        let mut user_passwords = BTreeMap::new();
        for k in 0..3 {
            let o = 0x44 + 0x10 + k * 0x14;
            let level = if k > 0 { u32_at(data, o - 4).unwrap_or(0) } else { 0 };
            user_passwords.insert(level, cstr(&xor_password(&data[o..o + 16])));
        }
        Ok(DeviceState {
            preset_name: str_at(data, 0, 16),
            preset_index: u32_at(data, 16).unwrap_or(0),
            current_user_mode: u32_at(data, 0x44 + 8).unwrap_or(0),
            user_passwords,
            network: NetworkConfig::parse(data, 0x8C),
            device: DeviceConfig::parse(data, 0x62C),
            versions: Versions {
                build: str_at(data, 0x65C, 16),
                mcu: str_at(data, 0x66C, 16),
                dsp: str_at(data, 0x67C, 16),
            },
            device_id: str_at(data, 0x68C, 32),
            vendor_locked: u32_at(data, 0x6AC).unwrap_or(0) != 0,
            raw: data.to_vec(),
        })
    }

    /// Raw bytes of an audio parameter (4 bytes, or 16 for text fields).
    pub fn audio_bytes(&self, addr: Address, len: usize) -> Option<&[u8]> {
        audio_bytes(&self.raw, Self::AUDIO_OFFSET, addr, len)
    }
}

/// GETPRESET reply: one preset stored on the amp.
#[derive(Clone, Debug)]
pub struct StoredPreset {
    pub name: String,
    pub index: u32,
    pub raw: Vec<u8>,
}

impl StoredPreset {
    pub const AUDIO_OFFSET: usize = 4 + 0x44;

    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < Self::AUDIO_OFFSET + AUDCFG_SIZE {
            return Err(Error::Protocol(format!("GETPRESET payload too short ({} bytes)", data.len())));
        }
        Ok(StoredPreset {
            name: str_at(data, 4, 16),
            index: u32_at(data, 4 + 16).unwrap_or(0),
            raw: data.to_vec(),
        })
    }

    pub fn audio_bytes(&self, addr: Address, len: usize) -> Option<&[u8]> {
        audio_bytes(&self.raw, Self::AUDIO_OFFSET, addr, len)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemAttributes {
    pub model: String,
    pub serial: String,
    pub raw: Vec<u8>,
}

impl SystemAttributes {
    pub fn parse(data: &[u8]) -> Self {
        SystemAttributes {
            model: cstr(&data[..data.len().min(16)]),
            serial: cstr(data.get(16..data.len().min(48)).unwrap_or_default()),
            raw: data.to_vec(),
        }
    }
}

/// GAINSLEVEL push (about 10 Hz while connected), in dB. Labels of the first three pairs were checked
/// against live audio; the others are inferred.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meters {
    /// Input signal, before input gain.
    pub input: [f64; 2],
    /// Output signal (-150 when muted).
    pub output: [f64; 2],
    /// After mixer and input gain.
    pub post_input: [f64; 2],
    /// Power stage / output voltage meter.
    pub amp: [f64; 2],
    /// Compressor / limiter gain reduction.
    pub gain_reduction_a: [f64; 2],
    pub gain_reduction_b: [f64; 2],
}

impl Meters {
    pub const SIZE: usize = 48;

    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut v = [0f64; 12];
        for (i, x) in v.iter_mut().enumerate() {
            *x = round(f32_at(data, i * 4)? as f64, 2);
        }
        let pair = |i: usize| [v[i * 2], v[i * 2 + 1]];
        Some(Meters {
            input: pair(0),
            output: pair(1),
            post_input: pair(2),
            amp: pair(3),
            gain_reduction_a: pair(4),
            gain_reduction_b: pair(5),
        })
    }
}

impl From<&Meters> for crate::model::Meters {
    fn from(m: &Meters) -> Self {
        let extra = [
            ("post_input", m.post_input),
            ("output_signal", m.output),
            // the two input signals are what ANALOG 1 and ANALOG 2 bring to the mixer; there is no
            // meter for AES 1 and 2
            ("raw_input", m.input),
            ("gain_reduction_a", m.gain_reduction_a),
            ("gain_reduction_b", m.gain_reduction_b),
        ];
        crate::model::Meters {
            input: m.input.to_vec(),
            // the pair that follows the output gain and the signal on a live IPX 1200; the "output
            // signal" pair (indices 2-3) read -150 dB with the outputs unmuted and the amp playing
            output: m.amp.to_vec(),
            extra: extra.into_iter().map(|(k, v)| (k.to_string(), v.to_vec())).collect(),
        }
    }
}

pub fn parse_preset_names(data: &[u8]) -> Vec<String> {
    data.as_chunks::<16>().0.iter().map(|c| cstr(c)).collect()
}

pub fn parse_valid_presets(data: &[u8]) -> Vec<bool> {
    data.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) != 0).collect()
}
