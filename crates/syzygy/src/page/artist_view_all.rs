//! One of an artist's sections in full, as a grid of cards loaded a page at
//! a time. The artist's other sections are tabs: switching one replaces
//! this Back stack entry rather than adding a step.

use iced::Element;
use iced::widget::{Column, button, column, row, text};
use std::sync::Arc;
use syzygy_catalog::artist::Content;
use syzygy_catalog::{Artist, Card, Paged, Read};

use super::cards;
use super::paged::List;
use super::{Action, Link, Load, PADDING, Remote, Route};
use crate::images::Images;
use crate::style;

pub struct State {
    id: u64,
    /// The view-all path of the section showing.
    section: String,
    /// For the artist's name and the tabs.
    artist: Remote<Artist>,
    cards: List<Card>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Artist(Read<Artist>),
    /// A read of a section's first page.
    Cards {
        section: String,
        read: Read<Paged<Card>>,
    },
    More {
        section: String,
        offset: usize,
        result: Result<Paged<Card>, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the grid is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    SelectTab(String),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(id: u64, section: String) -> (Self, Action) {
        let action = Action::Batch(vec![
            Action::Load(artist(id)),
            Action::Load(Load::ArtistViewAll {
                id,
                section: section.clone(),
            }),
        ]);
        let state = Self {
            id,
            section,
            artist: Remote::Loading,
            cards: List::new(),
        };
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Artist(read) => {
                self.artist.apply(read, "an artist");
                Action::None
            }
            // Reads for a tab the user just left are dropped.
            Message::Cards { section, read } if section == self.section => {
                self.cards.apply(read, "artist section");
                Action::None
            }
            Message::More {
                section,
                offset,
                result,
            } if section == self.section => {
                self.cards.more(offset, result);
                Action::None
            }
            Message::Cards { .. } | Message::More { .. } => Action::None,
            Message::EndInView => {
                let offset = self.cards.next();
                self.load_more(offset)
            }
            Message::RetryMore => {
                let offset = self.cards.retry();
                self.load_more(offset)
            }
            Message::SelectTab(section) if section != self.section => {
                self.section = section;
                self.cards = List::new();
                Action::Replace(
                    Route::ArtistViewAll {
                        id: self.id,
                        section: self.section.clone(),
                    },
                    Some(self.load()),
                )
            }
            Message::SelectTab(_) => Action::None,
            Message::Link(link) => link.follow(),
            Message::Retry => {
                self.cards = List::new();
                let cards = Action::Load(self.load());
                // The tabs come from the artist's read; it may have failed too.
                if self.artist.loaded().is_some() {
                    return cards;
                }
                self.artist = Remote::Loading;
                Action::Batch(vec![cards, Action::Load(artist(self.id))])
            }
        }
    }

    fn load(&self) -> Load {
        Load::ArtistViewAll {
            id: self.id,
            section: self.section.clone(),
        }
    }

    /// Load the section from `offset`, if there's one to load.
    fn load_more(&self, offset: Option<usize>) -> Action {
        match offset {
            Some(offset) => Action::Load(Load::MoreArtistViewAll {
                id: self.id,
                section: self.section.clone(),
                offset,
            }),
            None => Action::None,
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let artist = self.artist.loaded();
        let tabs: Vec<(&str, &str)> = artist
            .map(|artist| {
                artist
                    .sections
                    .iter()
                    .filter(|s| matches!(s.content, Content::Cards(_)))
                    .filter_map(|s| Some((s.title.as_str(), s.view_all.as_deref()?)))
                    .collect()
            })
            .unwrap_or_default();
        let title = tabs
            .iter()
            .find(|(_, path)| *path == self.section)
            .map_or("", |(title, _)| title);
        let header = column![
            text(title).size(32),
            text(artist.map_or("", |a| a.name.as_str()))
                .size(14)
                .color(style::TEXT_SECONDARY),
        ]
        .spacing(4);
        let tab_bar = row(tabs.iter().map(|(title, path)| {
            let style = if *path == self.section {
                style::selected_tab
            } else {
                style::unselected_tab
            };
            button(text(*title).size(14))
                .padding([8, 16])
                .style(style)
                .on_press(Message::SelectTab(path.to_string()))
                .into()
        }))
        .spacing(8)
        .wrap();
        let body = self.cards.list.view(Message::Retry, |_| {
            Column::new()
                .push(cards::grid(self.cards.items(), images).map(Message::Link))
                .push(self.cards.end(Message::EndInView, Message::RetryMore))
                .spacing(24)
                .into()
        });
        column![header, tab_bar, body]
            .spacing(24)
            .padding(PADDING)
            .into()
    }
}

/// The artist's read, for their name and the tabs.
fn artist(id: u64) -> Load {
    Load::Artist {
        id,
        then: |read| super::Message::ArtistViewAll(Message::Artist(read)),
    }
}
