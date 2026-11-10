//! The connections (ARCH-3): one tokio task per amplifier owns its connection and tells the hub what
//! happens; user intents go through the hub, which keeps the amp list, the tasks and the statuses
//! together. A task only reads from its amp: connecting, reconnecting and starting never write
//! (SEC-1), and nothing queues for an amp that is not ONLINE. The hub also keeps the DSP state of the
//! amps that are ONLINE, takes the intents that change it, and runs discovery scans.

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use labgruppen::{Access, AmpEvent, Channel, Meters, Param, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval, sleep, timeout};

mod access;
mod ceilings;
mod limiter;
mod link;
mod mode;
mod mute;
mod presets;

pub use ceilings::Above;
pub use presets::LoadReport;

use crate::dsp::{self, Range, Writer};
use crate::link::{self as pair, Side};
use crate::status::{Event, Status};
use crate::store::{Amp, AmpId, Ceilings, Preset, Protocol, Store, Switches};
use crate::{Connector, Error, Finder, Result};

/// Intervals and durations (BEH-12, BEH-18, BEH-42).
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// Wait before the first retry of a failed attempt or a lost connection.
    pub retry_first: Duration,
    /// Each retry waits twice as long as the one before, up to this.
    pub retry_max: Duration,
    /// Meters are passed on at most this often.
    pub meter_interval: Duration,
    /// An IPD is asked for its output mutes this often: it answers one thing at a time and slowly.
    pub mute_poll_interval: Duration,
    /// How long a scan searches. Over Wi-Fi an IPD can need a few seconds to answer a probe.
    pub scan_window: Duration,
    /// A parameter is written at most this often while it is being changed (SEC-4).
    pub write_interval: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            retry_first: Duration::from_secs(1),
            retry_max: Duration::from_secs(15),
            meter_interval: Duration::from_millis(100),
            mute_poll_interval: Duration::from_millis(200),
            scan_window: Duration::from_secs(5),
            write_interval: Duration::from_millis(50),
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
    /// The state of an amp that went ONLINE has been read ([`Hub::values`]).
    Loaded(AmpId),
    /// The amp did not take a value that was accepted: the text says why. The state still holds
    /// what the amp has.
    WriteFailed(AmpId, Param, String),
    /// The MODIFIED flag of the amp changed ([`Hub::modified`]).
    Modified(AmpId, bool),
    /// [`Hub::set`] took a value; it is shown before the amp confirms it ([`Hub::shown`]).
    Pending(AmpId, Param),
    /// The loaded preset of the amp changed (`Hub::amps`).
    Preset(AmpId),
    /// The user level or the rights of an IPD changed ([`Hub::access`]).
    Access(AmpId),
    /// A setting the app keeps for the amp changed: a ceiling or a limiter load (`Hub::amps`).
    Settings(AmpId),
    /// Whether a pair is shown linked changed: the amp flags it linked and its channels now differ, or
    /// are equal again, or the link was switched ([`Hub::linked`], [`Hub::link_diverged`]).
    LinkChanged(AmpId, Side),
    /// The order of the known amplifiers changed ([`Hub::move_amp`]).
    Listed,
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
    /// The DSP state, while ONLINE. The IPD fills it as the amp answers; `loaded` says it is all there.
    values: HashMap<Param, Value>,
    loaded: bool,
    /// Changed since the app recalled or loaded the preset (BEH-38).
    modified: bool,
    /// Per side (`Side as usize`): the amp flags the pair linked but its channels differ, so the pair is
    /// shown as two channels with the values stored. Set from what the amp reports, never from the
    /// app's own writes, which keep a linked pair equal.
    diverged: [bool; 2],
    /// The user level and rights, while ONLINE; `None` for an amp without levels (the IPX).
    access: Option<Access>,
    /// The open connection, while ONLINE and until it ends.
    session: Option<Session>,
    /// Values [`Hub::set`] took that the amp has not confirmed yet, shown meanwhile ([`Hub::shown`]).
    /// Gone when the amp confirms or refuses them, or when they are dropped (bulk change, connection).
    pending: HashMap<Param, Value>,
}

