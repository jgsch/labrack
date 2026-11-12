//! IPD user levels and login (BEH-13 to BEH-16). The app reads the level and the rights of the amp
//! when it goes ONLINE and after a login, refuses what the rights do not allow before anything is
//! sent, and logs in on request. A password is passed to the amp for that one call: the app does not
//! keep it, does not write it down, and never asks the amp for one (SEC-8).

use std::sync::Arc;

use labgruppen::{Access, AccessRights, Channel};

use super::{Hub, Update, lock};
use crate::dsp::Writer;
use crate::store::AmpId;
use crate::{Error, Result};

/// A password is 1 to 8 printable ASCII characters; the amp pads a short one.
const PASSWORD_LENGTH: std::ops::RangeInclusive<usize> = 1..=8;

/// A password is 1 to 8 printable characters, checked before anything is sent.
fn check_password(password: &str) -> Result<()> {
    if !PASSWORD_LENGTH.contains(&password.chars().count())
        || !password.chars().all(|c| c.is_ascii_graphic() || c == ' ')
    {
        return Err(Error::Invalid("The password has 1 to 8 letters, digits or symbols".into()));
    }
    Ok(())
}

impl Hub {
    /// The user level of the amp and what it may do (BEH-13); an IPX is `Locked` or `Unlocked`. `None` when the
    /// amp is not ONLINE.
    pub fn access(&self, id: AmpId) -> Option<Access> {
        lock(&self.0.shared.live).get(&id)?.access.clone()
    }

    /// Log in with a password (BEH-15) and return the access the amp gives afterwards. The amp does
    /// not say whether the password was right: a wrong one leaves the level as it was. Views are told
    /// ([`Update::Access`]) when the level or the rights changed. The password is used for this call
    /// only (BEH-16).
    pub async fn login(&self, id: AmpId, password: &str) -> Result<Access> {
        let session = self.session(id)?;
        check_password(password)?;
        let access = session.amp.login(password).await?;
        self.0.shared.set_access(id, &session.writer, &access);
        Ok(access)
    }

    /// Lock or unlock the amp (BEH-47) and return the access it has afterwards. An IPX only; no password. The
    /// views are told ([`Update::Access`]).
    pub async fn set_locked(&self, id: AmpId, locked: bool) -> Result<Access> {
        let session = self.session(id)?;
        let access = session.amp.set_locked(locked).await?;
        self.0.shared.set_access(id, &session.writer, &access);
        Ok(access)
    }

    /// Change the password the amp unlocks with (BEH-48), while it is unlocked. The password is used for this
    /// call only and the amp is not asked for it (BEH-16).
    pub async fn change_password(&self, id: AmpId, new: &str) -> Result<()> {
        let session = self.session(id)?;
        check_password(new)?;
        self.permit(id, |rights| rights.complete())?;
        Ok(session.amp.change_password(new).await?)
    }

    /// Refuse an intent the rights of the amp do not allow (BEH-14). With no access known there is
    /// nothing to hold back.
    pub(super) fn permit(&self, id: AmpId, allowed: impl Fn(&AccessRights) -> bool) -> Result<()> {
        let live = lock(&self.0.shared.live);
        match live.get(&id).and_then(|amp| amp.access.as_ref()) {
            Some(access) if !allowed(&access.rights) => Err(Error::AccessDenied(access.user.clone())),
            _ => Ok(()),
        }
    }

    /// The right to change the settings of `channel`: the input or the output channels.
    pub(super) fn permit_channel(&self, id: AmpId, channel: Channel) -> Result<()> {
        self.permit(id, |rights| match channel {
            Channel::Input(_) => rights.input_channel,
            Channel::Output(_) => rights.output_channel,
        })
    }
}

impl super::Shared {
    /// The amp of the connection of `token` has this access now; views are told if it differs.
    pub(super) fn set_access(&self, id: AmpId, token: &Arc<Writer>, access: &Access) {
        let changed = {
            let mut live = lock(&self.live);
            let current =
                |amp: &super::Live| amp.session.as_ref().is_some_and(|s| Arc::ptr_eq(&s.writer, token));
            let Some(amp) = live.get_mut(&id).filter(|amp| current(amp)) else { return };
            let changed = amp.access.as_ref() != Some(access);
            amp.access = Some(access.clone());
            changed
        };
        if changed {
            self.emit(Update::Access(id));
        }
    }
}
