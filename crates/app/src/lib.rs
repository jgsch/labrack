//! LAB.RACK, the Dioxus desktop app. The views turn the state of `labrack-core` into pages and the
//! input of the user into its intents; the logic lives in the core.

pub mod amplifiers;
mod app;
pub mod components;
pub mod control;
pub mod eq;
mod header;
pub mod login;
pub mod mixeq;
pub mod network;
pub mod page;
pub mod presets;
pub mod search;
mod style;

pub use amplifiers::{AccessView, AmpView, AmplifierPanel, AmplifiersPage, Level};
pub use app::App;
pub use components::{ConfirmButton, ErrorText, Fader, Meter, SignalMeter, Toasts, ValueField};
pub use header::Header;
pub use labrack_core::Status;
pub use mixeq::{
    AmpCard, EqBounds, EqPage, EqView, LevelRow, LevelsPage, LimiterStrip, MixRow, MixTab, MixeqPage,
    MixeqView, MixerPage, Strip,
};
pub use page::Page;
pub use search::{SearchPage, SearchView, Switch};
pub use style::{Style, stylesheet};
