//! "Add to playlist": a popover beside the menu item or button that
//! opened it, listing the user's Own playlists with the recent ones first,
//! a title filter, and "Create new". The playlists are a Catalog read of
//! every Own playlist, shown through `library::apply`, so the user's edits
//! show in it as everywhere else. A card's tracks are read while it's
//! open, and a playlist can be picked once they're in.

use iced::advanced::widget;
use iced::widget::{
    Column, button, column, container, mouse_area, opaque, pin, responsive, row, scrollable, space,
    stack, text, text_input,
};
use iced::{Alignment, Element, Length, Rectangle, Size, Task};
use std::sync::Arc;
use syzygy_catalog::library::Item;
use syzygy_catalog::{Playlist, Read, Track};

use crate::icons::{Icon, icon};
use crate::library::{self, Ask, Library, Listing};
use crate::page::menu;
use crate::settings::RECENT_PLAYLISTS;
use crate::style;

const WIDTH: f32 = 280.0;
/// The most the list of playlists takes before it scrolls.
const LIST_HEIGHT: f32 = 300.0;
/// About the tallest the popover gets, to keep it on screen.
const HEIGHT: f32 = LIST_HEIGHT + 120.0;
/// Between the popover and what opened it, and the window's edges.
const GAP: f32 = 4.0;
const MARGIN: f32 = 8.0;
const FILTER: widget::Id = widget::Id::new("add-to-playlist-filter");

pub struct Picker {
    /// What goes in: `None` while a card's tracks are read.
    tracks: Option<Vec<Track>>,
    /// The menu item or button that opened it, on screen.
    at: Rectangle,
    filter: String,
    /// The user's Own playlists as last read, as Library items.
    playlists: Playlists,
    /// The reads it started. Closing it aborts them.
    reads: Vec<iced::task::Handle>,
}

enum Playlists {
    Reading,
    Read(Vec<Item>),
    Failed,
}

#[derive(Debug, Clone)]
pub enum Message {
    Filter(String),
    Playlists(Read<Vec<Playlist>>),
    /// A card's tracks were read.
    Tracks(Result<Vec<Track>, Arc<syzygy_catalog::Error>>),
    Pick(Playlist),
    /// "Create new": the form for a new playlist with the tracks in it.
    New,
    Close,
}

/// What an answer to the picker does.
pub enum Outcome {
    /// It stays open.
    None,
    Close,
    /// It closes, and the Library takes the message.
    Library(Box<library::Message>),
    /// It closes, and the user is told why.
    Failed(String),
}

impl Picker {
    /// The picker for `tracks`, or for a card's, which are still to be
    /// read.
    pub fn new(tracks: Option<Vec<Track>>, at: Rectangle) -> Self {
        Self {
            tracks,
            at,
            filter: String::new(),
            playlists: Playlists::Reading,
            reads: Vec::new(),
        }
    }

    /// A read it started, aborted once it closes.
    pub fn reading(&mut self, handle: iced::task::Handle) {
        self.reads.push(handle.abort_on_drop());
    }

