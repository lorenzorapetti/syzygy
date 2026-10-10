//! The Mix Page: a mix's tracks.

use iced::widget::{Column, row, text};
use iced::{Alignment, Element};
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Mix, Read};

use super::track_list::{self, Columns, Hover, Mark};
use super::{
    Action, Link, Load, NowPlaying, PADDING, PLAY_BUTTONS_HEIGHT, Preview, Remote, Viewport, count,
    header_actions, hero, mix_source, play_buttons,
};
use crate::images::Images;
use crate::library::{Favorite, Library};
use crate::playback::{Source, Start};
use crate::style;

const SPACING: f32 = 32.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING + PLAY_BUTTONS_HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
    date_added: false,
};

pub struct State {
    id: String,
    preview: Option<Preview>,
    mix: Remote<Mix>,
    /// The track the pointer is over, by its place in the mix.
    hover: Hover,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Mix>),
    Link(Link),
    /// Play the mix, from a track by its place or all of it.
    Play(Start),
    TogglePlay,
    /// The pointer came over the row with this key.
    Hovered(usize),
    /// The pointer left the row with this key.
    Left(usize),
    Retry,
}

impl State {
    pub fn new(id: String, preview: Option<Preview>) -> (Self, Action) {
        let action = Action::Load(Load::Mix(id.clone()));
        let state = Self {
            id,
            preview,
            mix: Remote::Loading,
            hover: Hover::default(),
        };
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.mix.apply(read, "a mix");
                Action::None
            }
            Message::Link(link) => link.follow(),
            Message::Play(start) => match self.mix.loaded() {
                Some(mix) => {
                    let source = self.source(mix);
                    Action::Play(super::request(
                        source.kind,
                        &source.name,
                        &mix.tracks,
                        start,
                    ))
                }
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
                self.mix = Remote::Loading;
                Action::Load(Load::Mix(self.id.clone()))
            }
        }
    }

    pub fn shows_track(&self, track_id: u64) -> bool {
        self.mix
            .loaded()
            .is_some_and(|mix| mix.tracks.iter().any(|track| track.id == track_id))
    }

    fn source(&self, mix: &Mix) -> Source {
        let fallback = self.preview.as_ref().map_or("", |p| p.title.as_str());
        mix_source(&self.id, mix, fallback)
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        viewport: Viewport,
        now_playing: Option<NowPlaying<'a>>,
        allow_explicit: bool,
    ) -> Element<'a, Message> {
        let hero = match (&self.mix, &self.preview) {
            (Remote::Loaded(mix), preview) => Some(mix_hero(mix, preview.as_ref(), images)),
            (Remote::Loading, Some(preview)) => Some(hero::preview(images, "MIX", preview, false)),
            _ => None,
        };
        let body = self.mix.view(Message::Retry, |mix| {
            let source = self.source(mix);
            let card = Card {
                title: source.name.clone(),
                subtitle: mix.subtitle.clone().unwrap_or_default(),
                cover: mix
                    .cover
                    .clone()
                    .or_else(|| self.preview.as_ref().and_then(|p| p.cover.clone())),
                target: Target::Mix(self.id.clone()),
            };
            let favorite = Favorite::card(&card);
            let buttons = row![
                play_buttons(
                    &source.kind,
                    now_playing,
                    Message::Play,
                    Message::TogglePlay
                ),
                header_actions(card, favorite, library).map(Message::Link),
            ]
            .spacing(24)
            .align_y(Alignment::Center);
            let list = track_list::view(
                mix.tracks.len(),
                LIST_TOP,
                viewport,
                track_list::header(COLUMNS),
                |i| {
                    let track = &mix.tracks[i];
                    let liked = library.liked(track);
                    let mark = Mark::of(track, now_playing, self.hover.over(i));
                    let row = track_list::marked(
                        images,
                        i + 1,
                        track,
                        COLUMNS,
                        allow_explicit,
                        mark,
                        liked,
                    );
                    track_list::playable_track(
                        row.map(Message::Link),
                        mark,
                        Message::Play(Start::Track(i)),
                        i,
                        Message::Hovered,
                        Message::Left,
                    )
                },
            );
            Column::new()
                .push(buttons)
                .push(list)
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
}

/// A mix read from TIDAL's older endpoint has only its tracks, so the
/// card fills in what's missing.
fn mix_hero<'a>(
    mix: &'a Mix,
    preview: Option<&'a Preview>,
    images: &'a Images,
) -> Element<'a, Link> {
    let title = match (mix.title.as_str(), preview) {
        ("", Some(preview)) => preview.title.as_str(),
        (title, _) => title,
    };
    let cover = mix
        .cover
        .as_ref()
        .or_else(|| preview.and_then(|p| p.cover.as_ref()));
    let lines = mix
        .subtitle
        .iter()
        .map(|subtitle| text(subtitle).size(15).color(style::TEXT_SECONDARY))
        .chain([text(count(mix.tracks.len(), "Track"))
            .size(13)
            .color(style::TEXT_MUTED)])
        .map(Element::from)
        .collect();
    hero::view(images, "MIX", title, cover, false, lines)
}
