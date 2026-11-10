//! IPD series (IPD 1200 / 2400 / 4800, "IntelliDrive"). See docs/ipd-protocol.md.

pub mod client;
pub mod params;
pub mod protocol;
pub mod sim;

use std::any::Any;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;

use self::client::{Client, ClientEvent, Options, RawMeters};
use self::params::{Slot, Ty, from_wire, merge_bit, slot, to_wire};
use self::protocol::*;
use crate::amp::{Amp, check_preset_name};
use crate::error::{Error, Result};
use crate::model::*;

pub use self::protocol::Announcement;

/// How long a connect waits for the model name after the version came.
const MODEL_NAME_WAIT: Duration = Duration::from_millis(1500);

/// The amp has 100 slots; the app uses the first 50 (BEH-6).
const PRESET_SLOTS: RangeInclusive<u32> = 1..=50;

/// Reads of the saved name after the save command, and the wait between them.
const STORE_CHECKS: u32 = 10;
const STORE_CHECK_EVERY: Duration = Duration::from_millis(500);

/// CURRENT_USER value of the Developer level; higher values have fewer rights.
const USER_DEVELOPER: i64 = 2;

const fn global(member: u8) -> Key {
    Key::new(S_GLOBAL, member, 0, 0)
}

pub struct IpdAmp {
    client: Arc<Client>,
    info: AmpInfo,
    /// Model name; `info.model` is only what was known at connect, the amp may report it later.
    model: Arc<Mutex<String>>,
    events: broadcast::Sender<AmpEvent>,
    forwarder: JoinHandle<()>,
}

impl IpdAmp {
    pub async fn connect(host: &str) -> Result<IpdAmp> {
        Self::connect_with(host, TCP_PORT, Options::default()).await
    }

    pub async fn connect_with(host: &str, port: u16, opts: Options) -> Result<IpdAmp> {
        let client = Arc::new(Client::connect(host, port, opts).await?);
        // MODEL_NAME arrives together with SW_VERSION; firmware 1.2.6 has no MODEL_NUMBER. Asking for the name
        // itself waits for a reply that may not carry it (8 s on a real IPD 2400), so the version is asked for
        // and the name is taken if it came with it; the forwarder takes it when it comes later.
        let _ = client.get_raw(global(M_SW_VERSION), Some(Duration::from_secs(8))).await;
        // the name can follow in the next reply (an MK1 sends one fragment per reply): a moment is allowed for it
        let named = |client: &Client| match client.cached(global(M_MODEL_NAME)) {
            Some(Raw::Text(t)) if !t.trim().is_empty() => Some(t.trim().to_string()),
            _ => None,
        };
        let mut waited = Duration::ZERO;
        while named(&client).is_none() && waited < MODEL_NAME_WAIT {
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited += Duration::from_millis(50);
        }
        let model = named(&client).unwrap_or_else(|| "IPD".to_string());
        let (events, _) = broadcast::channel(256);
        let model = Arc::new(Mutex::new(model));
        let forwarder =
            tokio::spawn(forward(client.clone(), events.clone(), model.clone(), params::reverse_map()));
        let info = AmpInfo {
            driver: "ipd",
            host: host.to_string(),
            model: model.lock().map(|m| m.clone()).unwrap_or_default(),
        };
        Ok(IpdAmp { client, info, model, events, forwarder })
    }

    /// The protocol client, for raw `Key` access.
    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn user(&self) -> Result<String> {
        let raw = self.client.get_raw(global(M_CURRENT_USER), None).await?;
        Ok(user_name(raw.int().unwrap_or(-1)))
    }

    pub async fn access_rights(&self) -> Result<AccessRights> {
        let raw = self.client.get_raw(global(M_ACCESS_RIGHTS), None).await?;
        Ok(AccessRights::from_raw(raw.int().unwrap_or(0) as u32))
    }