    pub fn focus<T: Send + 'static>() -> Task<T> {
        iced::widget::operation::focus(FILTER)
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Filter(filter) => self.filter = filter,
            Message::Playlists(read) => match read {
                Read::Cached(playlists) | Read::Fresh(Ok(playlists)) => {
                    self.playlists =
                        Playlists::Read(playlists.into_iter().map(Item::Playlist).collect());
                }
                Read::Fresh(Err(e)) => {
                    log::warn!("Could not read the Own playlists: {e}");
                    // A cached list is better than none.
                    if matches!(self.playlists, Playlists::Reading) {
                        self.playlists = Playlists::Failed;
                    }
                }
            },
            Message::Tracks(Ok(tracks)) if tracks.is_empty() => {
                return Outcome::Failed("Nothing to add".to_string());
            }
            Message::Tracks(Ok(tracks)) => self.tracks = Some(tracks),
            Message::Tracks(Err(e)) => {
                log::warn!("Could not read the tracks to add: {e}");
                return Outcome::Failed(format!("Couldn't read the tracks: {e}"));
            }
            Message::Pick(playlist) => {
                let Some(tracks) = self.tracks.take() else {
                    return Outcome::None;
                };
                return Outcome::Library(Box::new(library::Message::AddTracks(playlist, tracks)));
            }
            Message::New => {
                let Some(tracks) = self.tracks.take() else {
                    return Outcome::None;
                };
                return Outcome::Library(Box::new(library::Message::Ask(Ask::NewPlaylist(tracks))));
            }
            Message::Close => return Outcome::Close,
        }
        Outcome::None
    }

    /// The popover beside what opened it, kept in the window. A click
    /// anywhere else closes it.
    pub fn view<'a>(&'a self, library: &'a Library, recent: &'a [String]) -> Element<'a, Message> {
        let outside = mouse_area(space().width(Length::Fill).height(Length::Fill))
            .on_press(Message::Close)
            .on_right_press(Message::Close);
        let at = self.at;
        let popover = responsive(move |window| {
            let (x, y) = place(at, window);
            pin(opaque(self.panel(library, recent))).x(x).y(y).into()
        });
        stack![outside, popover].into()
    }

    fn panel<'a>(&'a self, library: &'a Library, recent: &'a [String]) -> Element<'a, Message> {
        let ready = self.tracks.is_some();
        let filter = text_input("Find a playlist", &self.filter)
            .id(FILTER)
            .on_input(Message::Filter)
            .padding([8, 14])
            .size(14)
            .style(style::filter_input);
        let new = row![
            icon(Icon::Plus, 18.0, style::TEXT_MUTED),
            text("Create new").size(14).color(style::TEXT_SECONDARY),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let new = button(new)
            .padding([10, 16])
            .width(Length::Fill)
            .style(menu::item_style)
            .on_press_maybe(ready.then_some(Message::New));
        let list: Element<'a, Message> = match &self.playlists {
            Playlists::Reading => note("Loading\u{2026}"),
            Playlists::Failed => note("Couldn't load your playlists"),
            Playlists::Read(items) => {
                let shown: Vec<&Playlist> = library
                    .apply(items, Listing::Own)
                    .into_iter()
                    .filter_map(|item| match item {
                        Item::Playlist(playlist) if !library::is_placeholder(playlist) => {
                            Some(playlist)
                        }
                        _ => None,
                    })
                    .collect();
                let (recents, rest) = arrange(shown, recent, &self.filter);
                let rows = |playlists: Vec<&'a Playlist>| {
                    playlists
                        .into_iter()
                        .map(|playlist| pick_row(playlist, ready))
                };
                let mut list = Column::new();
                match (recents.is_empty(), rest.is_empty()) {
                    (true, true) if self.filter.trim().is_empty() => {
                        list = list.push(note("No playlists yet"));
                    }
                    (true, true) => list = list.push(note("No playlists match")),
                    (true, false) => list = list.extend(rows(rest)),
                    (false, _) => {
                        list = list.push(heading("Recent")).extend(rows(recents));
                        if !rest.is_empty() {
                            list = list.push(heading("All playlists")).extend(rows(rest));
                        }
                    }
                }
                scrollable(list)
                    .height(Length::Shrink)
                    .width(Length::Fill)
                    .into()
            }
        };
        let list = container(list).max_height(LIST_HEIGHT);
        let panel = column![
            container(text("Add to playlist").size(13).color(style::TEXT_MUTED)).padding([4, 16]),
            container(filter).padding([0, 12]),
            new,
            list,
        ]
        .spacing(4);
        container(panel)
            .padding([8, 0])
            .width(WIDTH)
            .style(menu::panel)
            .into()
    }
}

/// Where the popover goes: to the right of `at`, or to its left where the
/// window ends, with its top by `at`'s and kept in the window.
pub(super) fn place(at: Rectangle, window: Size) -> (f32, f32) {
    let right = at.x + at.width + GAP;
    let x = if right + WIDTH <= window.width - MARGIN {
        right
    } else {
        (at.x - GAP - WIDTH).max(MARGIN)
    };
    let y = (at.y - MARGIN)
        .min(window.height - HEIGHT - MARGIN)
        .max(MARGIN);
    (x, y)
}

