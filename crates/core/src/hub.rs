//! The connections (ARCH-3): one tokio task per amplifier owns its connection and tells the hub what
//! happens; user intents go through the hub, which keeps the amp list, the tasks and the statuses
//! together. A task only reads from its amp: connecting, reconnecting and starting never write
//! (SEC-1), and nothing queues for an amp that is not ONLINE.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use labgruppen::{AmpEvent, Meters, Param, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use tokio::time::{MissedTickBehavior, interval, sleep};

use crate::status::{Event, Status};
use crate::store::{Amp, AmpId, Preset, Protocol, Store};
use crate::{Connector, Result};

/// Intervals of the connection tasks (BEH-12, BEH-42).
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Wait before the first retry of a failed attempt or a lost connection.
    pub retry_first: Duration,
    /// Each retry waits twice as long as the one before, up to this.
    pub retry_max: Duration,
    /// Meters are passed on at most this often.
    pub meter_interval: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            retry_first: Duration::from_secs(1),
            retry_max: Duration::from_secs(15),
            meter_interval: Duration::from_millis(100),
        }
    }
}

/// What the views learn from the connections.
#[derive(Clone, Debug)]
pub enum Update {
    Status(AmpId, Status),
    /// At most once per [`Timing::meter_interval`] per amp.
    Meters(AmpId, Meters),
    /// A parameter changed on the amp (front panel, another controller).
    Changed(AmpId, Param, Value),
}

/// What a connection task needs to know about its amp; a change restarts the task's attempts.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    protocol: Protocol,
    ip: String,
    enabled: bool,
}

impl Target {
    fn of(amp: &Amp) -> Target {
        Target { protocol: amp.protocol, ip: amp.ip.clone(), enabled: amp.connected }
    }

    fn connectable(&self) -> bool {
        self.enabled && self.ip.parse::<Ipv4Addr>().is_ok()
    }

    fn first_event(&self) -> Event {
        match (self.enabled, self.connectable()) {
            (false, _) => Event::Stop,
            (true, false) => Event::BadIp,
            (true, true) => Event::Start,
        }
    }
}

struct Live {
    status: Status,
    meters: Meters,
}

struct Shared {
    store: Mutex<Store>,
    live: Mutex<HashMap<AmpId, Live>>,
    events: broadcast::Sender<Update>,
    connector: Arc<dyn Connector>,
    timing: Timing,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    fn emit(&self, update: Update) {
        let _ = self.events.send(update);
    }

    /// Only the amp's own task calls this.
    fn apply(&self, id: AmpId, event: Event) {
        let next = {
            let mut live = lock(&self.live);
            let Some(amp) = live.get_mut(&id) else { return };
            let next = amp.status.next(event);
            if next == amp.status {
                return;
            }
            amp.status = next;
            if next != Status::Online {
                amp.meters = Meters::default();
            }
            next
        };
        if next != Status::Online {
            let _ = self.store().set_preset(id, None);
        }
        self.emit(Update::Status(id, next));
    }

    fn set_meters(&self, id: AmpId, meters: Meters) {
        {
            let mut live = lock(&self.live);
            let Some(amp) = live.get_mut(&id) else { return };
            amp.meters = meters.clone();
        }
        self.emit(Update::Meters(id, meters));
    }
}

struct Inner {
    shared: Arc<Shared>,
    /// Dropping a sender ends the task of that amp, after it closed the connection.
    targets: Mutex<HashMap<AmpId, watch::Sender<Target>>>,
}

/// The amplifier list with its connections. Cheap to clone; the connections close when the last
/// clone is dropped. Create it inside a tokio runtime.
#[derive(Clone)]
pub struct Hub(Arc<Inner>);

impl Hub {
    /// Connects to every amp of the store that is not disconnected (BEH-42).
    pub fn new(store: Store, connector: Arc<dyn Connector>, timing: Timing) -> Hub {
        let (events, _) = broadcast::channel(1024);
        let shared = Shared { store: Mutex::new(store), live: Mutex::default(), events, connector, timing };
        let hub = Hub(Arc::new(Inner { shared: Arc::new(shared), targets: Mutex::default() }));
        hub.reconcile();
        hub
    }