    pub async fn standby(&self, on: bool) -> Result<()> {
        self.client.command(if on { CMD_GOTO_STANDBY } else { CMD_EXIT_STANDBY }).await
    }

    pub async fn locate(&self) -> Result<()> {
        self.client.command(CMD_LOCATE).await
    }

    /// The model name, as far as the amp has told us ("IPD" until then).
    pub fn model(&self) -> String {
        self.model.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn slot_of(&self, p: &Param) -> Result<Slot> {
        slot(p).ok_or_else(|| Error::Unsupported(format!("{p} on the IPD")))
    }

    /// What to write for `raw`: for one bit of a number the amp holds, that number with the bit changed,
    /// read from the amp just now; any other setting is written as it is.
    async fn merged(&self, slot: &Slot, raw: Raw) -> Result<Raw> {
        if !matches!(slot.ty, Ty::Bit(..)) {
            return Ok(raw);
        }
        let current = self.client.get_raw(slot.key, None).await?;
        let current =
            current.int().ok_or_else(|| Error::Protocol(format!("key {} is not a number", slot.key)))?;
        Ok(merge_bit(slot, current, &raw))
    }
}

fn user_name(level: i64) -> String {
    USERS.get(level as usize).map_or_else(|| format!("unknown({level})"), |s| s.to_string())
}

fn common_meters(m: &RawMeters) -> Meters {
    let f = |v: &[i8]| v.iter().map(|&x| x as f64).collect::<Vec<_>>();
    let mut out = Meters { input: f(&m.vu_in), output: f(&m.vu_out), extra: Default::default() };
    for (name, v) in [
        ("gain_reduction_in", f(&m.gr_in)),
        ("gain_reduction_out", f(&m.gr_out)),
        ("raw_input", f(&m.raw_vu_in)),
    ] {
        if !v.is_empty() {
            out.extra.insert(name.into(), v);
        }
    }
    if !m.output_voltage_db.is_empty() {
        out.extra.insert("output_voltage".into(), m.output_voltage_db.clone());
        out.extra.insert("output_current".into(), m.output_current_db.clone());
    }
    out
}

/// Translate client events (wire keys) to amp events (parameters).
async fn forward(
    client: Arc<Client>,
    tx: broadcast::Sender<AmpEvent>,
    model: Arc<Mutex<String>>,
    reverse: HashMap<Key, Param>,
) {
    let mut rx = client.subscribe();
    loop {
        let ev = match rx.recv().await {
            Ok(ev) => ev,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let out = match ev {
            ClientEvent::Changed(key, Raw::Text(name)) if key == global(M_MODEL_NAME) => {
                if let (false, Ok(mut m)) = (name.trim().is_empty(), model.lock()) {
                    *m = name.trim().to_string();
                }
                None
            }
            ClientEvent::Changed(key, raw) => match reverse.get(&key) {
                Some(p) => slot(p).map(|s| AmpEvent::Changed(p.clone(), from_wire(&s, &raw))),
                None => None,
            },
            ClientEvent::Meters => Some(AmpEvent::Meters(common_meters(&client.meters()))),
            ClientEvent::Disconnected(why) => Some(AmpEvent::Disconnected(why)),
        };
        if let Some(out) = out {
            let _ = tx.send(out);
        }
    }
}

#[async_trait]
impl Amp for IpdAmp {
    fn info(&self) -> &AmpInfo {
        &self.info
    }

    fn params(&self) -> Vec<Param> {
        params::all_params()
    }

    fn spec(&self, p: &Param) -> Option<ParamSpec> {
        params::spec(p, &self.model())
    }

    async fn get(&self, p: &Param) -> Result<Value> {
        let s = self.slot_of(p)?;
        Ok(from_wire(&s, &self.client.get_raw(s.key, None).await?))
    }

    async fn get_many(&self, ps: &[Param]) -> Result<Vec<Value>> {
        let slots = ps.iter().map(|p| self.slot_of(p)).collect::<Result<Vec<_>>>()?;
        let keys: Vec<Key> = slots.iter().map(|s| s.key).collect();
        let vals = self.client.get_many(&keys, Duration::from_secs(60)).await?;
        slots
            .iter()
            .zip(vals)
            .zip(ps)
            .map(|((s, v), p)| {
                Ok(from_wire(s, &v.ok_or_else(|| Error::Timeout(format!("no value for {p}")))?))
            })
            .collect()
    }

    async fn set(&self, p: &Param, value: Value) -> Result<()> {
        let spec = self.spec(p).ok_or_else(|| Error::Unsupported(format!("{p} on the IPD")))?;
        if !spec.writable {
            return Err(Error::Unsupported(format!("{p} is read-only")));
        }
        let value = spec.check(p, &value)?;
        let s = self.slot_of(p)?;
        let raw = self.merged(&s, to_wire(&s, &value)?).await?;
        self.client.set_raw(s.key, raw, None).await
    }

    async fn set_many(&self, items: &[(Param, Value)]) -> Vec<Result<()>> {
        let mut out: Vec<Option<Result<()>>> = items.iter().map(|_| None).collect();
        let mut batch = Vec::new();
        for (i, (p, v)) in items.iter().enumerate() {
            let prepared = async {
                let spec = self
                    .spec(p)
                    .filter(|s| s.writable)
                    .ok_or_else(|| Error::Unsupported(format!("{p} is not writable on the IPD")))?;
                let s = self.slot_of(p)?;
                let raw = self.merged(&s, to_wire(&s, &spec.check(p, v)?)?).await?;
                Ok::<_, Error>((s.key, raw))
            }
            .await;
            match prepared {
                Ok(item) => batch.push((i, item)),
                Err(e) => out[i] = Some(Err(e)),
            }
        }
        let results = self.client.set_many_raw(batch.iter().map(|(_, it)| it.clone()).collect(), None).await;
        for ((i, _), r) in batch.iter().zip(results) {
            out[*i] = Some(r);
        }
        out.into_iter().map(|r| r.unwrap_or(Ok(()))).collect()
    }

    async fn access(&self) -> Result<Option<Access>> {
        let keys = [global(M_CURRENT_USER), global(M_ACCESS_RIGHTS)];
        let vals = self.client.get_many(&keys, Duration::from_secs(15)).await?;
        let get = |i: usize| {
            vals[i]
                .as_ref()
                .and_then(Raw::int)
                .ok_or_else(|| Error::Timeout(format!("no value for key {}", keys[i])))
        };
        let (level, rights) = (get(0)?, get(1)?);
        Ok(Some(Access {
            user: user_name(level),
            locked: level == USER_LOCKED,
            rights: AccessRights::from_raw(rights as u32),
        }))
    }

    /// Send a password (8 characters; shorter ones are padded) and read the access afterwards.
    async fn login(&self, password: &str) -> Result<Access> {
        self.client.login(password).await?;
        self.access().await?.ok_or_else(|| Error::Protocol("no access after the login".into()))
    }

    async fn status(&self) -> Result<Status> {
        // CURRENT_USER also brings AMP_READINGS, ACCESS_RIGHTS the model number, SW_VERSION the model name.
        let pn = Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0);
        let pname = Key::new(S_PRESET, M_SHORT_NAME, 0, 0);
        let keys = [
            global(M_CURRENT_USER),
            global(M_ACCESS_RIGHTS),
            global(M_HARDWARE_STATUS),
            global(M_SW_VERSION),
            global(M_SHORT_NAME),
            pn,
            pname,
        ];
        let limit = Duration::from_secs(15);
        let vals = self.client.get_many(&keys, limit).await?;
        // The amp sends the readings along with other answers, some firmware only now and then.
        if self.client.cached(global(M_AMP_READINGS)).is_none() {
            let _ = self.client.get_raw(global(M_AMP_READINGS), Some(Duration::from_millis(1500))).await;
        }
        let get =
            |i: usize| vals[i].clone().ok_or_else(|| Error::Timeout(format!("no value for key {}", keys[i])));
        let user = user_name(get(0)?.int().unwrap_or(-1));
        let hw = get(2)?.int().unwrap_or(0) as u32;
        let model = match self.client.cached(global(M_MODEL_NAME)) {
            Some(Raw::Text(t)) if !t.trim().is_empty() => t.trim().to_string(),
            _ => self.model(),
        };
        let mut st = Status {
            driver: "ipd".into(),
            model,
            name: get(4)?.text().unwrap_or("").trim().to_string(),
            firmware: decode_version(get(3)?.int().unwrap_or(0) as u32),
            preset: Some(PresetInfo {
                index: get(5)?.int().unwrap_or(0) as u32,
                name: get(6)?.text().unwrap_or("").trim().to_string(),
            }),
            ..Default::default()
        };
        st.extra.insert("user".into(), user);
        st.extra.insert("faults".into(), decode_status(hw).join(", "));
        st.extra.insert("standby".into(), (hw & 4 != 0).to_string());
        st.extra.insert("generation".into(), if self.client.mk2() { "MK2" } else { "MK1" }.into());
        if let Some(Raw::Int(r)) = self.client.cached(global(M_AMP_READINGS)) {
            let (z, temp) = decode_amp_readings(r as u32);
            st.temperatures.push(temp as i32);
            st.extra.insert("load_impedance_ohm".into(), format!("{z:?}"));
        }
        Ok(st)
    }

    fn preset_slots(&self) -> RangeInclusive<u32> {
        PRESET_SLOTS
    }

    async fn presets(&self) -> Result<Vec<PresetInfo>> {
        let keys: Vec<Key> = PRESET_SLOTS.map(|n| Key::new(S_PRESET, M_SHORT_NAME, n as u8, 0)).collect();
        let vals = self.client.get_many(&keys, Duration::from_secs(60)).await?;
        Ok(keys
            .iter()
            .zip(vals)
            .filter_map(|(k, v)| {
                let name = v?.text()?.trim().to_string();
                (!name.is_empty() && !name.eq_ignore_ascii_case("empty"))
                    .then_some(PresetInfo { index: k.channel as u32, name })
            })
            .collect())
    }

    async fn recall_preset(&self, index: u32) -> Result<()> {
        if !PRESET_SLOTS.contains(&index) {
            return Err(Error::InvalidValue(format!("preset {index} (valid: 1..=50)")));
        }
        self.client.set_raw(Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0), Raw::Int(index as i64), None).await?;
        self.client.command(CMD_LOAD_PRESET).await
    }

