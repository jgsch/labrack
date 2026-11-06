use dioxus::desktop::tao::dpi::LogicalSize;
use dioxus::desktop::{Config, WindowBuilder};
use labrack_app::App;

fn main() {
    // The app is dark (UX-5): a desktop theme would otherwise draw a light title bar around it.
    // SAFETY: set before GTK starts and before any other thread runs.
    if std::env::var_os("GTK_THEME").is_none() {
        unsafe { std::env::set_var("GTK_THEME", "Adwaita:dark") };
    }

    let window = WindowBuilder::new()
        .with_title("LAB.RACK")
        .with_inner_size(LogicalSize::new(1200.0, 900.0))
        .with_min_inner_size(LogicalSize::new(640.0, 600.0));
    let config = Config::new().with_window(window).with_menu(None).with_background_color((17, 17, 27, 255));
    dioxus::LaunchBuilder::desktop().with_cfg(config).launch(App);
}
