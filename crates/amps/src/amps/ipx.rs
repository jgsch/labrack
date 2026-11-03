//! Driver for the IPX series (IPX 1200 / 2400 / 4800): binary protocol on TCP 50168.
//!
//! * [`protocol`]: frame format and parser (no I/O)
//! * [`structs`]: decoders for the bulk structures (DEV_PRM_SYNC, presets, meters)
//! * [`client`]: tokio client with request/response matching, heartbeat and push handling
//! * [`params`]: mapping from the common [`Param`] names to wire addresses and value codecs
//!
//! The IPX does not acknowledge writes. [`Amp::set`] therefore reads the value back and reports
//! [`Error::NotConfirmed`] when it differs; [`IpxAmp::set_unverified`] skips that round trip.

pub mod client;
pub mod params;
pub mod protocol;
pub mod structs;

use std::any::Any;
use std::ops::RangeInclusive;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::time::{Instant, timeout_at};
use tracing::{debug, warn};

use self::client::{Client, ClientConfig, ClientEvent};
use self::protocol::{Address, Ctrl, DISCOVERY_PORT, Env, Frame, FrameParser, Main, TCP_PORT};
use self::structs::{
    DeviceState, StoredPreset, SystemAttributes, cstr, parse_preset_names, parse_valid_presets,
};
use crate::amp::Amp;
use crate::error::{Error, Result};
use crate::model::*;

/// Reads of a written value before giving up (each waits the client timeout).
const READ_BACK_TRIES: u32 = 3;

/// The amp counts presets 0-49 on the wire (slot 50 gets no reply); like the vendor app, the API shows
/// index + 1 (BEH-6).
const PRESET_SLOTS: RangeInclusive<u32> = 1..=50;

/// Wire index of a preset slot.
fn wire_index(slot: u32) -> Result<u32> {
    if !PRESET_SLOTS.contains(&slot) {
        return Err(Error::InvalidValue(format!("preset {slot} (valid: 1..=50)")));
    }
    Ok(slot - 1)
}

pub struct IpxAmp {
    info: AmpInfo,
    serial: String,
    client: Client,
    events: broadcast::Sender<AmpEvent>,
}

impl IpxAmp {
    /// Connect on the default port with default timeouts.
    pub async fn connect(host: &str) -> Result<IpxAmp> {
        Self::connect_with_config(host, &ClientConfig::default()).await
    }

    /// `timeout` is the per-request reply timeout (bulk reads wait at least 5 s).
    pub async fn connect_with(host: &str, port: u16, timeout: Duration) -> Result<IpxAmp> {
        let config = ClientConfig {
            port,
            timeout,
            connect_timeout: timeout.max(Duration::from_secs(5)),
            ..ClientConfig::default()
        };
        Self::connect_with_config(host, &config).await
    }

    pub async fn connect_with_config(host: &str, config: &ClientConfig) -> Result<IpxAmp> {
        let client = Client::connect(host, config).await?;
        let (events, _) = broadcast::channel(256);
        tokio::spawn(forward(client.subscribe(), events.clone()));
        let attrs = match client.get_raw(Address::block(Env::SYSATTR)).await {
            Ok(raw) => SystemAttributes::parse(&raw),
            Err(e) => {
                client.close().await;
                return Err(e);
            }
        };
        let model = match attrs.model.trim() {
            "" => "IPX".to_string(),
            m => m.to_string(),
        };
        debug!(%host, %model, "connected to ipx");
        Ok(IpxAmp {
            info: AmpInfo { driver: "ipx", host: host.to_string(), model },
            serial: attrs.serial.trim().to_string(),
            client,
            events,
        })
    }

