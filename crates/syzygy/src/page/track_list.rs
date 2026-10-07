//! Track lists, windowed: only the rows near the viewport are built, with
//! spacers standing in for the rest, so a list of thousands scrolls as
//! smoothly as a short one. Covers are asked for only by rows that are built.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Length, Theme};
use std::ops::Range;
use syzygy_catalog::Track;

use super::{Link, Preview, Route, Viewport, artists, cover, duration, link};
use crate::images::Images;
use crate::style;

/// Every row is this tall, so where a row sits is a multiplication.
pub const ROW_HEIGHT: f32 = 60.0;
/// The column titles over the rows.
pub const HEADER_HEIGHT: f32 = 36.0;
/// Rows built past each edge of the viewport.
const OVERSCAN: usize = 8;
const COVER_SIZE: f32 = 40.0;
const NUMBER_WIDTH: f32 = 36.0;
const TIME_WIDTH: f32 = 56.0;

/// What a list shows besides each track's title and artists.
#[derive(Debug, Clone, Copy)]
pub struct Columns {
    pub cover: bool,
    pub album: bool,
}

/// A list of `rows` rows whose column titles start `top` down the Page.
/// Only the rows `window` picks are built, by `row`.
pub fn view<'a, Message: 'a>(
    rows: usize,
    top: f32,
    viewport: Viewport,
    columns: Columns,
    row: impl Fn(usize) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let range = window(rows, top + HEADER_HEIGHT, viewport);
    let above = range.start as f32 * ROW_HEIGHT;
    let below = (rows - range.end) as f32 * ROW_HEIGHT;
    column![
        header(columns),
        space().height(above),
        column(range.map(row)),
        space().height(below),
    ]
    .into()
}

fn header<'a, Message: 'a>(columns: Columns) -> Element<'a, Message> {
    let title = |label| text(label).size(12).color(style::TEXT_MUTED);
    let line = row![
        title("#").width(NUMBER_WIDTH),
        title("TITLE").width(Length::FillPortion(4)),
    ]
    .push(
        columns
            .album
            .then(|| title("ALBUM").width(Length::FillPortion(2))),
    )
    .push(
        title("TIME")
            .width(TIME_WIDTH)
            .align_x(iced::alignment::Horizontal::Right),
    )
    .spacing(16);
    container(line)
        .padding([0, 16])
        .center_y(HEADER_HEIGHT)
        .into()
}

/// One track: its number, the cover if the list shows covers, the title
/// over its artists, the album if shown, and how long it is.
pub fn track<'a>(
    images: &'a Images,
    number: usize,
    track: &'a Track,
    columns: Columns,
) -> Element<'a, Link> {
    let byline = row![]
        .push(track.explicit.then(|| badge("E")))
        .push(artists(&track.artists, 13.0))
        .spacing(6)
        .align_y(Alignment::Center);
    let title = column![
        text(&track.title).size(14).wrapping(text::Wrapping::None),
        byline,
    ]
    .spacing(4);
    let title = row![]
        .push(columns.cover.then(|| {
            cover(
                images,
                track.album.as_ref().and_then(|a| a.cover.as_ref()),
                COVER_SIZE,
            )
        }))
        .push(container(title).clip(true))
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(4));
    let album = columns.album.then(|| {
        let album: Element<'a, Link> = match &track.album {
            Some(album) => link(
                text(&album.title).size(13).wrapping(text::Wrapping::None),
                Link::Open(Route::Album {
                    id: album.id,
                    preview: Some(Preview {
                        title: album.title.clone(),
                        cover: album.cover.clone(),
                        artist: None,
                    }),
                }),
            ),
            None => space().into(),
        };
        container(album).clip(true).width(Length::FillPortion(2))
    });
    let line = row![
        text(number.to_string())
            .size(14)
            .color(style::TEXT_MUTED)
            .width(NUMBER_WIDTH),
        title,
    ]
    .push(album)
    .push(
        text(duration(track.duration))
            .size(14)
            .color(style::TEXT_MUTED)
            .width(TIME_WIDTH)
            .align_x(iced::alignment::Horizontal::Right),
    )
    .spacing(16)
    .align_y(Alignment::Center);
    container(line).padding([0, 16]).center_y(ROW_HEIGHT).into()
}

/// A heading that takes a row's place, as "Volume 2".
pub fn heading<'a, Message: 'a>(label: String) -> Element<'a, Message> {
    container(text(label).size(16))
        .padding(iced::Padding::new(0.0).left(16.0).bottom(12.0))
        .align_bottom(ROW_HEIGHT)
        .into()
}

fn badge<'a>(label: &'a str) -> Element<'a, Link> {
    container(text(label).size(10).color(style::TEXT_PRIMARY))
        .padding([1, 4])
        .style(badge_style)
        .into()
}

fn badge_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_BUTTON.into()),
        border: style::rounded(2.0),
        ..container::Style::default()
    }
}

/// The rows to build of a list of `rows` that starts `top` down the Page.
pub fn window(rows: usize, top: f32, viewport: Viewport) -> Range<usize> {
    let row_at = |y: f32| ((y - top) / ROW_HEIGHT).max(0.0);
    let first = row_at(viewport.offset).floor() as usize;
    let last = row_at(viewport.offset + viewport.height).ceil() as usize;
    let end = last.saturating_add(OVERSCAN).min(rows);
    first.saturating_sub(OVERSCAN).min(end)..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(offset: f32, height: f32) -> Viewport {
        Viewport { offset, height }
    }

    #[test]
    fn at_the_top_the_visible_rows_and_the_overscan_below_are_built() {
        // 300px of the list show: 5 rows, then 8 more.
        assert_eq!(window(1000, 300.0, viewport(0.0, 600.0)), 0..13);
    }

    #[test]
    fn scrolled_into_the_list_the_overscan_goes_both_ways() {
        // Rows 100 to 110 show.
        assert_eq!(window(1000, 300.0, viewport(6300.0, 600.0)), 92..118);
    }

    #[test]
    fn the_window_stops_at_the_end_of_the_list() {
        assert_eq!(window(20, 300.0, viewport(900.0, 600.0)), 2..20);
        assert_eq!(window(20, 300.0, viewport(50_000.0, 600.0)), 20..20);
    }

    #[test]
    fn a_list_below_the_viewport_builds_only_the_overscan() {
        assert_eq!(window(1000, 2000.0, viewport(0.0, 600.0)), 0..8);
    }
}
