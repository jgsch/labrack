//! Presets and settings files (BEH-36, BEH-37), and the runner for bulk changes that all of them
//! share (SEC-6, SEC-7).
//!
//! A bulk change replaces many settings at once. Recalling a preset, loading a file and storing a
//! preset mute both outputs first and leave them muted: unmuting is the user's decision, never the
//! app's, so no preset or file can make sound by itself. A preset the app stored carries muted
//! outputs, so recalling it has no unmuted moment at all; one stored by another tool may unmute an
//! output for the round trip the app needs to mute it again. Linking puts the mutes back as they were,
//! since it changes no scene. If anything fails along the way, the outputs stay muted and the error
//! says so: a muted output is never the dangerous state.

use std::future::Future;
use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use labgruppen::{Channel, Param, Snapshot, Value};
use tokio::time::timeout;

use super::{Hub, Session, load_state};
use crate::dsp;
use crate::link::{self, Side};
use crate::store::{AmpId, Ceilings, Preset};
use crate::{Error, Result};

/// What a settings file did (BEH-37).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadReport {
    /// Settings written.
    pub applied: usize,
    /// Entries of the file that were not written, with the reason: output mutes (they are never
    /// changed by a file), controls that cannot be set yet, and ones this amplifier does not have.
    pub skipped: Vec<String>,
}

/// Where the outputs are left after a bulk change.
#[derive(Clone, Copy)]
pub(super) enum End {
    /// Muted, whatever they were and whatever the change did (SEC-6).
    Muted,
    /// As they were before the change.
    AsBefore,
}

const OUTPUTS: [Param; 2] = [Param::Mute(Channel::Output(0)), Param::Mute(Channel::Output(1))];

/// The mute of both outputs, read from the amp.
async fn read_mutes(amp: &Arc<dyn labgruppen::Amp>) -> labgruppen::Result<[bool; 2]> {
    let values = timeout(Duration::from_secs(10), amp.get_many(&OUTPUTS))
        .await
        .map_err(|_| labgruppen::Error::Timeout("reading the output mutes".into()))??;
    let mute = |i: usize| {
        values
            .get(i)
            .and_then(Value::as_bool)
            .ok_or_else(|| labgruppen::Error::Protocol("no output mute in the answer".into()))
    };
    Ok([mute(0)?, mute(1)?])
}

impl Hub {
    /// The presets stored on the amp (BEH-36).
    pub async fn presets(&self, id: AmpId) -> Result<Vec<Preset>> {
        let session = self.session(id)?;
        // an IPD answers one thing at a time and reads these names slowly: after its state
        if !super::wait_loaded(&self.0.shared, id).await {
            return Err(Error::NotOnline);
        }
        let list = session.amp.presets().await?;
        let stored: Vec<Preset> =
            list.into_iter().map(|p| Preset { number: p.index, name: p.name }).collect();
        self.0.shared.resolve_preset(id, &stored);
        Ok(stored)
    }

    /// The slots of the amp's presets (BEH-6), for a view to list; `None` when it is not ONLINE.
    pub fn preset_slots(&self, id: AmpId) -> Option<RangeInclusive<u32>> {
        self.session(id).ok().map(|session| session.amp.preset_slots())
    }

    /// Recall a stored preset (BEH-37). The outputs are muted first and put back after; an output
    /// that was muted stays muted even if the preset says otherwise (SEC-6, SEC-7). Afterwards the
    /// preset is the loaded one, the state is read again, and MODIFIED is cleared (BEH-38).
    pub async fn recall_preset(&self, id: AmpId, slot: u32) -> Result<()> {
        let session = self.session(id)?;
        self.permit(id, |rights| rights.load_preset)?;
        self.check_slot(&session, slot)?;
        self.bulk(id, &session, End::Muted, true, |amp| async move { amp.recall_preset(slot).await }).await?;

        // what the amp says it holds now, not what was asked for
        let loaded = session.amp.status().await.ok().and_then(|s| s.preset);
        let shared = &self.0.shared;
        shared.set_loaded_preset(id, loaded.map(|p| Preset { number: p.index, name: p.name }));
        shared.set_modified(id, false);
        self.read_state_again(id, &session);
        Ok(())
    }

    /// Store the current settings in a slot under a name (BEH-37); it becomes the loaded preset.
    pub async fn store_preset(&self, id: AmpId, slot: u32, name: &str) -> Result<()> {
        let session = self.session(id)?;
        self.permit(id, |rights| rights.save_preset)?;
        self.check_slot(&session, slot)?;
        // a name the amp cannot hold is refused before the outputs are muted for it
        session.amp.check_preset_name(name)?;
        // the preset is stored with the outputs muted, and they stay muted. What waits to be written
        // stays too: it is not replaced by anything.
        let stored = name.to_string();
        self.bulk(
            id,
            &session,
            End::Muted,
            false,
            |amp| async move { amp.store_preset(slot, &stored).await },
        )
        .await?;
        let shared = &self.0.shared;
        shared.set_loaded_preset(id, Some(Preset { number: slot, name: name.trim().to_string() }));
        shared.set_modified(id, false);
        Ok(())
    }

