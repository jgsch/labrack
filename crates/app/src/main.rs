use dioxus::desktop::tao::dpi::LogicalSize;
use dioxus::desktop::{Config, WindowBuilder};
use labrack_app::{App, stylesheet};

fn main() {
    let window = WindowBuilder::new()
        .with_title("LAB.RACK")
        .with_inner_size(LogicalSize::new(1200.0, 900.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0));
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((17, 17, 27, 255))
        .with_custom_head(format!("<style>{}</style>", stylesheet()));
    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(App);
}