#[derive(Clone)]
struct Session {
    amp: Arc<dyn labgruppen::Amp>,
    writer: Arc<Writer>,
    /// Held while something writes to the amp: the writer for each batch, a bulk change throughout.
    gate: Arc<tokio::sync::Mutex<()>>,
}

impl Session {
    fn is_ipx(&self) -> bool {
        self.amp.info().driver == "ipx"
    }
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
                amp.values.clear();
                amp.loaded = false;
                amp.modified = false;
                amp.diverged = [false; 2];
                amp.access = None;
                amp.session = None;
                amp.pending.clear();
            }
            next
        };
        if next != Status::Online {
            let _ = self.store().set_preset(id, None);
        }
        self.emit(Update::Status(id, next));
    }

    /// Open the amp for intents; they are accepted from now until [`Shared::detach`].
    fn attach(&self, id: AmpId, session: Session, access: Option<Access>) {
        if let Some(amp) = lock(&self.live).get_mut(&id) {
            amp.session = Some(session);
            amp.access = access;
        }
    }

    fn detach(&self, id: AmpId) {
        if let Some(amp) = lock(&self.live).get_mut(&id) {
            amp.session = None;
            amp.access = None;
        }
    }

    /// The amp has `value` for `param`; `own` when the app just wrote it. Views are told of a real
    /// change, and of every confirmed write of the app. A change of something the app already knew
    /// marks the preset MODIFIED (BEH-38); the first sight of a value, as the state is read, does not.
    fn observed(&self, id: AmpId, param: &Param, value: &Value, own: bool) {
        let (changed, flipped, twin, regrouped) = {
            let mut live = lock(&self.live);
            let Some(amp) = live.get_mut(&id).filter(|amp| amp.session.is_some()) else { return };
            // an IPD mirrors a linked pair by itself and tells nothing of channel 2: the state follows
            let ipd = amp.session.as_ref().is_some_and(|s| !s.is_ipx());
            let linked = |values: &HashMap<Param, Value>, side: pair::Side| {
                values.get(&side.flag()) == Some(&Value::Bool(true))
            };
            // and it cannot differ (BEH-32): what it reports for channel 2 of a linked pair is channel 1's
            let value = match pair::mirrored(param) {
                Some(side) if ipd && pair::is_second(param) && linked(&amp.values, side) => {
                    pair::first(param)
                        .and_then(|first| amp.values.get(&first).cloned())
                        .unwrap_or_else(|| value.clone())
                }
                _ => value.clone(),
            };
            let value = &value;
            let old = amp.values.insert(param.clone(), value.clone());
            let changed = old.as_ref() != Some(value);
            let modifies = changed && (old.is_some() || own);
            let flipped = modifies && !amp.modified;
            amp.modified |= modifies;
            let twin = pair::mirrored(param)
                .filter(|side| ipd && !pair::is_second(param) && linked(&amp.values, *side))
                .and_then(|_| pair::second(param));
            if let Some(twin) = &twin {
                amp.values.insert(twin.clone(), value.clone());
            }
            // an IPD that links a pair by itself makes channel 2 equal at once
            if ipd
                && value == &Value::Bool(true)
                && let Some(side) = pair::flag_side(param)
            {
                mirror_ipd(&mut amp.values, side);
            }
            // what the amp reports about a pair decides whether it is shown linked: the app writes nothing
            let regrouped = (!own && changed && amp.loaded && !ipd)
                .then(|| pair::follows(param).or_else(|| pair::flag_side(param)))
                .flatten()
                .filter(|side| {
                    let flagged = amp.values.get(&side.flag()) == Some(&Value::Bool(true));
                    let diverged = flagged && !link::side_drift(&amp.values, *side).is_empty();
                    let moved = diverged != amp.diverged[*side as usize];
                    amp.diverged[*side as usize] = diverged;
                    moved
                });
            (changed, flipped, twin, regrouped)
        };
        if changed || own {
            self.emit(Update::Changed(id, param.clone(), value.clone()));
            if let Some(twin) = twin {
                self.emit(Update::Changed(id, twin, value.clone()));
            }
        }
        if flipped {
            self.emit(Update::Modified(id, true));
        }
        if let Some(side) = regrouped {
            self.emit(Update::LinkChanged(id, side));
        }
    }

    /// Show a pair as linked or, with `diverged`, as two channels; true when that changed.
    fn set_diverged(&self, id: AmpId, side: Side, diverged: bool) -> bool {
        let mut live = lock(&self.live);
        let Some(amp) = live.get_mut(&id) else { return false };
        std::mem::replace(&mut amp.diverged[side as usize], diverged) != diverged
    }

    fn set_modified(&self, id: AmpId, modified: bool) {
        let flipped = {
            let mut live = lock(&self.live);
            let Some(amp) = live.get_mut(&id) else { return };
            let flipped = amp.modified != modified;
            amp.modified = modified;
            flipped
        };
        if flipped {
            self.emit(Update::Modified(id, modified));
        }
    }

    /// An IPD can say the name of its preset and no number (0): once the stored presets are known, the
    /// number is the slot of the one that has that name, when exactly one has it.
    fn resolve_preset(&self, id: AmpId, stored: &[Preset]) {
        let Some(unknown) = self.store().get(id).and_then(|a| a.preset.clone()).filter(|p| p.number == 0)
        else {
            return;
        };
        let same = |p: &&Preset| p.name.trim().eq_ignore_ascii_case(unknown.name.trim());
        let mut matching = stored.iter().filter(same);
        if let (Some(found), None) = (matching.next(), matching.next()) {
            self.set_loaded_preset(id, Some(found.clone()));
        }
    }

    fn set_loaded_preset(&self, id: AmpId, preset: Option<Preset>) {
        let _ = self.store().set_preset(id, preset);
        self.emit(Update::Preset(id));
    }

    /// Forget the state except the output mutes, which are known to be right, and read it again.
    fn begin_reload(&self, id: AmpId) {
        if let Some(amp) = lock(&self.live).get_mut(&id) {
            amp.values.retain(|param, _| matches!(param, Param::Mute(Channel::Output(_))));
            amp.loaded = false;
            amp.diverged = [false; 2];
            amp.pending.clear();
        }
    }

    /// The values waiting to be written were dropped: they are not shown any more.
    fn drop_pending(&self, id: AmpId) {
        if let Some(amp) = lock(&self.live).get_mut(&id) {
            amp.pending.clear();
        }
    }

    /// The amp confirmed or refused `value` for `param`: it is no longer pending, unless a newer value is.
    fn settle(&self, id: AmpId, param: &Param, value: &Value) {
        if let Some(amp) = lock(&self.live).get_mut(&id)
            && amp.pending.get(param) == Some(value)
        {
            amp.pending.remove(param);
        }
    }

    /// The state read from the connection of `token`. What changed since is newer and stays.
    fn load(&self, id: AmpId, token: &Arc<Writer>, snapshot: labgruppen::Snapshot) {
        let mut shown_apart = Vec::new();
        let kept = self.store().get(id).map(|amp| amp.links).unwrap_or_default();
        let mut dropped = Vec::new();
        {
            let mut live = lock(&self.live);
            let current = |amp: &Live| amp.session.as_ref().is_some_and(|s| Arc::ptr_eq(&s.writer, token));
            let Some(amp) = live.get_mut(&id).filter(|amp| current(amp)) else { return };
            for (name, value) in snapshot.params {
                if let Ok(param) = name.parse::<Param>() {
                    amp.values.entry(param).or_insert(value);
                }
            }
            amp.loaded = true;
            let ipd = amp.session.as_ref().is_some_and(|s| !s.is_ipx());
            // a pair the amp flags linked while its channels differ is shown as two: connecting only reads.
            // An IPD cannot differ (BEH-32): its channel 2 is shown as channel 1, which the amp mirrors
            for side in [Side::Inputs, Side::Outputs] {
                let flagged = amp.values.get(&side.flag()) == Some(&Value::Bool(true));
                if ipd && flagged {
                    mirror_ipd(&mut amp.values, side);
                }
                // a link the app keeps for an IPD, whose channels differ now (changed by someone else), is
                // dropped: the pair is shown as two channels
                if ipd && kept[side as usize] && !flagged && !link::side_drift(&amp.values, side).is_empty() {
                    dropped.push(side);
                }
                let diverged = !ipd && flagged && !link::side_drift(&amp.values, side).is_empty();
                amp.diverged[side as usize] = diverged;
                if diverged {
                    shown_apart.push(side);
                }
            }
        }
        for side in dropped {
            let _ = self.store().set_link(id, side as usize, false);
            shown_apart.push(side);
        }
        self.emit(Update::Loaded(id));
        for side in shown_apart {
            self.emit(Update::LinkChanged(id, side));
        }
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

/// Channel 2 of a pair the IPD itself has linked takes channel 1's values in the state, as in the amp.
fn mirror_ipd(values: &mut HashMap<Param, Value>, side: Side) {
    let copies: Vec<(Param, Value)> = values
        .iter()
        .filter(|(param, _)| pair::mirrored(param) == Some(side) && !pair::is_second(param))
        .filter_map(|(param, value)| pair::second(param).map(|twin| (twin, value.clone())))
        .collect();
    values.extend(copies);
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
    /// Connects to every amp of the store that is not disconnected (BEH-42) and searches the
    /// protocols that are on (BEH-18).
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
        hub.scan(hub.enabled());
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

    /// What the amp has for `param`; `None` until the state is read, and when the amp is not ONLINE.
    pub fn value(&self, id: AmpId, param: &Param) -> Option<Value> {
        lock(&self.0.shared.live).get(&id)?.values.get(param).cloned()
    }

    /// MODIFIED (BEH-38): the app changed something, or saw it changed, since it recalled or loaded the
    /// preset. Cleared when the amp goes ONLINE, because the app cannot know then.
    pub fn modified(&self, id: AmpId) -> bool {
        lock(&self.0.shared.live).get(&id).is_some_and(|amp| amp.modified)
    }

    /// Whether the whole state of the amp has been read since it went ONLINE ([`Update::Loaded`]).
    /// Until then [`Hub::values`] holds what the amp has answered so far.
    pub fn loaded(&self, id: AmpId) -> bool {
        lock(&self.0.shared.live).get(&id).is_some_and(|amp| amp.loaded)
    }

    /// What a view shows for `param`: the value [`Hub::set`] took while the amp has not confirmed it,
    /// or else [`Hub::value`]. A box, a fader or a MUTE then never falls back to the old value meanwhile.
    pub fn shown(&self, id: AmpId, param: &Param) -> Option<Value> {
        let live = lock(&self.0.shared.live);
        let amp = live.get(&id)?;
        amp.pending.get(param).or_else(|| amp.values.get(param)).cloned()
    }

    /// The DSP state of the amp; empty when it is not ONLINE.
    pub fn values(&self, id: AmpId) -> HashMap<Param, Value> {
        lock(&self.0.shared.live).get(&id).map(|amp| amp.values.clone()).unwrap_or_default()
    }

    /// What `param` may be set to, for the control to show: the amp's own range, ended by the user
    /// ceiling. `None` when the amp is not ONLINE or the control cannot be set (yet).
    pub fn range(&self, id: AmpId, param: &Param) -> Option<Range> {
        let session = self.session(id).ok()?;
        let ceilings = self.0.shared.store().get(id)?.ceilings;
        let spec = session.amp.spec(param).filter(|spec| spec.writable)?;
        dsp::controllable(param).then(|| dsp::range(param, &spec, &ceilings))
    }

    /// Change a control of an amp that is ONLINE (BEH-10). The value is checked against the range of
    /// the amp (SEC-2) and the user ceilings (SEC-3) before anything is sent; it is then written
    /// within [`Timing::write_interval`] of the previous value of the same control, and a newer value
    /// replaces one that is still waiting (SEC-4). What the amp answers arrives as [`Update::Changed`]
    /// or [`Update::WriteFailed`].
    pub fn set(&self, id: AmpId, param: Param, value: Value) -> Result<()> {
        let session = self.session(id)?;
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;
        if !dsp::controllable(&param) {
            return Err(Error::NotControllable);
        }
        // a muted output is never the dangerous state: muting is always tried, and the amp has the last word
        if !(matches!(param, Param::Mute(_)) && value == Value::Bool(true)) {
            self.permit_channel(id, param.channel())?;
        }
        let spec = session.amp.spec(&param).ok_or(Error::NotControllable)?;
        let checked = dsp::check(&param, &spec, &value, &ceilings)?;
        let targets = self.route(id, &session, &param)?;
        if let Some(amp) = lock(&self.0.shared.live).get_mut(&id) {
            amp.pending.extend(targets.iter().map(|target| (target.clone(), checked.clone())));
        }
        session.writer.push_all(targets.into_iter().map(|target| (target, checked.clone())));
        self.0.shared.emit(Update::Pending(id, param));
        Ok(())
    }

    /// The user ceilings of an amp (SEC-3). Nothing is written to the amp: a control that is above
    /// a lowered ceiling stays where it is until the user moves it.
    pub fn set_ceilings(&self, id: AmpId, ceilings: Ceilings) -> Result<()> {
        let set = self.0.shared.store().set_ceilings(id, ceilings);
        if set.is_ok() {
            self.0.shared.emit(Update::Settings(id));
        }
        set
    }

    fn session(&self, id: AmpId) -> Result<Session> {
        let live = lock(&self.0.shared.live);
        live.get(&id).ok_or(Error::UnknownAmp)?.session.clone().ok_or(Error::NotOnline)
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

    /// Put the amp at `position` (0-based) in the list every view shows (BEH-45).
    pub fn move_amp(&self, id: AmpId, position: usize) -> Result<()> {
        self.0.shared.store().move_to(id, position)?;
        self.0.shared.emit(Update::Listed);
        Ok(())
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
        self.scan(self.enabled())
    }

    fn enabled(&self) -> Vec<Protocol> {
        let switches = self.switches();
        [Protocol::Ipd, Protocol::Ipx].into_iter().filter(|p| switches.allows(*p)).collect()
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
                    let live = Live {
                        status,
                        meters: Meters::default(),
                        values: HashMap::new(),
                        loaded: false,
                        modified: false,
                        diverged: [false; 2],
                        access: None,
                        session: None,
                        pending: HashMap::new(),
                    };
                    lock(&shared.live).insert(amp.id, live);
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
async fn session(
    shared: &Arc<Shared>,
    id: AmpId,
    now: &Target,
    target: &mut watch::Receiver<Target>,
) -> bool {
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
                // an amp still named by default is named after its model
                if shared.store().name_by_model(id, &link.model).unwrap_or(false) {
                    shared.emit(Update::Settings(id));
                }
                let writer = Arc::new(Writer::new(timing.write_interval));
                let gate = Arc::new(tokio::sync::Mutex::new(()));
                let session = Session { amp: link.amp.clone(), writer: writer.clone(), gate: gate.clone() };
                shared.attach(id, session, link.access.take());
                shared.apply(id, Event::Connected);
                // the helpers end with the connection: what they still hold is dropped, never sent later
                let mut helpers = JoinSet::new();
                helpers.spawn(load_state(shared.clone(), id, link.amp.clone(), writer.clone()));
                helpers.spawn(write_loop(shared.clone(), id, link.amp.clone(), writer, gate.clone()));
                helpers.spawn(poll_mutes(shared.clone(), id, link.amp.clone(), gate.clone()));
                let changed = tokio::select! {
                    changed = target.changed() => Some(changed.is_ok()),
                    () = pump(shared, id, &mut link.events) => None,
                };
                shared.detach(id);
                helpers.abort_all();
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
    access: Option<Access>,
    model: String,
}

/// An amp is ONLINE when it is connected and its state could be read.
async fn connect_and_read(shared: &Shared, target: &Target) -> labgruppen::Result<Link> {
    let amp = shared.connector.connect(target.protocol, &target.ip).await?;
    let events = amp.subscribe();
    // the level and rights are read before the amp is ONLINE, so that whoever sees ONLINE finds them
    match (amp.status().await, amp.access().await) {
        (Ok(status), Ok(access)) => {
            let slots = amp.preset_slots();
            // an IPD can say a name and no number (0): the preset is shown by its name until the number is
            // found among the stored ones ([`resolve_preset`])
            let preset = status
                .preset
                .filter(|p| slots.contains(&p.index) || !p.name.trim().is_empty())
                .map(|p| Preset { number: if slots.contains(&p.index) { p.index } else { 0 }, name: p.name });
            Ok(Link { amp, events, preset, access, model: status.model })
        }
        (Err(e), _) | (_, Err(e)) => {
            amp.close().await;
            Err(e)
        }
    }
}

/// Read the whole DSP state (reads only, SEC-1). It takes a while on an IPD, which is why the amp is
/// ONLINE before it is done.
///
/// A read that fails is tried again, as slowly as a connection is: link, copy, mode and the mute polling
/// wait for it. It ends with the connection, whose helpers are aborted with it.
async fn load_state(shared: Arc<Shared>, id: AmpId, amp: Arc<dyn labgruppen::Amp>, token: Arc<Writer>) {
    let mut delay = shared.timing.retry_first;
    loop {
        if let Ok(snapshot) = amp.snapshot().await {
            shared.load(id, &token, snapshot);
            return;
        }
        sleep(delay).await;
        delay = (delay * 2).min(shared.timing.retry_max);
    }
}

/// The IPD tells nothing of a mute made on its front panel, so the output mutes are asked for about
/// as often as the meters arrive (BEH-12). The IPX tells by itself, but not its device mode, which both
/// are asked for about once a second (BEH-44).
async fn poll_mutes(
    shared: Arc<Shared>,
    id: AmpId,
    amp: Arc<dyn labgruppen::Amp>,
    gate: Arc<tokio::sync::Mutex<()>>,
) {
    let ipd = amp.info().driver == "ipd";
    let with_mode = amp.params().contains(&Param::Mode);
    if !ipd && !with_mode {
        return;
    }
    // an IPD answers one thing at a time: the polling waits until the state is read, and never runs during
    // a bulk change, which holds the gate (it would make both slow, and the bulk change time out)
    if !wait_loaded(&shared, id).await {
        return;
    }
    let outputs = [Param::Mute(Channel::Output(0)), Param::Mute(Channel::Output(1))];
    // the device mode is changed on the front panel now and then: asked for once a second
    let period = if ipd { shared.timing.mute_poll_interval } else { Duration::from_secs(1) };
    let mode_every = (Duration::from_secs(1).as_millis() / period.as_millis().max(1)).max(1);
    let mut tick = interval(period);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut round = 0u128;
    loop {
        tick.tick().await;
        let mut asked = if ipd { outputs.to_vec() } else { Vec::new() };
        if with_mode && round.is_multiple_of(mode_every) {
            asked.push(Param::Mode);
        }
        round += 1;
        let _gate = gate.lock().await;
        if let Ok(Ok(values)) = timeout(Duration::from_secs(2), amp.get_many(&asked)).await {
            for (param, value) in asked.iter().zip(values) {
                shared.observed(id, param, &value, false);
            }
        }
    }
}

/// Wait until the state of the amp is read: an IPD answers one thing at a time, so what is not needed at
/// once waits for it. Gives up, with `false`, when the amp has no connection (lost, disconnected or removed).
async fn wait_loaded(shared: &Shared, id: AmpId) -> bool {
    loop {
        match lock(&shared.live).get(&id) {
            Some(amp) if amp.loaded => return true,
            Some(amp) if amp.session.is_some() => {}
            _ => return false,
        }
        sleep(shared.timing.meter_interval).await;
    }
}

/// Write the values that [`Hub::set`] accepted, each at its turn (SEC-4).
async fn write_loop(
    shared: Arc<Shared>,
    id: AmpId,
    amp: Arc<dyn labgruppen::Amp>,
    writer: Arc<Writer>,
    gate: Arc<tokio::sync::Mutex<()>>,
) {
    loop {
        let next = {
            // a bulk change holds the gate and drops what waited: take the batch only once it is ours
            let _gate = gate.lock().await;
            let (due, next) = writer.take_due();
            if !due.is_empty() {
                let results = amp.set_many(&due).await;
                for ((param, value), result) in due.into_iter().zip(results) {
                    shared.settle(id, &param, &value);
                    match result {
                        Ok(()) => shared.observed(id, &param, &value, true),
                        Err(e) => shared.emit(Update::WriteFailed(id, param, e.to_string())),
                    }
                }
                continue;
            }
            next
        };
        match next {
            Some(wait) => {
                tokio::select! {
                    () = sleep(wait) => {}
                    () = writer.wake.notified() => {}
                }
            }
            None => writer.wake.notified().await,
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
                Ok(AmpEvent::Changed(param, value)) => shared.observed(id, &param, &value, false),
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
