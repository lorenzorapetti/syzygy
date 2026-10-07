//! The iced application: state, messages, `update`, `view`.

use iced::keyboard::{self, key};
use iced::widget::{column, container, text};
use iced::{Color, Element, Event, Length, Subscription, Task, Theme, event, mouse, window};
use std::sync::Arc;
use std::time::Duration;
use syzygy_catalog::Catalog;
use syzygy_store::{DiskCache, Store};
use syzygy_tidal::models::{AuthTokens, SessionInfo};
use syzygy_tidal::{LoginMethod, TidalClient};

use crate::events::EventSource;
use crate::identity::{DISPLAY_NAME, Paths};
use crate::login;
use crate::page::{self, PageId, Route};
use crate::persist;
use crate::session::Session;
use crate::settings::Settings;
use crate::shell::{self, Shell};

/// What iced runs. Without a master key there are no `Services`, so a
/// failed boot has nothing but the fatal error screen.
pub enum State {
    /// An unrecoverable startup failure: no usable master key from the
    /// keyring or the key file, so nothing can be stored encrypted. Not a
    /// Page; you can't navigate away.
    Fatal(syzygy_store::Error),
    Running(Box<App>),
}

pub struct App {
    services: Services,
    settings: Settings,
    /// The signed-in Session, as last saved. `None` on the login screen.
    session: Option<Session>,
    tidal_events: EventSource<syzygy_tidal::Event>,
    phase: Phase,
}

/// What the window shows.
enum Phase {
    Login(Box<login::State>),
    Shell(Box<Shell>),
}

/// Cheap `Clone` handles to the engines, built in [`boot`] and cloned into
/// `Task` futures.
#[derive(Clone)]
pub struct Services {
    pub store: Store,
    pub paths: Paths,
    pub tidal: TidalClient,
    pub catalog: Catalog,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Page(PageId, page::Message),
    Navigate(Route),
    Back,
    Forward,
    /// The window came back into focus.
    WindowFocused,
    Shell(shell::Message),
    Tidal(syzygy_tidal::Event),
    /// The background account refresh.
    SessionInfo(Result<SessionInfo, Arc<syzygy_tidal::Error>>),
    /// The window was asked to close.
    Quit,
    SettingsSaved(Result<(), Arc<syzygy_store::Error>>),
    SessionSaved(Result<(), Arc<syzygy_store::Error>>),
}

