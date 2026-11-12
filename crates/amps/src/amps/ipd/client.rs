//! Async client for the IPD protocol.
//!
//! The protocol is strict request/reply. One worker task owns the socket and keeps polling at about
//! 10 Hz (faster while requests are queued); every reply also carries the meters. Reads and writes
//! are queued in shared state and resolved when the matching `RSP` fragment arrives, which may be a
//! few replies later (the amp pipelines).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Notify, broadcast, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tracing::{debug, warn};

use super::protocol::*;
use crate::error::{Error, Result};

/// The app never GETs these keys directly: it asks for another key and the firmware answers with
/// both (data_model.GET_replace_list).
fn get_replace(key: Key) -> Key {
    if key.struct_id != S_GLOBAL {
        return key;
    }
    match key.member_id {
        M_PIN | M_MODEL_NUMBER | M_MODEL_FIRMWARE => Key::new(S_GLOBAL, M_ACCESS_RIGHTS, 0, 0),
        M_BUILD_NUMBER | M_MODEL_NAME => Key::new(S_GLOBAL, M_SW_VERSION, 0, 0),
        M_AMP_READINGS | M_AMP_READINGS_2 => Key::new(S_GLOBAL, M_CURRENT_USER, 0, 0),
        _ => key,
    }
}

const SET_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const SET_MAX_TRIES: u32 = 50;
/// Passwords and PINs are never read, not even to confirm a write of them.
fn is_secret(member: u8) -> bool {
    matches!(member, M_SET_PASSWORD | M_CHECK_PASSWORD | M_PIN | M_CHECK_PIN)
}

/// Between two sends of a write, its value is asked for this often.
/// A read that has no answer is asked again after this long.
const GET_RETRY_INTERVAL: Duration = Duration::from_millis(200);
const READ_BACK_INTERVAL: Duration = Duration::from_millis(150);
const POLL_IDLE: Duration = Duration::from_millis(100);
const POLL_BUSY: Duration = Duration::from_millis(20);
/// A NACK means "busy"; resending at once with the NACK's packet id works (waiting 0.5 s does too, slowly).
const NACK_RESEND_DELAY: Duration = Duration::from_millis(10);

#[derive(Clone, Debug)]
pub struct Options {
    /// `None`: take the generation from the probe reply.
    pub mk2: Option<bool>,
    /// Default timeout for reads and confirmed writes.
    pub timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options { mk2: None, timeout: Duration::from_secs(10) }
    }
}

/// Meter readings as they come on the wire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawMeters {
    pub vu_in: Vec<i8>,
    pub vu_out: Vec<i8>,
    pub gr_in: Vec<i8>,
    pub gr_out: Vec<i8>,
    pub raw_vu_in: Vec<i8>,
    pub output_voltage_db: Vec<f64>,
    pub output_current_db: Vec<f64>,
}

#[derive(Clone, Debug)]
pub enum ClientEvent {
    Changed(Key, Raw),
    Meters,
    Disconnected(Option<String>),
}

struct PendingSet {
    raw: Raw,
    tries: u32,
    last: Option<Instant>,
    max_tries: u32,
    /// Any echo confirms (commands: the firmware may reset the value to 0).
    any_echo: bool,
    /// When the value was last asked for, between two sends (the amp can apply a write and echo it late).
    read_back: Option<Instant>,
    done: Option<oneshot::Sender<bool>>,
}

#[derive(Default)]
struct State {
    values: HashMap<Key, Raw>,
    /// Sequence number of the last `RSP` per key.
    updated: HashMap<Key, u64>,
    seq: u64,
    sets: BTreeMap<Key, PendingSet>,
    gets: BTreeMap<Key, Option<Instant>>,
    meters: RawMeters,
    error: Option<String>,
}

struct Shared {
    host: String,
    port: u16,
    timeout: Duration,
    mk2: bool,
    state: Mutex<State>,
    version: watch::Sender<u64>,
    events: broadcast::Sender<ClientEvent>,
    stop: AtomicBool,
    stop_notify: Notify,
    alive: AtomicBool,
    nacks: AtomicU64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }
    fn check_alive(&self) -> Result<()> {
        if self.alive.load(Ordering::Acquire) {
            return Ok(());
        }
        let why = self.lock().error.clone().unwrap_or_else(|| "not connected".into());
        Err(Error::Disconnected(why))
    }
}

