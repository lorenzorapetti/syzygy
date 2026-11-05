//! The Library Pages: one type of the user's Library in full (playlists
//! with their Folders, albums, artists or mixes) as a grid of cards loaded
//! a page at a time, or one Folder's playlists. The type is a tab:
//! switching replaces this Back stack entry rather than adding a step. Each
//! type's order is applied by TIDAL and kept in `Settings`.

use iced::widget::{Column, button, column, row, space, text};
use iced::{Alignment, Element};
use std::collections::BTreeMap;
use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::library::{self, Item};
use syzygy_catalog::{Direction, Kind, LibraryOrder, LibrarySort, Paged, Playlist, Read, Shelf};

use super::cards::{self, CARD_WIDTH};
use super::paged::List;
use super::{Action, Context, Link, Load, PADDING, Preview, Route, cover, folder_art, menu};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::library::{Ask, Library, Listing, is_placeholder, is_placeholder_folder};
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

    /// What's read under `tags` changed: read the shelf again if it's one
    /// of them, keeping it on screen until the new first page arrives.
    pub fn refresh(&mut self, tags: &[String]) -> Action {
        let Some(shelf) = self.shelf() else {
            return Action::None;
        };
        if !shelf.tags().iter().any(|tag| tags.contains(tag)) {
            return Action::None;
        }
        self.items.reread();
        Action::Load(Load::Library(shelf))
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

    /// The Folder on screen, as the user last named it.
    fn folder(&self, library: &Library) -> Option<library::Folder> {
        let folder = self.folder.as_ref()?;
        Some(library.folder(&library::Folder {
            id: folder.id.clone(),
            name: folder.name.clone(),
            playlists: None,
        }))
    }

    pub fn view<'a>(&'a self, images: &'a Images, library: &'a Library) -> Element<'a, Message> {
        let folder = self.folder(library);
        let title = match &folder {
            Some(folder) => folder.name.clone(),
            None => title(self.kind).to_string(),
        };
        let total = self
            .items
            .total()
            .map(|total| noun(self.kind, total))
            .unwrap_or_default();
        let ask = |ask| Message::Link(Link::Ask(Box::new(ask)));
        let new = |glyph, label, ask| {
            button(
                row![icon(glyph, 16.0, style::TEXT_PRIMARY), text(label).size(14)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding([8, 16])
            .style(style::pill_button)
            .on_press(ask)
        };
        // A Folder gets new playlists, and the top level Folders too.
        let buttons = match (&folder, self.kind) {
            (Some(folder), _) => row![new(
                Icon::Plus,
                "New playlist",
                ask(Ask::NewPlaylistIn(folder.clone()))
            )],
            (None, Kind::Playlists) => row![
                new(Icon::FolderPlus, "New folder", ask(Ask::NewFolder(None))),
                new(Icon::Plus, "New playlist", ask(Ask::NewPlaylist(vec![]))),
            ],
            (None, _) => row![],
        }
        .spacing(8);
        let header = column![
            row![text(title).size(32), space::horizontal(), buttons].align_y(Alignment::Center),
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
            let items = match self.shelf() {
                Some(shelf) => library.apply(self.items.items(), Listing::Shelf(&shelf)),
                None => Vec::new(),
            };
            if items.is_empty() {
                let empty = format!("No {} yet", plural(self.kind));
                return text(empty).color(style::TEXT_MUTED).into();
            }
            let here = self.folder.as_ref().map(|folder| folder.id.as_str());
            let tiles = items
                .into_iter()
                .map(|item| tile(item, here, self.user_id, images, library));
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

/// One thing on a shelf as a card, listed in the Folder `here` (`None`
/// is the top level). Folders open to their playlists, and they and
/// playlists open their menu when right-clicked, as cards do. A new Folder
/// or playlist TIDAL hasn't made yet does neither.
pub fn tile<'a>(
    item: &'a Item,
    here: Option<&str>,
    user_id: Option<u64>,
    images: &'a Images,
    library: &'a Library,
) -> Element<'a, Link> {
    match item {
        Item::Folder(folder) => {
            let placeholder = is_placeholder_folder(folder);
            let shown = library.folder(folder);
            let tile = cards::tile(
                folder_art(CARD_WIDTH, 4.0),
                shown.name.clone(),
                shown.subtitle(),
                (!placeholder).then(|| Link::Open(folder_route(&shown))),
            );
            if placeholder {
                return tile;
            }
            folder_menu(tile, folder, library)
        }
        Item::Playlist(playlist) => {
            let placeholder = is_placeholder(playlist);
            let tile = cards::tile(
                cover(images, playlist.cover.as_ref(), CARD_WIDTH),
                &playlist.title,
                library::playlist_subtitle(playlist, user_id),
                (!placeholder).then(|| Link::Open(playlist_route(playlist))),
            );
            if placeholder {
                return tile;
            }
            playlist_menu(tile, playlist, here, user_id, library)
        }
        Item::Card(card) => cards::card(card, images, library),
    }
}

/// `underlay`, opening a Folder's menu when right-clicked.
pub fn folder_menu<'a>(
    underlay: Element<'a, Link>,
    folder: &library::Folder,
    library: &Library,
) -> Element<'a, Link> {
    // The Folder as read: what it's asked about counts the pending edits
    // itself.
    let deletable = library.deletable(folder);
    let folder = folder.clone();
    menu::with_menu(underlay, move || menu::folder(&folder, deletable))
}

/// `underlay`, opening a playlist's card menu when right-clicked. An Own
/// playlist isn't a Favorite: its menu has no like, but edits it, moves it
/// out of the Folder `here` (`None` is the top level) and deletes it.
pub fn playlist_menu<'a>(
    underlay: Element<'a, Link>,
    playlist: &Playlist,
    here: Option<&str>,
    user_id: Option<u64>,
    library: &Library,
) -> Element<'a, Link> {
    let card = playlist_card(playlist);
    if playlist.is_own(user_id) {
        let playlist = playlist.clone();
        let here = here.map(str::to_string);
        menu::with_menu(underlay, move || {
            menu::own_playlist(&card, &playlist, Some(here.clone()))
        })
    } else {
        let liked = cards::liked(&card, library);
        menu::with_menu(underlay, move || menu::card(&card, liked))
    }
}

/// A playlist as a card, for its menu.
pub fn playlist_card(playlist: &Playlist) -> Card {
    Card {
        title: playlist.title.clone(),
        subtitle: String::new(),
        cover: playlist.cover.clone(),
        target: Target::Playlist(playlist.uuid.clone()),
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

pub fn order_label(order: LibraryOrder) -> &'static str {
    match order {
        LibraryOrder::DateAdded => "Date added",
        LibraryOrder::LastUpdated => "Last updated",
        LibraryOrder::Name => "Name",
        LibraryOrder::Artist => "Artist",
        LibraryOrder::ReleaseDate => "Release date",
        LibraryOrder::MixType => "Type",
    }
}
