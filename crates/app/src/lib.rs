//! LAB.RACK, the Dioxus desktop app. The views turn the state of `labrack-core` into pages and the
//! input of the user into its intents; the logic lives in the core.

mod app;
mod header;
mod page;
mod style;

pub use app::App;
pub use header::Header;
pub use page::Page;
pub use style::{Style, stylesheet};
