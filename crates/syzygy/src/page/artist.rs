//! The Artist Page: the artist's sections as TIDAL lays them out. Track
//! sections show their first tracks and lead to all the top tracks; card
//! sections lead to the section in full.

use iced::widget::{Column, column, row, space, text};
use iced::{Alignment, Element};
use syzygy_catalog::artist::{Content, Section};
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Artist, Read};

use super::cards::{self, Rows};
use super::track_list::{self, Columns, Hover, Mark};
use super::{
    Action, Link, Load, NowPlaying, PADDING, Preview, Remote, Route, header_actions, hero, link,
    play_buttons, top_tracks,
};
use crate::images::Images;
use crate::library::{Favorite, Library};
use crate::playback::{SourceRef, Start};
use crate::style;

const SPACING: f32 = 32.0;
/// How many of a track section's tracks show here.
const TRACKS_SHOWN: usize = 10;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
    date_added: false,
};

pub struct State {
    id: u64,
    preview: Option<Preview>,
    artist: Remote<Artist>,
    cards: Rows,
    /// The top track the pointer is over, by its place.
    hover: Hover,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Artist>),
    Cards(cards::Message),
    Link(Link),
    /// Play the top tracks, from one by its place or all of them.
    Play(Start),
    TogglePlay,
    /// The pointer came over the row with this key.
    Hovered(usize),
    /// The pointer left the row with this key.
    Left(usize),
    Retry,
}

