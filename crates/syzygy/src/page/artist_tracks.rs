//! All of an artist's top tracks, loaded a page at a time as the user
//! scrolls.

use iced::widget::{Column, column, container, text};
use iced::{Element, Length};
use std::sync::Arc;
use syzygy_catalog::{Artist, Paged, Read, Track};

use super::paged::List;
use super::track_list::{self, Columns};
use super::{Action, Link, Load, PADDING, Remote, Viewport};
use crate::images::Images;
use crate::style;

const SPACING: f32 = 24.0;
const TITLE_HEIGHT: f32 = 72.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + TITLE_HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
    date_added: false,
};

pub struct State {
    id: u64,
    /// For the artist's name.
    artist: Remote<Artist>,
    tracks: List<Track>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Artist(Read<Artist>),
    Tracks(Read<Paged<Track>>),
    More {
        offset: usize,
        result: Result<Paged<Track>, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the list is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    Link(Link),
    Retry,
}

impl State {
    pub fn new(id: u64) -> (Self, Action) {
        let state = Self {
            id,
            artist: Remote::Loading,
            tracks: List::new(),
        };
        let action = Action::Batch(vec![
            Action::Load(artist(id)),
            Action::Load(Load::ArtistTracks(id)),
        ]);
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Artist(read) => {
                self.artist.apply(read, "an artist");
                Action::None
            }
            Message::Tracks(read) => {
                self.tracks.apply(read, "artist's top tracks");
                Action::None
            }
            Message::More { offset, result } => {
                self.tracks.more(offset, result);
                Action::None
            }
            Message::EndInView => load_more(self.id, self.tracks.next()),
            Message::RetryMore => load_more(self.id, self.tracks.retry()),
            Message::Link(link) => link.follow(),
            Message::Retry => {
                self.tracks = List::new();
                let tracks = Action::Load(Load::ArtistTracks(self.id));
                // The name comes from the artist's read; it may have failed too.
                if self.artist.loaded().is_some() {
                    return tracks;
                }
                self.artist = Remote::Loading;
                Action::Batch(vec![tracks, Action::Load(artist(self.id))])
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
        let name = self
            .artist
            .loaded()
            .map_or("", |artist| artist.name.as_str());
        let title = container(
            column![
                text("Popular tracks").size(32),
                text(name).size(14).color(style::TEXT_SECONDARY),
            ]
            .spacing(4),
        )
        .width(Length::Fill)
        .align_bottom(TITLE_HEIGHT);
        let body = self.tracks.list.view(Message::Retry, |_| {
            let tracks = self.tracks.items();
            let list = track_list::view(
                tracks.len(),
                LIST_TOP,
                viewport,
                track_list::header(COLUMNS),
                |i| track_list::track(images, i + 1, &tracks[i], COLUMNS).map(Message::Link),
            );
            Column::new()
                .push(list)
                .push(self.tracks.end(Message::EndInView, Message::RetryMore))
                .into()
        });
        column![title, body]
            .spacing(SPACING)
            .padding(PADDING)
            .into()
    }
}

/// Load the tracks from `offset`, if there's one to load.
fn load_more(id: u64, offset: Option<usize>) -> Action {
    match offset {
        Some(offset) => Action::Load(Load::MoreArtistTracks { id, offset }),
        None => Action::None,
    }
}

/// The artist's read, for their name.
fn artist(id: u64) -> Load {
    Load::Artist {
        id,
        then: |read| super::Message::ArtistTracks(Message::Artist(read)),
    }
}