    /// Save the settings of the amp as a JSON file: the library's snapshot (BEH-37).
    pub async fn save_file(&self, id: AmpId, path: impl AsRef<Path>) -> Result<()> {
        let session = self.session(id)?;
        let snapshot = session.amp.snapshot().await?;
        let json = serde_json::to_string_pretty(&snapshot).map_err(|e| Error::Invalid(e.to_string()))?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Load a settings file into the amp (BEH-37). The whole file is checked first, against the
    /// ranges of the amp and the user ceilings; if one value fails, nothing is written, not even a
    /// mute. Then the outputs are muted, the settings written, and the outputs put back. Output
    /// mutes in the file are never applied (SEC-7), and what cannot be set yet is skipped (see
    /// [`LoadReport`]). Afterwards the preset is MODIFIED: the file is not the stored preset.
    pub async fn load_file(&self, id: AmpId, path: impl AsRef<Path>) -> Result<LoadReport> {
        let session = self.session(id)?;
        // a file sets inputs and outputs, so it needs the right for both
        self.permit(id, |rights| rights.input_channel && rights.output_channel)?;
        let text = tokio::fs::read_to_string(path).await?;
        let snapshot: Snapshot =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("Not a settings file: {e}")))?;
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;
        let linked = [self.linked(id, Side::Inputs), self.linked(id, Side::Outputs)];
        let mirrors = [self.amp_keeps_equal(id, Side::Inputs), self.amp_keeps_equal(id, Side::Outputs)];
        let (accepted, skipped) = plan_load(&session, &snapshot, &ceilings, linked, mirrors)?;

        let report = self
            .bulk(id, &session, End::Muted, true, |amp| async move {
                let report = amp.restore(&accepted, false).await?;
                match report.failed.first() {
                    Some((name, why)) => Err(labgruppen::Error::NotConfirmed(format!(
                        "{} of {} settings were not taken, first {name}: {why}",
                        report.failed.len(),
                        accepted.params.len()
                    ))),
                    None => Ok(report),
                }
            })
            .await?;

        self.0.shared.set_modified(id, true);
        self.read_state_again(id, &session);
        Ok(LoadReport { applied: report.applied, skipped })
    }

    fn check_slot(&self, session: &Session, slot: u32) -> Result<()> {
        match session.amp.preset_slots().contains(&slot) {
            true => Ok(()),
            false => Err(Error::Invalid(format!("There is no preset {slot}"))),
        }
    }

    /// The settings of the amp changed under the cache: forget them and read them again.
    fn read_state_again(&self, id: AmpId, session: &Session) {
        self.0.shared.begin_reload(id);
        tokio::spawn(load_state(self.0.shared.clone(), id, session.amp.clone(), session.writer.clone()));
    }

    /// Run `change` on an amp with its outputs muted (SEC-6).
    ///
    /// 1. wait for the amp's writes to finish and, if `drop_waiting`, drop the values still waiting:
    ///    they were meant for settings that are about to be replaced;
    /// 2. read the mutes, and mute the outputs that are not muted;
    /// 3. `change`;
    /// 4. read the mutes again and put them right: see [`End`].
    ///
    /// Any failure stops there and leaves the outputs as they are, muted.
    pub(super) async fn bulk<T, F, Fut>(
        &self,
        id: AmpId,
        session: &Session,
        end: End,
        drop_waiting: bool,
        change: F,
    ) -> Result<T>
    where
        F: FnOnce(Arc<dyn labgruppen::Amp>) -> Fut,
        Fut: Future<Output = labgruppen::Result<T>>,
    {
        let _gate = session.gate.lock().await;
        // a MUTE the user asked for that is not written yet: dropping it with the rest must not lose it
        let asked = session.writer.muting();
        if drop_waiting {
            session.writer.clear();
            self.0.shared.drop_pending(id);
        }
        let amp = &session.amp;
        let left_muted = |e: labgruppen::Error| Error::LeftMuted(e.to_string());

        // nothing is written if the mutes cannot even be read
        let before = read_mutes(amp).await?;
        let to_mute: Vec<(Param, Value)> =
            (0..2).filter(|&i| !before[i]).map(|i| (OUTPUTS[i].clone(), Value::Bool(true))).collect();
        // the outputs as they were, and muted if the user asked for it meanwhile (SEC-6: a mute wins)
        let kept = |now: [bool; 2]| [0, 1].map(|i| before[i] || asked[i] || now[i]);
        for result in amp.set_many(&to_mute).await {
            result.map_err(left_muted)?;
        }

        let done = match change(amp.clone()).await {
            Ok(done) => done,
            // refused before anything was sent: the amp is as it was, so take back only the mute added here
            Err(refused @ (labgruppen::Error::Denied(_) | labgruppen::Error::InvalidValue(_))) => {
                let keep = kept(session.writer.muting());
                let undo: Vec<(Param, Value)> =
                    (0..2).filter(|&i| !keep[i]).map(|i| (OUTPUTS[i].clone(), Value::Bool(false))).collect();
                for result in amp.set_many(&undo).await {
                    result.map_err(left_muted)?;
                }
                for (param, muted) in OUTPUTS.iter().zip(keep) {
                    self.0.shared.observed(id, param, &Value::Bool(muted), false);
                }
                return Err(refused.into());
            }
            Err(e) => return Err(left_muted(e)),
        };

        // a change that is no preset or file leaves the mutes as this function set them: no need to ask the
        // amp again, which an IPD answers in seconds
        let after = match end {
            End::AsBefore => [true, true],
            End::Muted => read_mutes(amp).await.map_err(left_muted)?,
        };
        let want: [bool; 2] = match end {
            End::AsBefore => kept(session.writer.muting()),
            End::Muted => [true, true],
        };
        // mutes before unmutes
        let fix = |muted: bool| -> Vec<(Param, Value)> {
            (0..2)
                .filter(|&i| want[i] == muted && after[i] != muted)
                .map(|i| (OUTPUTS[i].clone(), Value::Bool(muted)))
                .collect()
        };
        for result in amp.set_many(&fix(true)).await.into_iter().chain(amp.set_many(&fix(false)).await) {
            result.map_err(left_muted)?;
        }

        for (param, muted) in OUTPUTS.iter().zip(want) {
            self.0.shared.observed(id, param, &Value::Bool(muted), false);
        }
        Ok(done)
    }
}

