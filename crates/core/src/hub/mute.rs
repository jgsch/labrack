//! Mute (BEH-24 to BEH-26). A mute is a control like any other (`Hub::set`); what is added here is
//! one per amp, one for all, and the state the buttons need to label themselves. The mute state is
//! the DSP state every view reads, so it is the same on all of them.

use labgruppen::{Channel, Param, Value};

use super::{Hub, lock};
use crate::Result;
use crate::store::AmpId;

const OUTPUTS: u8 = 2;

impl Hub {
    /// MUTE / UNMUTE of one output (`output` is 0 or 1).
    pub fn set_mute(&self, id: AmpId, output: u8, muted: bool) -> Result<()> {
        self.set(id, Param::Mute(Channel::Output(output)), Value::Bool(muted))
    }

    /// MUTE / UNMUTE of both outputs of an amp. Both are tried; the first error is returned.
    pub fn mute_amp(&self, id: AmpId, muted: bool) -> Result<()> {
        let results: Vec<_> = (0..OUTPUTS).map(|output| self.set_mute(id, output, muted)).collect();
        results.into_iter().collect()
    }

    /// MUTE ALL: both outputs of every amp that is ONLINE (BEH-25). Returns how many amps that is.
    /// Nothing is kept for the others.
    pub fn mute_all(&self) -> usize {
        self.mute_every(true)
    }

    /// UNMUTE ALL, on the amps that are ONLINE. The confirmation is up to the caller (SEC-5).
    pub fn unmute_all(&self) -> usize {
        self.mute_every(false)
    }

    fn mute_every(&self, muted: bool) -> usize {
        let online = self.online();
        self.set_all_muted(muted);
        online.len()
    }

    /// MUTE ALL or UNMUTE ALL on the amps that are ONLINE, saying which amps refused and why: an amp that
    /// is busy with a bulk change refuses an unmute (SEC-6), and the user should hear of it.
    pub fn set_all_muted(&self, muted: bool) -> Vec<(AmpId, crate::Error)> {
        self.online().into_iter().filter_map(|id| self.mute_amp(id, muted).err().map(|e| (id, e))).collect()
    }

    /// Both outputs of the amp are muted, as far as the app knows: the label of its button.
    pub fn amp_muted(&self, id: AmpId) -> bool {
        let live = lock(&self.0.shared.live);
        let Some(amp) = live.get(&id) else { return false };
        (0..OUTPUTS)
            .all(|output| amp.values.get(&Param::Mute(Channel::Output(output))) == Some(&Value::Bool(true)))
    }

    /// There is an amp ONLINE and all of them are muted: MUTE ALL becomes UNMUTE ALL.
    pub fn all_muted(&self) -> bool {
        let online = self.online();
        !online.is_empty() && online.into_iter().all(|id| self.amp_muted(id))
    }

    /// The amps that are ONLINE, in list order.
    fn online(&self) -> Vec<AmpId> {
        let live = lock(&self.0.shared.live);
        self.0
            .shared
            .store()
            .amps()
            .iter()
            .map(|amp| amp.id)
            .filter(|id| live.get(id).is_some_and(|amp| amp.session.is_some()))
            .collect()
    }
}
