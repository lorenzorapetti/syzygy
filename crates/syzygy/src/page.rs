//! Pages: the places in the Catalog the user navigates to. Each Page is a
//! module with `State`, `Message`, `update -> Action` and `view`. Pages never
//! touch `Services`; they ask the Shell for what they need through
//! [`Action`]s.

pub mod album;
pub mod artist;
pub mod artist_tracks;
pub mod artist_view_all;
pub mod cards;
pub mod explore;
pub mod favorites;
pub mod feed;
mod hero;
pub mod home;
pub mod library;
pub mod mix;
pub mod paged;
pub mod playlist;
pub mod profile;
pub mod profile_playlists;
pub mod search;
mod track_list;

use iced::widget::{Row, Text, button, column, container, row, text};
use iced::{Alignment, Color, Element, Length, Task};
use std::sync::Arc;
use syzygy_catalog::artist::Content;
use syzygy_catalog::home_feed::{Card, Cover};
use syzygy_catalog::track::{AlbumRef, ArtistRef};
use syzygy_catalog::{Album, Artist, Kind, Mix, Read, Shelf, Track, TrackSort};

use crate::icons::{Icon, filled, icon};
use crate::images::{self, Images};
use crate::playback::{PlayRequest, Source, SourceRef, Start};
use crate::settings::{Settings, Sort};
use crate::style;

/// How round a cover's corners are.
const COVER_RADIUS: f32 = 4.0;
/// Around every Page's content.
const PADDING: f32 = 24.0;
/// The row of Play and Shuffle under a Page's hero.
const PLAY_BUTTONS_HEIGHT: f32 = 40.0;

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
    /// One type of the user's Library in full. The type is the tab.
    Library {
        kind: Kind,
    },
    /// A Folder's playlists, by its id, with its name for the title.
    Folder {
        id: String,
        name: String,
    },
    /// What a search found. The type is the tab.
    Search {
        query: String,
        tab: search::Tab,
    },
    Explore,
    /// A Page under Explore (a genre, a mood, a decade, an editorial page)
    /// by the path TIDAL reads it from, with its title.
    ExplorePage {
        path: String,
        title: String,
    },
    /// New releases from the artists the user follows.
    Feed,
    /// A user's profile: the signed-in user's or anyone's.
    Profile {
        user_id: u64,
    },
    /// A user's public playlists in full.
    ProfilePlaylists {
        user_id: u64,
    },
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

/// The current track, for track lists to mark, and where it plays from,
/// for a Page's Play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NowPlaying<'a> {
    pub track_id: u64,
    /// While it plays: seconds on the clock its animation runs on.
    pub playing: Option<f32>,
    pub source: &'a SourceRef,
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
    /// Start playing a source.
    Play(PlayRequest),
    /// Read what a card leads to, then play all of it.
    PlayCard(Card),
    /// Pause or resume what's playing.
    TogglePlay,
    /// This Page now shows `route`, as after a tab switch: its Back stack
    /// entry changes and no step is added. It starts this read, if any.
    Replace(Route, Option<Load>),
    /// Run a widget operation, such as a scroll. Its messages come back as
    /// this Page's.
    Run(Task<Message>),
    /// Load these covers into the image cache.
    FetchImages(Vec<String>),
    /// Remember an order the user picked, in `Settings`.
    SaveSort(Sort),
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
    /// The first page of a shelf of the user's Library.
    Library(Shelf),
    /// A shelf after the first `offset` items, from `cursor` for playlists.
    MoreLibrary {
        shelf: Shelf,
        offset: usize,
        cursor: Option<String>,
    },
    /// The first page of the user's Loved tracks.
    LovedTracks {
        user_id: u64,
        sort: Option<TrackSort>,
    },
    /// The Loved tracks after the first `offset`.
    MoreLovedTracks {
        user_id: u64,
        sort: Option<TrackSort>,
        offset: usize,
    },
    /// What a search for a query finds.
    Search(String),
    /// An Explore Page, by its path.
    Explore(String),
    /// The user's Feed.
    Feed(u64),
    /// A user's profile, read by the Profile Page and the Page of its
    /// playlists, each into its own message.
    Profile {
        user_id: u64,
        then: fn(Read<syzygy_catalog::Profile>) -> Message,
    },
}

