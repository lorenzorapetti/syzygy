//! The Library Pages: one type of the user's Library in full (playlists
//! with their Folders, albums, artists or mixes) as a grid of cards loaded
//! a page at a time, or one Folder's playlists. The type is a tab:
//! switching replaces this Back stack entry rather than adding a step. Each
//! type's order is applied by TIDAL and kept in `Settings`.

use iced::widget::{Column, button, column, row, text};
use iced::{Alignment, Element};
use std::collections::BTreeMap;
use std::sync::Arc;
use syzygy_catalog::library::{self, Item};
use syzygy_catalog::{Direction, Kind, LibraryOrder, LibrarySort, Paged, Playlist, Read, Shelf};

use super::cards::{self, CARD_WIDTH};
use super::paged::List;
use super::{Action, Context, Link, Load, PADDING, Preview, Route, cover, folder_art};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::settings::Sort;
use crate::style;

/// A Folder the Page shows the playlists of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRef {
    pub id: String,
    pub name: String,
}

pub struct State {
    kind: Kind,
    /// Only playlists are in Folders.
    folder: Option<FolderRef>,
    user_id: Option<u64>,
    /// Each type's order, as `Settings` had them when the Page opened and
    /// as the user picks them here.
    sorts: BTreeMap<Kind, LibrarySort>,
    items: List<Item>,
}

#[derive(Debug, Clone)]
pub enum Message {
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
    /// The end of the grid is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    SelectKind(Kind),
    /// An order was picked.
    Sort(LibraryOrder),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(kind: Kind, folder: Option<FolderRef>, context: &Context) -> (Self, Action) {
        let sorts = Kind::ALL
            .into_iter()
            .map(|kind| (kind, context.settings.library_sort(kind)))
            .collect();
        let mut state = Self {
            kind,
            folder,
            user_id: context.user_id,
            sorts,
            items: List::new(),
        };
        let action = state.load().map_or(Action::None, Action::Load);
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            // Reads for a type or an order the user has moved on from are
            // dropped.
            Message::Items { shelf, read } if Some(&shelf) == self.shelf().as_ref() => {
                self.items.apply(read, "Library");
                Action::None
            }
            Message::More {
                shelf,
                offset,
                result,
            } if Some(&shelf) == self.shelf().as_ref() => {
                self.items.more(offset, result);
                Action::None
            }
            Message::Items { .. } | Message::More { .. } => Action::None,
            Message::EndInView => {
                let offset = self.items.next();
                self.load_more(offset)
            }
            Message::RetryMore => {
                let offset = self.items.retry();
                self.load_more(offset)
            }
            Message::SelectKind(kind) if kind != self.kind && self.folder.is_none() => {
                self.kind = kind;
                match self.load() {
                    Some(load) => Action::Replace(Route::Library { kind }, Some(load)),
                    None => Action::None,
                }
            }
            Message::SelectKind(_) => Action::None,
            Message::Sort(order) => {
                let sort = LibrarySort::picked(self.sort(), order);
                self.sorts.insert(self.kind, sort);
                let save = Action::SaveSort(Sort::Library(self.kind, sort));
                match self.load() {
                    Some(load) => Action::Batch(vec![Action::Load(load), save]),
                    None => save,
                }
            }
            Message::Link(link) => link.follow(),
            Message::Retry => self.load().map_or(Action::None, Action::Load),
        }
    }

    fn sort(&self) -> LibrarySort {
        self.sorts
            .get(&self.kind)
            .copied()
            .unwrap_or_else(|| self.kind.default_sort())
    }

    /// The shelf on screen. None until TIDAL has said who the user is.
    fn shelf(&self) -> Option<Shelf> {
        Some(Shelf {
            user_id: self.user_id?,
            kind: self.kind,
            folder: self.folder.as_ref().map(|folder| folder.id.clone()),
            sort: self.sort(),
        })
    }

    /// Read the shelf again from the first page.
    fn load(&mut self) -> Option<Load> {
        self.items = List::new();
        let shelf = self.shelf();
        if shelf.is_none() {
            self.items.fail(syzygy_catalog::Error::UnknownUser);
        }
        shelf.map(Load::Library)
    }

    /// Load the shelf from `offset`, if there's one to load.
    fn load_more(&self, offset: Option<usize>) -> Action {
        match (self.shelf(), offset) {
            (Some(shelf), Some(offset)) => Action::Load(Load::MoreLibrary {
                shelf,
                offset,
                cursor: self.items.cursor(),
            }),
            _ => Action::None,
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let title = match &self.folder {
            Some(folder) => folder.name.as_str(),
            None => title(self.kind),
        };
        let total = self
            .items
            .total()
            .map(|total| noun(self.kind, total))
            .unwrap_or_default();
        let header = column![
            text(title).size(32),
            row![
                text(total).size(14).color(style::TEXT_MUTED),
                self.sort_picker()
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        ]
        .spacing(8);
        let tabs = self.folder.is_none().then(|| {
            row(Kind::ALL.into_iter().map(|kind| {
                let style = if kind == self.kind {
                    style::selected_tab
                } else {
                    style::unselected_tab
                };
                button(text(label(kind)).size(14))
                    .padding([8, 16])
                    .style(style)
                    .on_press(Message::SelectKind(kind))
                    .into()
            }))
            .spacing(8)
            .wrap()
        });
        let body = self.items.list.view(Message::Retry, |_| {
            let items = self.items.items();
            if items.is_empty() {
                let empty = format!("No {} yet", plural(self.kind));
                return text(empty).color(style::TEXT_MUTED).into();
            }
            let tiles = items.iter().map(|item| tile(item, self.user_id, images));
            Column::new()
                .push(cards::wrapped(tiles).map(Message::Link))
                .push(self.items.end(Message::EndInView, Message::RetryMore))
                .spacing(24)
                .into()
        });
        Column::new()
            .push(header)
            .push(tabs)
            .push(body)
            .spacing(24)
            .padding(PADDING)
            .into()
    }

    /// The orders this type can be in, the one it's in marked with which
    /// way it goes. Picking it again turns it around.
    fn sort_picker(&self) -> Element<'_, Message> {
        let sort = self.sort();
        let options = self.kind.orders().iter().map(|&order| {
            let picked = order == sort.order;
            let arrow = match sort.direction {
                Direction::Ascending => " ↑",
                Direction::Descending => " ↓",
            };
            let label = format!("{}{}", order_label(order), if picked { arrow } else { "" });
            let color = if picked {
                style::ACCENT
            } else {
                style::TEXT_SECONDARY
            };
            button(text(label).size(12).color(color))
                .padding([4, 8])
                .style(style::icon_button)
                .on_press(Message::Sort(order))
                .into()
        });
        row![icon(Icon::ArrowUpDown, 14.0, style::TEXT_MUTED)]
            .extend(options)
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
    }
}

