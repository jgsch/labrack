//! LAB.RACK logic without a UI: the amplifier list and its TOML config. Later changes add the
//! connection state machine, discovery, safety rules and link mirroring.

mod config;
mod error;
mod store;

pub use error::{Error, Result};
pub use store::{Amp, AmpId, Ceilings, Preset, Protocol, Source, Store, Switches};
