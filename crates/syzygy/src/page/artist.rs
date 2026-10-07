//! The Artist Page: the artist's sections as TIDAL lays them out. Track
//! sections show their first tracks and lead to all the top tracks; card
//! sections lead to the section in full.

use iced::widget::{Column, column, row, space, text};
use iced::{Alignment, Element};
use syzygy_catalog::artist::{Content, Section};
use syzygy_catalog::{Artist, Read};

use super::cards::{self, Rows};
use super::track_list::{self, Columns};
use super::{Action, Link, Load, PADDING, Preview, Remote, Route, hero, link};
use crate::images::Images;
use crate::style;

const SPACING: f32 = 32.0;
/// How many of a track section's tracks show here.
const TRACKS_SHOWN: usize = 10;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
};

pub struct State {
    id: u64,
    preview: Option<Preview>,
    artist: Remote<Artist>,
    cards: Rows,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Artist>),
    Cards(cards::Message),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(id: u64, preview: Option<Preview>) -> (Self, Action) {
        let state = Self {
            id,
            preview,
            artist: Remote::Loading,
            cards: Rows::default(),
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
            Message::Retry => {
                self.artist = Remote::Loading;
                Action::Load(load(self.id))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
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
            let sections = artist
                .sections
                .iter()
                .enumerate()
                .map(|(index, section)| self.section(index, section, images));
            Column::from_iter(sections).spacing(SPACING).into()
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
                    .map(|(i, track)| track_list::track(images, i + 1, track, COLUMNS));
                Element::from(column![header].extend(rows).spacing(8)).map(Message::Link)
            }
            Content::Cards(cards) => {
                let view_all = section.view_all.as_ref().map(|path| Route::ArtistViewAll {
                    id: self.id,
                    section: path.clone(),
                });
                self.cards
                    .view(index, &section.title, cards, view_all, images)
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
