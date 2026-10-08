//! The Album Page: the album's tracks, by volume when it has more than one,
//! then "More by …" and the other sections TIDAL sends with it.

use iced::Element;
use iced::widget::{Column, column, mouse_area, text};
use syzygy_catalog::{Album, Read};

use super::cards::{self, Rows};
use super::track_list::{self, Columns, Mark};
use super::{
    Action, Link, Load, NowPlaying, PADDING, PLAY_BUTTONS_HEIGHT, Preview, Remote, Viewport,
    album_tracks, artists, count, duration, hero, play_buttons,
};
use crate::images::Images;
use crate::playback::{SourceRef, Start};
use crate::style;

/// Between the hero, the list and the sections.
const SPACING: f32 = 32.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING + PLAY_BUTTONS_HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: false,
    album: false,
    date_added: false,
};

pub struct State {
    id: u64,
    preview: Option<Preview>,
    album: Remote<Album>,
    /// The list's rows, with volume headings when there's more than one.
    rows: Vec<Row>,
    cards: Rows,
    /// The track the pointer is over, by its place in the album.
    hovered: Option<usize>,
}

enum Row {
    Volume(u32),
    /// A track by its place in the album, and its number in its volume.
    Track(usize, usize),
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Album>),
    Cards(cards::Message),
    Link(Link),
    /// Play the album, from a track by its place in the album or all of it.
    Play(Start),
    TogglePlay,
    /// The pointer came over the track at this place.
    Hovered(usize),
    /// The pointer left the track at this place.
    Left(usize),
    Retry,
}

impl State {
    pub fn new(id: u64, preview: Option<Preview>) -> (Self, Action) {
        let state = Self {
            id,
            preview,
            album: Remote::Loading,
            rows: Vec::new(),
            cards: Rows::default(),
            hovered: None,
        };
        (state, Action::Load(Load::Album(id)))
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.album.apply(read, "an album");
                if let Remote::Loaded(album) = &self.album {
                    self.rows = rows(album);
                }
                Action::None
            }
            Message::Cards(message) => {
                let album = self.album.loaded();
                let cards = |index: usize| {
                    album
                        .and_then(|album| album.sections.get(index))
                        .map(|section| section.cards.len())
                };
                self.cards.update(message, cards, |message| {
                    super::Message::Album(Message::Cards(message))
                })
            }
            Message::Link(link) => link.follow(),
            Message::Play(start) => match self.album.loaded() {
                Some(album) => Action::Play(super::request(
                    SourceRef::Album(self.id),
                    &album.title,
                    &album_tracks(self.id, album),
                    start,
                )),
                None => Action::None,
            },
            Message::TogglePlay => Action::TogglePlay,
            Message::Hovered(index) => {
                self.hovered = Some(index);
                Action::None
            }
            // Only the row it left: the next row's arrival may come first.
            Message::Left(index) => {
                if self.hovered == Some(index) {
                    self.hovered = None;
                }
                Action::None
            }
            Message::Retry => {
                self.album = Remote::Loading;
                Action::Load(Load::Album(self.id))
            }
        }
    }

    pub fn shows_track(&self, track_id: u64) -> bool {
        self.album
            .loaded()
            .is_some_and(|album| album.tracks.iter().any(|track| track.id == track_id))
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        viewport: Viewport,
        now_playing: Option<NowPlaying<'a>>,
    ) -> Element<'a, Message> {
        let hero = match (&self.album, &self.preview) {
            (Remote::Loaded(album), _) => Some(album_hero(album, images)),
            (Remote::Loading, Some(preview)) => {
                Some(hero::preview(images, "ALBUM", preview, false))
            }
            _ => None,
        };
        let body = self.album.view(Message::Retry, |album| {
            let row = |i| self.row(i, album, images, now_playing);
            let list = track_list::view(
                self.rows.len(),
                LIST_TOP,
                viewport,
                track_list::header(COLUMNS),
                row,
            );
            let footer = |line| text(line).size(12).color(style::TEXT_DISABLED);
            let footer = column![]
                .push(album.release_date.as_deref().map(footer))
                .push(album.copyright.as_deref().map(footer))
                .spacing(4);
            let sections = album.sections.iter().enumerate().map(|(index, section)| {
                self.cards
                    .view(index, &section.title, &section.cards, None, images)
                    .map(Message::Cards)
            });
            let buttons = play_buttons(
                &SourceRef::Album(self.id),
                now_playing,
                Message::Play,
                Message::TogglePlay,
            );
            column![buttons, list, footer]
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

    fn row<'a>(
        &self,
        i: usize,
        album: &'a Album,
        images: &'a Images,
        now_playing: Option<NowPlaying>,
    ) -> Element<'a, Message> {
        match self.rows[i] {
            Row::Volume(volume) => track_list::heading(format!("Volume {volume}")),
            Row::Track(index, number) => {
                let track = &album.tracks[index];
                let now = now_playing.filter(|now| now.track_id == track.id);
                let hovered = self.hovered == Some(index);
                let mark = match now {
                    Some(NowPlaying {
                        playing: Some(at), ..
                    }) => Mark::Playing(at),
                    Some(_) => Mark::Current { hovered },
                    None if hovered => Mark::Hovered,
                    None => Mark::None,
                };
                let line = track_list::marked(images, number, track, COLUMNS, mark);
                let row = track_list::playable(
                    line.map(Message::Link),
                    Message::Play(Start::Track(index)),
                    now.is_some(),
                );
                mouse_area(row)
                    .on_enter(Message::Hovered(index))
                    .on_exit(Message::Left(index))
                    .into()
            }
        }
    }
}

fn album_hero<'a>(album: &'a Album, images: &'a Images) -> Element<'a, Link> {
    let year = album.release_date.as_deref().and_then(|d| d.get(..4));
    let facts = [
        year.map(str::to_string),
        Some(count(album.tracks.len(), "Track")),
        Some(duration(album.duration)),
        album.quality.map(str::to_string),
    ];
    let facts: Vec<String> = facts.into_iter().flatten().collect();
    let lines = vec![
        artists(&album.artists, 15.0).into(),
        text(facts.join(" · "))
            .size(13)
            .color(style::TEXT_MUTED)
            .into(),
    ];
    let kind = match album.kind.as_deref() {
        Some("EP") => "EP",
        Some("SINGLE") => "SINGLE",
        _ => "ALBUM",
    };
    hero::view(
        images,
        kind,
        &album.title,
        album.cover.as_ref(),
        false,
        lines,
    )
}

/// The tracks, numbered within their volume, under a heading for each
/// volume when there's more than one.
fn rows(album: &Album) -> Vec<Row> {
    let volumes = album.tracks.iter().map(|t| t.volume).max().unwrap_or(1);
    let mut rows = Vec::with_capacity(album.tracks.len() + volumes as usize);
    let mut volume = None;
    let mut number = 0;
    for (index, track) in album.tracks.iter().enumerate() {
        if volume != Some(track.volume) {
            volume = Some(track.volume);
            number = 0;
            if volumes > 1 {
                rows.push(Row::Volume(track.volume));
            }
        }
        number += 1;
        rows.push(Row::Track(index, number));
    }
    rows
}
