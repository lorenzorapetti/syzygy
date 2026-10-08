//! The Search Page: what a search found, a tab per type (sone's
//! `SearchView`, without videos). The tab is part of the route, and
//! switching it replaces this Back stack entry rather than adding a step.
//! The results are read once and every tab draws from them.

use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Alignment, Element, Length, Theme};
use std::sync::Arc;
use syzygy_catalog::home_feed::Card;
use syzygy_catalog::{Hit, Read, SearchResults, Track};

use super::cards;
use super::track_list::{self, Columns};
use super::{Action, Link, Load, PADDING, Remote, Route, Viewport, rounded_cover, single};
use crate::images::Images;
use crate::playback::{SourceRef, Start};
use crate::style;

/// Between the tabs and the results, and between sections.
const SPACING: f32 = 24.0;
/// The row of tabs is always this tall, so the track list's place is known.
const TABS_HEIGHT: f32 = 36.0;
/// Where the Tracks tab's list starts.
const LIST_TOP: f32 = PADDING + TABS_HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
    date_added: false,
};
/// How many of each type All Results shows, as in sone.
const TRACKS_PREVIEW: usize = 8;
const CARDS_PREVIEW: usize = 6;
const HIT_ART: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    All,
    TopHits,
    Tracks,
    Playlists,
    Albums,
    Artists,
}

impl Tab {
    const TABS: [Tab; 6] = [
        Tab::All,
        Tab::TopHits,
        Tab::Tracks,
        Tab::Playlists,
        Tab::Albums,
        Tab::Artists,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::All => "All Results",
            Tab::TopHits => "Top Hits",
            Tab::Tracks => "Tracks",
            Tab::Playlists => "Playlists",
            Tab::Albums => "Albums",
            Tab::Artists => "Artists",
        }
    }
}

pub struct State {
    query: String,
    tab: Tab,
    results: Remote<SearchResults>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<SearchResults, Arc<syzygy_catalog::Error>>),
    SelectTab(Tab),
    Link(Link),
    /// Play the tracks found, from the one at this place.
    Play(usize),
    Retry,
}

impl State {
    pub fn new(query: String, tab: Tab) -> (Self, Action) {
        let load = Load::Search(query.clone());
        let state = Self {
            query,
            tab,
            results: Remote::Loading,
        };
        (state, Action::Load(load))
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(result) => {
                self.results.apply(Read::Fresh(result), "a search");
                Action::None
            }
            Message::SelectTab(tab) if tab != self.tab => {
                self.tab = tab;
                let route = Route::Search {
                    query: self.query.clone(),
                    tab,
                };
                Action::Replace(route, None)
            }
            Message::SelectTab(_) => Action::None,
            Message::Link(link) => link.follow(),
            Message::Play(index) => match self.results.loaded() {
                Some(results) => Action::Play(super::request(
                    SourceRef::Search(self.query.clone()),
                    &self.query,
                    &results.tracks,
                    Start::Track(index),
                )),
                None => Action::None,
            },
            Message::Retry => {
                self.results = Remote::Loading;
                Action::Load(Load::Search(self.query.clone()))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
        let tabs = row(Tab::TABS.into_iter().map(|tab| {
            let style = if tab == self.tab {
                style::selected_tab
            } else {
                style::unselected_tab
            };
            button(text(tab.label()).size(13))
                .padding([8, 16])
                .style(style)
                .on_press(Message::SelectTab(tab))
                .into()
        }))
        .spacing(8);
        let tabs = container(tabs).center_y(TABS_HEIGHT);
        let body = self.results.view(Message::Retry, |results| {
            if results.is_empty() {
                return column![
                    text("No results found").size(20),
                    text("Try a different search term").color(style::TEXT_MUTED),
                ]
                .spacing(8)
                .into();
            }
            match self.tab {
                Tab::All => all(results, images),
                Tab::TopHits => top_hits(&results.top_hits, images),
                Tab::Tracks => tracks(&results.tracks, images, viewport),
                Tab::Playlists => grid(&results.playlists, "playlists", images),
                Tab::Albums => grid(&results.albums, "albums", images),
                Tab::Artists => grid(&results.artists, "artists", images),
            }
        });
        Column::new()
            .push(tabs)
            .push(body)
            .spacing(SPACING)
            .padding(PADDING)
            .into()
    }
}

/// A few of each type, each under its title with a way to its tab.
fn all<'a>(results: &'a SearchResults, images: &'a Images) -> Element<'a, Message> {
    let section = |title: &'a str, tab: Tab, content: Element<'a, Message>| {
        let header = row![
            text(title).size(22),
            space::horizontal(),
            button(text("View all").size(13))
                .padding(0)
                .style(view_all_style)
                .on_press(Message::SelectTab(tab)),
        ]
        .align_y(Alignment::Center);
        column![header, content].spacing(12).into()
    };
    let cards = |cards: &'a [Card]| {
        let cards = cards.iter().take(CARDS_PREVIEW);
        cards::wrapped(cards.map(|card| cards::card(card, images))).map(Message::Link)
    };
    let mut sections: Vec<Element<'a, Message>> = Vec::new();
    if !results.tracks.is_empty() {
        let rows = results
            .tracks
            .iter()
            .take(TRACKS_PREVIEW)
            .enumerate()
            .map(|(i, track)| track_row(images, i, track));
        let list = Column::with_children(rows).into();
        sections.push(section("Tracks", Tab::Tracks, list));
    }
    for (title, tab, list) in [
        ("Playlists", Tab::Playlists, &results.playlists),
        ("Albums", Tab::Albums, &results.albums),
        ("Artists", Tab::Artists, &results.artists),
    ] {
        if !list.is_empty() {
            sections.push(section(title, tab, cards(list)));
        }
    }
    Column::with_children(sections).spacing(32).into()
}