    fn check_preset_name(&self, name: &str) -> Result<String> {
        check_preset_name(name, 16)
    }

    async fn store_preset(&self, slot: u32, name: &str) -> Result<()> {
        if !PRESET_SLOTS.contains(&slot) {
            return Err(Error::InvalidValue(format!("preset {slot} (valid: 1..=50)")));
        }
        let name = self.check_preset_name(name)?;
        if !self.access_rights().await?.save_preset {
            return Err(Error::Denied("the current user level may not save presets".into()));
        }
        // As the vendor app: slots 1..=range are read-only below Developer level. Without the
        // setting (it is not always answered) assume no protected range.
        let startup = self.client.get_raw(global(M_STARTUP_PRESET), None).await.ok().and_then(|r| r.int());
        let read_only = (startup.unwrap_or(0) as u32 & 0xFF00) >> 8;
        // an unreadable level counts as the lowest
        let level = self.client.get_raw(global(M_CURRENT_USER), None).await?.int().unwrap_or(i64::MAX);
        if slot <= read_only && level > USER_DEVELOPER {
            return Err(Error::Denied(format!("presets 1 to {read_only} are protected")));
        }
        // The amp saves the current settings to the current preset number, under the current name.
        self.client.set_raw(Key::new(S_PRESET, M_PRESET_NUMBER, 0, 0), Raw::Int(slot as i64), None).await?;
        self.client.set_raw(Key::new(S_PRESET, M_SHORT_NAME, 0, 0), Raw::Text(name.clone()), None).await?;
        // The echo confirms the command. The stored settings cannot be read back, so when the slot
        // already had this name, the name check below adds nothing to it.
        self.client.command(CMD_SAVE_PRESET).await?;
        let key = Key::new(S_PRESET, M_SHORT_NAME, slot as u8, 0);
        for _ in 0..STORE_CHECKS {
            if let Ok(Raw::Text(t)) = self.client.get_raw(key, None).await
                && t.trim() == name
            {
                return Ok(());
            }
            tokio::time::sleep(STORE_CHECK_EVERY).await;
        }
        Err(Error::NotConfirmed(format!("preset {slot}: the amp did not store {name:?}")))
    }

