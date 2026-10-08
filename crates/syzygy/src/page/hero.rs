//! The top of an Album, Artist, Mix or Playlist Page: the cover, what kind
//! of Page it is, the title and a few lines under it. It's drawn from the
//! card's Preview until the Page's data arrives.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Font, font};

use super::{Link, Preview, rounded_cover};
use crate::images::Images;
use crate::style;
use syzygy_catalog::home_feed::Cover;

/// The hero is always this tall, so what's under it sits at a known place.
pub const HEIGHT: f32 = 232.0;
/// How round a square cover's corners are, bigger than a card's.
const RADIUS: f32 = 8.0;

/// `lines` go under the title. A round cover is an artist's.
pub fn view<'a>(
    images: &'a Images,
    kind: &'a str,
    title: &'a str,
    cover: Option<&Cover>,
    round: bool,
    lines: Vec<Element<'a, Link>>,
) -> Element<'a, Link> {
    let radius = if round { HEIGHT / 2.0 } else { RADIUS };
    with_art(
        rounded_cover(images, cover, HEIGHT, radius),
        kind,
        title,
        lines,
    )
}

/// The hero with `art` in the cover's place.
pub fn with_art<'a>(
    art: Element<'a, Link>,
    kind: &'a str,
    title: &'a str,
    lines: Vec<Element<'a, Link>>,
) -> Element<'a, Link> {
    let details = column![
        text(kind)
            .size(12)
            .font(bold())
            .color(style::TEXT_SECONDARY),
        text(title)
            .size(42)
            .font(bold())
            .wrapping(text::Wrapping::WordOrGlyph),
    ]
    .extend(lines)
    .spacing(8);
    container(
        row![art, container(details).clip(true)]
            .spacing(28)
            .align_y(Alignment::End),
    )
    .align_bottom(HEIGHT)
    .clip(true)
    .into()
}

/// The hero from the card that led here, until the Page's data arrives.
pub fn preview<'a>(
    images: &'a Images,
    kind: &'a str,
    preview: &'a Preview,
    round: bool,
) -> Element<'a, Link> {
    let lines = preview
        .artist
        .iter()
        .map(|artist| text(artist).size(14).color(style::TEXT_SECONDARY).into())
        .collect();
    view(
        images,
        kind,
        &preview.title,
        preview.cover.as_ref(),
        round,
        lines,
    )
}

pub fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::default()
    }
}
