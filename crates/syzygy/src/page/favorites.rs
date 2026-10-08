//! The Favorites Page: the user's Loved tracks a page at a time, last added
//! first or sorted by TIDAL, and filtered here.

use iced::widget::{Column, column, container, text, text_input};
use iced::{Alignment, Element, Length};
use std::sync::Arc;
use syzygy_catalog::{Direction, Paged, Read, Track, TrackOrder, TrackSort};

use super::paged::List;
use super::track_list::{self, Columns};
use super::{Action, Context, Link, Load, PADDING, Viewport, count, hero, loved_art};
use crate::images::Images;
use crate::settings::Sort;
use crate::style;

const SPACING: f32 = 32.0;
const FILTER_HEIGHT: f32 = 40.0;
const FILTER_WIDTH: f32 = 360.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING + FILTER_HEIGHT + SPACING;
const COLUMNS: Columns = Columns {
    cover: true,
    album: true,
    date_added: true,
};
/// The Loved tracks' order until the user picks another, as in sone.
const LAST_ADDED_FIRST: TrackSort = TrackSort {
    order: TrackOrder::DateAdded,
    direction: Direction::Descending,
};

pub struct State {
    user_id: Option<u64>,
    /// `None` is last added first.
    sort: Option<TrackSort>,
    tracks: List<Track>,
    filter: String,
    /// The loaded tracks the filter lets through, by their place in the list.
    shown: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The first page of the tracks, in `sort`'s order.
    Tracks {
        sort: Option<TrackSort>,
        read: Read<Paged<Track>>,
    },
    More {
        sort: Option<TrackSort>,
        offset: usize,
        result: Result<Paged<Track>, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the list is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    /// A column title was clicked: sort by it, or `None` for last added
    /// first.
    Sort(Option<TrackOrder>),
    Filter(String),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(context: &Context) -> (Self, Action) {
        let mut state = Self {
            user_id: context.user_id,
            sort: context.settings.loved_tracks_sort,
            tracks: List::new(),
            filter: String::new(),
            shown: Vec::new(),
        };
        let action = state.load();
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Tracks { sort, read } if sort == self.sort => {
                self.tracks.apply(read, "Loved tracks");
                self.loaded()
            }
            Message::More {
                sort,
                offset,
                result,
            } if sort == self.sort => {
                self.tracks.more(offset, result);
                self.loaded()
            }
            // From before the sort changed.
            Message::Tracks { .. } | Message::More { .. } => Action::None,
            Message::EndInView => self.load_more(),
            Message::RetryMore => {
                let offset = self.tracks.retry();
                self.more_from(offset)
            }
            Message::Sort(order) => {
                let sort = TrackSort::clicked(Some(self.shown_sort()), order)
                    .filter(|sort| *sort != LAST_ADDED_FIRST);
                if sort == self.sort {
                    return Action::None;
                }
                self.sort = sort;
                Action::Batch(vec![self.load(), Action::SaveSort(Sort::LovedTracks(sort))])
            }
            Message::Filter(filter) => {
                self.filter = filter;
                self.loaded()
            }
            Message::Link(link) => link.follow(),
            Message::Retry => self.load(),
        }
    }

    /// Read the tracks again from the first page.
    fn load(&mut self) -> Action {
        self.tracks = List::new();
        self.shown.clear();
        match self.user_id {
            Some(user_id) => Action::Load(Load::LovedTracks {
                user_id,
                sort: self.sort,
            }),
            None => {
                self.tracks.fail(syzygy_catalog::Error::UnknownUser);
                Action::None
            }
        }
    }

    /// The tracks or the filter changed: filter them again, and while a
    /// filter is on, load the rest so it sees every track.
    fn loaded(&mut self) -> Action {
        self.shown = track_list::matching(self.tracks.items(), &self.filter);
        if self.filter.trim().is_empty() {
            Action::None
        } else {
            self.load_more()
        }
    }

    fn load_more(&mut self) -> Action {
        let offset = self.tracks.next();
        self.more_from(offset)
    }

    fn more_from(&self, offset: Option<usize>) -> Action {
        match (self.user_id, offset) {
            (Some(user_id), Some(offset)) => Action::Load(Load::MoreLovedTracks {
                user_id,
                sort: self.sort,
                offset,
            }),
            _ => Action::None,
        }
    }

    /// The order the list is in, as the column titles show it.
    fn shown_sort(&self) -> TrackSort {
        self.sort.unwrap_or(LAST_ADDED_FIRST)
    }

    pub fn view<'a>(&'a self, images: &'a Images, viewport: Viewport) -> Element<'a, Message> {
        let total = self
            .tracks
            .total()
            .map(|total| count(total, "Track").to_uppercase())
            .unwrap_or_default();
        let hero = hero::with_art(
            loved_art(hero::HEIGHT, 8.0),
            "COLLECTION",
            "Loved Tracks",
            vec![text(total).size(12).color(style::TEXT_MUTED).into()],
        )
        .map(Message::Link);
        let filter = container(
            text_input("Filter on title, artist or album", &self.filter)
                .on_input(Message::Filter)
                .padding([8, 14])
                .size(14)
                .width(FILTER_WIDTH)
                .style(style::filter_input),
        )
        .center_y(FILTER_HEIGHT);
        let body = self.tracks.list.view(Message::Retry, |_| {
            let tracks = self.tracks.items();
            if tracks.is_empty() {
                return empty("No Loved tracks yet", "Like a track to see it here.");
            }
            let header =
                track_list::sortable_header(COLUMNS, Some(self.shown_sort()), Message::Sort);
            let list = track_list::view(self.shown.len(), LIST_TOP, viewport, header, |i| {
                let position = self.shown[i];
                track_list::track(images, position + 1, &tracks[position], COLUMNS)
                    .map(Message::Link)
            });
            let nothing_matches = (self.shown.is_empty() && !self.filter.trim().is_empty())
                .then(|| empty("Nothing matches", "Try another title, artist or album."));
            Column::new()
                .push(list)
                .push(nothing_matches)
                .push(self.tracks.end(Message::EndInView, Message::RetryMore))
                .spacing(SPACING)
                .into()
        });
        column![hero, filter, body]
            .spacing(SPACING)
            .padding(PADDING)
            .into()
    }
}

fn empty<'a>(title: &'a str, line: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(18),
            text(line).size(14).color(style::TEXT_MUTED),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([48, 0])
    .into()
}
