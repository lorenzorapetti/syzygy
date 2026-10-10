//! Track lists, windowed: only the rows near the viewport are built, with
//! spacers standing in for the rest, so a list of thousands scrolls as
//! smoothly as a short one. Covers are asked for only by rows that are built.

use iced::widget::{Row, button, column, container, mouse_area, row, space, text};
use iced::{Alignment, Element, Length, Theme};
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};
use syzygy_catalog::{Direction, Track, TrackOrder, TrackSort};

use super::{Link, NowPlaying, Preview, Route, Viewport, artists, cover, duration, link, menu};
use crate::icons::{Icon, filled, icon};
use crate::images::Images;
use crate::library::Removal;
use crate::style;

/// Every row is this tall, so where a row sits is a multiplication.
pub const ROW_HEIGHT: f32 = 60.0;
/// The column titles over the rows.
pub const HEADER_HEIGHT: f32 = 36.0;
/// Rows built past each edge of the viewport.
const OVERSCAN: usize = 8;
const COVER_SIZE: f32 = 40.0;
const NUMBER_WIDTH: f32 = 36.0;
const DATE_WIDTH: f32 = 110.0;
const TIME_WIDTH: f32 = 56.0;
/// The heart before the time.
const HEART_WIDTH: f32 = 32.0;
/// The button after the time that adds a recommended track to the
/// playlist on screen.
const ADD_WIDTH: f32 = 32.0;

/// What a list shows besides each track's title and artists.
#[derive(Debug, Clone, Copy)]
pub struct Columns {
    pub cover: bool,
    pub album: bool,
    /// When each track was added, for the user's Own playlists.
    pub date_added: bool,
}

/// A list of `rows` rows under `header`, which starts `top` down the Page.
/// Only the rows `window` picks are built, by `row`.
pub fn view<'a, Message: 'a>(
    rows: usize,
    top: f32,
    viewport: Viewport,
    header: Element<'a, Message>,
    row: impl Fn(usize) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let range = window(rows, top + HEADER_HEIGHT, viewport);
    let above = range.start as f32 * ROW_HEIGHT;
    let below = (rows - range.end) as f32 * ROW_HEIGHT;
    column![
        header,
        space().height(above),
        column(range.map(row)),
        space().height(below),
    ]
    .into()
}

/// The column titles.
pub fn header<'a, Message: Clone + 'a>(columns: Columns) -> Element<'a, Message> {
    titles(columns, false, |label, _| title(label))
}

/// The column titles, each of which sorts the list by its column through
/// `on_sort` ("#" back to the own order), with an arrow by the one it's
/// sorted by.
pub fn sortable_header<'a, Message: Clone + 'a>(
    columns: Columns,
    sort: Option<TrackSort>,
    on_sort: fn(Option<TrackOrder>) -> Message,
) -> Element<'a, Message> {
    titles(columns, true, move |label, order| {
        let arrow = sort.filter(|s| Some(s.order) == order).map(|s| {
            let glyph = match s.direction {
                Direction::Ascending => Icon::ChevronUp,
                Direction::Descending => Icon::ChevronDown,
            };
            icon(glyph, 14.0, style::TEXT_MUTED)
        });
        let label = row![title(label)]
            .push(arrow)
            .spacing(2)
            .align_y(Alignment::Center);
        button(label)
            .padding(0)
            .style(sort_style)
            .on_press(on_sort(order))
            .into()
    })
}

