//! LAB.RACK, the Dioxus desktop app. The views turn the state of `labrack-core` into pages and the
//! input of the user into its intents; the logic lives in the core.

mod app;
pub mod components;
pub mod control;
mod header;
mod page;
mod style;

pub use app::App;
pub use components::{ConfirmButton, ErrorText, Fader, Meter, ValueField};
pub use header::Header;
pub use page::Page;
pub use style::{Style, stylesheet};
