//! The Mix Page: a mix's tracks.

use iced::Element;
use iced::widget::{Column, text};
use syzygy_catalog::{Mix, Read};

use super::track_list::{self, Columns};
use super::{Action, Link, Load, PADDING, Preview, Remote, Viewport, count, hero};
use crate::images::Images;
use crate::style;

const SPACING: f32 = 32.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
};

pub struct State {
    id: String,
    preview: Option<Preview>,
    mix: Remote<Mix>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Mix>),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(id: String, preview: Option<Preview>) -> (Self, Action) {
        let action = Action::Load(Load::Mix(id.clone()));
        let state = Self {
            id,
            preview,
            mix: Remote::Loading,
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
            Message::Retry => {
                self.mix = Remote::Loading;
                Action::Load(Load::Mix(self.id.clone()))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
        let hero = match (&self.mix, &self.preview) {
            (Remote::Loaded(mix), preview) => Some(mix_hero(mix, preview.as_ref(), images)),
            (Remote::Loading, Some(preview)) => Some(hero::preview(images, "MIX", preview, false)),
            _ => None,
        };
        let body = self.mix.view(Message::Retry, |mix| {
            track_list::view(mix.tracks.len(), LIST_TOP, viewport, COLUMNS, |i| {
                track_list::track(images, i + 1, &mix.tracks[i], COLUMNS).map(Message::Link)
            })
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
