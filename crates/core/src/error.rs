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
    #[error("Cannot read or write the config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("The config file is not valid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Cannot write the config file: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
