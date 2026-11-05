//! The one amplifier list every page reads (BEH-39), saved to the config after every change.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{self, Entry, File};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Ipd,
    Ipx,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Ipd => "IPD",
            Protocol::Ipx => "IPX",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Auto,
    Manual,
}

/// Stable for the session, unlike the index; never reused. Intents name an amp by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AmpId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub number: u32,
    pub name: String,
}

/// User safety ceilings (SEC-3). `None`: no ceiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ceilings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_gain_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_limiter_dbu: Option<f64>,
}

/// Which protocols discovery searches (BEH-17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Switches {
    pub ipd: bool,
    pub ipx: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Switches { ipd: true, ipx: true }
    }
}

impl Switches {
    pub fn allows(self, protocol: Protocol) -> bool {
        match protocol {
            Protocol::Ipd => self.ipd,
            Protocol::Ipx => self.ipx,
        }
    }

    /// Both off: scanning is disabled.
    pub fn none(self) -> bool {
        !self.ipd && !self.ipx
    }
}

#[derive(Clone, Debug)]
pub struct Amp {
    pub id: AmpId,
    pub name: String,
    pub protocol: Protocol,
    /// Text as entered: an edited IP may not be valid (CHECK IP, BEH-8). Valid ones are canonical.
    pub ip: String,
    pub source: Source,
    /// False after DISCONNECT (BEH-9).
    pub connected: bool,
    /// IPD load per output in ohm (BEH-5).
    pub load_ohms: [f64; 2],
    pub ceilings: Ceilings,
    /// The links the app keeps (inputs, outputs): only an IPD uses them, as its own flag mirrors more
    /// than a link shares (BEH-32). An IPX keeps its link in the amp.
    pub links: [bool; 2],
    /// Not saved: the app only knows it while connected.
    pub preset: Option<Preset>,
}

impl Amp {
    pub fn ip_is_valid(&self) -> bool {
        self.ip.parse::<Ipv4Addr>().is_ok()
    }
}

pub struct Store {
    path: Option<PathBuf>,
    amps: Vec<Amp>,
    switches: Switches,
    next_id: u64,
}

impl Store {
    /// A list that is not saved anywhere.
    pub fn in_memory() -> Store {
        Store { path: None, amps: Vec::new(), switches: Switches::default(), next_id: 0 }
    }