/// The recent playlists, in the order they were last added to and at most
/// [`RECENT_PLAYLISTS`], then the rest, both cut to those whose title has `filter` in it.
fn arrange<'a>(
    shown: Vec<&'a Playlist>,
    recent: &[String],
    filter: &str,
) -> (Vec<&'a Playlist>, Vec<&'a Playlist>) {
    let filter = filter.trim().to_lowercase();
    let matches = |playlist: &&Playlist| playlist.title.to_lowercase().contains(&filter);
    let recents: Vec<&Playlist> = recent
        .iter()
        .take(RECENT_PLAYLISTS)
        .filter_map(|uuid| {
            shown
                .iter()
                .find(|playlist| playlist.uuid == *uuid)
                .copied()
        })
        .collect();
    let rest = shown
        .into_iter()
        .filter(|playlist| !recents.iter().any(|recent| recent.uuid == playlist.uuid))
        .filter(matches)
        .collect();
    (recents.into_iter().filter(matches).collect(), rest)
}

fn pick_row(playlist: &Playlist, ready: bool) -> Element<'_, Message> {
    let count = match playlist.tracks {
        1 => "1 track".to_string(),
        n => format!("{n} tracks"),
    };
    let line = row![
        text(&playlist.title)
            .size(14)
            .color(style::TEXT_SECONDARY)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
        text(count).size(12).color(style::TEXT_DISABLED),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    button(container(line).clip(true))
        .padding([8, 16])
        .width(Length::Fill)
        .style(menu::item_style)
        .on_press_maybe(ready.then(|| Message::Pick(playlist.clone())))
        .into()
}

pub(super) fn heading<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    container(text(label).size(11).color(style::TEXT_DISABLED))
        .padding([6, 16])
        .into()
}

pub(super) fn note<'a, M: 'a>(says: &'a str) -> Element<'a, M> {
    container(text(says).size(13).color(style::TEXT_MUTED))
        .padding([10, 16])
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_catalog::PlaylistFields;

    fn playlist(uuid: &str, title: &str) -> Playlist {
        PlaylistFields {
            title: title.to_string(),
            ..PlaylistFields::new()
        }
        .playlist(uuid.to_string(), 7)
    }

    fn titles(playlists: &[&Playlist]) -> Vec<String> {
        playlists.iter().map(|p| p.title.clone()).collect()
    }

    #[test]
    fn the_recent_playlists_go_first_in_their_order_and_not_again() {
        let all = [
            playlist("a", "Alpha"),
            playlist("b", "Beta"),
            playlist("c", "Gamma"),
        ];
        let recent = ["c".to_string(), "gone".to_string(), "a".to_string()];

        let (recents, rest) = arrange(all.iter().collect(), &recent, "");

        assert_eq!(titles(&recents), ["Gamma", "Alpha"]);
        assert_eq!(titles(&rest), ["Beta"]);
    }

    #[test]
    fn the_filter_keeps_titles_that_have_it_whatever_the_case() {
        let all = [
            playlist("a", "Road trip"),
            playlist("b", "Focus"),
            playlist("c", "TRIPS"),
        ];
        let recent = ["b".to_string(), "c".to_string()];

        let (recents, rest) = arrange(all.iter().collect(), &recent, " trip ");

        assert_eq!(titles(&recents), ["TRIPS"]);
        assert_eq!(titles(&rest), ["Road trip"]);
    }

    #[test]
    fn the_popover_opens_to_the_right_unless_the_window_ends_there() {
        let window = Size::new(1200.0, 800.0);
        let item = Rectangle::new(iced::Point::new(100.0, 200.0), Size::new(240.0, 40.0));
        assert_eq!(place(item, window), (344.0, 192.0));

        let late = Rectangle::new(iced::Point::new(900.0, 700.0), Size::new(240.0, 40.0));
        let (x, y) = place(late, window);
        assert_eq!(x, 900.0 - GAP - WIDTH);
        assert_eq!(y, 800.0 - HEIGHT - MARGIN);
    }
}
