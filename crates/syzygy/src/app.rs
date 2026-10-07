//! The iced application: state, messages, `update`, `view`.

use iced::widget::{column, container, text};
use iced::{Color, Element, Length, Subscription, Task, Theme, window};
use std::sync::Arc;
use syzygy_store::Store;

use crate::identity::{DISPLAY_NAME, Paths};
use crate::settings::Settings;

/// What iced runs. Without a master key there are no `Services`, so a
/// failed boot has nothing but the fatal error screen.
pub enum State {
    /// An unrecoverable startup failure: no usable master key from the
    /// keyring or the key file, so nothing can be stored encrypted. Not a
    /// Page; you can't navigate away.
    Fatal(syzygy_store::Error),
    Running(App),
}

pub struct App {
    services: Services,
    settings: Settings,
    phase: Phase,
}

/// What the window shows. Login arrives with the Session.
enum Phase {
    /// The main window. Empty until the Shell's Pages land.
    Shell,
}

/// Cheap `Clone` handles to the engines, built in [`boot`] and cloned into
/// `Task` futures.
#[derive(Clone)]
pub struct Services {
    pub store: Store,
    pub paths: Paths,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The window was asked to close.
    Quit,
    SettingsSaved(Result<(), Arc<syzygy_store::Error>>),
}

pub fn boot(paths: Paths) -> State {
    let store = match Store::open(&paths.key_source()) {
        Ok(store) => store,
        Err(e) => {
            log::error!("Cannot open encrypted storage: {e}");
            return State::Fatal(e);
        }
    };
    let settings = Settings::load(&store, &paths.settings_file());

    State::Running(App {
        services: Services { store, paths },
        settings,
        phase: Phase::Shell,
    })
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match (state, message) {
        (State::Running(app), Message::Quit) => app.save_settings().chain(iced::exit()),
        (State::Fatal(_), Message::Quit) => iced::exit(),
        (_, Message::SettingsSaved(Ok(()))) => Task::none(),
        (_, Message::SettingsSaved(Err(e))) => {
            log::error!("Failed to save settings: {e}");
            Task::none()
        }
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    match state {
        State::Fatal(error) => fatal(error),
        State::Running(app) => match app.phase {
            Phase::Shell => container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        },
    }
}

pub fn subscription(_state: &State) -> Subscription<Message> {
    window::close_requests().map(|_| Message::Quit)
}

pub fn title(_state: &State) -> String {
    DISPLAY_NAME.to_string()
}

pub fn theme(_state: &State) -> Theme {
    Theme::custom(
        DISPLAY_NAME,
        iced::theme::Palette {
            background: Color::from_rgb8(0x13, 0x0F, 0x1A),
            text: Color::WHITE,
            primary: Color::from_rgb8(0xA8, 0x55, 0xF7),
            ..iced::theme::Palette::DARK
        },
    )
}

impl App {
    /// The only way settings reach disk: encrypted and written off the UI
    /// thread.
    fn save_settings(&self) -> Task<Message> {
        let services = self.services.clone();
        Task::perform(
            self.settings
                .clone()
                .save(services.store, services.paths.settings_file()),
            Message::SettingsSaved,
        )
    }
}

fn fatal(error: &syzygy_store::Error) -> Element<'_, Message> {
    let body = column![
        text(format!("{DISPLAY_NAME} can't start")).size(24),
        text(
            "It couldn't get its encryption key from the system keyring or \
             its key file, so it has nowhere safe to keep your data. Nothing \
             has been stored unencrypted.",
        ),
        text(error.to_string())
            .size(13)
            .color(Color::from_rgb8(0x9C, 0x92, 0xAD)),
    ]
    .spacing(12)
    .max_width(520);

    container(body).center(Length::Fill).padding(24).into()
}