pub struct Client {
    shared: Arc<Shared>,
    announcement: Option<Announcement>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct Conn {
    stream: TcpStream,
    parser: ReplyParser,
}

async fn open(host: &str, port: u16, limit: Duration) -> Result<Conn> {
    let stream = timeout(limit, TcpStream::connect((host, port)))
        .await
        .map_err(|_| Error::Timeout(format!("connect to {host}:{port}")))??;
    stream.set_nodelay(true)?;
    Ok(Conn { stream, parser: ReplyParser::new() })
}

/// Ask a unit for its announcement (what the vendor app does to every address of the subnet).
pub async fn probe(host: &str, port: u16, limit: Duration) -> Option<Announcement> {
    let work = async {
        let mut c = open(host, port, limit).await.ok()?;
        c.stream.write_all(&build_probe()).await.ok()?;
        let mut data = Vec::new();
        let mut buf = [0u8; 1024];
        while data.len() < 42 {
            match c.stream.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => data.extend_from_slice(&buf[..n]),
            }
        }
        parse_probe_reply(&data)
    };
    timeout(limit, work).await.ok().flatten()
}

impl Client {
    pub async fn connect(host: &str, port: u16, opts: Options) -> Result<Client> {
        let announcement = probe(host, port, opts.timeout.min(Duration::from_secs(5))).await;
        let mk2 = opts.mk2.unwrap_or_else(|| announcement.as_ref().is_none_or(|a| a.mk2));
        let hw_id = announcement.as_ref().map_or(0, |a| a.hw_packet_id);
        let conn = open(host, port, opts.timeout).await?;
        let (events, _) = broadcast::channel(256);
        let shared = Arc::new(Shared {
            host: host.to_string(),
            port,
            timeout: opts.timeout,
            mk2,
            state: Mutex::new(State::default()),
            version: watch::channel(0).0,
            events,
            stop: AtomicBool::new(false),
            stop_notify: Notify::new(),
            alive: AtomicBool::new(true),
            nacks: AtomicU64::new(0),
        });
        let worker = tokio::spawn(run(shared.clone(), conn, hw_id));
        Ok(Client { shared, announcement, worker: Mutex::new(Some(worker)) })
    }

    pub fn announcement(&self) -> Option<&Announcement> {
        self.announcement.as_ref()
    }
    pub fn mk2(&self) -> bool {
        self.shared.mk2
    }
    pub fn nacks(&self) -> u64 {
        self.shared.nacks.load(Ordering::Relaxed)
    }
    pub fn connected(&self) -> bool {
        self.shared.alive.load(Ordering::Acquire)
    }
    pub fn meters(&self) -> RawMeters {
        self.shared.lock().meters.clone()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.shared.events.subscribe()
    }
    /// Last known value, without asking the amp.
    pub fn cached(&self, key: Key) -> Option<Raw> {
        self.shared.lock().values.get(&key).cloned()
    }

    /// Queue GETs without waiting.
    pub fn request(&self, keys: &[Key]) {
        let mut st = self.shared.lock();
        for &k in keys {
            st.gets.entry(get_replace(k)).or_insert(None);
        }
    }

