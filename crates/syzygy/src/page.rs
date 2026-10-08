//! Pages: the places in the Catalog the user navigates to. Each Page is a
//! module with `State`, `Message`, `update -> Action` and `view`. Pages never
//! touch `Services`; they ask the Shell for what they need through
//! [`Action`]s.

pub mod album;
pub mod artist;
pub mod artist_tracks;
pub mod artist_view_all;
mod cards;
mod hero;
pub mod home;
pub mod mix;
mod paged;
pub mod playlist;
mod track_list;
mod unbuilt;

use iced::widget::{Row, Text, button, column, container, row, text};
use iced::{Alignment, Element, Length, Task, Theme};
use std::sync::Arc;
use syzygy_catalog::home_feed::Cover;
use syzygy_catalog::track::ArtistRef;
use syzygy_catalog::{Read, TrackSort};

use crate::images::{self, Images};
use crate::settings::Settings;
use crate::style;

/// How round a cover's corners are.
const COVER_RADIUS: f32 = 4.0;
/// Around every Page's content.
const PADDING: f32 = 24.0;

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
    /// All of an artist's top tracks.
    ArtistTracks {
        id: u64,
    },
    /// One of an artist's sections in full. The section is the tab, by the
    /// path TIDAL reads it from.
    ArtistViewAll {
        id: u64,
        section: String,
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

/// The part of the Page in view: how far down it's scrolled and how tall
/// the window shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub offset: f32,
    pub height: f32,
}

/// What a Page reads from the rest of the app as it opens.
pub struct Context<'a> {
    /// Who's signed in, when TIDAL has said.
    pub user_id: Option<u64>,
    pub settings: &'a Settings,
}

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
    /// Remember a playlist's track sort in `Settings`; `None` forgets it.
    SaveTrackSort(String, Option<TrackSort>),
    /// All of these.
    Batch(Vec<Action>),
}

/// A Catalog read a Page wants. The Shell runs it and maps what comes back
/// into the Page's message.
#[derive(Debug)]
pub enum Load {
    /// A Home feed tab, by slug.
    HomeFeed(String),
    /// A Home feed tab straight from TIDAL.
    RefreshHomeFeed(String),
    /// The sections of a Home feed tab after a cursor.
    MoreHomeFeed { tab: String, cursor: String },
    /// An album's Page.
    Album(u64),
    /// An artist's Page, read by the Artist Page and the Pages under it,
    /// each into its own message.
    Artist {
        id: u64,
        then: fn(Read<syzygy_catalog::Artist>) -> Message,
    },
    /// The first page of an artist's top tracks.
    ArtistTracks(u64),
    /// An artist's top tracks after the first `offset`.
    MoreArtistTracks { id: u64, offset: usize },
    /// The first page of an artist's section.
    ArtistViewAll { id: u64, section: String },
    /// An artist's section after the first `offset` items.
    MoreArtistViewAll {
        id: u64,
        section: String,
        offset: usize,
    },
    /// A mix's Page.
    Mix(String),
    /// A playlist: what it is and who made it.
    Playlist(String),
    /// The first page of a playlist's tracks.
    PlaylistTracks {
        uuid: String,
        sort: Option<TrackSort>,
    },
    /// A playlist's tracks after the first `offset`.
    MorePlaylistTracks {
        uuid: String,
        sort: Option<TrackSort>,
        offset: usize,
    },
    /// The tracks TIDAL recommends for a playlist, from `offset`.
    PlaylistRecommendations { uuid: String, offset: usize },
}

pub enum Page {
    Home(home::State),
    Album(album::State),
    Artist(artist::State),
    ArtistTracks(artist_tracks::State),
    ArtistViewAll(artist_view_all::State),
    Mix(mix::State),
    Playlist(playlist::State),
    Unbuilt(unbuilt::State),
}

#[derive(Debug, Clone)]
pub enum Message {
    Home(home::Message),
    Album(album::Message),
    Artist(artist::Message),
    ArtistTracks(artist_tracks::Message),
    ArtistViewAll(artist_view_all::Message),
    Mix(mix::Message),
    Playlist(playlist::Message),
    /// From a Page with no messages of its own.
    Link(Link),
}

/// What any Page's covers, cards and links ask for.
#[derive(Debug, Clone)]
pub enum Link {
    /// A cover came into view.
    CoverWanted(String),
    Open(Route),
}

impl Link {
    pub fn follow(self) -> Action {
        match self {
            Link::CoverWanted(url) => Action::FetchImages(vec![url]),
            Link::Open(route) => Action::Navigate(route),
        }
    }
}

