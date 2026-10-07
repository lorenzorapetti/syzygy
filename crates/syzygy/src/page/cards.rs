//! Cards: one album, artist, playlist or mix each, laid out in rows that
//! scroll sideways with arrows, or in a grid. Home, Album, Artist and the
//! artist's view-all Page share them.

use iced::widget::{
    self as widget, Text, button, column, container, operation, row, scrollable, sensor, space,
    text,
};
use iced::{Alignment, Element, Theme};
use std::collections::HashMap;
use syzygy_catalog::home_feed::{Card, Target};

use super::{Action, Link, Preview, Route, cover};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::style;

pub const CARD_WIDTH: f32 = 160.0;
const CARD_GAP: f32 = 16.0;
/// A row this close to an end counts as at that end.
const SCROLL_SLACK: f32 = 10.0;

/// The card rows' scroll state, by each row's place on the Page.
#[derive(Debug, Default)]
pub struct Rows {
    rows: HashMap<usize, RowScroll>,
}

/// How far a card row is scrolled, and how wide it is on screen.
#[derive(Debug, Clone, Copy, Default)]
struct RowScroll {
    offset: f32,
    width: f32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Link(Link),
    /// A card row scrolled to this offset.
    Scrolled(usize, f32),
    /// A card row is this wide on screen.
    Sized(usize, f32),
    /// An arrow: a page of cards left (-1) or right (1).
    Step(usize, f32),
}

impl Rows {
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// `cards` says how many cards the row at an index has. Scrolls come
    /// back as the Page's messages through `wrap`.
    pub fn update(
        &mut self,
        message: Message,
        cards: impl FnOnce(usize) -> Option<usize>,
        wrap: fn(Message) -> super::Message,
    ) -> Action {
        match message {
            Message::Link(link) => link.follow(),
            Message::Scrolled(index, offset) => {
                self.rows.entry(index).or_default().offset = offset;
                Action::None
            }
            Message::Sized(index, width) => {
                self.rows.entry(index).or_default().width = width;
                Action::None
            }
            Message::Step(index, step) => {
                let Some(cards) = cards(index) else {
                    return Action::None;
                };
                // A scroll operation isn't reported through `on_scroll`, so
                // the new offset is worked out and kept here.
                let row = self.rows.entry(index).or_default();
                let end = (row_width(cards) - row.width).max(0.0);
                row.offset = (row.offset + step * row_page(row.width)).clamp(0.0, end);
                let offset = scrollable::AbsoluteOffset {
                    x: row.offset,
                    y: 0.0,
                };
                Action::Run(operation::scroll_to(row_id(index), offset).map(wrap))
            }
        }
    }

    /// A titled row of cards, with arrows, and a "View all" link when
    /// there's somewhere to see the whole section.
    pub fn view<'a>(
        &self,
        index: usize,
        title: &'a str,
        cards: &'a [Card],
        view_all: Option<Route>,
        images: &'a Images,
    ) -> Element<'a, Message> {
        let scroll = self.rows.get(&index).copied().unwrap_or_default();
        let content = row_width(cards.len());
        // Until the row has been measured, assume there's more to the right.
        let can_left = scroll.offset > SCROLL_SLACK;
        let can_right =
            scroll.width == 0.0 || scroll.offset + scroll.width < content - SCROLL_SLACK;
        let arrow = |glyph, step, enabled: bool| {
            let color = if enabled {
                style::TEXT_PRIMARY
            } else {
                style::TEXT_DISABLED
            };
            button(container(icon(glyph, 18.0, color)).center(32))
                .padding(0)
                .style(row_arrow)
                .on_press_maybe(enabled.then_some(Message::Step(index, step)))
        };
        let view_all = view_all.map(|route| {
            super::link(text("View all").size(13), Link::Open(route)).map(Message::Link)
        });
        let header = row![text(title).size(22), space::horizontal()]
            .push(view_all)
            .push(arrow(Icon::ChevronLeft, -1.0, can_left))
            .push(arrow(Icon::ChevronRight, 1.0, can_right))
            .spacing(8)
            .align_y(Alignment::Center);
        let cards = cards.iter().map(|c| card(c, images).map(Message::Link));
        let cards = scrollable(row(cards).spacing(CARD_GAP))
            .id(row_id(index))
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(move |viewport| Message::Scrolled(index, viewport.absolute_offset().x));
        let cards = sensor(cards)
            .on_show(move |size| Message::Sized(index, size.width))
            .on_resize(move |size| Message::Sized(index, size.width));
        column![header, cards].spacing(16).into()
    }
}

/// Cards wrapped onto as many lines as they need.
pub fn grid<'a>(cards: &'a [Card], images: &'a Images) -> Element<'a, Link> {
    row(cards.iter().map(|c| card(c, images)))
        .spacing(CARD_GAP)
        .wrap()
        .vertical_spacing(24)
        .into()
}

pub fn card<'a>(card: &'a Card, images: &'a Images) -> Element<'a, Link> {
    let line = |line: Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(CARD_WIDTH)
            .clip(true)
    };
    button(
        column![
            cover(images, card.cover.as_ref(), CARD_WIDTH),
            line(text(&card.title).size(14)),
            line(text(&card.subtitle).size(12).style(text::secondary)),
        ]
        .spacing(6),
    )
    .padding(0)
    .style(card_button)
    .on_press_maybe(route(card).map(Link::Open))
    .into()
}

/// Where a card leads, with what its Page can draw straight away. Tracks
/// and videos play once there's playback.
pub fn route(card: &Card) -> Option<Route> {
    // Only an album card's subtitle is its artist.
    let preview = |artist: bool| {
        Some(Preview {
            title: card.title.clone(),
            cover: card.cover.clone(),
            artist: (artist && !card.subtitle.is_empty()).then(|| card.subtitle.clone()),
        })
    };
    match &card.target {
        Target::Album(id) => Some(Route::Album {
            id: *id,
            preview: preview(true),
        }),
        Target::Artist(id) => Some(Route::Artist {
            id: *id,
            preview: preview(false),
        }),
        Target::Playlist(uuid) => Some(Route::Playlist {
            uuid: uuid.clone(),
            preview: preview(false),
        }),
        Target::Mix(id) => Some(Route::Mix {
            id: id.clone(),
            preview: preview(false),
        }),
        Target::Favorites => Some(Route::Favorites),
        Target::Track(_) | Target::Video(_) | Target::None => None,
    }
}

/// Each card row's scrollable, by its place on the Page.
fn row_id(index: usize) -> widget::Id {
    widget::Id::from(format!("card-row-{index}"))
}

/// The width of a row of `cards` cards.
fn row_width(cards: usize) -> f32 {
    (cards as f32 * (CARD_WIDTH + CARD_GAP) - CARD_GAP).max(0.0)
}

/// How far an arrow scrolls a row: as many whole cards as fit, so the row
/// lands on a card's edge.
fn row_page(width: f32) -> f32 {
    let stride = CARD_WIDTH + CARD_GAP;
    ((width + CARD_GAP) / stride).floor().max(1.0) * stride
}

fn card_button(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => style::ACCENT,
        _ => style::TEXT_PRIMARY,
    };
    button::Style {
        background: None,
        text_color,
        ..button::Style::default()
    }
}

fn row_arrow(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Disabled => None,
        button::Status::Hovered | button::Status::Pressed => Some(style::BG_BUTTON_HOVER.into()),
        button::Status::Active => Some(style::BG_BUTTON.into()),
    };
    button::Style {
        background,
        border: style::rounded(16.0),
        ..button::Style::default()
    }
}
