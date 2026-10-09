//! The sidebar: Home, and the user's Library one type at a time, with
//! covers. The Shell keeps it for as long as the user is signed in, and a
//! list stays on screen while it's read again, until the new read arrives,
//! so the sidebar is never emptied by a reload. The root playlists and
//! Folders are the Library's; the other types' lists are the sidebar's.
//! Every list shows through the Library's merge with its pending edits.

use iced::widget::{Column, button, column, container, row, scrollable, space, stack, text};
use iced::{Alignment, Element, Length, Theme};
use std::collections::BTreeMap;
use std::sync::Arc;
use syzygy_catalog::home_feed::Target;
use syzygy_catalog::library::{self, Item};
use syzygy_catalog::{Kind, LibrarySort, Paged, Read, Shelf};

use crate::icons::{Icon, icon};
use crate::identity::DISPLAY_NAME;
use crate::images::Images;
use crate::library::{Ask, Library, Listing, Shelved, is_placeholder};
use crate::page::library::{folder_route, label, playlist_menu, playlist_route};
use crate::page::paged::List;
use crate::page::{Link, Remote, Route, cards, folder_art, loved_art, menu, rounded_cover};
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
    /// Every type's list but the playlists, which are the Library's.
    lists: BTreeMap<Kind, Shelved>,
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
    pub fn new(user_id: Option<u64>, settings: &Settings, library: &mut Library) -> (Self, Effect) {
        let lists = Kind::ALL
            .into_iter()
            .filter(|&kind| kind != Kind::Playlists)
            .map(|kind| (kind, Shelved::new(settings.library_sort(kind))))
            .collect();
        let mut sidebar = Self {
            user_id,
            kind: Kind::Playlists,
            lists,
        };
        let effect = sidebar.ask(Kind::Playlists, library);
        (sidebar, effect)
    }

    /// TIDAL said who's signed in: read what was waiting for it.
    pub fn user_known(&mut self, user_id: u64, library: &mut Library) -> Effect {
        if self.user_id.is_some() {
            return Effect::None;
        }
        self.user_id = Some(user_id);
        self.ask(self.kind, library)
    }

    /// A Library type is now read in `sort`. A list that's been read is
    /// read again, and keeps showing in its old order until that arrives.
    pub fn sorted(&mut self, kind: Kind, sort: LibrarySort, library: &mut Library) -> Effect {
        let list = self.list_mut(kind, library);
        list.sort = sort;
        if !list.asked {
            return Effect::None;
        }
        self.shelf(kind, library).map_or(Effect::None, Effect::Read)
    }

    /// An edit landed: each list that's been read and is read under one of
    /// `tags` is read again. The new first page replaces it, even one
    /// paged past it, and it shows until then.
    pub fn refresh(&mut self, tags: &[String], library: &mut Library) -> Vec<Effect> {
        Kind::ALL
            .into_iter()
            .filter_map(|kind| {
                let shelf = self.shelf(kind, library)?;
                let list = self.list_mut(kind, library);
                let stale = list.asked && shelf.tags().iter().any(|tag| tags.contains(tag));
                stale.then(|| {
                    list.read_in = None;
                    list.items.reread();
                    Effect::Read(shelf)
                })
            })
            .collect()
    }

    pub fn update(&mut self, message: Message, library: &mut Library) -> Effect {
        match message {
            Message::Select(kind) => {
                self.kind = kind;
                self.ask(kind, library)
            }
            Message::Items { shelf, read } => {
                if Some(&shelf) != self.shelf(shelf.kind, library).as_ref() {
                    return Effect::None;
                }
                let list = self.list_mut(shelf.kind, library);
                // The first read in a new order, or after an edit, replaces
                // the old list.
                if list.read_in != Some(shelf.sort) {
                    list.items.reread();
                    list.read_in = Some(shelf.sort);
                }
                list.items.apply(read, "sidebar's Library");
                Effect::None
            }
            Message::More {
                shelf,
                offset,
                result,
            } => {
                if Some(&shelf) != self.shelf(shelf.kind, library).as_ref() {
                    return Effect::None;
                }
                let list = self.list_mut(shelf.kind, library);
                if list.read_in == Some(shelf.sort) {
                    list.items.more(offset, result);
                }
                Effect::None
            }
            Message::EndInView => {
                let offset = self.list_mut(self.kind, library).items.next();
                self.more(offset, library)
            }
            Message::RetryMore => {
                let offset = self.list_mut(self.kind, library).items.retry();
                self.more(offset, library)
            }
            Message::Retry => {
                let list = self.list_mut(self.kind, library);
                list.items = List::new();
                list.read_in = None;
                self.shelf(self.kind, library)
                    .map_or(Effect::None, Effect::Read)
            }
            Message::Link(link) => Effect::Link(link),
        }
    }

    /// Read a type's list the first time it's wanted.
    fn ask(&mut self, kind: Kind, library: &mut Library) -> Effect {
        let Some(shelf) = self.shelf(kind, library) else {
            return Effect::None;
        };
        let list = self.list_mut(kind, library);
        if list.asked {
            return Effect::None;
        }
        list.asked = true;
        Effect::Read(shelf)
    }

    /// Read on from `offset`, unless the list on screen is in an order
    /// that's being replaced: its offset means nothing in the new one.
    fn more(&self, offset: Option<usize>, library: &Library) -> Effect {
        let showing = self.list(self.kind, library);
        if showing.read_in != Some(showing.sort) {
            return Effect::None;
        }
        match (self.shelf(self.kind, library), offset) {
            (Some(shelf), Some(offset)) => Effect::More {
                shelf,
                offset,
                cursor: showing.items.cursor(),
            },
            _ => Effect::None,
        }
    }

    /// What a type's list is read as now: the top level, in its order.
    fn shelf(&self, kind: Kind, library: &Library) -> Option<Shelf> {
        Some(Shelf {
            user_id: self.user_id?,
            kind,
            folder: None,
            sort: self.list(kind, library).sort,
        })
    }

    fn list<'a>(&'a self, kind: Kind, library: &'a Library) -> &'a Shelved {
        match kind {
            Kind::Playlists => &library.root,
            _ => &self.lists[&kind],
        }
    }

    fn list_mut<'a>(&'a mut self, kind: Kind, library: &'a mut Library) -> &'a mut Shelved {
        match kind {
            Kind::Playlists => &mut library.root,
            _ => self.lists.get_mut(&kind).expect("every type has a list"),
        }
    }

    /// The sidebar, with the Page at `current` marked where it's listed,
    /// and a dot on Feed when it has something `unseen`.
    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        current: &Route,
        unseen: bool,
    ) -> Element<'a, Message> {
        let home = nav(
            icon(Icon::House, 20.0, style::TEXT_PRIMARY).into(),
            "Home",
            Route::home(),
            matches!(current, Route::Home { .. }),
        );
        let explore = nav(
            icon(Icon::Compass, 20.0, style::TEXT_PRIMARY).into(),
            "Explore",
            Route::Explore,
            matches!(current, Route::Explore | Route::ExplorePage { .. }),
        );
        let bell = icon(Icon::Bell, 20.0, style::TEXT_PRIMARY);
        let bell = if unseen {
            let dot = container(space()).style(dot).width(10).height(10);
            stack![bell, container(dot).align_right(20).align_top(20)].into()
        } else {
            bell.into()
        };
        let feed = nav(bell, "Feed", Route::Feed, matches!(current, Route::Feed));
        let pages = column![home, explore, feed].spacing(2);
        let show_all = button(text("Show all").size(12))
            .padding([4, 8])
            .style(show_all_style)
            .on_press(Message::Link(Link::Open(Route::Library {
                kind: self.kind,
            })));
        let new_playlist = button(icon(Icon::Plus, 16.0, style::TEXT_SECONDARY))
            .padding(4)
            .style(style::icon_button)
            .on_press(Message::Link(Link::Ask(Box::new(Ask::NewPlaylist))));
        let header = row![
            icon(Icon::Library, 20.0, style::TEXT_SECONDARY),
            text("Your Library").size(13).color(style::TEXT_SECONDARY),
            space::horizontal(),
            new_playlist,
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
        let list = scrollable(self.rows(images, library, current))
            .height(Length::Fill)
            .width(Length::Fill);
        let body = column![text(DISPLAY_NAME).size(22), pages, header, pills, list].spacing(12);
        container(body)
            .padding(16)
            .width(WIDTH)
            .height(Length::Fill)
            .style(frame)
            .into()
    }

    /// The type showing, with Loved tracks pinned above the playlists.
    fn rows<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        current: &Route,
    ) -> Element<'a, Message> {
        let shelved = self.list(self.kind, library);
        let items = match self.shelf(self.kind, library) {
            Some(shelf) => library.apply(shelved.items.items(), Listing::Shelf(&shelf)),
            None => Vec::new(),
        };
        let loved = (self.kind == Kind::Playlists).then(|| {
            entry(
                loved_art(ART_SIZE, ART_RADIUS),
                "Loved Tracks",
                "Collection".to_string(),
                Some(Route::Favorites),
                current,
            )
            .map(Message::Link)
        });
        let rows: Element<'a, Message> = match &shelved.items.list {
            Remote::Loading => notice("Loading…"),
            Remote::NotFound | Remote::Failed(_) => column![
                notice("Couldn't load your Library"),
                container(button(text("Retry").size(12)).on_press(Message::Retry)).padding([0, 12]),
            ]
            .spacing(8)
            .into(),
            Remote::Loaded(_) if items.is_empty() => notice(empty(self.kind)),
            Remote::Loaded(_) => Column::with_children(
                items
                    .into_iter()
                    .map(|item| self.item(item, images, library, current)),
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

    /// One row. Playlists and cards open their card menu when
    /// right-clicked.
    fn item<'a>(
        &self,
        item: &'a Item,
        images: &'a Images,
        library: &'a Library,
        current: &Route,
    ) -> Element<'a, Message> {
        let art = |cover, radius| rounded_cover(images, cover, ART_SIZE, radius);
        let row = match item {
            Item::Folder(folder) => entry(
                folder_art(ART_SIZE, ART_RADIUS),
                &folder.name,
                folder.subtitle(),
                Some(folder_route(folder)),
                current,
            ),
            // A new playlist TIDAL hasn't made yet leads nowhere.
            Item::Playlist(playlist) if is_placeholder(playlist) => entry(
                art(None, ART_RADIUS),
                &playlist.title,
                library::playlist_subtitle(playlist, self.user_id),
                None,
                current,
            ),
            Item::Playlist(playlist) => {
                let row = entry(
                    art(playlist.cover.as_ref(), ART_RADIUS),
                    &playlist.title,
                    library::playlist_subtitle(playlist, self.user_id),
                    Some(playlist_route(playlist)),
                    current,
                );
                playlist_menu(row, playlist, self.user_id, library)
            }
            Item::Card(card) => {
                let radius = match card.target {
                    Target::Artist(_) => ART_SIZE / 2.0,
                    _ => ART_RADIUS,
                };
                let Some(route) = cards::route(card) else {
                    return space().into();
                };
                let row = entry(
                    art(card.cover.as_ref(), radius),
                    &card.title,
                    card.subtitle.clone(),
                    Some(route),
                    current,
                );
                let liked = cards::liked(card, library);
                menu::with_menu(row, move || menu::card(card, liked))
            }
        };
        row.map(Message::Link)
    }
}

/// A Page that's always there, as Home, by its icon.
fn nav<'a>(
    art: Element<'a, Message>,
    label: &'a str,
    route: Route,
    here: bool,
) -> Element<'a, Message> {
    button(
        row![art, text(label).size(14)]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .on_press(Message::Link(Link::Open(route)))
    .style(item_style(here))
    .width(Length::Fill)
    .into()
}

/// sone's unseen dot: the accent, ringed in the sidebar's colour.
fn dot(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::ACCENT.into()),
        border: iced::Border {
            color: style::BG_SIDEBAR,
            width: 2.0,
            radius: 5.0.into(),
        },
        ..container::Style::default()
    }
}

/// One row: the art, the title over a line about it, leading to `route`.
fn entry<'a>(
    art: Element<'a, Link>,
    title: &'a str,
    subtitle: String,
    route: Option<Route>,
    current: &Route,
) -> Element<'a, Link> {
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
    let here = route
        .as_ref()
        .is_some_and(|route| same_place(route, current));
    button(row![art, words].spacing(10).align_y(Alignment::Center))
        .padding([6, 6])
        .width(Length::Fill)
        .style(item_style(here))
        .on_press_maybe(route.map(Link::Open))
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
