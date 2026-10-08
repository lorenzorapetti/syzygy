//! Cards: one album, artist, playlist or mix each, laid out in rows that
//! scroll sideways with arrows, or in a grid. Home, Album, Artist and the
//! artist's view-all Page share them.

use iced::widget::{
    self as widget, Text, button, column, container, hover, operation, row, scrollable, sensor,
    space, text,
};
use iced::{Alignment, Element, Length, Theme};
use std::collections::HashMap;
use syzygy_catalog::home_feed::{Card, Target};

use super::{Action, Link, Preview, Route, cover};
use crate::icons::{Icon, filled, icon};
use crate::images::Images;
use crate::style;

pub const CARD_WIDTH: f32 = 160.0;
const CARD_GAP: f32 = 16.0;
/// The play button over a card's cover.
const PLAY_SIZE: f32 = 40.0;
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
    wrapped(cards.iter().map(|c| card(c, images)))
}

/// Cards or tiles wrapped onto as many lines as they need.
pub fn wrapped<'a>(tiles: impl IntoIterator<Item = Element<'a, Link>>) -> Element<'a, Link> {
    row(tiles)
        .spacing(CARD_GAP)
        .wrap()
        .vertical_spacing(24)
        .into()
}

/// A card. Under the pointer, what can play shows a play button over its
/// cover; a track's card plays wherever it's clicked.
pub fn card<'a>(card: &'a Card, images: &'a Images) -> Element<'a, Link> {
    let art = cover(images, card.cover.as_ref(), CARD_WIDTH);
    let art = match play_button(card) {
        Some(play) => hover(
            art,
            container(play)
                .padding(8)
                .align_bottom(Length::Fill)
                .align_right(Length::Fill),
        ),
        None => art,
    };
    tile(art, &card.title, &card.subtitle, open(card))
}

/// What clicking a card does: open its Page, or play a track's.
pub fn open(card: &Card) -> Option<Link> {
    match card.target {
        Target::Track(_) => Some(Link::PlayCard(card.clone())),
        _ => route(card).map(Link::Open),
    }
}

/// The accent disc that plays all of what a card leads to, if it can play.
pub fn play_button<'a>(card: &Card) -> Option<Element<'a, Link>> {
    let playable = matches!(
        card.target,
        Target::Album(_)
            | Target::Artist(_)
            | Target::Playlist(_)
            | Target::Mix(_)
            | Target::Favorites
            | Target::Track(_)
    );
    playable.then(|| {
        button(container(filled(Icon::Play, 18.0, style::TEXT_PRIMARY)).center(PLAY_SIZE))
            .padding(0)
            .style(play_disc)
            .on_press(Link::PlayCard(card.clone()))
            .into()
    })
}

/// A card's shape for anything: `art` over a title and a subtitle, opening
/// `open` when clicked.
pub fn tile<'a>(
    art: Element<'a, Link>,
    title: impl text::IntoFragment<'a>,
    subtitle: impl text::IntoFragment<'a>,
    open: Option<Link>,
) -> Element<'a, Link> {
    let line = |line: Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(CARD_WIDTH)
            .clip(true)
    };
    button(
        column![
            art,
            line(text(title).size(14)),
            line(text(subtitle).size(12).style(text::secondary)),
        ]
        .spacing(6),
    )
    .padding(0)
    .style(card_button)
    .on_press_maybe(open)
    .into()
}

/// Where a card leads, with what its Page can draw straight away. A track's
/// card plays instead, and a video's leads nowhere yet.
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

/// A card's play button: an accent disc.
fn play_disc(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(style::accent(status).into()),
        border: style::rounded(PLAY_SIZE / 2.0),
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
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