/// The header row, with each title built by `title` from its label and
/// the order it sorts by. Artists have no column, so a sortable header
/// puts theirs by the title's.
fn titles<'a, Message: 'a>(
    columns: Columns,
    sortable: bool,
    title: impl Fn(&'static str, Option<TrackOrder>) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let cell = |content, width| container(content).width(width);
    let title_and_artist: Row<'a, Message> = row![title("TITLE", Some(TrackOrder::Title))]
        .push(sortable.then(|| text("·").size(12).color(style::TEXT_MUTED)))
        .push(sortable.then(|| title("ARTIST", Some(TrackOrder::Artist))))
        .spacing(6)
        .align_y(Alignment::Center);
    let line = row![
        cell(title("#", None), Length::Fixed(NUMBER_WIDTH)),
        cell(title_and_artist.into(), Length::FillPortion(4)),
    ]
    .push(columns.album.then(|| {
        cell(
            title("ALBUM", Some(TrackOrder::Album)),
            Length::FillPortion(2),
        )
    }))
    .push(columns.date_added.then(|| {
        cell(
            title("DATE ADDED", Some(TrackOrder::DateAdded)),
            Length::Fixed(DATE_WIDTH),
        )
    }))
    .push(space().width(HEART_WIDTH))
    .push(
        container(title("TIME", Some(TrackOrder::Duration)))
            .width(TIME_WIDTH)
            .align_right(TIME_WIDTH),
    )
    .spacing(16)
    .align_y(Alignment::Center);
    container(line)
        .padding([0, 16])
        .center_y(HEADER_HEIGHT)
        .into()
}

fn title<'a, Message: 'a>(label: &'static str) -> Element<'a, Message> {
    text(label).size(12).color(style::TEXT_MUTED).into()
}

fn sort_style(_theme: &Theme, status: button::Status) -> button::Style {
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

/// What a row's number gives way to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    None,
    /// The pointer is over a row that plays when clicked.
    Hovered,
    /// The current track, not playing: the title in the accent, and the
    /// number too unless the pointer is over it, which shows a play icon.
    Current {
        hovered: bool,
    },
    /// The current track, playing: bouncing bars, this many seconds into
    /// their animation.
    Playing(f32),
}

impl Mark {
    /// `track`'s mark: whether it's the current track, playing or not, and
    /// whether the pointer is over its row.
    pub fn of(track: &Track, now_playing: Option<NowPlaying>, hovered: bool) -> Self {
        match now_playing.filter(|now| now.track_id == track.id) {
            Some(NowPlaying {
                playing: Some(at), ..
            }) => Mark::Playing(at),
            Some(_) => Mark::Current { hovered },
            None if hovered => Mark::Hovered,
            None => Mark::None,
        }
    }

    pub fn is_current(self) -> bool {
        matches!(self, Mark::Current { .. } | Mark::Playing(_))
    }
}

/// The row the pointer is over, by a key its Page picks, for the rows'
/// marks.
#[derive(Debug, Default)]
pub struct Hover(Option<usize>);

impl Hover {
    pub fn entered(&mut self, key: usize) {
        self.0 = Some(key);
    }

    /// Only the row it left: the next row's arrival may come first.
    pub fn left(&mut self, key: usize) {
        if self.0 == Some(key) {
            self.0 = None;
        }
    }

    pub fn over(&self, key: usize) -> bool {
        self.0 == Some(key)
    }
}

/// One track: its number, the cover if the list shows covers, the title
/// over its artists, the album if shown, its heart and how long it is.
/// Right-clicked, it opens the track's menu. Dimmed if it can't play: TIDAL
/// won't stream it, or it's explicit and `allow_explicit` is off. `liked`
/// is whether it's a Loved track, `None` while that isn't known.
pub fn track<'a>(
    images: &'a Images,
    number: usize,
    track: &'a Track,
    columns: Columns,
    allow_explicit: bool,
    liked: Option<bool>,
) -> Element<'a, Link> {
    marked(
        images,
        number,
        track,
        columns,
        allow_explicit,
        Mark::None,
        liked,
    )
}

/// [`marked`] in one of the user's Own playlists, whose menu can take it
/// out.
#[allow(clippy::too_many_arguments)]
pub fn own<'a>(
    images: &'a Images,
    number: usize,
    track: &'a Track,
    columns: Columns,
    allow_explicit: bool,
    mark: Mark,
    liked: Option<bool>,
    removal: Removal,
) -> Element<'a, Link> {
    let row = line(images, number, track, columns, allow_explicit, mark, liked);
    menu::with_menu(row, move || menu::own_track(track, liked, &removal))
}

/// A recommended track's `row`, ended by a button that sends `add`, which
/// adds the track to the playlist on screen. Its header is
/// [`header_with_add`].
pub fn with_add<'a, Message: Clone + 'a>(
    row: Element<'a, Message>,
    add: Message,
) -> Element<'a, Message> {
    let add = button(container(icon(Icon::ListPlus, 18.0, style::TEXT_MUTED)).center(ADD_WIDTH))
        .padding(0)
        .style(style::icon_button)
        .on_press(add);
    row![container(row).width(Length::Fill), add]
        .align_y(Alignment::Center)
        .into()
}

/// The column titles over rows [`with_add`] ends.
pub fn header_with_add<'a, Message: Clone + 'a>(columns: Columns) -> Element<'a, Message> {
    row![container(header(columns)).width(Length::Fill)]
        .push(space().width(ADD_WIDTH))
        .into()
}

/// [`track`], with its number marked.
pub fn marked<'a>(
    images: &'a Images,
    number: usize,
    track: &'a Track,
    columns: Columns,
    allow_explicit: bool,
    mark: Mark,
    liked: Option<bool>,
) -> Element<'a, Link> {
    let row = line(images, number, track, columns, allow_explicit, mark, liked);
    menu::with_menu(row, move || menu::track(track, liked))
}

