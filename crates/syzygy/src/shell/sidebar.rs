//! The sidebar: Home, and the user's Library one type at a time, with
//! covers. The Shell keeps it for as long as the user is signed in, and a
//! list stays on screen while it's read again, until the new read arrives,
//! so the sidebar is never emptied by a reload.

use iced::widget::{Column, button, column, container, row, scrollable, space, text};
use iced::{Alignment, Element, Length, Theme};
use std::collections::BTreeMap;
use std::sync::Arc;
use syzygy_catalog::home_feed::Target;
use syzygy_catalog::library::{self, Item};
use syzygy_catalog::{Kind, LibrarySort, Paged, Read, Shelf};

use crate::icons::{Icon, icon};
use crate::identity::DISPLAY_NAME;
use crate::images::Images;
use crate::page::library::{folder_route, label, playlist_route};
use crate::page::paged::List;
use crate::page::{Link, Remote, Route, cards, folder_art, loved_art, rounded_cover};
use crate::settings::Settings;
use crate::style;

pub const WIDTH: f32 = 280.0;
const ART_SIZE: f32 = 40.0;
const ART_RADIUS: f32 = 4.0;

pub struct Sidebar {
    /// Who's signed in. Nothing is read until TIDAL has said.
    user_id: Option<u64>,
    /// The Library type showing.
    kind: Kind,
    lists: BTreeMap<Kind, Shelved>,
}

/// One Library type's list in the sidebar.
struct Shelved {
    /// The order it's wanted in.
    sort: LibrarySort,
    /// The order `items` was read in, once a read has arrived.
    read_in: Option<LibrarySort>,
    items: List<Item>,
    /// It has been asked for, so a new order reads it again.
    asked: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A Library type's pill.
    Select(Kind),
    /// A read of a shelf's first page.
    Items {
        shelf: Shelf,
        read: Read<Paged<Item>>,
    },
    More {
        shelf: Shelf,
        offset: usize,
        result: Result<Paged<Item>, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the list is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    Retry,
    Link(Link),
}

/// What the sidebar asks the Shell to do.
pub enum Effect {
    None,
    /// Read a shelf's first page.
    Read(Shelf),
    /// Read a shelf after the first `offset` items.
    More {
        shelf: Shelf,
        offset: usize,
        cursor: Option<String>,
    },
    Link(Link),
}

impl Sidebar {
    /// The sidebar on the playlists, and their first read.
    pub fn new(user_id: Option<u64>, settings: &Settings) -> (Self, Effect) {
        let lists = Kind::ALL
            .into_iter()
            .map(|kind| {
                let shelved = Shelved {
                    sort: settings.library_sort(kind),
                    read_in: None,
                    items: List::new(),
                    asked: false,
                };
                (kind, shelved)
            })
            .collect();
        let mut sidebar = Self {
            user_id,
            kind: Kind::Playlists,
            lists,
        };
        let effect = sidebar.ask(Kind::Playlists);
        (sidebar, effect)
    }

    /// TIDAL said who's signed in: read what was waiting for it.
    pub fn user_known(&mut self, user_id: u64) -> Effect {
        if self.user_id.is_some() {
            return Effect::None;
        }
        self.user_id = Some(user_id);
        self.ask(self.kind)
    }

    /// A Library type is now read in `sort`. A list that's been read is
    /// read again, and keeps showing in its old order until that arrives.
    pub fn sorted(&mut self, kind: Kind, sort: LibrarySort) -> Effect {
        let Some(list) = self.lists.get_mut(&kind) else {
            return Effect::None;
        };
        list.sort = sort;
        if !list.asked {
            return Effect::None;
        }
        self.shelf(kind).map_or(Effect::None, Effect::Read)
    }

    pub fn update(&mut self, message: Message) -> Effect {
        match message {
            Message::Select(kind) => {
                self.kind = kind;
                self.ask(kind)
            }
            Message::Items { shelf, read } => {
                if Some(&shelf) != self.shelf(shelf.kind).as_ref() {
                    return Effect::None;
                }
                if let Some(list) = self.lists.get_mut(&shelf.kind) {
                    // The first read in a new order replaces the old list.
                    if list.read_in != Some(shelf.sort) {
                        list.items = List::new();
                        list.read_in = Some(shelf.sort);
                    }
                    list.items.apply(read, "sidebar's Library");
                }
                Effect::None
            }
            Message::More {
                shelf,
                offset,
                result,
            } => {
                if Some(&shelf) != self.shelf(shelf.kind).as_ref() {
                    return Effect::None;
                }
                if let Some(list) = self.lists.get_mut(&shelf.kind)
                    && list.read_in == Some(shelf.sort)
                {
                    list.items.more(offset, result);
                }
                Effect::None
            }
            Message::EndInView => {
                let offset = self.showing_mut().items.next();
                self.more(offset)
            }
            Message::RetryMore => {
                let offset = self.showing_mut().items.retry();
                self.more(offset)
            }
            Message::Retry => {
                let list = self.showing_mut();
                list.items = List::new();
                list.read_in = None;
                self.shelf(self.kind).map_or(Effect::None, Effect::Read)
            }
            Message::Link(link) => Effect::Link(link),
        }
    }