    fn meters(&self) -> Meters {
        common_meters(&self.client.meters())
    }

    fn subscribe(&self) -> broadcast::Receiver<AmpEvent> {
        self.events.subscribe()
    }

    async fn close(&self) {
        self.client.close().await;
        self.forwarder.abort();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn snapshot(&self) -> Result<Snapshot> {
        let params = self.params();
        let slots: Vec<Slot> = params.iter().filter_map(slot).collect();
        let keys: Vec<Key> = slots.iter().map(|s| s.key).collect();
        let mut vals = self.client.get_many(&keys, Duration::from_secs(60)).await?;
        let missing: Vec<Key> =
            keys.iter().zip(&vals).filter(|(_, v)| v.is_none()).map(|(k, _)| *k).collect();
        if !missing.is_empty() {
            let again = self.client.get_many(&missing, Duration::from_secs(30)).await?;
            let mut again = again.into_iter();
            for (i, v) in vals.iter_mut().enumerate() {
                if v.is_none() {
                    *v = again.next().flatten().or_else(|| self.client.cached(keys[i]));
                }
            }
        }
        let mut snap = Snapshot::new("ipd", &self.model());
        for ((p, s), v) in params.iter().zip(&slots).zip(vals) {
            let raw = v.ok_or_else(|| Error::Timeout(format!("no value for {p}")))?;
            snap.params.insert(p.to_string(), from_wire(s, &raw));
        }
        Ok(snap)
    }
}

impl Drop for IpdAmp {
    fn drop(&mut self) {
        self.forwarder.abort();
    }
}

// ---- discovery ----

pub async fn probe(host: &str, limit: Duration) -> Option<Announcement> {
    client::probe(host, TCP_PORT, limit).await
}

/// Probe `x.y.z.1-254` for IPD units; `subnet` like `192.168.1`. Over Wi-Fi the amp needs a few seconds.
pub async fn scan(subnet: &str, limit: Duration) -> Vec<(String, Announcement)> {
    let mut set = JoinSet::new();
    for i in 1..=254u32 {
        let ip = format!("{subnet}.{i}");
        set.spawn(async move { probe(&ip, limit).await.map(|a| (ip, a)) });
    }
    let mut found = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(Some(hit)) = r {
            found.push(hit);
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Passive discovery: amps broadcast 42-byte `DDB` packets to UDP 50674.
pub async fn listen(limit: Duration) -> Result<Vec<(String, Announcement)>> {
    let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    sock.set_reuse_address(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::from(([0, 0, 0, 0], ANNOUNCE_PORT)).into())?;
    let sock = UdpSocket::from_std(sock.into())?;
    let mut found: HashMap<String, Announcement> = HashMap::new();
    let mut buf = [0u8; 1024];
    let _ = timeout(limit, async {
        while let Ok((n, from)) = sock.recv_from(&mut buf).await {
            if let Some(a) = parse_announcement(&buf[..n]) {
                found.insert(from.ip().to_string(), a);
            }
        }
    })
    .await;
    let mut v: Vec<_> = found.into_iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(v)
}