/// A track's row, without its menu.
fn line<'a>(
    images: &'a Images,
    number: usize,
    track: &'a Track,
    columns: Columns,
    allow_explicit: bool,
    mark: Mark,
    liked: Option<bool>,
) -> Element<'a, Link> {
    let current = mark.is_current();
    let dimmed = !track.available || (track.explicit && !allow_explicit);
    let date_added = columns.date_added.then(|| {
        let date = track
            .date_added
            .as_deref()
            .map_or_else(String::new, |date| added(date, today()));
        text(date)
            .size(13)
            .color(style::TEXT_MUTED)
            .wrapping(text::Wrapping::None)
            .width(DATE_WIDTH)
    });
    let byline = row![]
        .push(track.explicit.then(|| badge("E")))
        .push(artists(&track.artists, 13.0))
        .spacing(6)
        .align_y(Alignment::Center);
    let title = column![
        text(&track.title)
            .size(14)
            .color(if current {
                style::ACCENT
            } else if dimmed {
                style::TEXT_DISABLED
            } else {
                style::TEXT_PRIMARY
            })
            .wrapping(text::Wrapping::None),
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
    let lead: Element<'a, Link> = match mark {
        Mark::Playing(at) => bars(at),
        Mark::Hovered | Mark::Current { hovered: true } => {
            filled(Icon::Play, 14.0, style::TEXT_PRIMARY).into()
        }
        Mark::Current { hovered: false } => text(number.to_string())
            .size(14)
            .color(style::ACCENT)
            .into(),
        Mark::None => text(number.to_string())
            .size(14)
            .color(if dimmed {
                style::TEXT_DISABLED
            } else {
                style::TEXT_MUTED
            })
            .into(),
    };
    let line = row![container(lead).width(NUMBER_WIDTH), title]
        .push(album)
        .push(date_added)
        .push(heart(track, liked))
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

/// A row's heart: filled in the accent for a Loved track.
fn heart<'a>(track: &Track, liked: Option<bool>) -> Element<'a, Link> {
    const SIZE: f32 = 16.0;
    let glyph = match liked {
        Some(true) => filled(Icon::Heart, SIZE, style::ACCENT),
        Some(false) => icon(Icon::Heart, SIZE, style::TEXT_MUTED),
        None => icon(Icon::Heart, SIZE, style::TEXT_DISABLED),
    };
    let favorite = crate::library::Favorite::track(track);
    button(container(glyph).center(HEART_WIDTH))
        .padding(0)
        .style(style::icon_button)
        .on_press_maybe(liked.map(|liked| Link::Favorite(Box::new(favorite), !liked)))
        .into()
}

/// sone's playing indicator: three accent bars bouncing between 40% and
/// full height once a second, each 0.2 s behind the last.
fn bars<'a>(at: f32) -> Element<'a, Link> {
    const HEIGHT: f32 = 16.0;
    let bar = |delay: f32| {
        // Eased like CSS `ease-in-out`: 0.4 at the ends of the cycle, 1 halfway.
        let phase = (at - delay).rem_euclid(1.0);
        let scale = 0.4 + 0.6 * (1.0 - (phase * std::f32::consts::TAU).cos()) / 2.0;
        container(space())
            .width(3)
            .height(HEIGHT * scale)
            .style(|_| container::Style {
                background: Some(style::ACCENT.into()),
                border: style::rounded(1.5),
                ..container::Style::default()
            })
    };
    container(
        row![bar(0.0), bar(0.2), bar(0.4)]
            .spacing(3)
            .align_y(Alignment::End),
    )
    .align_bottom(HEIGHT)
    .into()
}

/// A row that plays when clicked, lit while the pointer is over it, or
/// all the time for the current track. Links inside it still go where they
/// lead.
pub fn playable<'a, Message: Clone + 'a>(
    row: Element<'a, Message>,
    on_play: Message,
    current: bool,
) -> Element<'a, Message> {
    button(row)
        .padding(0)
        .width(Length::Fill)
        .style(move |theme, status| {
            let status = if current {
                button::Status::Hovered
            } else {
                status
            };
            style::list_row(theme, status)
        })
        .on_press(on_play)
        .into()
}

/// [`playable`] for a track's row, marked `mark` and lit while it's the
/// current track. It sends `entered` and `left` with `key` as the pointer
/// comes over it and leaves, for its Page's [`Hover`].
pub fn playable_track<'a, Message: Clone + 'a>(
    row: Element<'a, Message>,
    mark: Mark,
    on_play: Message,
    key: usize,
    entered: fn(usize) -> Message,
    left: fn(usize) -> Message,
) -> Element<'a, Message> {
    mouse_area(playable(row, on_play, mark.is_current()))
        .on_enter(entered(key))
        .on_exit(left(key))
        .into()
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

