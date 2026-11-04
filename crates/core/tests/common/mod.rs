#![allow(dead_code)]

//! Test harness: a connector that reaches the IPD simulator and the fake IPX on local ports, by the
//! IP the amp has in the list (the loopback addresses cannot tell the amps apart).

use std::collections::HashMap;
use std::io;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use labgruppen::amps::ipd::IpdAmp;
use labgruppen::amps::ipd::client::Options;
use labgruppen::amps::ipd::sim::SimServer;
use labgruppen::amps::ipx::IpxAmp;
use labgruppen::amps::ipx::fake::FakeAmp;
use labgruppen::amps::ipx::params as ipx_params;
use labgruppen::amps::ipx::structs::DeviceState;
use labgruppen::{Param, Value};
use labrack_core::{AmpId, Connector, Finder, Hub, Protocol, Scan, Status, Store, Timing, Update};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

pub struct Local {
    routes: Mutex<HashMap<String, u16>>,
    attempts: AtomicUsize,
}

impl Local {
    pub fn new() -> Arc<Local> {
        Arc::new(Local { routes: Mutex::default(), attempts: AtomicUsize::new(0) })
    }

    /// Amps listed under `ip` are reached on `port` of the loopback address.
    pub fn route(&self, ip: &str, port: u16) {
        self.routes.lock().unwrap().insert(ip.to_string(), port);
    }

    /// How many times the app tried to connect to any amp.
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Connector for Local {
    async fn connect(&self, protocol: Protocol, ip: &str) -> labgruppen::Result<Arc<dyn labgruppen::Amp>> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        let port = self.routes.lock().unwrap().get(ip).copied();
        let port =
            port.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no route to {ip}")))?;
        Ok(match protocol {
            Protocol::Ipx => Arc::new(IpxAmp::connect_with("127.0.0.1", port, Duration::from_secs(1)).await?),
            Protocol::Ipd => Arc::new(
                IpdAmp::connect_with(
                    "127.0.0.1",
                    port,
                    Options { mk2: None, timeout: Duration::from_secs(5) },
                )
                .await?,
            ),
        })
    }
}

/// Short retries so the tests do not wait.
pub fn timing() -> Timing {
    Timing {
        retry_first: Duration::from_millis(30),
        retry_max: Duration::from_millis(100),
        ..Timing::default()
    }
}

pub fn hub() -> (Hub, Arc<Local>) {
    hub_with(Store::in_memory(), timing())
}

pub fn hub_with(store: Store, timing: Timing) -> (Hub, Arc<Local>) {
    let local = Local::new();
    (Hub::new(store, local.clone(), Heard::nothing(), timing), local)
}

/// A finder that "hears" the addresses it is given, after `delay`, and records what was searched.
pub struct Heard {
    heard: Mutex<HashMap<Protocol, labgruppen::Result<Vec<Ipv4Addr>>>>,
    searched: Mutex<Vec<Protocol>>,
    pub delay: Mutex<Duration>,
}

impl Heard {
    pub fn nothing() -> Arc<Heard> {
        Arc::new(Heard { heard: Mutex::default(), searched: Mutex::default(), delay: Mutex::default() })
    }

    pub fn hear(&self, protocol: Protocol, ips: &[&str]) {
        let ips = ips.iter().map(|ip| ip.parse().unwrap()).collect();
        self.heard.lock().unwrap().insert(protocol, Ok(ips));
    }

    pub fn fail(&self, protocol: Protocol, why: &str) {
        let err = labgruppen::Error::Io(io::Error::other(why.to_string()));
        self.heard.lock().unwrap().insert(protocol, Err(err));
    }

    /// Forget what was searched so far.
    pub fn forget(&self) {
        self.searched.lock().unwrap().clear();
    }

    /// Protocols searched so far, in order of the requests.
    pub fn searched(&self) -> Vec<Protocol> {
        self.searched.lock().unwrap().clone()
    }
}

#[async_trait]
impl Finder for Heard {
    async fn find(&self, protocol: Protocol, _window: Duration) -> labgruppen::Result<Vec<Ipv4Addr>> {
        self.searched.lock().unwrap().push(protocol);
        let delay = *self.delay.lock().unwrap();
        sleep(delay).await;
        match self.heard.lock().unwrap().get(&protocol) {
            Some(Ok(ips)) => Ok(ips.clone()),
            Some(Err(e)) => Err(labgruppen::Error::Io(io::Error::other(e.to_string()))),
            None => Ok(Vec::new()),
        }
    }
}

