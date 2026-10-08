//! The Feed Page: new releases from the artists the user follows, and
//! their monthly history mixes, grouped by month. The Shell marks it seen
//! as it opens.

use iced::widget::{Column, button, column, container, row, text};
use iced::{Alignment, Element, Length};
use std::sync::Arc;
use syzygy_catalog::feed::{Entry, Month};
use syzygy_catalog::{Feed, Read};

use super::{Action, Context, Link, Load, PADDING, Remote, cards, cover};
use crate::images::Images;
use crate::style;

const ART_SIZE: f32 = 56.0;

pub struct State {
    user_id: Option<u64>,
    feed: Remote<Feed>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Feed, Arc<syzygy_catalog::Error>>),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(context: &Context) -> (Self, Action) {
        let mut state = Self {
            user_id: context.user_id,
            feed: Remote::Loading,
        };
        let action = state.load();
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(result) => {
                self.feed.apply(Read::Fresh(result), "the Feed");
                Action::None
            }
            Message::Link(link) => link.follow(),
            Message::Retry => self.load(),
        }
    }

    fn load(&mut self) -> Action {
        match self.user_id {
            Some(user_id) => {
                self.feed = Remote::Loading;
                Action::Load(Load::Feed(user_id))
            }
            None => {
                self.feed = Remote::Failed(Arc::new(syzygy_catalog::Error::UnknownUser));
                Action::None
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let body = self.feed.view(Message::Retry, |feed| {
            let groups = feed.grouped(Month::now());
            if groups.is_empty() {
                return text("Nothing here yet").style(text::secondary).into();
            }
            let groups = groups.into_iter().map(|(period, entries)| {
                let rows = entries.into_iter().map(|e| entry(e, images));
                column![
                    text(period.label()).size(14).color(style::TEXT_SECONDARY),
                    Column::with_children(rows).spacing(4),
                ]
                .spacing(12)
                .into()
            });
            Column::with_children(groups).spacing(32).into()
        });
        column![text("Feed").size(32), body]
            .spacing(32)
            .padding(PADDING)
            .into()
    }
}

/// A release or mix: its cover, title and what it is, leading to it.
fn entry<'a>(entry: &'a Entry, images: &'a Images) -> Element<'a, Message> {
    let card = &entry.card;
    let line = |line: text::Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(Length::Fill)
            .clip(true)
    };
    let words = column![
        line(text(&card.title).size(14)),
        line(text(&card.subtitle).size(14).color(style::TEXT_SECONDARY)),
    ]
    .spacing(2);
    let open = cards::route(card).map(|route| Message::Link(Link::Open(route)));
    button(
        row![
            cover(images, card.cover.as_ref(), ART_SIZE).map(Message::Link),
            words
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding(8)
    .width(Length::Fill)
    .style(style::list_row)
    .on_press_maybe(open)
    .into()
}