    /// `labrack/config.toml` in the user's config directory.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("labrack").join("config.toml"))
    }

    /// Reads the config at `path`, then saves every change there. Reading never writes.
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let path = path.as_ref().to_path_buf();
        let file = config::read(&path)?;
        let mut store = Store { path: Some(path), switches: file.switches, ..Store::in_memory() };
        for entry in file.amps {
            let id = store.new_id();
            store.amps.push(entry.into_amp(id));
        }
        Ok(store)
    }

    /// In list order; the index of an amp is its position + 1.
    pub fn amps(&self) -> &[Amp] {
        &self.amps
    }

    pub fn get(&self, id: AmpId) -> Option<&Amp> {
        self.amps.iter().find(|a| a.id == id)
    }

    /// 1-based position in the list.
    pub fn index(&self, id: AmpId) -> Option<usize> {
        self.amps.iter().position(|a| a.id == id).map(|i| i + 1)
    }

    pub fn switches(&self) -> Switches {
        self.switches
    }

    pub fn set_switches(&mut self, switches: Switches) -> Result<()> {
        self.switches = switches;
        self.save()
    }

    /// An empty name becomes "Amp n". The IP must be a valid IPv4 address no other amp has.
    pub fn add_manual(&mut self, name: &str, protocol: Protocol, ip: &str) -> Result<AmpId> {
        let ip = self.check_new_ip(ip, None)?;
        self.push(name, protocol, ip, Source::Manual)
    }

    /// Adds an amp that discovery found, with a default name. `None`: the IP is already in the list.
    pub fn add_discovered(&mut self, protocol: Protocol, ip: Ipv4Addr) -> Result<Option<AmpId>> {
        match self.check_new_ip(&ip.to_string(), None) {
            Ok(ip) => self.push("", protocol, ip, Source::Auto).map(Some),
            Err(Error::DuplicateIp) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn push(&mut self, name: &str, protocol: Protocol, ip: String, source: Source) -> Result<AmpId> {
        let name = match name.trim() {
            "" => self.default_name(),
            name => name.to_string(),
        };
        let id = self.new_id();
        self.amps.push(Amp {
            id,
            name,
            protocol,
            ip,
            source,
            connected: true,
            load_ohms: config::default_load(),
            ceilings: Ceilings::default(),
            links: [false; 2],
            preset: None,
        });
        self.save()?;
        Ok(id)
    }

    /// An empty name is rejected and the old one stays.
    pub fn rename(&mut self, id: AmpId, name: &str) -> Result<()> {
        let name = name.trim();
        let amp = self.get_mut(id)?;
        if name.is_empty() {
            return Err(Error::EmptyName);
        }
        amp.name = name.to_string();
        self.save()
    }

    pub fn remove(&mut self, id: AmpId) -> Result<()> {
        let at = self.amps.iter().position(|a| a.id == id).ok_or(Error::UnknownAmp)?;
        self.amps.remove(at);
        self.save()
    }

    /// Text that is not a valid IPv4 address is kept (the amp shows CHECK IP). An address another
    /// amp has is rejected and the old IP stays.
    pub fn set_ip(&mut self, id: AmpId, ip: &str) -> Result<()> {
        self.get_mut(id)?;
        let ip = match self.check_new_ip(ip, Some(id)) {
            Ok(ip) => ip,
            Err(Error::InvalidIp) => ip.trim().to_string(),
            Err(e) => return Err(e),
        };
        self.get_mut(id)?.ip = ip;
        self.save()
    }

    pub fn set_connected(&mut self, id: AmpId, connected: bool) -> Result<()> {
        self.get_mut(id)?.connected = connected;
        self.save()
    }

    /// The user ceilings of an amp (SEC-3); NaN and infinity are refused.
    pub fn set_ceilings(&mut self, id: AmpId, ceilings: Ceilings) -> Result<()> {
        for value in [ceilings.max_output_gain_db, ceilings.max_limiter_dbu].into_iter().flatten() {
            if !value.is_finite() {
                return Err(Error::Invalid(format!("A ceiling must be a number, not {value}")));
            }
        }
        self.get_mut(id)?.ceilings = ceilings;
        self.save()
    }

    /// The load of an IPD output in ohm (BEH-5, BEH-40), which only scales the watts it shows.
    pub fn set_load(&mut self, id: AmpId, output: u8, ohm: f64) -> Result<()> {
        let (min, max) = labgruppen::limiter::OHM_RANGE;
        if !(min..=max).contains(&ohm) {
            return Err(Error::Invalid(format!("The load must be between {min} and {max} Ohm")));
        }
        let amp = self.get_mut(id)?;
        *amp.load_ohms.get_mut(usize::from(output)).ok_or(Error::NotControllable)? = ohm;
        self.save()
    }

    /// Whether the app keeps the pair linked (`side` is `Side as usize`).
    pub fn set_link(&mut self, id: AmpId, side: usize, linked: bool) -> Result<()> {
        *self.get_mut(id)?.links.get_mut(side).ok_or(Error::NotControllable)? = linked;
        self.save()
    }

    pub fn set_preset(&mut self, id: AmpId, preset: Option<Preset>) -> Result<()> {
        self.get_mut(id)?.preset = preset;
        Ok(())
    }

    fn get_mut(&mut self, id: AmpId) -> Result<&mut Amp> {
        self.amps.iter_mut().find(|a| a.id == id).ok_or(Error::UnknownAmp)
    }

    fn new_id(&mut self) -> AmpId {
        self.next_id += 1;
        AmpId(self.next_id)
    }

    /// The canonical text of `ip`, if it is valid and no amp except `except` has it.
    fn check_new_ip(&self, ip: &str, except: Option<AmpId>) -> Result<String> {
        let ip: Ipv4Addr = ip.trim().parse().map_err(|_| Error::InvalidIp)?;
        let taken = self.amps.iter().any(|a| Some(a.id) != except && a.ip.parse() == Ok(ip));
        if taken { Err(Error::DuplicateIp) } else { Ok(ip.to_string()) }
    }

    fn default_name(&self) -> String {
        let taken = |n: usize| self.amps.iter().any(|a| a.name == format!("Amp {n}"));
        let n = (1..).find(|&n| !taken(n)).expect("endless range");
        format!("Amp {n}")
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let file = File { switches: self.switches, amps: self.amps.iter().map(Entry::from_amp).collect() };
        config::write(path, &file)
    }
}