pub enum Page {
    Home(home::State),
    Album(album::State),
    Artist(artist::State),
    ArtistTracks(artist_tracks::State),
    ArtistViewAll(artist_view_all::State),
    Mix(mix::State),
    Playlist(playlist::State),
    Favorites(favorites::State),
    Library(library::State),
    Search(search::State),
    /// Explore and the Pages under it.
    Explore(explore::State),
    Feed(feed::State),
    Profile(profile::State),
    ProfilePlaylists(profile_playlists::State),
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
    Favorites(favorites::Message),
    Library(library::Message),
    Search(search::Message),
    Explore(explore::Message),
    Feed(feed::Message),
    Profile(profile::Message),
    ProfilePlaylists(profile_playlists::Message),
}

/// What any Page's covers, cards and links ask for.
#[derive(Debug, Clone)]
pub enum Link {
    /// A cover came into view.
    CoverWanted(String),
    Open(Route),
    Play(PlayRequest),
    /// A card's play button.
    PlayCard(Card),
}

impl Link {
    pub fn follow(self) -> Action {
        match self {
            Link::CoverWanted(url) => Action::FetchImages(vec![url]),
            Link::Open(route) => Action::Navigate(route),
            Link::Play(request) => Action::Play(request),
            Link::PlayCard(card) => Action::PlayCard(card),
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
            Route::Favorites => {
                let (state, action) = favorites::State::new(context);
                (Page::Favorites(state), action)
            }
            Route::Library { kind } => {
                let (state, action) = library::State::new(*kind, None, context);
                (Page::Library(state), action)
            }
            Route::Folder { id, name } => {
                let folder = library::FolderRef {
                    id: id.clone(),
                    name: name.clone(),
                };
                let (state, action) = library::State::new(Kind::Playlists, Some(folder), context);
                (Page::Library(state), action)
            }
            Route::Search { query, tab } => {
                let (state, action) = search::State::new(query.clone(), *tab);
                (Page::Search(state), action)
            }
            Route::Explore => {
                let (state, action) = explore::State::root();
                (Page::Explore(state), action)
            }
            Route::ExplorePage { path, title } => {
                let (state, action) = explore::State::page(path.clone(), title.clone());
                (Page::Explore(state), action)
            }
            Route::Feed => {
                let (state, action) = feed::State::new(context);
                (Page::Feed(state), action)
            }
            Route::Profile { user_id } => {
                let (state, action) = profile::State::new(*user_id);
                (Page::Profile(state), action)
            }
            Route::ProfilePlaylists { user_id } => {
                let (state, action) = profile_playlists::State::new(*user_id);
                (Page::ProfilePlaylists(state), action)
            }
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
            (Page::Favorites(state), Message::Favorites(message)) => state.update(message),
            (Page::Library(state), Message::Library(message)) => state.update(message),
            (Page::Search(state), Message::Search(message)) => state.update(message),
            (Page::Explore(state), Message::Explore(message)) => state.update(message),
            (Page::Feed(state), Message::Feed(message)) => state.update(message),
            (Page::Profile(state), Message::Profile(message)) => state.update(message),
            (Page::ProfilePlaylists(state), Message::ProfilePlaylists(message)) => {
                state.update(message)
            }
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

    /// Whether a track list on this Page shows the track, so its row needs
    /// animating while it plays.
    pub fn shows_track(&self, track_id: u64) -> bool {
        match self {
            Page::Album(state) => state.shows_track(track_id),
            _ => false,
        }
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        viewport: Viewport,
        now_playing: Option<NowPlaying<'a>>,
    ) -> Element<'a, Message> {
        match self {
            Page::Home(state) => state.view(images).map(Message::Home),
            Page::Album(state) => state
                .view(images, viewport, now_playing)
                .map(Message::Album),
            Page::Artist(state) => state.view(images, now_playing).map(Message::Artist),
            Page::ArtistTracks(state) => state
                .view(images, viewport, now_playing)
                .map(Message::ArtistTracks),
            Page::ArtistViewAll(state) => state.view(images).map(Message::ArtistViewAll),
            Page::Mix(state) => state.view(images, viewport, now_playing).map(Message::Mix),
            Page::Playlist(state) => state
                .view(images, viewport, now_playing)
                .map(Message::Playlist),
            Page::Favorites(state) => state
                .view(images, viewport, now_playing)
                .map(Message::Favorites),
            Page::Library(state) => state.view(images).map(Message::Library),
            Page::Search(state) => state.view(images, viewport).map(Message::Search),
            Page::Explore(state) => state.view(images).map(Message::Explore),
            Page::Feed(state) => state.view(images).map(Message::Feed),
            Page::Profile(state) => state.view(images).map(Message::Profile),
            Page::ProfilePlaylists(state) => state.view(images).map(Message::ProfilePlaylists),
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

/// The Loved tracks' picture: a heart on sone's gradient, `size` square.
pub fn loved_art<'a, M: 'a>(size: f32, radius: f32) -> Element<'a, M> {
    container(filled(Icon::Heart, (size * 0.375).round(), Color::WHITE))
        .center(size)
        .style(move |_| style::loved(radius))
        .into()
}

/// A Folder's picture: an open folder, `size` square.
pub fn folder_art<'a, M: 'a>(size: f32, radius: f32) -> Element<'a, M> {
    container(icon(
        Icon::FolderOpen,
        (size * 0.45).min(32.0).round(),
        style::TEXT_MUTED,
    ))
    .center(size)
    .style(move |theme| style::placeholder(theme, radius))
    .into()
}

/// A round placeholder with a person in it, `size` across: a profile
/// with no picture.
pub fn no_picture<'a, M: 'a>(size: f32) -> Element<'a, M> {
    container(icon(Icon::User, (size * 0.4).round(), style::TEXT_MUTED))
        .center(size)
        .style(move |theme| style::placeholder(theme, size / 2.0))
        .into()
}

/// Text that goes somewhere when clicked.
pub fn link<'a>(label: Text<'a>, link: Link) -> Element<'a, Link> {
    button(label)
        .padding(0)
        .style(style::text_link)
        .on_press(link)
        .into()
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

