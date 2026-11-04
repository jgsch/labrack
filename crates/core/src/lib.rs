//! LAB.RACK logic without a UI: the amplifier list and its TOML config, and the connections to the
//! amps. Later changes add discovery, safety rules and link mirroring.

mod config;
mod connector;
mod error;
mod hub;
mod status;
mod store;

pub use connector::{Connector, Network};
pub use error::{Error, Result};
pub use hub::{Hub, Timing, Update};
pub use status::Status;
pub use store::{Amp, AmpId, Ceilings, Preset, Protocol, Source, Store, Switches};