    /// The underlying protocol client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn get_raw(&self, addr: Address) -> Result<Vec<u8>> {
        self.client.get_raw(addr).await
    }

    pub async fn set_raw(&self, addr: Address, payload: &[u8]) -> Result<()> {
        self.client.set_raw(addr, payload).await
    }

    pub async fn ctrl(&self, cmd: u16, payload: &[u8], timeout: Option<Duration>) -> Result<Frame> {
        self.client.ctrl(cmd, payload, timeout).await
    }

    /// The complete device state.
    pub async fn sync(&self) -> Result<DeviceState> {
        self.client.sync().await
    }

    pub async fn temperatures(&self) -> Result<Vec<i32>> {
        let raw = self.client.get_raw(Address::block(Env::ALL_TEMPERATURES)).await?;
        Ok(raw.as_chunks::<4>().0.iter().map(|c| i32::from_le_bytes(*c)).collect())
    }

    pub async fn device_name(&self) -> Result<String> {
        Ok(cstr(&self.client.get_raw(Address::block(Env::DEVICE_NAME)).await?))
    }

    /// A stored preset (settings, not just the name); `slot` is 1-50.
    pub async fn stored_preset(&self, slot: u32) -> Result<StoredPreset> {
        let index = wire_index(slot)?;
        let reply = self.client.ctrl(Ctrl::GETPRESET, &index.to_le_bytes(), None).await?;
        StoredPreset::parse(&reply.payload)
    }

    /// Write a parameter without reading it back.
    pub async fn set_unverified(&self, param: &Param, value: Value) -> Result<()> {
        self.encode(param, value).await.map(|_| ())
    }

    /// Check, encode and send. Returns the value the amp should hold afterwards.
    async fn encode(&self, param: &Param, value: Value) -> Result<Value> {
        let slot = params::slot(param).ok_or_else(|| unsupported(param))?;
        if !slot.writable {
            return Err(Error::Unsupported(format!("{param} is read-only")));
        }
        let value = slot.spec().check(param, &value)?;
        let bytes = slot.encode(&value)?;
        self.client.set_raw(slot.addr, &bytes).await?;
        slot.decode(&bytes)
    }
}

fn unsupported(param: &Param) -> Error {
    Error::Unsupported(format!("{param} is not available on the IPX"))
}

fn changed(frame: &Frame) -> Option<AmpEvent> {
    let param = params::param_for(frame.address())?;
    let value = params::slot(param)?.decode(&frame.payload).ok()?;
    Some(AmpEvent::Changed(param.clone(), value))
}

async fn forward(mut rx: broadcast::Receiver<ClientEvent>, tx: broadcast::Sender<AmpEvent>) {
    loop {
        let event = match rx.recv().await {
            Ok(ClientEvent::Meters(m)) => Some(AmpEvent::Meters((&m).into())),
            Ok(ClientEvent::Frame(f)) if f.main == Main::GETPRM || f.main == Main::DEVPRMCHAGE => changed(&f),
            Ok(ClientEvent::Frame(_)) => None,
            Ok(ClientEvent::Disconnected(reason)) => {
                let _ = tx.send(AmpEvent::Disconnected(reason));
                return;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                debug!("ipx event stream lagged by {n}");
                None
            }
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if let Some(event) = event {
            let _ = tx.send(event);
        }
    }
}

#[async_trait]
impl Amp for IpxAmp {
    fn info(&self) -> &AmpInfo {
        &self.info
    }

    fn params(&self) -> Vec<Param> {
        params::all().to_vec()
    }

    fn spec(&self, param: &Param) -> Option<ParamSpec> {
        params::spec(param)
    }

    async fn get(&self, param: &Param) -> Result<Value> {
        let slot = params::slot(param).ok_or_else(|| unsupported(param))?;
        slot.decode_single(&self.client.get_raw(slot.addr).await?)
    }

    async fn set(&self, param: &Param, value: Value) -> Result<()> {
        let expected = self.encode(param, value).await?;
        // The amp occasionally drops a reply (seen over Wi-Fi); the write itself has no reply to lose.
        let mut tries = 0;
        let got = loop {
            tries += 1;
            match self.get(param).await {
                Err(Error::Timeout(_)) if tries < READ_BACK_TRIES => continue,
                other => break other?,
            }
        };
        if got != expected {
            return Err(Error::NotConfirmed(format!("{param}: wrote {expected}, amp reports {got}")));
        }
        Ok(())
    }

    async fn status(&self) -> Result<Status> {
        let (name, state) = tokio::try_join!(self.device_name(), self.sync())?;
        let temperatures = self.temperatures().await.unwrap_or_else(|e| {
            warn!("cannot read temperatures: {e}");
            Vec::new()
        });
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("serial".to_string(), self.serial.clone());
        extra.insert("device_id".to_string(), state.device_id.clone());
        extra.insert("dsp".to_string(), state.versions.dsp.clone());
        extra.insert("build".to_string(), state.versions.build.clone());
        extra.insert("ip".to_string(), state.network.ip.clone());
        extra.insert("port".to_string(), state.network.port.to_string());
        extra.insert("user_mode".to_string(), state.current_user_mode.to_string());
        Ok(Status {
            driver: self.info.driver.to_string(),
            model: self.info.model.clone(),
            name,
            firmware: state.versions.mcu.clone(),
            preset: Some(PresetInfo { index: state.preset_index + 1, name: state.preset_name.clone() }),
            temperatures,
            extra,
        })
    }

    fn preset_slots(&self) -> RangeInclusive<u32> {
        PRESET_SLOTS
    }

    async fn presets(&self) -> Result<Vec<PresetInfo>> {
        let (names, valid) = tokio::try_join!(
            self.client.ctrl(Ctrl::GETALLPRESETNAME, &[], None),
            self.client.ctrl(Ctrl::GETVALIDPRESETS, &[], None)
        )?;
        let names = parse_preset_names(&names.payload);
        let valid = parse_valid_presets(&valid.payload);
        Ok(names
            .into_iter()
            .zip(valid)
            .enumerate()
            .filter(|(i, (_, ok))| *ok && PRESET_SLOTS.contains(&(*i as u32 + 1)))
            .map(|(i, (name, _))| PresetInfo { index: i as u32 + 1, name })
            .collect())
    }

    async fn recall_preset(&self, slot: u32) -> Result<()> {
        let index = wire_index(slot)?;
        self.client.send_ctrl(Ctrl::RECALLPRESET, &index.to_le_bytes()).await
    }

    fn meters(&self) -> Meters {
        self.client.meters().map(|m| (&m).into()).unwrap_or_default()
    }

    fn subscribe(&self) -> broadcast::Receiver<AmpEvent> {
        self.events.subscribe()
    }

    async fn close(&self) {
        self.client.close().await;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        let state = self.sync().await?;
        let mut snap = Snapshot::new(self.info.driver, &self.info.model);
        for param in params::all() {
            let Some(slot) = params::slot(param) else { continue };
            let bytes = state
                .audio_bytes(slot.addr, slot.wire_len())
                .ok_or_else(|| Error::Protocol(format!("{param} is outside the audio config")))?;
            snap.params.insert(param.to_string(), slot.decode(bytes)?);
        }
        Ok(snap)
    }
}

/// An amp seen on the network via its DEVBROADCAST packets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Discovered {
    /// Address the packet came from.
    pub source: String,
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub model: u16,
    pub flag: i32,
}

