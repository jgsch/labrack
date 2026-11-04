#![allow(dead_code)]

//! Test harness: a connector that reaches the IPD simulator and the fake IPX on local ports, by the
//! IP the amp has in the list (the loopback addresses cannot tell the amps apart).

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use labgruppen::amps::ipd::IpdAmp;
use labgruppen::amps::ipd::client::Options;
use labgruppen::amps::ipx::IpxAmp;
use labrack_core::{AmpId, Connector, Hub, Protocol, Status, Store, Timing, Update};
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
    (Hub::new(store, local.clone(), timing), local)
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
