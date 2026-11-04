//! LAB.RACK logic without a UI: the amplifier list and its TOML config, the connections to the amps
//! and discovery. Later changes add safety rules and link mirroring.

mod config;
mod connector;
mod discovery;
mod dsp;
mod error;
mod hub;
mod status;
mod store;

pub use connector::{Connector, Network};
pub use discovery::{Finder, Lan};
pub use dsp::Range;
pub use error::{Error, Result};
pub use hub::{Hub, LoadReport, Scan, Timing, Update};
pub use status::Status;
pub use store::{Amp, AmpId, Ceilings, Preset, Protocol, Source, Store, Switches};
