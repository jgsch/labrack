//! Tokio client for the IPX control protocol.
//!
//! One TCP connection, a reader task that parses frames and routes replies to waiting requests, and
//! a heartbeat task. Replies are matched by `(main, sub-header)`; the amp has no request ids.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Mutex, broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval_at, timeout};
use tracing::{debug, trace, warn};

use super::protocol::{Address, Ctrl, Env, Frame, FrameParser, Main, build_frame, ctrl_sub};
use super::structs::{DeviceState, Meters};
use crate::error::{Error, Result};

const NO_TEMPERATURE: i64 = i64::MIN;
const LONG_REPLY: Duration = Duration::from_secs(5);
const FIRST_READ: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub port: u16,
    pub connect_timeout: Duration,
    /// Default time to wait for a reply (DEV_PRM_SYNC and GETPRESET wait at least 5 s).
    pub timeout: Duration,
    /// `None` disables the heartbeat.
    pub heartbeat: Option<Duration>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            port: super::protocol::TCP_PORT,
            connect_timeout: Duration::from_secs(5),
            timeout: Duration::from_secs(2),
            heartbeat: Some(Duration::from_secs(2)),
        }
    }
}

/// Things the amp sends on its own.
#[derive(Clone, Debug)]
pub enum ClientEvent {
    /// A GAINSLEVEL push (about 10 Hz).
    Meters(Meters),
    /// A GETPRM or DEVPRMCHAGE frame nobody asked for.
    Frame(Frame),
    /// The connection ended; `None` after a local [`Client::close`].
    Disconnected(Option<String>),
}

type Key = (u16, [u8; 4]);

#[derive(Default)]
struct Waiters {
    closed: Option<String>,
    queues: HashMap<Key, VecDeque<oneshot::Sender<Result<Frame>>>>,
}

struct Inner {
    writer: Mutex<Option<OwnedWriteHalf>>,
    waiters: StdMutex<Waiters>,
    events: broadcast::Sender<ClientEvent>,
    meters: StdMutex<Option<Meters>>,
    temperature: AtomicI64,
    timeout: Duration,
    /// When bytes last came from the amp: the meters alone arrive about 10 times a second.
    last_rx: StdMutex<Instant>,
}

impl Inner {
    fn waiters(&self) -> std::sync::MutexGuard<'_, Waiters> {
        self.waiters.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn register(&self, key: Key) -> Result<oneshot::Receiver<Result<Frame>>> {
        let (tx, rx) = oneshot::channel();
        let mut w = self.waiters();
        if let Some(reason) = &w.closed {
            return Err(Error::Disconnected(reason.clone()));
        }
        let q = w.queues.entry(key).or_default();
        q.retain(|t| !t.is_closed());
        q.push_back(tx);
        Ok(rx)
    }

    /// Hand `frame` to the oldest live waiter for its key; gives it back when nobody waits.
    fn deliver(&self, frame: Frame) -> Option<Frame> {
        let key = (frame.main, frame.sub);
        let mut w = self.waiters();
        let Some(q) = w.queues.get_mut(&key) else { return Some(frame) };
        while let Some(tx) = q.pop_front() {
            if !tx.is_closed() {
                let _ = tx.send(Ok(frame));
                return None;
            }
        }
        w.queues.remove(&key);
        Some(frame)
    }

    /// Fail every pending request and refuse new ones. Returns false when already closed.
    fn fail_all(&self, reason: &str) -> bool {
        let mut w = self.waiters();
        if w.closed.is_some() {
            return false;
        }
        w.closed = Some(reason.to_string());
        for (_, q) in w.queues.drain() {
            for tx in q {
                let _ = tx.send(Err(Error::Disconnected(reason.to_string())));
            }
        }
        true
    }

    fn dispatch(&self, frame: Frame) {
        trace!(main = frame.main, sub = ?frame.sub, len = frame.payload.len(), "rx");
        let mut frame = frame;
        if frame.main == Main::GETPRM {
            match frame.sub[0] {
                Env::GAINSLEVEL if frame.payload.len() >= Meters::SIZE => {
                    if let Some(m) = Meters::parse(&frame.payload) {
                        *self.meters.lock().unwrap_or_else(|e| e.into_inner()) = Some(m.clone());
                        let _ = self.events.send(ClientEvent::Meters(m));
                    }
                    // Only reaches a caller that asked for GAINSLEVEL explicitly.
                    let _ = self.deliver(frame);
                    return;
                }
                Env::TEMPERATURE if frame.payload.len() == 4 => {
                    let p = &frame.payload;
                    let t = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                    self.temperature.store(t as i64, Ordering::Relaxed);
                }
                _ => {}
            }
        }
        match self.deliver(frame) {
            None => return,
            Some(f) => frame = f,
        }
        if matches!(frame.main, Main::GETPRM | Main::DEVPRMCHAGE) {
            let _ = self.events.send(ClientEvent::Frame(frame));
        }
    }