/// One thing on a shelf as a card. Folders open to their playlists.
pub fn tile<'a>(item: &'a Item, user_id: Option<u64>, images: &'a Images) -> Element<'a, Link> {
    match item {
        Item::Folder(folder) => cards::tile(
            folder_art(CARD_WIDTH, 4.0),
            &folder.name,
            folder.subtitle(),
            Some(Link::Open(folder_route(folder))),
        ),
        Item::Playlist(playlist) => cards::tile(
            cover(images, playlist.cover.as_ref(), CARD_WIDTH),
            &playlist.title,
            library::playlist_subtitle(playlist, user_id),
            Some(Link::Open(playlist_route(playlist))),
        ),
        Item::Card(card) => cards::card(card, images),
    }
}

pub fn folder_route(folder: &library::Folder) -> Route {
    Route::Folder {
        id: folder.id.clone(),
        name: folder.name.clone(),
    }
}

pub fn playlist_route(playlist: &Playlist) -> Route {
    Route::Playlist {
        uuid: playlist.uuid.clone(),
        preview: Some(Preview {
            title: playlist.title.clone(),
            cover: playlist.cover.clone(),
            artist: None,
        }),
    }
}

/// A Library type's tab.
pub fn label(kind: Kind) -> &'static str {
    match kind {
        Kind::Playlists => "Playlists",
        Kind::Albums => "Albums",
        Kind::Artists => "Artists",
        Kind::Mixes => "Mixes",
    }
}

/// A Library type's Page title, as sone's.
fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Playlists => "Playlists",
        Kind::Albums => "Your favorite albums",
        Kind::Artists => "Artists you follow",
        Kind::Mixes => "Mixes & Radios you liked",
    }
}

fn plural(kind: Kind) -> &'static str {
    match kind {
        Kind::Playlists => "playlists",
        Kind::Albums => "albums",
        Kind::Artists => "artists",
        Kind::Mixes => "mixes",
    }
}

/// "1 album", "12 mixes".
fn noun(kind: Kind, n: usize) -> String {
    let one = match kind {
        Kind::Playlists => "playlist",
        Kind::Albums => "album",
        Kind::Artists => "artist",
        Kind::Mixes => "mix",
    };
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {}", plural(kind))
    }
}

fn order_label(order: LibraryOrder) -> &'static str {
    match order {
        LibraryOrder::DateAdded => "Date added",
        LibraryOrder::LastUpdated => "Last updated",
        LibraryOrder::Name => "Name",
        LibraryOrder::Artist => "Artist",
        LibraryOrder::ReleaseDate => "Release date",
        LibraryOrder::MixType => "Type",
    }
}
