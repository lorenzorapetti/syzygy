//! Pages: the places in the Catalog the user navigates to. Each Page is a
//! module with `State`, `Message`, `update -> Action` and `view`. Pages never
//! touch `Services`; they ask the Shell for what they need through
//! [`Action`]s.

pub mod home;

use iced::widget::{button, column, container, text};
use iced::{Element, Length};
use std::sync::Arc;
use syzygy_catalog::Read;

/// Where a Page is. Plain data, so the Back stack can rebuild a Page from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Home,
}

/// Stamped on each navigation. Messages carry it, and the Shell drops any
/// that belong to a Page no longer showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u64);

/// What a Page asks the Shell to do.
#[derive(Debug)]
pub enum Action {
    None,
    /// Start a Catalog read. Its values come back as this Page's messages,
    /// and it is aborted when the Page goes away.
    Load(Load),
}

/// A Catalog read a Page wants. The Shell runs it and maps each [`Read`]
/// into the Page's message.
#[derive(Debug)]
pub enum Load {
    /// A Home feed tab, by slug.
    HomeFeed(String),
}

pub enum Page {
    Home(home::State),
}

#[derive(Debug, Clone)]
pub enum Message {
    Home(home::Message),
}

impl Page {
    /// Build the Page for a route, and what it needs first.
    pub fn open(route: &Route) -> (Self, Action) {
        match route {
            Route::Home => {
                let (state, action) = home::State::new();
                (Page::Home(state), action)
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Action {
        match (self, message) {
            (Page::Home(state), Message::Home(message)) => state.update(message),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self {
            Page::Home(state) => state.view().map(Message::Home),
        }
    }
}

/// A Page's data from the Catalog.
#[derive(Debug)]
pub enum Remote<T> {
    Loading,
    Loaded(T),
    NotFound,
    Failed(Arc<syzygy_catalog::Error>),
}

impl<T> Remote<T> {
    /// Take one value from a Catalog read. A failed refresh after the cached
    /// copy arrived keeps the cached copy on screen and is only logged.
    pub fn apply(&mut self, read: Read<T>, what: &str) {
        match read {
            Read::Cached(value) | Read::Fresh(Ok(value)) => *self = Remote::Loaded(value),
            Read::Fresh(Err(e)) => match self {
                Remote::Loaded(_) => log::warn!("Could not refresh {what}: {e}"),
                _ if e.is_not_found() => *self = Remote::NotFound,
                _ => {
                    log::warn!("Could not load {what}: {e}");
                    *self = Remote::Failed(e);
                }
            },
        }
    }

    /// The data once it's here; otherwise a loading line, a not-found
    /// message, or the error with Retry.
    pub fn view<'a, Message: Clone + 'a>(
        &'a self,
        retry: Message,
        loaded: impl FnOnce(&'a T) -> Element<'a, Message>,
    ) -> Element<'a, Message> {
        let notice = match self {
            Remote::Loaded(value) => return loaded(value),
            Remote::Loading => column![text("Loading…")],
            Remote::NotFound => column![
                text("Not found").size(20),
                text("This page doesn't exist, or it's not available in your country."),
            ],
            Remote::Failed(e) => column![
                text("Couldn't load this page").size(20),
                text(e.to_string()).size(13),
                button(text("Retry")).on_press(retry),
            ],
        };
        container(notice.spacing(12))
            .width(Length::Fill)
            .padding(24)
            .into()
    }
}
