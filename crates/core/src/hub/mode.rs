//! The device mode of an amp (BEH-44): which inputs feed which outputs. An IPD and an IPX have one (stereo,
//! two ways); it is set on the front panel or here, and shown as the amp holds it, however it was changed.

use labgruppen::{DEVICE_MODES, Param, Value};

use super::Hub;
use super::presets::End;
use crate::store::AmpId;
use crate::{Error, Result};

impl Hub {
    /// The device mode the amp holds (`stereo`, `two_way`); `None` for an amp that is not ONLINE and until
    /// the state is read.
    pub fn device_mode(&self, id: AmpId) -> Option<String> {
        match self.value(id, &Param::Mode)? {
            Value::Text(mode) => Some(mode),
            _ => None,
        }
    }

    /// Change the device mode (BEH-44). It changes which input feeds which output, so it is a bulk change:
    /// the outputs are muted around it and put back as they were (SEC-6, SEC-7). The other settings of
    /// the amp are not touched. Nothing happens when the amp already holds that mode.
    pub async fn set_device_mode(&self, id: AmpId, mode: &str) -> Result<()> {
        if !DEVICE_MODES.contains(&mode) {
            return Err(Error::Invalid(format!(
                "The device mode must be one of {}",
                DEVICE_MODES.join(", ")
            )));
        }
        let session = self.session(id)?;
        if !self.loaded(id) {
            return Err(Error::NotLoaded);
        }
        let Some(current) = self.device_mode(id) else { return Err(Error::NotControllable) };
        // it re-routes the inputs to the outputs: both kinds of channel
        self.permit(id, |rights| rights.input_channel && rights.output_channel)?;
        if current == mode {
            return Ok(());
        }
        let wanted = Value::Text(mode.to_string());
        let write = wanted.clone();
        self.bulk(
            id,
            &session,
            End::AsBefore,
            false,
            |amp| async move { amp.set(&Param::Mode, write).await },
        )
        .await?;
        self.0.shared.observed(id, &Param::Mode, &wanted, true);
        Ok(())
    }
}