impl State {
    pub fn new(id: u64, preview: Option<Preview>) -> (Self, Action) {
        let state = Self {
            id,
            preview,
            artist: Remote::Loading,
            cards: Rows::default(),
            hover: Hover::default(),
        };
        (state, Action::Load(load(id)))
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.artist.apply(read, "an artist");
                Action::None
            }
            Message::Cards(message) => {
                let artist = self.artist.loaded();
                let cards = |index: usize| match artist
                    .and_then(|a| a.sections.get(index))
                    .map(|s| &s.content)
                {
                    Some(Content::Cards(cards)) => Some(cards.len()),
                    _ => None,
                };
                self.cards.update(message, cards, |message| {
                    super::Message::Artist(Message::Cards(message))
                })
            }
            Message::Link(link) => link.follow(),
            Message::Play(start) => match self.artist.loaded() {
                Some(artist) => match top_tracks(artist) {
                    Some((_, tracks)) => Action::Play(super::request(
                        SourceRef::Artist(self.id),
                        &artist.name,
                        tracks,
                        start,
                    )),
                    None => Action::None,
                },
                None => Action::None,
            },
            Message::TogglePlay => Action::TogglePlay,
            Message::Hovered(key) => {
                self.hover.entered(key);
                Action::None
            }
            Message::Left(key) => {
                self.hover.left(key);
                Action::None
            }
            Message::Retry => {
                self.artist = Remote::Loading;
                Action::Load(load(self.id))
            }
        }
    }

    /// Whether the top tracks shown here have the track.
    pub fn shows_track(&self, track_id: u64) -> bool {
        self.artist
            .loaded()
            .and_then(top_tracks)
            .is_some_and(|(_, tracks)| {
                tracks
                    .iter()
                    .take(TRACKS_SHOWN)
                    .any(|track| track.id == track_id)
            })
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        now_playing: Option<NowPlaying<'a>>,
        allow_explicit: bool,
    ) -> Element<'a, Message> {
        let hero = match (&self.artist, &self.preview) {
            (Remote::Loaded(artist), _) => Some(artist_hero(artist, images)),
            (Remote::Loading, Some(preview)) => {
                Some(hero::preview(images, "ARTIST", preview, true))
            }
            _ => None,
        };
        let body = self.artist.view(Message::Retry, |artist| {
            if artist.sections.is_empty() {
                return text("This artist doesn't have any tracks or albums yet.")
                    .style(text::secondary)
                    .into();
            }
            let top = top_tracks(artist).map(|(index, _)| index);
            let buttons = top.map(|_| {
                play_buttons(
                    &SourceRef::Artist(self.id),
                    now_playing,
                    Message::Play,
                    Message::TogglePlay,
                )
            });
            let card = Card {
                title: artist.name.clone(),
                subtitle: "Artist".to_string(),
                cover: artist.picture.clone(),
                target: Target::Artist(self.id),
            };
            let favorite = Favorite::card(&card);
            let actions = header_actions(card, favorite, library).map(Message::Link);
            let buttons = row![]
                .push(buttons)
                .push(actions)
                .spacing(24)
                .align_y(Alignment::Center);
            let sections = artist.sections.iter().enumerate().map(|(index, section)| {
                // Only the top tracks play, as the artist's source.
                let now_playing = (top == Some(index)).then_some(now_playing);
                self.section(index, section, images, library, now_playing, allow_explicit)
            });
            Column::new()
                .push(buttons)
                .extend(sections)
                .spacing(SPACING)
                .into()
        });
        Column::new()
            .push(hero.map(|hero| hero.map(Message::Link)))
            .push(body)
            .spacing(SPACING)
            .padding(PADDING)
            .into()
    }

    fn section<'a>(
        &'a self,
        index: usize,
        section: &'a Section,
        images: &'a Images,
        library: &'a Library,
        playable: Option<Option<NowPlaying>>,
        allow_explicit: bool,
    ) -> Element<'a, Message> {
        match &section.content {
            Content::Tracks(tracks) => {
                let all = Route::ArtistTracks { id: self.id };
                let header = row![
                    text(&section.title).size(22),
                    space::horizontal(),
                    link(text("View all").size(13), Link::Open(all)),
                ]
                .align_y(Alignment::Center);
                let rows = tracks
                    .iter()
                    .take(TRACKS_SHOWN)
                    .enumerate()
                    .map(|(i, track)| {
                        let liked = library.liked(track);
                        let mark = match playable {
                            Some(now_playing) => Mark::of(track, now_playing, self.hover.over(i)),
                            None => Mark::None,
                        };
                        let row = track_list::marked(
                            images,
                            i + 1,
                            track,
                            COLUMNS,
                            allow_explicit,
                            mark,
                            liked,
                        )
                        .map(Message::Link);
                        match playable {
                            Some(_) => track_list::playable_track(
                                row,
                                mark,
                                Message::Play(Start::Track(i)),
                                i,
                                Message::Hovered,
                                Message::Left,
                            ),
                            None => row,
                        }
                    });
                column![Element::from(header).map(Message::Link)]
                    .extend(rows)
                    .spacing(8)
                    .into()
            }
            Content::Cards(cards) => {
                let view_all = section.view_all.as_ref().map(|path| Route::ArtistViewAll {
                    id: self.id,
                    section: path.clone(),
                });
                self.cards
                    .view(index, &section.title, cards, view_all, images, library)
                    .map(Message::Cards)
            }
        }
    }
}

fn load(id: u64) -> Load {
    Load::Artist {
        id,
        then: |read| super::Message::Artist(Message::Loaded(read)),
    }
}

fn artist_hero<'a>(artist: &'a Artist, images: &'a Images) -> Element<'a, Link> {
    let fans = artist
        .followers
        .filter(|&n| n > 0)
        .map(|n| text(format!("{} fans", compact(n))).size(14));
    let bio = artist.bio.as_deref().map(|bio| {
        // Its first sentence: the hero has no room for more.
        let first = bio.split_terminator(['.', '\n']).next().unwrap_or(bio);
        text(format!("{}.", first.trim()))
            .size(13)
            .color(style::TEXT_MUTED)
    });
    let lines = fans.into_iter().chain(bio).map(Element::from).collect();
    hero::view(
        images,
        "ARTIST",
        &artist.name,
        artist.picture.as_ref(),
        true,
        lines,
    )
}

/// "1.2M", "34.5K", "980", as `Intl.NumberFormat`'s compact notation.
fn compact(n: u64) -> String {
    let (value, unit) = match n {
        1_000_000.. => (n as f64 / 1_000_000.0, "M"),
        1_000.. => (n as f64 / 1_000.0, "K"),
        _ => return n.to_string(),
    };
    let rounded = format!("{value:.1}");
    format!("{}{unit}", rounded.trim_end_matches(".0"))
}
