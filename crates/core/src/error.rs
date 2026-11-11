/// The first four messages are shown to the user next to the control that caused them.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Not a valid IPv4 address")]
    InvalidIp,
    #[error("An amplifier with this IP is already in the list")]
    DuplicateIp,
    #[error("The name cannot be empty")]
    EmptyName,
    #[error("Unknown amplifier")]
    UnknownAmp,
    #[error("The amplifier is not online")]
    NotOnline,
    #[error("This control cannot be set")]
    NotControllable,
    #[error("The settings of the amplifier are still being read")]
    NotLoaded,
    /// The user level of the IPD (its name) does not have the right for this (BEH-14).
    #[error("The user level {0} of the amplifier does not allow this")]
    AccessDenied(String),
    /// An unmute while a bulk change runs on the amp (SEC-6): it would be written after the change.
    #[error("The amplifier is busy changing its settings. Unmute when it is done.")]
    Busy,
    /// A setting of channel 2 of a linked pair: it follows channel 1.
    #[error("This channel follows channel 1 while the pair is linked")]
    Linked,
    /// A value outside the range of the control, of the wrong kind, NaN or infinite.
    #[error("{0}")]
    Invalid(String),
    #[error("Above the ceiling of {ceiling} {unit}")]
    AboveCeiling { ceiling: f64, unit: &'static str },
    /// The amp refused or did not confirm something (a protected slot, a missing right, a timeout).
    #[error("{0}")]
    Amp(#[from] labgruppen::Error),
    /// A bulk change failed after the outputs were muted for it: they stay muted.
    #[error("{0}. The outputs that were muted for it stay muted.")]
    LeftMuted(String),
    /// The outputs could not all be muted before a bulk change, so it was not started (SEC-6).
    #[error("{0}. Nothing was changed, but the outputs could not all be muted.")]
    NotMuted(String),
    #[error("Cannot read or write the config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("The config file is not valid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Cannot write the config file: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