    /// Ask for `key` and wait for a fresh value.
    pub async fn get_raw(&self, key: Key, limit: Option<Duration>) -> Result<Raw> {
        self.shared.check_alive()?;
        let mut rx = self.shared.version.subscribe();
        let asked = self.shared.lock().seq;
        self.request(&[key]);
        let deadline = Instant::now() + limit.unwrap_or(self.shared.timeout);
        loop {
            {
                let st = self.shared.lock();
                if st.updated.get(&key).is_some_and(|&u| u > asked)
                    && let Some(v) = st.values.get(&key)
                {
                    return Ok(v.clone());
                }
            }
            self.shared.check_alive()?;
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout(format!("no value for key {key}")));
            }
            // A key the amp answers together with another one may be skipped; ask again.
            if timeout(left.min(Duration::from_secs(1)), rx.changed()).await.is_err() {
                self.request(&[key]);
            }
        }
    }

    /// Ask for many keys and wait until all answered (or `limit`); missing ones come back as `None`.
    /// Keys the firmware only answers together with another key are not waited for.
    pub async fn get_many(&self, keys: &[Key], limit: Duration) -> Result<Vec<Option<Raw>>> {
        self.shared.check_alive()?;
        let mut rx = self.shared.version.subscribe();
        let asked = self.shared.lock().seq;
        self.request(keys);
        let deadline = Instant::now() + limit;
        loop {
            let done = {
                let st = self.shared.lock();
                keys.iter().all(|&k| get_replace(k) != k || st.updated.get(&k).is_some_and(|&u| u > asked))
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if done || left.is_zero() || !self.connected() {
                break;
            }
            if timeout(left.min(Duration::from_secs(1)), rx.changed()).await.is_err() {
                let st = self.shared.lock();
                let pending: Vec<Key> =
                    keys.iter().copied().filter(|k| !st.updated.get(k).is_some_and(|&u| u > asked)).collect();
                drop(st);
                self.request(&pending);
            }
        }
        // gone while it waited: nothing it holds is an answer
        self.shared.check_alive()?;
        // a key that did not answer after the request is missing, not whatever it held before; a key the
        // firmware only answers together with another one keeps what it has
        let st = self.shared.lock();
        Ok(keys
            .iter()
            .map(|k| {
                let fresh = get_replace(*k) != *k || st.updated.get(k).is_some_and(|&u| u > asked);
                st.values.get(k).filter(|_| fresh).cloned()
            })
            .collect())
    }

    /// Write and wait until the amp echoes the value (the worker resends every 0.5 s, up to 50 times).
    pub async fn set_raw(&self, key: Key, raw: Raw, limit: Option<Duration>) -> Result<()> {
        let rx = self.register(key, raw)?;
        self.confirm(key, rx, Instant::now() + limit.unwrap_or(self.shared.timeout)).await
    }

    /// Write many values at once: all are queued, the worker sends them as fast as the amp takes them.
    /// One result per item. `limit` defaults to the client timeout plus 0.3 s per item.
    pub async fn set_many_raw(&self, items: Vec<(Key, Raw)>, limit: Option<Duration>) -> Vec<Result<()>> {
        let limit = limit.unwrap_or(self.shared.timeout + Duration::from_millis(300) * items.len() as u32);
        let deadline = Instant::now() + limit;
        let waiters: Vec<_> = items.into_iter().map(|(key, raw)| (key, self.register(key, raw))).collect();
        let mut out = Vec::with_capacity(waiters.len());
        for (key, rx) in waiters {
            out.push(match rx {
                Ok(rx) => self.confirm(key, rx, deadline).await,
                Err(e) => Err(e),
            });
        }
        out
    }

    fn register(&self, key: Key, raw: Raw) -> Result<oneshot::Receiver<bool>> {
        self.shared.check_alive()?;
        // Compare against what the amp will echo: padded / truncated text, range-checked numbers.
        let raw = decode_raw(key.member_id, &encode_raw(key.member_id, &raw)?).unwrap_or(raw);
        let (tx, rx) = oneshot::channel();
        let ps = PendingSet {
            raw,
            tries: 0,
            last: None,
            max_tries: SET_MAX_TRIES,
            any_echo: false,
            read_back: None,
            done: Some(tx),
        };
        self.shared.lock().sets.insert(key, ps);
        Ok(rx)
    }

    async fn confirm(&self, key: Key, rx: oneshot::Receiver<bool>, deadline: Instant) -> Result<()> {
        let left = deadline.saturating_duration_since(Instant::now());
        if matches!(timeout(left, rx).await, Ok(Ok(true))) {
            return Ok(());
        }
        self.give_up(key);
        Err(Error::NotConfirmed(format!("SET key {key}")))
    }

    /// Drop the pending write of `key` whose waiter gave up. A newer write of the same key took its place
    /// when it was registered; that one still has a waiter and stays.
    fn give_up(&self, key: Key) {
        let mut st = self.shared.lock();
        if st.sets.get(&key).is_some_and(|ps| ps.done.as_ref().is_none_or(|done| done.is_closed())) {
            st.sets.remove(&key);
        }
    }

    /// SET Command[cmd] = 1. Any echo confirms it; at most 3 sends.
    pub async fn command(&self, cmd: u8) -> Result<()> {
        let key = Key::new(S_COMMAND, M_COMMAND, 0, cmd);
        self.submit(
            key,
            PendingSet {
                raw: Raw::Int(1),
                tries: 0,
                last: None,
                max_tries: 3,
                any_echo: true,
                read_back: None,
                done: None,
            },
            None,
        )
        .await
    }

    async fn submit(&self, key: Key, mut ps: PendingSet, limit: Option<Duration>) -> Result<()> {
        self.shared.check_alive()?;
        let (tx, rx) = oneshot::channel();
        ps.done = Some(tx);
        self.shared.lock().sets.insert(key, ps);
        let confirmed = matches!(timeout(limit.unwrap_or(self.shared.timeout), rx).await, Ok(Ok(true)));
        if !confirmed {
            self.give_up(key);
            return Err(Error::NotConfirmed(format!("SET key {key}")));
        }
        Ok(())
    }

    /// Send a password (8 characters). The echo is masked, so this does not wait for one; read the
    /// current user afterwards.
    pub async fn login(&self, password: &str) -> Result<()> {
        self.send_password(Key::new(S_GLOBAL, M_CHECK_PASSWORD, 0, 0), password).await
    }

    /// Set the password of user `level` (1 to 5). Like a login, it is sent without waiting for an echo.
    pub async fn set_password(&self, level: u8, password: &str) -> Result<()> {
        self.send_password(Key::new(S_GLOBAL, M_SET_PASSWORD, level, 0), password).await
    }

    async fn send_password(&self, key: Key, password: &str) -> Result<()> {
        self.shared.check_alive()?;
        let mut pw: String = password.chars().take(8).collect();
        while pw.len() < 8 {
            pw.push(' ');
        }
        let ps = PendingSet {
            raw: Raw::Text(pw),
            tries: 0,
            last: None,
            max_tries: SET_MAX_TRIES,
            any_echo: false,
            read_back: None,
            done: None,
        };
        self.shared.lock().sets.insert(key, ps);
        sleep(Duration::from_millis(500)).await;
        self.shared.lock().sets.remove(&key);
        Ok(())
    }

    pub async fn close(&self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.stop_notify.notify_waiters();
        let worker = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(w) = worker {
            let _ = timeout(Duration::from_secs(3), w).await;
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.stop_notify.notify_waiters();
    }
}

// ---- worker ----

type Items = Vec<(Key, Option<Raw>)>;

/// A write that mutes goes out before the others, whatever their keys: a MUTE must not wait behind the
/// writes queued before it, which an MK1 takes one per command (SEC-4, SEC-6). The order of the rest stays.
fn mute_first(keys: &mut [Key], sets: &BTreeMap<Key, PendingSet>) {
    let mutes = |key: &Key| key.struct_id == S_MUTE && sets.get(key).is_some_and(|ps| ps.raw == Raw::Int(1));
    keys.sort_by_key(|key| !mutes(key));
}

fn next_items(shared: &Shared) -> Items {
    let budget = fragment_budget(shared.mk2);
    let now = Instant::now();
    let (mut items, mut size) = (Items::new(), 4usize);
    let mut st = shared.lock();
    let mut keys: Vec<Key> = st.sets.keys().copied().collect();
    mute_first(&mut keys, &st.sets);
    for key in keys {
        let Some(ps) = st.sets.get_mut(&key) else { continue };
        if ps.tries >= ps.max_tries {
            warn!("SET aborted for key {key}");
            if let Some(tx) = ps.done.take() {
                let _ = tx.send(false);
            }
            st.sets.remove(&key);
            continue;
        }
        if let Some(last) = ps.last
            && now.duration_since(last) < SET_RETRY_INTERVAL
        {
            // between two sends: ask for the value, which confirms the write as soon as the amp has it
            let due = now.duration_since(last) >= READ_BACK_INTERVAL
                && ps.read_back.is_none_or(|t| now.duration_since(t) >= READ_BACK_INTERVAL);
            if due && !ps.any_echo && !is_secret(key.member_id) && size < budget {
                ps.read_back = Some(now);
                items.push((key, None));
                size += 7;
            }
            continue;
        }
        if size >= budget {
            break;
        }
        let n = 6 + encode_raw(key.member_id, &ps.raw).map_or(0, |v| v.len());
        ps.tries += 1;
        ps.last = Some(now);
        items.push((key, Some(ps.raw.clone())));
        size += n;
    }
    for (key, last) in st.gets.iter_mut() {
        if size >= budget {
            break;
        }
        if last.is_some_and(|t| now.duration_since(t) < GET_RETRY_INTERVAL) {
            continue;
        }
        *last = Some(now);
        items.push((*key, None));
        size += 7;
    }
    items
}

async fn run(shared: Arc<Shared>, mut conn: Conn, mut hw_id: u8) {
    let mut my_id: u8 = 1;
    let result: Result<()> = async {
        while !shared.stop.load(Ordering::Acquire) {
            let items = next_items(&shared);
            let busy = !items.is_empty();
            match exchange(&shared, &mut conn, &items, &mut hw_id, my_id).await? {
                Some(reply) => {
                    my_id = my_id % 254 + 1;
                    hw_id = reply.hw_id;
                    dispatch(&shared, reply);
                }
                None => break,
            }
            let pause = if busy || has_work(&shared) { POLL_BUSY } else { POLL_IDLE };
            tokio::select! {
                _ = sleep(pause) => {}
                _ = shared.stop_notify.notified() => {}
            }
        }
        Ok(())
    }
    .await;
    let _ = conn.stream.write_all(RESET_CONNECTION).await;
    let msg = result.err().map(|e| e.to_string());
    if let Some(m) = &msg {
        warn!("IPD worker stopped: {m}");
    }
    shared.lock().error = msg.clone();
    shared.alive.store(false, Ordering::Release);
    shared.bump();
    let _ = shared.events.send(ClientEvent::Disconnected(msg));
}

fn has_work(shared: &Shared) -> bool {
    let st = shared.lock();
    !st.sets.is_empty() || !st.gets.is_empty()
}

/// Send one command and wait for its data reply. Resend after 0.5 s, reconnect after 1 s (twice),
/// give up after 3 s.
async fn exchange(
    shared: &Shared,
    conn: &mut Conn,
    items: &Items,
    hw_id: &mut u8,
    my_id: u8,
) -> Result<Option<Reply>> {
    let start = Instant::now();
    let mut sent = start;
    let mut reconnects = 0u32;
    let mut cmd = build_command(items, shared.mk2, *hw_id, my_id)?;
    conn.stream.write_all(&cmd).await?;
    let mut buf = vec![0u8; 65536];
    while !shared.stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let waited = now.duration_since(start);
        if waited > Duration::from_secs(3) {
            return Err(Error::Timeout(format!("no reply from {}", shared.host)));
        }
        if waited > Duration::from_secs(1 + reconnects as u64) && reconnects < 2 {
            reconnects += 1;
            reopen(shared, conn, &cmd).await;
            sent = now;
        } else if now.duration_since(sent) > Duration::from_millis(500) {
            let _ = conn.stream.write_all(&cmd).await;
            sent = now;
        }
        let n = match timeout(Duration::from_millis(100), conn.stream.read(&mut buf)).await {
            Err(_) => continue,
            Ok(Ok(n)) if n > 0 => n,
            // closed or reset by the amp: open a new connection and keep trying until the 3 s are over
            Ok(_) => {
                if !reopen(shared, conn, &cmd).await {
                    sleep(Duration::from_millis(100)).await;
                }
                sent = Instant::now();
                continue;
            }
        };
        let mut nacked = false;
        for r in conn.parser.feed(&buf[..n]) {
            match r.kind {
                ReplyKind::Data if r.checksum_ok => return Ok(Some(r)),
                ReplyKind::Nack if r.hw_id != 0 => {
                    *hw_id = r.hw_id;
                    nacked = true;
                }
                _ => {}
            }
        }
        if nacked {
            shared.nacks.fetch_add(1, Ordering::Relaxed);
            sleep(NACK_RESEND_DELAY).await;
            cmd = build_command(items, shared.mk2, *hw_id, my_id)?;
            conn.stream.write_all(&cmd).await?;
            sent = Instant::now();
        }
    }
    Ok(None)
}