/// A Page's Play and Shuffle. Play starts `source` as Shuffle says, or
/// pauses and resumes it while it's what plays. Shuffle is a Shuffle play.
pub fn play_buttons<'a, M: Clone + 'a>(
    source: &SourceRef,
    now_playing: Option<NowPlaying>,
    play_from: fn(Start) -> M,
    toggle: M,
) -> Element<'a, M> {
    let this = now_playing.filter(|now| now.source == source);
    let (glyph, label, on_play) = match this {
        Some(NowPlaying {
            playing: Some(_), ..
        }) => (Icon::Pause, "Pause", toggle),
        Some(_) => (Icon::Play, "Resume", toggle),
        None => (Icon::Play, "Play", play_from(Start::All)),
    };
    let play = pill(
        filled(glyph, 18.0, style::TEXT_PRIMARY),
        label,
        style::accent_pill,
        on_play,
    );
    let shuffle = pill(
        icon(Icon::Shuffle, 18.0, style::TEXT_PRIMARY),
        "Shuffle",
        style::pill_button,
        play_from(Start::Shuffled),
    );
    container(row![play, shuffle].spacing(12))
        .center_y(PLAY_BUTTONS_HEIGHT)
        .into()
}

fn pill<'a, M: Clone + 'a>(
    glyph: impl Into<Element<'a, M>>,
    label: &'a str,
    style: fn(&iced::Theme, button::Status) -> button::Style,
    on_press: M,
) -> Element<'a, M> {
    button(
        row![glyph.into(), text(label).size(14).font(hero::bold())]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([10, 24])
    .style(style)
    .on_press(on_press)
    .into()
}

