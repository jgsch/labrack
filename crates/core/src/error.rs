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
    #[error("Cannot read or write the config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("The config file is not valid: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Cannot write the config file: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
