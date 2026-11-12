//! The interface every amp driver implements.

use std::any::Any;
use std::ops::RangeInclusive;

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::error::{Error, Result};
use crate::model::*;

/// Check a preset name for both families (the vendor apps refuse "Empty" and odd characters) and
/// return it trimmed. `max` is the most characters the amp can hold.
pub(crate) fn check_preset_name(name: &str, max: usize) -> Result<String> {
    let name = name.trim();
    let bad = |why: &str| Error::InvalidValue(format!("preset name {name:?}: {why}"));
    if name.is_empty() || name.eq_ignore_ascii_case("empty") {
        return Err(bad("must not be empty or \"Empty\""));
    }
    if name.len() > max {
        return Err(bad(&format!("longer than {max} characters")));
    }
    if !name.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err(bad("only printable ASCII characters"));
    }
    Ok(name.to_string())
}

/// A connected amplifier.
///
/// Drivers translate the common [`Param`] vocabulary to their wire protocol. Anything a driver does
/// not support is simply absent from [`Amp::params`]. Driver-specific features (IPD users and
/// standby, IPX raw addresses) are available on the concrete types through [`Amp::as_any`].
#[async_trait]
pub trait Amp: Send + Sync {
    fn info(&self) -> &AmpInfo;

    /// All parameters this amp supports, in a stable order.
    fn params(&self) -> Vec<Param>;

    /// Type, unit and range of a parameter; `None` when this amp does not have it.
    fn spec(&self, param: &Param) -> Option<ParamSpec>;

    async fn get(&self, param: &Param) -> Result<Value>;

    /// Write a value. Returns once the amp confirmed it (IPD: echo, IPX: read back).
    async fn set(&self, param: &Param, value: Value) -> Result<()>;

    /// Write several parameters; one result per item. Drivers override this to pipeline the writes.
    async fn set_many(&self, items: &[(Param, Value)]) -> Vec<Result<()>> {
        let mut out = Vec::with_capacity(items.len());
        for (p, v) in items {
            out.push(self.set(p, v.clone()).await);
        }
        out
    }

    async fn status(&self) -> Result<Status>;

    /// The current user level and what it may do; for the IPX `Locked` with no rights or `Unlocked` with all.
    async fn access(&self) -> Result<Option<Access>> {
        Ok(None)
    }

    /// Log in with a password and return the access the amp gives afterwards. The amp does not say
    /// whether the password was right: a wrong one leaves the level as it was. The password is used for
    /// this call only. Errors: [`Error::Unsupported`] for amps without user levels.
    async fn login(&self, _password: &str) -> Result<Access> {
        Err(Error::Unsupported("user levels".into()))
    }

    /// Lock or unlock the amp and return the access it has afterwards (BEH-47). Errors: [`Error::Denied`] for
    /// unlocking an IPD, which takes its password ([`Amp::login`]), [`Error::NotConfirmed`] when the amp does
    /// not take it.
    async fn set_locked(&self, _locked: bool) -> Result<Access> {
        Err(Error::Unsupported("locking the amp".into()))
    }

    /// Change the password the amp unlocks with (BEH-48): 1 to 8 characters, the amp pads a short one. The
    /// amp does not say whether it took it, and the password is not read back. Errors:
    /// [`Error::Unsupported`] for the IPX.
    async fn change_password(&self, _new: &str) -> Result<()> {
        Err(Error::Unsupported("a password".into()))
    }

    /// The front-panel display (BEH-49); `None` for an amp that does not have the setting.
    async fn display(&self) -> Result<Option<DisplayMode>> {
        Ok(None)
    }

    /// Set the front-panel display (BEH-49). Returns once the amp confirmed it. Errors:
    /// [`Error::Unsupported`], [`Error::NotConfirmed`].
    async fn set_display(&self, _display: DisplayMode) -> Result<()> {
        Err(Error::Unsupported("the display setting".into()))
    }

    /// The address and mask the amp is set to (BEH-50). Errors: [`Error::Unsupported`] for the IPX.
    async fn network(&self) -> Result<Network> {
        Err(Error::Unsupported("network settings".into()))
    }

    /// Write the network settings and tell the amp to use them (BEH-50). The amp restarts its network and
    /// drops the connection, so this ends with the amp closed. The values are checked by the caller.
    /// Errors: [`Error::Unsupported`] for the IPX, [`Error::NotConfirmed`] when the address or mask is not
    /// taken (then the amp is told nothing).
    async fn set_network(&self, _network: Network) -> Result<()> {
        Err(Error::Unsupported("network settings".into()))
    }

    /// Usable preset slots, numbered as on the amp.
    fn preset_slots(&self) -> RangeInclusive<u32>;