    async fn send(&self, main: u16, sub: [u8; 4], payload: &[u8]) -> Result<()> {
        if let Some(reason) = &self.waiters().closed {
            return Err(Error::Disconnected(reason.clone()));
        }
        let bytes = build_frame(main, sub, payload)?;
        let mut guard = self.writer.lock().await;
        let w = guard.as_mut().ok_or_else(|| Error::Disconnected("not connected".into()))?;
        match timeout(self.timeout.max(Duration::from_secs(1)), w.write_all(&bytes)).await {
            Ok(r) => r.map_err(Error::Io),
            Err(_) => Err(Error::Timeout("write to amp".into())),
        }
    }

    /// The connection is over although the amp did not close it: fail what waits, tell the listeners and
    /// release the socket, so that the amp can serve another controller.
    async fn lose(&self, reason: &str) {
        if self.fail_all(reason) {
            let _ = self.events.send(ClientEvent::Disconnected(Some(reason.to_string())));
        }
        if let Some(mut w) = self.writer.lock().await.take() {
            let _ = w.shutdown().await;
        }
    }

    async fn request(&self, main: u16, sub: [u8; 4], payload: &[u8], wait: Duration) -> Result<Frame> {
        let rx = self.register((main, sub))?;
        self.send(main, sub, payload).await?;
        match timeout(wait, rx).await {
            Err(_) => Err(Error::Timeout(format!(
                "no reply to {} {}",
                Main::name(main).unwrap_or("?"),
                sub.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ))),
            Ok(Err(_)) => Err(Error::Disconnected("connection closed".into())),
            Ok(Ok(result)) => result,
        }
    }
}

async fn reader(inner: Arc<Inner>, mut rd: OwnedReadHalf, mut parser: FrameParser) {
    let mut buf = vec![0u8; 65536];
    let reason = loop {
        match rd.read(&mut buf).await {
            Ok(0) => break "connection closed by amp".to_string(),
            Ok(n) => {
                *inner.last_rx.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                for f in parser.feed(&buf[..n]) {
                    inner.dispatch(f);
                }
            }
            Err(e) => break e.to_string(),
        }
    };
    debug!(%reason, "ipx reader stopped");
    if inner.fail_all(&reason) {
        let _ = inner.events.send(ClientEvent::Disconnected(Some(reason)));
    }
}

/// Sends the heartbeat, and watches the amp: one that has vanished from the network (power off, cable
/// pulled, Wi-Fi gone) closes nothing, but it stops sending meters too. After this many heartbeat periods,
/// at least a second, without a byte the connection is lost.
const SILENT_PERIODS: u32 = 3;

async fn heartbeat(inner: Arc<Inner>, period: Duration) {
    let limit = (period * SILENT_PERIODS).max(Duration::from_secs(1));
    let mut tick = interval_at(Instant::now() + period, period);
    loop {
        tick.tick().await;
        let silent = inner.last_rx.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
        if silent > limit {
            warn!("ipx: no data for {silent:?}");
            inner.lose(&format!("no data from the amp for {} s", silent.as_secs())).await;
            return;
        }
        if let Err(e) = inner.send(Main::SYSCTRL, ctrl_sub(Ctrl::HEARTBEAT), &[]).await {
            warn!("ipx heartbeat failed: {e}");
            inner.lose(&format!("heartbeat failed: {e}")).await;
            return;
        }
    }
}

/// A connection to one amp. The amp serves a single controller at a time.
pub struct Client {
    inner: Arc<Inner>,
    tasks: StdMutex<Vec<JoinHandle<()>>>,
}

