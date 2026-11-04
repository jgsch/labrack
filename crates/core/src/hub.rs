//! The connections (ARCH-3): one tokio task per amplifier owns its connection and tells the hub what
//! happens; user intents go through the hub, which keeps the amp list, the tasks and the statuses
//! together. A task only reads from its amp: connecting, reconnecting and starting never write
//! (SEC-1), and nothing queues for an amp that is not ONLINE. The hub also runs discovery scans and
//! adds what they find to the list.

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use labgruppen::{AmpEvent, Meters, Param, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use tokio::time::{MissedTickBehavior, interval, sleep};

use crate::status::{Event, Status};
use crate::store::{Amp, AmpId, Preset, Protocol, Store, Switches};
use crate::{Connector, Finder, Result};

/// Intervals and durations (BEH-12, BEH-18, BEH-42).
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Wait before the first retry of a failed attempt or a lost connection.
    pub retry_first: Duration,
    /// Each retry waits twice as long as the one before, up to this.
    pub retry_max: Duration,
    /// Meters are passed on at most this often.
    pub meter_interval: Duration,
    /// How long a scan searches. Over Wi-Fi an IPD can need a few seconds to answer a probe.
    pub scan_window: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            retry_first: Duration::from_secs(1),
            retry_max: Duration::from_secs(15),
            meter_interval: Duration::from_millis(100),
            scan_window: Duration::from_secs(5),
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
    Scan(Scan),
}

/// A discovery scan (BEH-18). Whether one is running: [`Hub::scanning`].
#[derive(Clone, Debug)]
pub enum Scan {
    Started,
    Finished {
        /// The amps this scan added to the list, in list order.
        added: Vec<AmpId>,
        /// One text per protocol that could not be searched, or per amp that could not be saved.
        errors: Vec<String>,
    },
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
    finder: Arc<dyn Finder>,
    scans: AtomicUsize,
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
    /// Connects to every amp of the store that is not disconnected (BEH-42). Starting searches
    /// nothing (BEH-18).
    pub fn new(store: Store, connector: Arc<dyn Connector>, finder: Arc<dyn Finder>, timing: Timing) -> Hub {
        let (events, _) = broadcast::channel(1024);
        let shared = Shared {
            store: Mutex::new(store),
            live: Mutex::default(),
            events,
            connector,
            finder,
            scans: AtomicUsize::new(0),
            timing,
        };
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

    pub fn switches(&self) -> Switches {
        self.0.shared.store().switches()
    }

    /// Saves the switches. A protocol that was off and is on now is searched at once (BEH-18).
    pub fn set_switches(&self, switches: Switches) -> Result<()> {
        let (before, saved) = {
            let mut store = self.0.shared.store();
            (store.switches(), store.set_switches(switches))
        };
        let newly_on = [Protocol::Ipd, Protocol::Ipx]
            .into_iter()
            .filter(|p| switches.allows(*p) && !before.allows(*p))
            .collect();
        self.scan(newly_on);
        saved
    }

    /// SCAN NOW: search every protocol that is on. False, and nothing is searched, when both are off
    /// (BEH-17) or a scan is already running.
    pub fn scan_now(&self) -> bool {
        if self.scanning() {
            return false;
        }
        let switches = self.switches();
        let on = [Protocol::Ipd, Protocol::Ipx].into_iter().filter(|p| switches.allows(*p)).collect();
        self.scan(on)
    }

    pub fn scanning(&self) -> bool {
        self.0.shared.scans.load(Ordering::SeqCst) > 0
    }

    /// Search `protocols` for [`Timing::scan_window`], then add the new amps to the list. A scan does
    /// not keep the hub alive.
    fn scan(&self, protocols: Vec<Protocol>) -> bool {
        if protocols.is_empty() {
            return false;
        }
        let shared = self.0.shared.clone();
        shared.scans.fetch_add(1, Ordering::SeqCst);
        shared.emit(Update::Scan(Scan::Started));
        let hub = Arc::downgrade(&self.0);
        tokio::spawn(async move {
            let window = shared.timing.scan_window;
            let search = |protocol: Protocol| {
                let finder = shared.finder.clone();
                let wanted = protocols.contains(&protocol);
                async move {
                    match wanted {
                        true => Some(finder.find(protocol, window).await),
                        false => None,
                    }
                }
            };
            let (ipx, ipd) = tokio::join!(search(Protocol::Ipx), search(Protocol::Ipd));
            let (mut found, mut errors) = (Vec::new(), Vec::new());
            for (protocol, result) in [(Protocol::Ipx, ipx), (Protocol::Ipd, ipd)] {
                match result {
                    Some(Ok(ips)) => found.extend(ips.into_iter().map(|ip| (protocol, ip))),
                    Some(Err(e)) => errors.push(format!("{}: {e}", protocol.label())),
                    None => {}
                }
            }
            let added = match hub.upgrade() {
                Some(inner) => Hub(inner).merge(found, &mut errors),
                None => Vec::new(),
            };
            // not scanning any more by the time anyone hears that the scan finished
            shared.scans.fetch_sub(1, Ordering::SeqCst);
            shared.emit(Update::Scan(Scan::Finished { added, errors }));
        });
        true
    }

    /// Add what a scan found, unless its protocol was switched off meanwhile (BEH-17) or the IP is
    /// known (BEH-19). Returns the new amps.
    fn merge(&self, mut found: Vec<(Protocol, Ipv4Addr)>, errors: &mut Vec<String>) -> Vec<AmpId> {
        found.sort_by_key(|(_, ip)| *ip);
        let known: HashSet<AmpId> = self.0.shared.store().amps().iter().map(|a| a.id).collect();
        {
            let mut store = self.0.shared.store();
            let switches = store.switches();
            for (protocol, ip) in found {
                if switches.allows(protocol)
                    && let Err(e) = store.add_discovered(protocol, ip)
                {
                    errors.push(e.to_string());
                }
            }
        }
        self.reconcile();
        let amps = self.0.shared.store().amps().to_vec();
        amps.into_iter().map(|a| a.id).filter(|id| !known.contains(id)).collect()
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