/// A hub that searches with `finder`; the connector reaches nothing, so found amps stay UNREACHABLE.
pub fn hub_finding(store: Store, finder: Arc<Heard>) -> Hub {
    Hub::new(store, Local::new(), finder, timing())
}

/// Read updates until a scan finishes; returns the ids it added and the errors it reports.
pub async fn scan_finished(rx: &mut broadcast::Receiver<Update>) -> (Vec<AmpId>, Vec<String>) {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(Update::Scan(Scan::Finished { added, errors })) => return (added, errors),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("{e}"),
            }
        }
    };
    timeout(Duration::from_secs(15), wait).await.expect("a scan finishes")
}

/// Poll until the amp has `want`.
pub async fn wait_status(hub: &Hub, id: AmpId, want: Status) {
    let poll = async {
        while hub.status(id) != Some(want) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    if timeout(Duration::from_secs(15), poll).await.is_err() {
        panic!("status stayed {:?}, expected {want:?}", hub.status(id));
    }
}

/// Read updates until the amp's status becomes `want`; subscribe before the action that causes it.
pub async fn next_status(rx: &mut broadcast::Receiver<Update>, id: AmpId, want: Status) {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(Update::Status(i, s)) if i == id && s == want => return,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("{e}"),
            }
        }
    };
    timeout(Duration::from_secs(15), wait).await.unwrap_or_else(|_| panic!("never saw {want:?}"));
}

/// A port nothing listens on.
pub async fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port()
}

/// Whether another controller (the vendor app) can use the IPX at `port` now: it can once nobody else
/// holds it. Retries for a few seconds, as the amp's owner closes asynchronously.
pub async fn another_controller_can_connect(port: u16) -> bool {
    let attempt = async {
        loop {
            if let Ok(amp) = IpxAmp::connect_with("127.0.0.1", port, Duration::from_millis(300)).await {
                use labgruppen::Amp;
                amp.close().await;
                return;
            }
            sleep(Duration::from_millis(50)).await;
        }
    };
    timeout(Duration::from_secs(5), attempt).await.is_ok()
}

// ---- shared by the DSP and preset tests ----

pub fn param(s: &str) -> Param {
    s.parse().unwrap()
}

pub fn f(v: f64) -> Value {
    Value::Float(v)
}

/// What the fake IPX holds for `name`.
pub fn ipx_value(fake: &FakeAmp, name: &str) -> Value {
    let state = DeviceState::parse(&fake.state().sync).unwrap();
    let slot = ipx_params::slot(&param(name)).unwrap();
    slot.decode(state.audio_bytes(slot.addr, slot.wire_len()).unwrap()).unwrap()
}

pub async fn online_ipx(timing: Timing) -> (Hub, AmpId, FakeAmp) {
    let fake = FakeAmp::start().await;
    let (hub, local) = hub_with(Store::in_memory(), timing);
    local.route("10.0.0.1", fake.port);
    let id = hub.add_manual("Left", Protocol::Ipx, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    (hub, id, fake)
}

pub async fn online_ipd() -> (Hub, AmpId, SimServer) {
    let sim = SimServer::spawn("127.0.0.1:0".parse().unwrap(), true, 0).await.unwrap();
    let (hub, local) = hub();
    local.route("10.0.0.1", sim.addr.port());
    let id = hub.add_manual("Right", Protocol::Ipd, "10.0.0.1").unwrap();
    wait_loaded(&hub, id).await;
    (hub, id, sim)
}

/// ONLINE and the state read.
pub async fn wait_loaded(hub: &Hub, id: AmpId) {
    wait_status(hub, id, Status::Online).await;
    let loaded = async {
        while !hub.loaded(id) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    timeout(Duration::from_secs(20), loaded).await.expect("the state is read");
}

pub async fn wait_value(hub: &Hub, id: AmpId, name: &str, want: Value) {
    let p = param(name);
    let seen = async {
        while hub.value(id, &p).as_ref() != Some(&want) {
            sleep(Duration::from_millis(10)).await;
        }
    };
    if timeout(Duration::from_secs(10), seen).await.is_err() {
        panic!("{name} stayed {:?}, expected {want:?}", hub.value(id, &p));
    }
}

/// What the simulated IPD holds, read through a connection of its own.
pub async fn ipd_value(sim: &SimServer, name: &str) -> Value {
    let options = Options { mk2: None, timeout: Duration::from_secs(5) };
    let amp = IpdAmp::connect_with("127.0.0.1", sim.addr.port(), options).await.unwrap();
    let value = labgruppen::Amp::get(&amp, &param(name)).await.unwrap();
    labgruppen::Amp::close(&amp).await;
    value
}
