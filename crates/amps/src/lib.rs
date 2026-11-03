//! Control library for Lab.gruppen amplifiers.
//!
//! * [`amps::ipx`]: IPX series (binary protocol, TCP 50168)
//! * [`amps::ipd`]: IPD series (IntelliDrive protocol, TCP 50675)
//!
//! Both implement the [`Amp`] trait over the shared vocabulary in [`model`]. See `docs/` for the
//! wire protocols.

pub mod amp;
pub mod amps;
pub mod error;
pub mod model;

pub use amp::Amp;
pub use amps::{Driver, connect};
pub use error::{Error, Result};
pub use model::*;