    /// Presets stored on the amp, in [`Amp::preset_slots`].
    async fn presets(&self) -> Result<Vec<PresetInfo>>;

    /// Load a stored preset. This changes the audio settings, including mutes.
    /// Slots outside [`Amp::preset_slots`] are rejected with [`Error::InvalidValue`].
    async fn recall_preset(&self, index: u32) -> Result<()>;

    /// Store the amp's current settings in `slot` under `name`, replacing what is there. Returns once
    /// the amp lists the new name. When the slot already had that name, the IPX also has to report
    /// the current settings for the slot; the IPD only has its echo of the save command.
    ///
    /// Errors: [`Error::InvalidValue`] for a slot outside [`Amp::preset_slots`] or a name the amp
    /// cannot hold, [`Error::Denied`] for a protected slot or missing access right,
    /// [`Error::NotConfirmed`] when the amp does not take it.
    async fn store_preset(&self, slot: u32, name: &str) -> Result<()>;

    /// The name as [`Amp::store_preset`] would store it, or [`Error::InvalidValue`] when the amp cannot
    /// hold it: asked first by whoever must protect something before a store. The default holds what the
    /// shortest name allows.
    fn check_preset_name(&self, name: &str) -> Result<String> {
        check_preset_name(name, 15)
    }

    /// Latest meter readings.
    fn meters(&self) -> Meters;

    /// The temperature the amp reports now, in percent as the vendor app shows it; `None` when the amp has
    /// no reading to give.
    async fn temperature(&self) -> Result<Option<i32>> {
        Ok(None)
    }

    fn subscribe(&self) -> broadcast::Receiver<AmpEvent>;

    async fn close(&self);

    fn as_any(&self) -> &dyn Any;

    /// Read several parameters. Drivers override this when requests can be pipelined.
    async fn get_many(&self, params: &[Param]) -> Result<Vec<Value>> {
        let mut out = Vec::with_capacity(params.len());
        for p in params {
            out.push(self.get(p).await?);
        }
        Ok(out)
    }

    /// Read every parameter. Drivers override this when the amp has a bulk read.
    async fn snapshot(&self) -> Result<Snapshot> {
        let info = self.info();
        let mut snap = Snapshot::new(info.driver, &info.model);
        for p in self.params() {
            snap.params.insert(p.to_string(), self.get(&p).await?);
        }
        Ok(snap)
    }

    /// Write the writable parameters of `snap` that this amp has.
    async fn restore(&self, snap: &Snapshot, dry_run: bool) -> Result<RestoreReport> {
        if snap.format != Snapshot::FORMAT {
            return Err(Error::Parse(format!("unsupported snapshot format {}", snap.format)));
        }
        let mut report = RestoreReport::default();
        let mut todo: Vec<(String, Param, Value)> = Vec::new();
        for (name, value) in &snap.params {
            let Ok(param) = name.parse::<Param>() else {
                report.skipped.push(name.clone());
                continue;
            };
            // a file must not silently re-route the unit
            let Some(spec) = self.spec(&param).filter(|s| s.writable && param != Param::Mode) else {
                report.skipped.push(name.clone());
                continue;
            };
            let limiter = matches!(param, Param::Limiter { .. });
            // the limiter threshold is dBV on an IPD and Vpk on an IPX: a number from the other family is no
            // threshold
            if limiter && snap.driver != self.info().driver {
                report.skipped.push(name.clone());
                continue;
            }
            // a limiter a hair beyond the range, as the amp itself stores it, is taken at the edge
            let value = match value {
                Value::Float(x) if limiter => &Value::Float(spec.near(*x)),
                _ => value,
            };
            match spec.check(&name, value) {
                Ok(v) => todo.push((name.clone(), param, v)),
                Err(e) => report.failed.push((name.clone(), e.to_string())),
            }
        }
        if dry_run {
            report.applied = todo.len();
            return Ok(report);
        }
        // Order matters: mute first and unmute last, and switch filters on/off after their other
        // settings (the IPX turns a filter on when its type is written).
        todo.sort_by_key(|(_, p, v)| match (p, v) {
            (Param::Mute(_), Value::Bool(true)) => 0,
            (
                Param::Eq { field: EqField::Enabled, .. } | Param::Xover { field: XoverField::Enabled, .. },
                _,
            ) => 2,
            (Param::Mute(_) | Param::Mixer { field: MixerField::Mute, .. }, _) => 3,
            _ => 1,
        });
        let items: Vec<(Param, Value)> = todo.iter().map(|(_, p, v)| (p.clone(), v.clone())).collect();
        for ((name, ..), result) in todo.iter().zip(self.set_many(&items).await) {
            match result {
                Ok(()) => report.applied += 1,
                Err(e) => report.failed.push((name.clone(), e.to_string())),
            }
        }
        Ok(report)
    }
}