/// Check a settings file against the amp before anything is written: the part of it that can be
/// written, and why the rest cannot.
fn plan_load(
    session: &Session,
    snapshot: &Snapshot,
    ceilings: &Ceilings,
    linked: [bool; 2],
    mirrors: [bool; 2],
) -> Result<(Snapshot, Vec<String>)> {
    if snapshot.format != Snapshot::FORMAT {
        return Err(Error::Invalid(format!(
            "Not a settings file this version reads (format {})",
            snapshot.format
        )));
    }
    let same_family = snapshot.driver == session.amp.info().driver;
    let mut accepted = Snapshot::new(&snapshot.driver, &snapshot.model);
    let (mut skipped, mut refused) = (Vec::new(), Vec::new());
    for (name, value) in &snapshot.params {
        let Ok(param) = name.parse::<Param>() else {
            skipped.push(format!("{name}: unknown control"));
            continue;
        };
        if matches!(param, Param::Mute(Channel::Output(_))) {
            skipped.push(format!("{name}: output mutes are left as they are"));
            continue;
        }
        // the limiter threshold is dBV on an IPD and Vpk on an IPX: a number from the other family is no threshold
        if !same_family && matches!(param, Param::Limiter { .. }) {
            skipped.push(format!("{name}: its unit differs between IPD and IPX; set it on the amplifier"));
            continue;
        }
        if !dsp::restorable(&param) {
            skipped.push(format!("{name}: cannot be set from a file yet"));
            continue;
        }
        let Some(spec) = session.amp.spec(&param).filter(|spec| spec.writable) else {
            skipped.push(format!("{name}: this amplifier does not have it"));
            continue;
        };
        // the amp stores a hair more than its limiter range (36.02 dBu for the 35.99 dBu of 69 Vpk): its own
        // file loads with the nearest value in range. Anything further out is refused; the ceilings still
        // decide below.
        let value = match (value, matches!(param, Param::Limiter { .. })) {
            (Value::Float(x), true) => &Value::Float(near_range(*x, spec.min, spec.max)),
            _ => value,
        };
        let checked = match dsp::check(&param, &spec, value, ceilings) {
            Ok(checked) => checked,
            Err(e) => {
                refused.push(format!("{name}: {e}"));
                continue;
            }
        };
        // an IPD that links a pair itself mirrors all of channel 1 and ignores channel 2
        if let Some(side) = link::mirrored(&param)
            && mirrors[side as usize]
            && link::is_second(&param)
        {
            skipped.push(format!("{name}: the amplifier keeps channel 2 equal to channel 1"));
            continue;
        }
        // a linked pair shares its level: channel 2 is not read from the file, it gets channel 1's value
        if let Some(side) = link::follows(&param)
            && linked[side as usize]
        {
            if link::is_second(&param) {
                skipped.push(format!("{name}: follows channel 1 while the pair is linked"));
                continue;
            }
            if !mirrors[side as usize]
                && let Some(twin) = link::second(&param)
            {
                accepted.params.insert(twin.to_string(), checked.clone());
            }
        }
        accepted.params.insert(name.clone(), checked);
    }
    if !refused.is_empty() {
        let first: Vec<_> = refused.iter().take(3).cloned().collect();
        return Err(Error::Invalid(format!(
            "{} values in the file are not valid for this amplifier: {}",
            refused.len(),
            first.join("; ")
        )));
    }
    Ok((accepted, skipped))
}

/// `x`, or the edge of the range when it lies within 1 % of that edge (rounding in what the amp stores).
fn near_range(x: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let near = |edge: f64| (x - edge).abs() <= 0.01 * edge.abs().max(1.0);
    match (min, max) {
        (Some(min), _) if x < min && near(min) => min,
        (_, Some(max)) if x > max && near(max) => max,
        _ => x,
    }
}