impl Client {
    pub async fn connect(host: &str, config: &ClientConfig) -> Result<Client> {
        let mut stream = timeout(config.connect_timeout, TcpStream::connect((host, config.port)))
            .await
            .map_err(|_| Error::Timeout(format!("connecting to {host}:{}", config.port)))??;
        stream.set_nodelay(true)?;

        // A busy amp accepts the connection and closes it at once.
        let mut first = vec![0u8; 65536];
        let n = match timeout(FIRST_READ, stream.read(&mut first)).await {
            Err(_) => 0,
            Ok(Ok(0)) => return Err(busy(host)),
            Ok(Ok(n)) => n,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionReset => return Err(busy(host)),
            Ok(Err(e)) => return Err(e.into()),
        };
        let (rd, wr) = stream.into_split();
        let (events, _) = broadcast::channel(256);
        let inner = Arc::new(Inner {
            writer: Mutex::new(Some(wr)),
            waiters: StdMutex::new(Waiters::default()),
            events,
            meters: StdMutex::new(None),
            temperature: AtomicI64::new(NO_TEMPERATURE),
            timeout: config.timeout,
            last_rx: StdMutex::new(Instant::now()),
        });
        let mut parser = FrameParser::new();
        for f in parser.feed(&first[..n]) {
            inner.dispatch(f);
        }
        let mut tasks = vec![tokio::spawn(reader(inner.clone(), rd, parser))];
        if let Some(period) = config.heartbeat {
            tasks.push(tokio::spawn(heartbeat(inner.clone(), period)));
        }
        Ok(Client { inner, tasks: StdMutex::new(tasks) })
    }

    pub fn is_connected(&self) -> bool {
        self.inner.waiters().closed.is_none()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.inner.events.subscribe()
    }

    /// Latest GAINSLEVEL push.
    pub fn meters(&self) -> Option<Meters> {
        self.inner.meters.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Latest TEMPERATURE push.
    pub fn temperature(&self) -> Option<i32> {
        match self.inner.temperature.load(Ordering::Relaxed) {
            NO_TEMPERATURE => None,
            t => Some(t as i32),
        }
    }

    pub async fn get_raw(&self, addr: Address) -> Result<Vec<u8>> {
        let wait = if addr.env == Env::DEV_PRM_SYNC {
            self.inner.timeout.max(LONG_REPLY)
        } else {
            self.inner.timeout
        };
        self.get_raw_timeout(addr, wait).await
    }

    pub async fn get_raw_timeout(&self, addr: Address, wait: Duration) -> Result<Vec<u8>> {
        Ok(self.inner.request(Main::GETPRM, addr.pack(), &[], wait).await?.payload)
    }

    /// Write one parameter. The amp sends no acknowledgement.
    pub async fn set_raw(&self, addr: Address, payload: &[u8]) -> Result<()> {
        self.inner.send(Main::SETPRM, addr.pack(), payload).await
    }

    /// SYSCTRL command with a reply. `wait` defaults to 5 s for GETPRESET, else the client timeout.
    pub async fn ctrl(&self, cmd: u16, payload: &[u8], wait: Option<Duration>) -> Result<Frame> {
        let wait = wait.unwrap_or(if cmd == Ctrl::GETPRESET {
            self.inner.timeout.max(LONG_REPLY)
        } else {
            self.inner.timeout
        });
        self.inner.request(Main::SYSCTRL, ctrl_sub(cmd), payload, wait).await
    }

    /// SYSCTRL command the amp does not answer (RECALLPRESET).
    pub async fn send_ctrl(&self, cmd: u16, payload: &[u8]) -> Result<()> {
        self.inner.send(Main::SYSCTRL, ctrl_sub(cmd), payload).await
    }

    /// Read the complete device state (what IPX Controller does on connect).
    pub async fn sync(&self) -> Result<DeviceState> {
        DeviceState::parse(&self.get_raw(Address::block(Env::DEV_PRM_SYNC)).await?)
    }

    pub async fn close(&self) {
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap_or_else(|e| e.into_inner()));
        for t in &tasks {
            t.abort();
        }
        if let Some(mut w) = self.inner.writer.lock().await.take() {
            let _ = w.shutdown().await;
        }
        if self.inner.fail_all("closed") {
            let _ = self.inner.events.send(ClientEvent::Disconnected(None));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        for t in self.tasks.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            t.abort();
        }
    }
}

fn busy(host: &str) -> Error {
    Error::Busy(format!("{host} closed the connection; is IPX Controller connected?"))
}