/// When a track was added, as sone writes it: "This week", "Last week" or
/// "Last month", else as "Mar 5, 2024". `today` is in days since 1970.
fn added(date: &str, today: i64) -> String {
    let Some((year, month, day)) = ymd(date) else {
        return String::new();
    };
    match (today - days_from_civil(year, month, day)).abs() {
        0..=7 => "This week".to_string(),
        8..=14 => "Last week".to_string(),
        15..=30 => "Last month".to_string(),
        _ => {
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{} {day}, {year}", MONTHS[month as usize - 1])
        }
    }
}

/// Today, in days since 1970 (UTC).
fn today() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    (seconds / 86_400) as i64
}

/// The year, month and day an ISO 8601 date starts with.
fn ymd(date: &str) -> Option<(i64, u32, u32)> {
    let year = date.get(0..4)?.parse().ok()?;
    let month = date
        .get(5..7)?
        .parse()
        .ok()
        .filter(|m| (1..=12).contains(m))?;
    let day = date
        .get(8..10)?
        .parse()
        .ok()
        .filter(|d| (1..=31).contains(d))?;
    (date.get(4..5)? == "-" && date.get(7..8)? == "-").then_some((year, month, day))
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Where in `tracks` the ones whose title, artists or album contain
/// `filter` are, ignoring case. All of them for an empty filter.
pub fn matching<'a>(tracks: impl IntoIterator<Item = &'a Track>, filter: &str) -> Vec<usize> {
    let filter = filter.trim().to_lowercase();
    let contains = |s: &str| s.to_lowercase().contains(&filter);
    tracks
        .into_iter()
        .enumerate()
        .filter(|(_, track)| {
            filter.is_empty()
                || contains(&track.title)
                || track.artists.iter().any(|artist| contains(&artist.name))
                || track
                    .album
                    .as_ref()
                    .is_some_and(|album| contains(&album.title))
        })
        .map(|(i, _)| i)
        .collect()
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
    use syzygy_catalog::track::{AlbumRef, ArtistRef};

    fn track(id: u64, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id,
            title: title.to_string(),
            artists: vec![ArtistRef {
                id: 1,
                name: artist.to_string(),
            }],
            album: Some(AlbumRef {
                id: 2,
                title: album.to_string(),
                cover: None,
            }),
            duration: 200,
            explicit: false,
            available: true,
            volume: 1,
            date_added: None,
            track_radio: None,
        }
    }

    fn tracks() -> Vec<Track> {
        vec![
            track(1, "Jóga", "Björk", "Homogenic"),
            track(2, "Hyperballad", "Björk", "Post"),
            track(3, "Teardrop", "Massive Attack", "Mezzanine"),
            track(4, "Postcards", "Someone", "Letters"),
        ]
    }

    #[test]
    fn an_empty_filter_lets_every_track_through() {
        assert_eq!(matching(&tracks(), "  "), vec![0, 1, 2, 3]);
    }

    #[test]
    fn the_filter_looks_at_title_artist_and_album_ignoring_case() {
        assert_eq!(matching(&tracks(), "teardrop"), vec![2]);
        assert_eq!(matching(&tracks(), "BJÖRK"), vec![0, 1]);
        assert_eq!(matching(&tracks(), "post"), vec![1, 3]);
    }

    #[test]
    fn filtered_rows_keep_their_place_in_the_list() {
        assert_eq!(matching(&tracks(), "mezzanine"), vec![2]);
    }

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

    /// 2024-03-20, as days since 1970-01-01.
    const MARCH_20: i64 = 19_802;

    #[test]
    fn a_date_in_the_last_month_says_how_long_ago() {
        assert_eq!(added("2024-03-20T08:00:00.000+0000", MARCH_20), "This week");
        assert_eq!(added("2024-03-13T08:00:00.000+0000", MARCH_20), "This week");
        assert_eq!(added("2024-03-12T08:00:00.000+0000", MARCH_20), "Last week");
        assert_eq!(added("2024-03-06T08:00:00.000+0000", MARCH_20), "Last week");
        assert_eq!(
            added("2024-02-19T08:00:00.000+0000", MARCH_20),
            "Last month"
        );
    }

    #[test]
    fn an_older_date_is_written_out() {
        assert_eq!(
            added("2024-02-18T08:00:00.000+0000", MARCH_20),
            "Feb 18, 2024"
        );
        assert_eq!(added("2019-12-01", MARCH_20), "Dec 1, 2019");
    }

    #[test]
    fn a_date_that_doesnt_parse_shows_nothing() {
        assert_eq!(added("yesterday", MARCH_20), "");
        assert_eq!(added("2024-13-01", MARCH_20), "");
    }

    #[test]
    fn a_list_below_the_viewport_builds_only_the_overscan() {
        assert_eq!(window(1000, 2000.0, viewport(0.0, 600.0)), 0..8);
    }
}