impl Page {
    /// Build the Page for a route, and what it needs first.
    pub fn open(route: &Route, context: &Context) -> (Self, Action) {
        match route {
            Route::Home { tab } => {
                let (state, action) = home::State::new(tab.clone());
                (Page::Home(state), action)
            }
            Route::Album { id, preview } => {
                let (state, action) = album::State::new(*id, preview.clone());
                (Page::Album(state), action)
            }
            Route::Artist { id, preview } => {
                let (state, action) = artist::State::new(*id, preview.clone());
                (Page::Artist(state), action)
            }
            Route::ArtistTracks { id } => {
                let (state, action) = artist_tracks::State::new(*id);
                (Page::ArtistTracks(state), action)
            }
            Route::ArtistViewAll { id, section } => {
                let (state, action) = artist_view_all::State::new(*id, section.clone());
                (Page::ArtistViewAll(state), action)
            }
            Route::Mix { id, preview } => {
                let (state, action) = mix::State::new(id.clone(), preview.clone());
                (Page::Mix(state), action)
            }
            Route::Playlist { uuid, preview } => {
                let (state, action) = playlist::State::new(uuid.clone(), preview.clone(), context);
                (Page::Playlist(state), action)
            }
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
            (Page::Album(state), Message::Album(message)) => state.update(message),
            (Page::Artist(state), Message::Artist(message)) => state.update(message),
            (Page::ArtistTracks(state), Message::ArtistTracks(message)) => state.update(message),
            (Page::ArtistViewAll(state), Message::ArtistViewAll(message)) => state.update(message),
            (Page::Mix(state), Message::Mix(message)) => state.update(message),
            (Page::Playlist(state), Message::Playlist(message)) => state.update(message),
            (_, Message::Link(link)) => link.follow(),
            // A message for another kind of Page.
            _ => Action::None,
        }
    }

    /// The window came back into focus.
    pub fn focused(&mut self) -> Action {
        match self {
            Page::Home(state) => state.focused(),
            _ => Action::None,
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
        match self {
            Page::Home(state) => state.view(images).map(Message::Home),
            Page::Album(state) => state.view(images, viewport).map(Message::Album),
            Page::Artist(state) => state.view(images).map(Message::Artist),
            Page::ArtistTracks(state) => state.view(images, viewport).map(Message::ArtistTracks),
            Page::ArtistViewAll(state) => state.view(images).map(Message::ArtistViewAll),
            Page::Mix(state) => state.view(images, viewport).map(Message::Mix),
            Page::Playlist(state) => state.view(images, viewport).map(Message::Playlist),
            Page::Unbuilt(state) => state.view(images).map(Message::Link),
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

    pub fn loaded(&self) -> Option<&T> {
        match self {
            Remote::Loaded(value) => Some(value),
            _ => None,
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
            .padding(PADDING)
            .into()
    }
}

/// A `size` square cover, fetched at twice that for sharp HiDPI. Until it's
/// loaded a placeholder, which asks for it as it comes into view. Just the
/// placeholder when there's no cover.
pub fn cover<'a>(images: &'a Images, cover: Option<&Cover>, size: f32) -> Element<'a, Link> {
    rounded_cover(images, cover, size, COVER_RADIUS)
}

/// [`cover`] with its own corner radius: half the size for a round one.
pub fn rounded_cover<'a>(
    images: &'a Images,
    cover: Option<&Cover>,
    size: f32,
    radius: f32,
) -> Element<'a, Link> {
    match cover {
        Some(cover) => {
            let url = cover.url((size * 2.0) as u32);
            let wanted = Link::CoverWanted(url.clone());
            images.cover(&url, size, radius, wanted)
        }
        None => images::placeholder(size, radius),
    }
}

/// Text that goes somewhere when clicked.
pub fn link<'a>(label: Text<'a>, link: Link) -> Element<'a, Link> {
    button(label)
        .padding(0)
        .style(link_style)
        .on_press(link)
        .into()
}

fn link_style(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => style::TEXT_PRIMARY,
        _ => style::TEXT_SECONDARY,
    };
    button::Style {
        background: None,
        text_color,
        ..button::Style::default()
    }
}

/// "3:07", or "1:02:45" past the hour (sone's `formatTotalDuration`).
pub fn duration(seconds: u32) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// "1 Track", "12 Tracks".
pub fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// Each artist's name, leading to their Page, separated by commas.
pub fn artists<'a>(artists: &'a [ArtistRef], size: f32) -> Row<'a, Link> {
    let names = artists.iter().enumerate().map(|(i, artist)| {
        let name = link(
            text(&artist.name).size(size),
            Link::Open(Route::Artist {
                id: artist.id,
                preview: Some(Preview {
                    title: artist.name.clone(),
                    cover: None,
                    artist: None,
                }),
            }),
        );
        if i + 1 < artists.len() {
            row![name, text(",").size(size).color(style::TEXT_SECONDARY)].into()
        } else {
            name
        }
    });
    row(names).spacing(4).align_y(Alignment::Center)
}