    /// Read a type's list the first time it's wanted.
    fn ask(&mut self, kind: Kind) -> Effect {
        let Some(shelf) = self.shelf(kind) else {
            return Effect::None;
        };
        match self.lists.get_mut(&kind) {
            Some(list) if !list.asked => {
                list.asked = true;
                Effect::Read(shelf)
            }
            _ => Effect::None,
        }
    }

    /// Read on from `offset`, unless the list on screen is in an order
    /// that's being replaced: its offset means nothing in the new one.
    fn more(&self, offset: Option<usize>) -> Effect {
        let showing = self.showing();
        if showing.read_in != Some(showing.sort) {
            return Effect::None;
        }
        match (self.shelf(self.kind), offset) {
            (Some(shelf), Some(offset)) => Effect::More {
                shelf,
                offset,
                cursor: self.showing().items.cursor(),
            },
            _ => Effect::None,
        }
    }

    /// What a type's list is read as now: the top level, in its order.
    fn shelf(&self, kind: Kind) -> Option<Shelf> {
        Some(Shelf {
            user_id: self.user_id?,
            kind,
            folder: None,
            sort: self.lists.get(&kind)?.sort,
        })
    }

    fn showing(&self) -> &Shelved {
        &self.lists[&self.kind]
    }

    fn showing_mut(&mut self) -> &mut Shelved {
        self.lists
            .get_mut(&self.kind)
            .expect("every type has a list")
    }

    /// The sidebar, with the Page at `current` marked where it's listed.
    pub fn view<'a>(&'a self, images: &'a Images, current: &Route) -> Element<'a, Message> {
        let home = button(
            row![
                icon(Icon::House, 20.0, style::TEXT_PRIMARY),
                text("Home").size(14)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .on_press(Message::Link(Link::Open(Route::home())))
        .style(item_style(matches!(current, Route::Home { .. })))
        .width(Length::Fill);
        let show_all = button(text("Show all").size(12))
            .padding([4, 8])
            .style(show_all_style)
            .on_press(Message::Link(Link::Open(Route::Library {
                kind: self.kind,
            })));
        let header = row![
            icon(Icon::Library, 20.0, style::TEXT_SECONDARY),
            text("Your Library").size(13).color(style::TEXT_SECONDARY),
            space::horizontal(),
            show_all,
        ]
        .spacing(12)
        .padding([0, 12])
        .align_y(Alignment::Center);
        let pills = row(Kind::ALL.into_iter().map(|kind| {
            let style = if kind == self.kind {
                pill_selected
            } else {
                pill_unselected
            };
            button(text(label(kind)).size(12))
                .padding([4, 10])
                .style(style)
                .on_press(Message::Select(kind))
                .into()
        }))
        .spacing(6)
        .padding([0, 8]);
        // Wider than the sidebar: they scroll sideways, with no scrollbar,
        // as in sone.
        let pills = scrollable(pills).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ));
        let list = scrollable(self.list(images, current))
            .height(Length::Fill)
            .width(Length::Fill);
        let body = column![text(DISPLAY_NAME).size(22), home, header, pills, list].spacing(12);
        container(body)
            .padding(16)
            .width(WIDTH)
            .height(Length::Fill)
            .style(frame)
            .into()
    }

    /// The type showing, with Loved tracks pinned above the playlists.
    fn list<'a>(&'a self, images: &'a Images, current: &Route) -> Element<'a, Message> {
        let shelved = self.showing();
        let loved = (self.kind == Kind::Playlists).then(|| {
            entry(
                loved_art(ART_SIZE, ART_RADIUS),
                "Loved Tracks",
                "Collection".to_string(),
                Route::Favorites,
                current,
            )
        });
        let rows: Element<'a, Message> = match &shelved.items.list {
            Remote::Loading => notice("Loading…"),
            Remote::NotFound | Remote::Failed(_) => column![
                notice("Couldn't load your Library"),
                container(button(text("Retry").size(12)).on_press(Message::Retry)).padding([0, 12]),
            ]
            .spacing(8)
            .into(),
            Remote::Loaded(_) if shelved.items.items().is_empty() => notice(empty(self.kind)),
            Remote::Loaded(_) => Column::with_children(
                shelved
                    .items
                    .items()
                    .iter()
                    .map(|item| self.item(item, images, current)),
            )
            .spacing(2)
            .into(),
        };
        Column::new()
            .push(loved)
            .push(rows)
            .push(shelved.items.end(Message::EndInView, Message::RetryMore))
            .spacing(2)
            .padding(iced::Padding::new(0.0).right(8.0))
            .into()
    }

