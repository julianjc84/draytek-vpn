mod app;
mod config;
mod glib_channels;
mod logging;
mod messages;
mod tunnel;
mod ui;

fn main() {
    app::build_app();
}