/// A track on its own: its own Playback source.
pub fn single(track: &Track) -> PlayRequest {
    PlayRequest {
        source: Source {
            kind: SourceRef::Track(track.id),
            name: track.title.clone(),
        },
        first_page: vec![track.clone()],
        start: Start::Track(0),
    }
}

/// An album's tracks, each carrying the album, for the player bar's cover,
/// even where TIDAL left it out.
pub fn album_tracks(id: u64, album: &Album) -> Vec<Track> {
    album
        .tracks
        .iter()
        .map(|track| {
            let mut track = track.clone();
            track.album.get_or_insert_with(|| AlbumRef {
                id,
                title: album.title.clone(),
                cover: album.cover.clone(),
            });
            track
        })
        .collect()
}

/// A mix as a Playback source: a track's mix plays as its Track radio.
/// Named `fallback` when TIDAL sent no title.
pub fn mix_source(id: &str, mix: &Mix, fallback: &str) -> Source {
    let kind = if mix.track_radio {
        SourceRef::TrackRadio(id.to_string())
    } else {
        SourceRef::Mix(id.to_string())
    };
    let name = if mix.title.is_empty() {
        fallback
    } else {
        &mix.title
    };
    Source {
        kind,
        name: name.to_string(),
    }
}

/// What plays as the artist: their first track section with tracks in it,
/// by its place among their sections.
pub fn top_tracks(artist: &Artist) -> Option<(usize, &[Track])> {
    artist
        .sections
        .iter()
        .enumerate()
        .find_map(|(index, section)| match &section.content {
            Content::Tracks(tracks) if !tracks.is_empty() => Some((index, tracks.as_slice())),
            _ => None,
        })
}

/// `tracks` from `source`, started as `start` says.
pub fn request(kind: SourceRef, name: &str, tracks: &[Track], start: Start) -> PlayRequest {
    PlayRequest {
        source: Source {
            kind,
            name: name.to_string(),
        },
        first_page: tracks.to_vec(),
        start,
    }
}

/// Where "Playing from" leads: the source's Page, or for a track played on
/// its own, its album's.
pub fn source_route(source: &Source, track: &Track) -> Option<Route> {
    let preview = || {
        Some(Preview {
            title: source.name.clone(),
            cover: None,
            artist: None,
        })
    };
    let route = match &source.kind {
        SourceRef::Album(id) => Route::Album {
            id: *id,
            preview: Some(Preview {
                title: source.name.clone(),
                cover: track.album.as_ref().and_then(|album| album.cover.clone()),
                artist: None,
            }),
        },
        SourceRef::Playlist(uuid) => Route::Playlist {
            uuid: uuid.clone(),
            preview: preview(),
        },
        SourceRef::Mix(id) | SourceRef::TrackRadio(id) => Route::Mix {
            id: id.clone(),
            preview: preview(),
        },
        SourceRef::Artist(id) => Route::Artist {
            id: *id,
            preview: preview(),
        },
        SourceRef::LovedTracks => Route::Favorites,
        SourceRef::Search(query) => Route::Search {
            query: query.clone(),
            tab: search::Tab::Tracks,
        },
        SourceRef::Track(_) => {
            let album = track.album.as_ref()?;
            Route::Album {
                id: album.id,
                preview: Some(Preview {
                    title: album.title.clone(),
                    cover: album.cover.clone(),
                    artist: None,
                }),
            }
        }
    };
    Some(route)
}