async fn reopen(shared: &Shared, conn: &mut Conn, cmd: &[u8]) -> bool {
    match open(&shared.host, shared.port, Duration::from_secs(2)).await {
        Ok(c) => {
            *conn = c;
            conn.stream.write_all(cmd).await.is_ok()
        }
        Err(e) => {
            debug!("reconnect to {} failed: {e}", shared.host);
            false
        }
    }
}

fn same_value(a: &Raw, b: &Raw) -> bool {
    match (a, b) {
        (Raw::Text(x), Raw::Text(y)) => x.trim_end() == y.trim_end(),
        _ => a == b,
    }
}

fn dispatch(shared: &Shared, reply: Reply) {
    let mut changed = Vec::new();
    let mut meters_changed = false;
    {
        let mut st = shared.lock();
        for (op, data) in &reply.fragments {
            match *op {
                OP_RSP => {
                    let Some((key, raw)) = decode_rsp(data) else { continue };
                    st.seq += 1;
                    let seq = st.seq;
                    st.updated.insert(key, seq);
                    st.values.insert(key, raw.clone());
                    let confirmed = st
                        .sets
                        .get(&key)
                        .is_some_and(|ps| ps.tries > 0 && (ps.any_echo || same_value(&ps.raw, &raw)));
                    if confirmed
                        && let Some(mut ps) = st.sets.remove(&key)
                        && let Some(tx) = ps.done.take()
                    {
                        let _ = tx.send(true);
                    }
                    st.gets.remove(&key);
                    changed.push((key, raw));
                }
                OP_VU_IN => (st.meters.vu_in, meters_changed) = (signed(data), true),
                OP_VU_OUT => (st.meters.vu_out, meters_changed) = (signed(data), true),
                OP_GR_IN => (st.meters.gr_in, meters_changed) = (signed(data), true),
                OP_GR_OUT => (st.meters.gr_out, meters_changed) = (signed(data), true),
                OP_RAW_VU_IN => (st.meters.raw_vu_in, meters_changed) = (signed(data), true),
                OP_VU_READING => {
                    st.meters.output_voltage_db =
                        data.iter().step_by(2).map(|&b| b as f64 / 4.0 - 3.75).collect();
                    st.meters.output_current_db =
                        data.iter().skip(1).step_by(2).map(|&b| b as f64 / 4.0 - 13.75).collect();
                    meters_changed = true;
                }
                OP_ERR => warn!("amp reported an error fragment {data:02x?}"),
                other => debug!("ignored fragment opcode {other}"),
            }
        }
    }
    shared.bump();
    for (k, v) in changed {
        let _ = shared.events.send(ClientEvent::Changed(k, v));
    }
    if meters_changed {
        let _ = shared.events.send(ClientEvent::Meters);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(raw: i64) -> PendingSet {
        PendingSet {
            raw: Raw::Int(raw),
            tries: 0,
            last: None,
            max_tries: 1,
            any_echo: false,
            read_back: None,
            done: None,
        }
    }

    // SEC-4, SEC-6: a mute is sent before gain, mixer, delay, filter, EQ and limiter writes; an unmute is not
    #[test]
    fn a_mute_is_sent_before_the_other_writes() {
        let mute = Key::new(S_MUTE, M_MUTE, 0x81, 0);
        let unmute = Key::new(S_MUTE, M_MUTE, 0x80, 0);
        let others = [
            Key::new(S_GAIN, M_GAIN, 0, 0),
            Key::new(S_MIXER, M_GAIN, 0, 1),
            Key::new(S_PEQ, M_Q, 0, 3),
            Key::new(S_LIMITER, M_THRESHOLD, 0x80, 0),
        ];
        let mut sets = BTreeMap::new();
        for key in others {
            sets.insert(key, pending(5));
        }
        sets.insert(mute, pending(1));
        sets.insert(unmute, pending(0));
        let mut keys: Vec<Key> = sets.keys().copied().collect();
        mute_first(&mut keys, &sets);
        assert_eq!(keys[0], mute);
        assert_eq!(keys.len(), 6);
        let rest: Vec<Key> = keys[1..].to_vec();
        let mut expected: Vec<Key> = sets.keys().copied().filter(|k| *k != mute).collect();
        expected.sort();
        assert_eq!(rest, expected, "the order of the others is kept");
    }
}