pub fn boot(paths: Paths) -> (State, Task<Message>) {
    let store = match Store::open(&paths.key_source()) {
        Ok(store) => store,
        Err(e) => {
            log::error!("Cannot open encrypted storage: {e}");
            return (State::Fatal(e), Task::none());
        }
    };
    let settings = Settings::load(&store, &paths.settings_file());
    let session = Session::load(&store, &paths.session_file());

    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    let tidal = TidalClient::new(http_client(), events);
    let catalog = Catalog::new(
        tidal.clone(),
        DiskCache::new(&paths.catalog_cache_dir, &store),
    );

    let mut app = App {
        services: Services {
            store,
            paths,
            tidal,
            catalog,
        },
        settings,
        session: None,
        tidal_events: EventSource::new("tidal", receiver),
        phase: Phase::Login(Box::default()),
    };
    // A stored Session opens straight into the Shell; the account refresh
    // runs behind it.
    let task = match session {
        Some(session) => {
            app.services.tidal.restore_session(
                session.tokens.clone(),
                session.login_method,
                session.country_code.clone(),
            );
            app.session = Some(session);
            let shell = app.open_shell();
            Task::batch([shell, app.refresh_session_info()])
        }
        None => Task::none(),
    };
    (State::Running(Box::new(app)), task)
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|e| {
            log::warn!("Could not build the HTTP client, using the defaults: {e}");
            reqwest::Client::new()
        })
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match state {
        State::Fatal(_) => match message {
            Message::Quit => iced::exit(),
            _ => Task::none(),
        },
        State::Running(app) => app.update(message),
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    match state {
        State::Fatal(error) => fatal(error),
        State::Running(app) => match &app.phase {
            Phase::Login(login) => login.view().map(Message::Login),
            Phase::Shell(shell) => shell.view(),
        },
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    let close = window::close_requests().map(|_| Message::Quit);
    match state {
        State::Fatal(_) => close,
        // Always on: the receiver can be taken only once (ADR 0003).
        State::Running(app) => {
            let tidal = app.tidal_events.subscription().map(Message::Tidal);
            match app.phase {
                Phase::Shell(_) => {
                    Subscription::batch([close, tidal, event::listen_with(shell_events)])
                }
                Phase::Login(_) => Subscription::batch([close, tidal]),
            }
        }
    }
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
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Quit => self.save_settings().chain(iced::exit()),
            Message::Login(message) => {
                let Phase::Login(login) = &mut self.phase else {
                    return Task::none();
                };
                let effects = login.update(message);
                self.run_login(effects)
            }
            Message::Page(id, message) => {
                self.in_shell(|shell, services| shell.update_page(id, message, services))
            }
            Message::Navigate(route) => {
                self.in_shell(|shell, services| shell.navigate(route, services))
            }
            Message::Back => self.in_shell(Shell::back),
            Message::Forward => self.in_shell(Shell::forward),
            Message::WindowFocused => self.in_shell(Shell::focused),
            Message::Shell(message) => self.in_shell(|shell, _| shell.update(message)),
            Message::Tidal(syzygy_tidal::Event::TokensRefreshed(tokens)) => {
                self.update_session(|session| session.tokens = tokens)
            }
            Message::Tidal(syzygy_tidal::Event::SessionExpired) => {
                // A stub until Session expiry handling: no banner, and the
                // whole file goes rather than just the tokens, so a relaunch
                // doesn't open a dead Session.
                log::warn!("Session expired, back to login");
                self.session = None;
                self.phase = Phase::Login(Box::default());
                let services = self.services.clone();
                Task::perform(
                    persist::remove(services.store, services.paths.session_file()),
                    Message::SessionSaved,
                )
            }
            Message::SessionInfo(Ok(info)) => self.update_session(|session| {
                session.user_id = Some(info.user_id);
                if info.country_code.is_some() {
                    session.country_code = info.country_code;
                }
            }),
            Message::SessionInfo(Err(e)) => {
                log::warn!("Could not refresh the account info: {e}");
                Task::none()
            }
            Message::SettingsSaved(result) => {
                if let Err(e) = result {
                    log::error!("Failed to save settings: {e}");
                }
                Task::none()
            }
            Message::SessionSaved(result) => {
                if let Err(e) = result {
                    log::error!("Failed to save the Session: {e}");
                }
                Task::none()
            }
        }
    }

    /// Change the signed-in Session and save it. Nothing when signed out.
    fn update_session(&mut self, change: impl FnOnce(&mut Session)) -> Task<Message> {
        match &mut self.session {
            Some(session) => {
                change(session);
                self.save_session()
            }
            None => Task::none(),
        }
    }

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

    fn save_session(&self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let services = self.services.clone();
        Task::perform(
            session.save(services.store, services.paths.session_file()),
            Message::SessionSaved,
        )
    }

    /// Hand a message to the Shell. Nothing when signed out.
    fn in_shell(
        &mut self,
        f: impl FnOnce(&mut Shell, &Services) -> Task<Message>,
    ) -> Task<Message> {
        match &mut self.phase {
            Phase::Shell(shell) => f(shell, &self.services),
            Phase::Login(_) => Task::none(),
        }
    }

    /// Show the Shell at Home with an empty Back stack.
    fn open_shell(&mut self) -> Task<Message> {
        let (shell, task) = Shell::new(&self.services);
        self.phase = Phase::Shell(Box::new(shell));
        task
    }

    /// Ask TIDAL who the user is and where, without blocking anything.
    fn refresh_session_info(&self) -> Task<Message> {
        let tidal = self.services.tidal.clone();
        Task::perform(
            async move { tidal.get_session_info().await.map_err(Arc::new) },
            Message::SessionInfo,
        )
    }

    fn sign_in(&mut self, login_method: LoginMethod, tokens: AuthTokens) -> Task<Message> {
        log::info!("Signed in with {login_method:?}");
        self.session = Some(Session {
            user_id: tokens.user_id,
            tokens,
            login_method,
            country_code: None,
        });
        Task::batch([
            self.open_shell(),
            self.save_session(),
            self.refresh_session_info(),
        ])
    }

    fn run_login(&mut self, effects: Vec<login::Effect>) -> Task<Message> {
        let tasks: Vec<_> = effects
            .into_iter()
            .map(|effect| self.run_login_effect(effect))
            .collect();
        Task::batch(tasks)
    }

    fn run_login_effect(&mut self, effect: login::Effect) -> Task<Message> {
        let tidal = self.services.tidal.clone();
        match effect {
            login::Effect::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    log::warn!("Could not open the browser: {e}");
                }
                Task::none()
            }
            login::Effect::CopyToClipboard(contents) => iced::clipboard::write(contents),
            login::Effect::ReadClipboard => iced::clipboard::read()
                .map(|contents| Message::Login(login::Message::Pasted(contents))),
            login::Effect::ExchangeCode { code, pkce } => Task::perform(
                async move {
                    tidal
                        .exchange_pkce_code(&code, &pkce)
                        .await
                        .map_err(Arc::new)
                },
                |result| Message::Login(login::Message::Exchanged(result)),
            ),
            login::Effect::StartDeviceAuth => Task::perform(
                async move { tidal.start_device_auth().await.map_err(Arc::new) },
                |result| Message::Login(login::Message::DeviceAuthStarted(result)),
            ),
            login::Effect::PollDeviceToken {
                device_code,
                interval,
                expires_in,
            } => {
                let (task, handle) = Task::perform(
                    login::poll_device_token(tidal, device_code, interval, expires_in),
                    |result| Message::Login(login::Message::DevicePollEnded(result)),
                )
                .abortable();
                if let Phase::Login(login) = &mut self.phase {
                    login.track_poll(handle);
                }
                task
            }
            login::Effect::SignedIn(login_method, tokens) => self.sign_in(login_method, tokens),
        }
    }
}

/// Back and forward from the mouse side buttons and Alt+←/→ (a text field
/// that takes the arrow keys keeps them), and the window regaining focus.
fn shell_events(event: Event, status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back)) => Some(Message::Back),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Forward)) => Some(Message::Forward),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            modifiers,
            ..
        }) if modifiers.alt() && status == event::Status::Ignored => match named {
            key::Named::ArrowLeft => Some(Message::Back),
            key::Named::ArrowRight => Some(Message::Forward),
            _ => None,
        },
        _ => None,
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