    /// The list, in order (BEH-39).
    pub fn amps(&self) -> Vec<Amp> {
        self.0.shared.store().amps().to_vec()
    }

    pub fn status(&self, id: AmpId) -> Option<Status> {
        lock(&self.0.shared.live).get(&id).map(|amp| amp.status)
    }

    /// The latest meters; empty when the amp is not ONLINE (BEH-10).
    pub fn meters(&self, id: AmpId) -> Option<Meters> {
        lock(&self.0.shared.live).get(&id).map(|amp| amp.meters.clone())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.0.shared.events.subscribe()
    }

    pub fn add_manual(&self, name: &str, protocol: Protocol, ip: &str) -> Result<AmpId> {
        let added = self.0.shared.store().add_manual(name, protocol, ip);
        self.reconcile();
        added
    }

    pub fn rename(&self, id: AmpId, name: &str) -> Result<()> {
        self.0.shared.store().rename(id, name)
    }

    pub fn remove(&self, id: AmpId) -> Result<()> {
        let removed = self.0.shared.store().remove(id);
        self.reconcile();
        removed
    }

    pub fn set_ip(&self, id: AmpId, ip: &str) -> Result<()> {
        let set = self.0.shared.store().set_ip(id, ip);
        self.reconcile();
        set
    }

    /// CONNECT (BEH-9).
    pub fn connect(&self, id: AmpId) -> Result<()> {
        self.set_connected(id, true)
    }

    /// DISCONNECT: closes the connection so another controller can use the amp (BEH-9).
    pub fn disconnect(&self, id: AmpId) -> Result<()> {
        self.set_connected(id, false)
    }

    fn set_connected(&self, id: AmpId, connected: bool) -> Result<()> {
        let set = self.0.shared.store().set_connected(id, connected);
        self.reconcile();
        set
    }

    /// Make the tasks match the list: start one for a new amp, hand a changed target to its task,
    /// end the task of a removed one. Runs after every intent, also a failed one, because a store
    /// that could not save has changed all the same.
    fn reconcile(&self) {
        let shared = &self.0.shared;
        let mut targets = lock(&self.0.targets);
        let amps = shared.store().amps().to_vec();
        targets.retain(|id, _| amps.iter().any(|amp| amp.id == *id));
        lock(&shared.live).retain(|id, _| amps.iter().any(|amp| amp.id == *id));
        for amp in &amps {
            let target = Target::of(amp);
            match targets.get(&amp.id) {
                Some(sender) => {
                    sender.send_if_modified(|current| {
                        let changed = *current != target;
                        if changed {
                            *current = target;
                        }
                        changed
                    });
                }
                None => {
                    let status = Status::Disconnected.next(target.first_event());
                    lock(&shared.live).insert(amp.id, Live { status, meters: Meters::default() });
                    let (sender, receiver) = watch::channel(target);
                    targets.insert(amp.id, sender);
                    tokio::spawn(run(shared.clone(), amp.id, receiver));
                }
            }
        }
    }
}

/// The task of one amp: follow its target until the amp is removed.
async fn run(shared: Arc<Shared>, id: AmpId, mut target: watch::Receiver<Target>) {
    loop {
        let now = target.borrow_and_update().clone();
        shared.apply(id, now.first_event());
        let more = if now.connectable() {
            session(&shared, id, &now, &mut target).await
        } else {
            target.changed().await.is_ok()
        };
        if !more {
            return;
        }
    }
}