impl Discovered {
    /// Decode a DEVBROADCAST payload; `source` is the sender's address.
    pub fn from_payload(source: String, payload: &[u8]) -> Option<Self> {
        if payload.len() < 0x48 {
            return None;
        }
        Some(Discovered {
            source,
            id: cstr(&payload[0..16]),
            name: cstr(&payload[16..48]),
            ip: cstr(&payload[48..64]),
            port: u16::from_le_bytes([payload[0x40], payload[0x41]]),
            model: u16::from_le_bytes([payload[0x42], payload[0x43]]),
            flag: i32::from_le_bytes([payload[0x44], payload[0x45], payload[0x46], payload[0x47]]),
        })
    }
}

/// Listen for DEVBROADCAST packets (UDP 52350) for `timeout`. The amp does not broadcast on every network.
pub async fn discover(timeout: Duration) -> Result<Vec<Discovered>> {
    let socket = {
        use socket2::{Domain, Protocol, Socket, Type};
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        s.set_reuse_address(true)?;
        s.set_nonblocking(true)?;
        s.bind(&std::net::SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT)).into())?;
        UdpSocket::from_std(s.into())?
    };
    let end = Instant::now() + timeout;
    let mut found: Vec<Discovered> = Vec::new();
    let mut buf = vec![0u8; 65535];
    while let Ok(received) = timeout_at(end, socket.recv_from(&mut buf)).await {
        let (n, from) = received?;
        let source = from.ip().to_string();
        for frame in FrameParser::new().feed(&buf[..n]) {
            if frame.main != Main::DEVBROADCAST {
                continue;
            }
            let Some(d) = Discovered::from_payload(source.clone(), &frame.payload) else { continue };
            match found.iter_mut().find(|f| f.source == d.source) {
                Some(existing) => *existing = d,
                None => found.push(d),
            }
        }
    }
    Ok(found)
}

/// Default TCP port, re-exported for callers that build their own [`ClientConfig`].
pub const DEFAULT_PORT: u16 = TCP_PORT;