fn top_hits<'a>(hits: &'a [Hit], images: &'a Images) -> Element<'a, Message> {
    if hits.is_empty() {
        return none_found("top hits");
    }
    let hits = Column::with_children(hits.iter().map(|h| hit(h, images)))
        .spacing(4)
        .width(Length::Fill);
    Element::from(hits).map(Message::Link)
}

fn tracks<'a>(tracks: &'a [Track], images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
    if tracks.is_empty() {
        return none_found("tracks");
    }
    track_list::view(
        tracks.len(),
        LIST_TOP,
        viewport,
        track_list::header(COLUMNS),
        |i| track_row(images, i, &tracks[i]),
    )
}

/// A track found, which plays the tracks found from there.
fn track_row<'a>(images: &'a Images, index: usize, track: &'a Track) -> Element<'a, Message> {
    let row = track_list::track(images, index + 1, track, COLUMNS).map(Message::Link);
    track_list::playable(row, Message::Play(index), false)
}

fn grid<'a>(cards: &'a [Card], what: &str, images: &'a Images) -> Element<'a, Message> {
    if cards.is_empty() {
        return none_found(what);
    }
    cards::grid(cards, images).map(Message::Link)
}

fn none_found<'a>(what: &str) -> Element<'a, Message> {
    text(format!("No {what} found"))
        .color(style::TEXT_MUTED)
        .into()
}

/// One of the best matches, as a row: its picture, its title, and what it
/// is and by whom. A track plays on its own.
pub fn hit<'a>(hit: &'a Hit, images: &'a Images) -> Element<'a, Link> {
    let (art, title, subtitle, open) = match hit {
        Hit::Card(card) => {
            let round = matches!(card.target, syzygy_catalog::home_feed::Target::Artist(_));
            let radius = if round { HIT_ART / 2.0 } else { 4.0 };
            (
                rounded_cover(images, card.cover.as_ref(), HIT_ART, radius),
                card.title.as_str(),
                card.subtitle.clone(),
                cards::route(card).map(Link::Open),
            )
        }
        Hit::Track(track) => {
            let album = track.album.as_ref();
            let names: Vec<&str> = track.artists.iter().map(|a| a.name.as_str()).collect();
            let subtitle = if names.is_empty() {
                "Track".to_string()
            } else {
                format!("Track · {}", names.join(", "))
            };
            let cover = album.and_then(|album| album.cover.as_ref());
            (
                rounded_cover(images, cover, HIT_ART, 4.0),
                track.title.as_str(),
                subtitle,
                Some(Link::Play(single(track))),
            )
        }
    };
    let line = |line: text::Text<'a>| container(line.wrapping(text::Wrapping::None)).clip(true);
    let words = column![
        line(text(title).size(14)),
        line(text(subtitle).size(12).color(style::TEXT_MUTED)),
    ]
    .spacing(2);
    button(row![art, words].spacing(12).align_y(Alignment::Center))
        .padding([6, 12])
        .width(Length::Fill)
        .style(style::list_row)
        .on_press_maybe(open)
        .into()
}

fn view_all_style(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => style::TEXT_PRIMARY,
        _ => style::TEXT_MUTED,
    };
    button::Style {
        background: None,
        text_color,
        ..button::Style::default()
    }
}
