mod app;
mod events;
mod icons;
mod identity;
mod images;
mod logging;
mod login;
mod page;
mod persist;
mod playback;
mod plugin_path;
mod session;
mod settings;
mod shell;
mod style;

use iced::window;

use identity::{APP_ID, Paths};

fn main() -> iced::Result {
    // Before anything starts a thread.
    plugin_path::select();
    let paths = Paths::locate();
    // Named, not `_`, so the handle lives until iced returns and the log is
    // flushed on drop.
    let _logger = logging::init(&paths.log_dir);
    log::info!("Starting syzygy {}", env!("CARGO_PKG_VERSION"));

    iced::application(move || app::boot(paths.clone()), app::update, app::view)
        .title(app::title)
        .theme(app::theme)
        .subscription(app::subscription)
        .window(window::Settings {
            // Closing goes through `Message::Quit`, which saves before exiting.
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: APP_ID.to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}
