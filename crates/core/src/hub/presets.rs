//! Presets and settings files (BEH-36, BEH-37), and the runner for bulk changes that all of them
//! share (SEC-6, SEC-7).
//!
//! A bulk change replaces many settings at once. The runner reads the mute of both outputs, mutes
//! those that are not, lets the change happen, then puts the mutes right, and no output ends up
//! unmuted that was muted before. If anything fails along the way, the outputs stay muted and the
//! error says so: a muted output is never the dangerous state.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use labgruppen::{Channel, Param, Snapshot, Value};
use tokio::time::timeout;

use super::{Hub, Session, load_state};
use crate::dsp;
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

/// What the change does to the mutes the amp holds afterwards.
#[derive(Clone, Copy)]
enum Effect {
    /// The amp swaps its whole configuration, mutes included (a recall): what it holds afterwards is
    /// the preset's. An output ends muted if it was or the preset mutes it.
    Replaces,
    /// The mutes are left as the runner set them (a file that carries no output mute): every output
    /// goes back to what it was.
    Keeps,
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
        let list = session.amp.presets().await?;
        Ok(list.into_iter().map(|p| Preset { number: p.index, name: p.name }).collect())
    }

    /// Recall a stored preset (BEH-37). The outputs are muted first and put back after; an output
    /// that was muted stays muted even if the preset says otherwise (SEC-6, SEC-7). Afterwards the
    /// preset is the loaded one, the state is read again, and MODIFIED is cleared (BEH-38).
    pub async fn recall_preset(&self, id: AmpId, slot: u32) -> Result<()> {
        let session = self.session(id)?;
        self.check_slot(&session, slot)?;
        self.bulk(id, &session, Effect::Replaces, |amp| async move { amp.recall_preset(slot).await }).await?;

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
        self.check_slot(&session, slot)?;
        let _gate = session.gate.lock().await;
        session.amp.store_preset(slot, name).await?;
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
        let text = tokio::fs::read_to_string(path).await?;
        let snapshot: Snapshot =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("Not a settings file: {e}")))?;
        let ceilings = self.0.shared.store().get(id).ok_or(Error::UnknownAmp)?.ceilings;
        let (accepted, skipped) = plan_load(&session, &snapshot, &ceilings)?;

        let report = self
            .bulk(id, &session, Effect::Keeps, |amp| async move {
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
    /// 1. wait for the amp's writes to finish and drop the values still waiting: they were meant for
    ///    settings that are about to be replaced;
    /// 2. read the mutes, and mute the outputs that are not muted;
    /// 3. `change`;
    /// 4. read the mutes again and put them right: see [`Effect`].
    ///
    /// Any failure stops there and leaves the outputs as they are, muted.
    async fn bulk<T, F, Fut>(&self, id: AmpId, session: &Session, effect: Effect, change: F) -> Result<T>
    where
        F: FnOnce(Arc<dyn labgruppen::Amp>) -> Fut,
        Fut: Future<Output = labgruppen::Result<T>>,
    {
        let _gate = session.gate.lock().await;
        session.writer.clear();
        let amp = &session.amp;
        let left_muted = |e: labgruppen::Error| Error::LeftMuted(e.to_string());

        // nothing is written if the mutes cannot even be read
        let before = read_mutes(amp).await?;
        let to_mute: Vec<(Param, Value)> =
            (0..2).filter(|&i| !before[i]).map(|i| (OUTPUTS[i].clone(), Value::Bool(true))).collect();
        for result in amp.set_many(&to_mute).await {
            result.map_err(left_muted)?;
        }

        let done = change(amp.clone()).await.map_err(left_muted)?;

        let after = read_mutes(amp).await.map_err(left_muted)?;
        let want: [bool; 2] = match effect {
            Effect::Keeps => before,
            Effect::Replaces => [before[0] || after[0], before[1] || after[1]],
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
fn plan_load(session: &Session, snapshot: &Snapshot, ceilings: &Ceilings) -> Result<(Snapshot, Vec<String>)> {
    if snapshot.format != Snapshot::FORMAT {
        return Err(Error::Invalid(format!(
            "Not a settings file this version reads (format {})",
            snapshot.format
        )));
    }
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
        if !dsp::controllable(&param) {
            skipped.push(format!("{name}: cannot be set from a file yet"));
            continue;
        }
        let Some(spec) = session.amp.spec(&param).filter(|spec| spec.writable) else {
            skipped.push(format!("{name}: this amplifier does not have it"));
            continue;
        };
        match dsp::check(&param, &spec, value, ceilings) {
            Ok(checked) => {
                accepted.params.insert(name.clone(), checked);
            }
            Err(e) => refused.push(format!("{name}: {e}")),
        }
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