    fn item<'a>(
        &self,
        item: &'a Item,
        images: &'a Images,
        current: &Route,
    ) -> Element<'a, Message> {
        let art = |cover, radius| rounded_cover(images, cover, ART_SIZE, radius).map(Message::Link);
        match item {
            Item::Folder(folder) => entry(
                folder_art(ART_SIZE, ART_RADIUS),
                &folder.name,
                folder.subtitle(),
                folder_route(folder),
                current,
            ),
            Item::Playlist(playlist) => entry(
                art(playlist.cover.as_ref(), ART_RADIUS),
                &playlist.title,
                library::playlist_subtitle(playlist, self.user_id),
                playlist_route(playlist),
                current,
            ),
            Item::Card(card) => {
                let radius = match card.target {
                    Target::Artist(_) => ART_SIZE / 2.0,
                    _ => ART_RADIUS,
                };
                let Some(route) = cards::route(card) else {
                    return space().into();
                };
                entry(
                    art(card.cover.as_ref(), radius),
                    &card.title,
                    card.subtitle.clone(),
                    route,
                    current,
                )
            }
        }
    }
}

/// One row: the art, the title over a line about it, leading to `route`.
fn entry<'a>(
    art: Element<'a, Message>,
    title: &'a str,
    subtitle: String,
    route: Route,
    current: &Route,
) -> Element<'a, Message> {
    let line = |line: text::Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(Length::Fill)
            .clip(true)
    };
    let words = column![
        line(text(title).size(14)),
        line(text(subtitle).size(12).color(style::TEXT_MUTED)),
    ]
    .spacing(2);
    let here = same_place(&route, current);
    button(row![art, words].spacing(10).align_y(Alignment::Center))
        .padding([6, 6])
        .width(Length::Fill)
        .style(item_style(here))
        .on_press(Message::Link(Link::Open(route)))
        .into()
}

fn notice<'a>(line: &'a str) -> Element<'a, Message> {
    container(text(line).size(13).color(style::TEXT_MUTED))
        .padding([8, 12])
        .into()
}

fn empty(kind: Kind) -> &'static str {
    match kind {
        Kind::Playlists => "Create your first playlist",
        Kind::Albums => "No favorite albums yet",
        Kind::Artists => "No followed artists yet",
        Kind::Mixes => "No favorite mixes yet",
    }
}

/// Whether two routes lead to the same Page, whatever they carry to draw
/// it with.
fn same_place(a: &Route, b: &Route) -> bool {
    match (a, b) {
        (Route::Album { id: a, .. }, Route::Album { id: b, .. })
        | (Route::Artist { id: a, .. }, Route::Artist { id: b, .. }) => a == b,
        (Route::Playlist { uuid: a, .. }, Route::Playlist { uuid: b, .. })
        | (Route::Mix { id: a, .. }, Route::Mix { id: b, .. })
        | (Route::Folder { id: a, .. }, Route::Folder { id: b, .. }) => a == b,
        (Route::Favorites, Route::Favorites) => true,
        _ => false,
    }
}

fn frame(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_SIDEBAR.into()),
        ..container::Style::default()
    }
}

/// A row, marked when it's the Page showing.
fn item_style(here: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let background = match status {
            _ if here => Some(style::HL_MED.into()),
            button::Status::Hovered | button::Status::Pressed => Some(style::HL_FAINT.into()),
            button::Status::Active | button::Status::Disabled => None,
        };
        button::Style {
            background,
            text_color: style::TEXT_PRIMARY,
            border: style::rounded(6.0),
            ..button::Style::default()
        }
    }
}

fn show_all_style(_theme: &Theme, status: button::Status) -> button::Style {
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

/// The Library type showing: sone's accent-tinted pill.
fn pill_selected(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(style::ACCENT.scale_alpha(0.15).into()),
        text_color: style::ACCENT,
        border: style::rounded(999.0),
        ..button::Style::default()
    }
}

fn pill_unselected(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => style::BG_INSET,
        _ => style::HL_MED,
    };
    button::Style {
        background: Some(background.into()),
        text_color: style::TEXT_SECONDARY,
        border: style::rounded(999.0),
        ..button::Style::default()
    }
}