/// Connect, stay connected, and connect again after a loss, until the target changes (true) or the
/// amp is removed (false). The connection is closed before this returns.
async fn session(shared: &Shared, id: AmpId, now: &Target, target: &mut watch::Receiver<Target>) -> bool {
    let timing = shared.timing;
    let mut delay = timing.retry_first;
    loop {
        let attempt = tokio::select! {
            changed = target.changed() => return changed.is_ok(),
            attempt = connect_and_read(shared, now) => attempt,
        };
        match attempt {
            Ok(mut link) => {
                delay = timing.retry_first;
                // the preset first: whoever sees ONLINE finds it
                let _ = shared.store().set_preset(id, link.preset.take());
                shared.apply(id, Event::Connected);
                let changed = tokio::select! {
                    changed = target.changed() => Some(changed.is_ok()),
                    () = pump(shared, id, &mut link.events) => None,
                };
                link.amp.close().await;
                if let Some(more) = changed {
                    return more;
                }
                shared.apply(id, Event::Lost);
            }
            Err(e) => shared.apply(id, failure(&e)),
        }
        tokio::select! {
            changed = target.changed() => return changed.is_ok(),
            () = sleep(delay) => {}
        }
        delay = (delay * 2).min(timing.retry_max);
    }
}

fn failure(error: &labgruppen::Error) -> Event {
    match error {
        labgruppen::Error::Busy(_) => Event::OtherController,
        _ => Event::NoAnswer,
    }
}

struct Link {
    amp: Arc<dyn labgruppen::Amp>,
    events: broadcast::Receiver<AmpEvent>,
    preset: Option<Preset>,
}

/// An amp is ONLINE when it is connected and its state could be read.
async fn connect_and_read(shared: &Shared, target: &Target) -> labgruppen::Result<Link> {
    let amp = shared.connector.connect(target.protocol, &target.ip).await?;
    let events = amp.subscribe();
    match amp.status().await {
        Ok(status) => {
            let slots = amp.preset_slots();
            let preset = status
                .preset
                .filter(|p| slots.contains(&p.index))
                .map(|p| Preset { number: p.index, name: p.name });
            Ok(Link { amp, events, preset })
        }
        Err(e) => {
            amp.close().await;
            Err(e)
        }
    }
}

/// Pass on what the amp sends until the connection ends. Meters arrive faster than the views need
/// them; only the latest goes on, once per interval.
async fn pump(shared: &Shared, id: AmpId, events: &mut broadcast::Receiver<AmpEvent>) {
    let mut tick = interval(shared.timing.meter_interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut latest: Option<Meters> = None;
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(AmpEvent::Meters(meters)) => latest = Some(meters),
                Ok(AmpEvent::Changed(param, value)) => shared.emit(Update::Changed(id, param, value)),
                Ok(AmpEvent::Disconnected(_)) | Err(RecvError::Closed) => return,
                Err(RecvError::Lagged(_)) => {}
            },
            _ = tick.tick() => {
                if let Some(meters) = latest.take() {
                    shared.set_meters(id, meters);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use labgruppen::Error;

    use super::*;

    // BEH-8: BUSY only for an amp that has another controller; anything else is no answer
    #[test]
    fn errors_map_to_busy_or_unreachable() {
        assert_eq!(failure(&Error::Busy("x".into())), Event::OtherController);
        for e in [
            Error::Timeout("x".into()),
            Error::Io(std::io::ErrorKind::ConnectionRefused.into()),
            Error::Disconnected("x".into()),
            Error::Protocol("x".into()),
            Error::Unsupported("x".into()),
        ] {
            assert_eq!(failure(&e), Event::NoAnswer, "{e}");
        }
    }

    // BEH-8: what an amp shows before its task has done anything
    #[test]
    fn first_event_follows_the_flag_and_the_ip() {
        let target = |ip: &str, enabled| Target { protocol: Protocol::Ipx, ip: ip.into(), enabled };
        let first = |t: Target| Status::Disconnected.next(t.first_event());
        assert_eq!(first(target("10.0.0.1", true)), Status::Connecting);
        assert_eq!(first(target("10.0.0.", true)), Status::CheckIp);
        assert_eq!(first(target("10.0.0.1", false)), Status::Disconnected);
        assert_eq!(first(target("10.0.0.", false)), Status::Disconnected);
    }
}
