//! Pages: the places in the Catalog the user navigates to. Each Page is a
//! module with `State`, `Message`, `update -> Action` and `view`. Pages never
//! touch `Services`; they ask the Shell for what they need through
//! [`Action`]s.

pub mod home;
mod unbuilt;

use iced::widget::{button, column, container, text};
use iced::{Element, Length, Task};
use std::sync::Arc;
use syzygy_catalog::Read;
use syzygy_catalog::home_feed::Cover;

use crate::images::{self, Images};

/// How round a cover's corners are.
const COVER_RADIUS: f32 = 4.0;

/// Where a Page is. Plain data, so the Back stack can rebuild a Page from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// A Home feed tab, by slug.
    Home {
        tab: String,
    },
    Album {
        id: u64,
        preview: Option<Preview>,
    },
    Artist {
        id: u64,
        preview: Option<Preview>,
    },
    Playlist {
        uuid: String,
        preview: Option<Preview>,
    },
    Mix {
        id: String,
        preview: Option<Preview>,
    },
    /// The user's Loved tracks.
    Favorites,
}

impl Route {
    /// Home on its default tab.
    pub fn home() -> Self {
        Route::Home {
            tab: home::DEFAULT_TAB.to_string(),
        }
    }
}

/// What a Page can draw straight away from the card that led to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub title: String,
    pub cover: Option<Cover>,
    pub artist: Option<String>,
}

/// Stamped on each navigation. Messages carry it, and the Shell drops any
/// that belong to a Page no longer showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u64);

/// What a Page asks the Shell to do.
pub enum Action {
    None,
    /// Start a Catalog read. Its values come back as this Page's messages,
    /// and it is aborted when the Page goes away.
    Load(Load),
    /// Go somewhere new.
    Navigate(Route),
    /// This Page now shows `route`, as after a tab switch: its Back stack
    /// entry changes and no step is added. It starts with this read.
    Replace(Route, Load),
    /// Run a widget operation, such as a scroll. Its messages come back as
    /// this Page's.
    Run(Task<Message>),
    /// Load these covers into the image cache.
    FetchImages(Vec<String>),
}

/// A Catalog read a Page wants. The Shell runs it and maps what comes back
/// into the Page's message.
#[derive(Debug)]
#[expect(
    clippy::enum_variant_names,
    reason = "the other Pages' reads land here"
)]
pub enum Load {
    /// A Home feed tab, by slug.
    HomeFeed(String),
    /// A Home feed tab straight from TIDAL.
    RefreshHomeFeed(String),
    /// The sections of a Home feed tab after a cursor.
    MoreHomeFeed { tab: String, cursor: String },
}

pub enum Page {
    Home(home::State),
    Unbuilt(unbuilt::State),
}

#[derive(Debug, Clone)]
pub enum Message {
    Home(home::Message),
    /// A cover on a Page with no messages of its own came into view.
    CoverWanted(String),
}

impl Page {
    /// Build the Page for a route, and what it needs first.
    pub fn open(route: &Route) -> (Self, Action) {
        match route {
            Route::Home { tab } => {
                let (state, action) = home::State::new(tab.clone());
                (Page::Home(state), action)
            }
            Route::Album { preview, .. }
            | Route::Artist { preview, .. }
            | Route::Playlist { preview, .. }
            | Route::Mix { preview, .. } => (
                Page::Unbuilt(unbuilt::State::new(preview.clone())),
                Action::None,
            ),
            Route::Favorites => (
                Page::Unbuilt(unbuilt::State::new(Some(Preview {
                    title: "Loved Tracks".to_string(),
                    cover: None,
                    artist: None,
                }))),
                Action::None,
            ),
        }
    }

    pub fn update(&mut self, message: Message) -> Action {
        match (self, message) {
            (Page::Home(state), Message::Home(message)) => state.update(message),
            (_, Message::CoverWanted(url)) => Action::FetchImages(vec![url]),
            (Page::Unbuilt(_), Message::Home(_)) => Action::None,
        }
    }

    /// The window came back into focus.
    pub fn focused(&mut self) -> Action {
        match self {
            Page::Home(state) => state.focused(),
            Page::Unbuilt(_) => Action::None,
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        match self {
            Page::Home(state) => state.view(images).map(Message::Home),
            Page::Unbuilt(state) => state.view(images),
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

/// A `size` square cover, fetched at twice that for sharp HiDPI. Until it's
/// loaded a placeholder, which asks for it with `wanted` as it comes into
/// view. Just the placeholder when there's no cover.
pub fn cover<'a, Message: Clone + 'a>(
    images: &'a Images,
    cover: Option<&Cover>,
    size: f32,
    wanted: impl FnOnce(String) -> Message,
) -> Element<'a, Message> {
    match cover {
        Some(cover) => {
            let url = cover.url((size * 2.0) as u32);
            let wanted = wanted(url.clone());
            images.cover(&url, size, COVER_RADIUS, wanted)
        }
        None => images::placeholder(size, COVER_RADIUS),
    }
}
