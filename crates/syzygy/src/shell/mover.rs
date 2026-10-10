//! "Move to folder": a popover beside the menu item that opened it,
//! listing the Folders at the top level by name with the recent ones
//! first, the top level itself when the playlist is in a Folder, and "New
//! folder", which makes a Folder with the playlist in it. The Folders are
//! a Catalog read of the top level, shown through `library::apply`, so the
//! user's edits show in it as everywhere else.

use iced::widget::{
    Column, button, column, container, mouse_area, opaque, pin, responsive, row, scrollable, space,
    stack, text,
};
use iced::{Alignment, Element, Length, Rectangle};
use syzygy_catalog::library::{Folder, Item};
use syzygy_catalog::{Kind, Read, Shelf};

use super::picker::{heading, note, place};
use crate::icons::{Icon, icon};
use crate::library::{self, Ask, Library, Listing, Placed};
use crate::page::menu;
use crate::settings::RECENT_FOLDERS;
use crate::style;

const WIDTH: f32 = 280.0;
/// The most the list of Folders takes before it scrolls.
const LIST_HEIGHT: f32 = 300.0;

pub struct Mover {
    /// The playlist moving, and the Folder it's in.
    playlist: Placed,
    /// The menu item that opened it, on screen.
    at: Rectangle,
    /// The top level of the user's playlists, which the Folders are read
    /// as.
    shelf: Shelf,
    folders: Folders,
    /// The read it started. Closing it aborts it.
    read: Option<iced::task::Handle>,
}

enum Folders {
    Reading,
    Read(Vec<Item>),
    Failed,
}

#[derive(Debug, Clone)]
pub enum Message {
    Folders(Read<Vec<Folder>>),
    /// Move it here: `None` is the top level.
    Move(Option<Folder>),
    /// "New folder": the form for a Folder with the playlist in it.
    New,
    Close,
}

/// What an answer to the popover does.
pub enum Outcome {
    /// It stays open.
    None,
    Close,
    /// It closes, and the Library takes the message.
    Library(Box<library::Message>),
}

impl Mover {
    pub fn new(playlist: Placed, at: Rectangle, user_id: u64) -> Self {
        Self {
            playlist,
            at,
            shelf: Shelf {
                user_id,
                kind: Kind::Playlists,
                folder: None,
                sort: Kind::Playlists.default_sort(),
            },
            folders: Folders::Reading,
            read: None,
        }
    }

    /// The read it started, aborted once it closes.
    pub fn reading(&mut self, handle: iced::task::Handle) {
        self.read = Some(handle.abort_on_drop());
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Folders(read) => match read {
                Read::Cached(folders) | Read::Fresh(Ok(folders)) => {
                    self.folders = Folders::Read(folders.into_iter().map(Item::Folder).collect());
                }
                Read::Fresh(Err(e)) => {
                    log::warn!("Could not read the Folders: {e}");
                    // A cached list is better than none.
                    if matches!(self.folders, Folders::Reading) {
                        self.folders = Folders::Failed;
                    }
                }
            },
            Message::Move(to) => {
                return Outcome::Library(Box::new(library::Message::Move(
                    self.playlist.clone(),
                    to,
                )));
            }
            Message::New => {
                return Outcome::Library(Box::new(library::Message::Ask(Ask::NewFolder(Some(
                    self.playlist.clone(),
                )))));
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
        let new = action(Icon::FolderPlus, "New folder", Message::New);
        // The top level, unless it's there already.
        let root = self
            .playlist
            .folder
            .is_some()
            .then(|| action(Icon::Library, "Your Library", Message::Move(None)));
        let list: Element<'a, Message> = match &self.folders {
            Folders::Reading => note("Loading\u{2026}"),
            Folders::Failed => note("Couldn't load your folders"),
            Folders::Read(items) => {
                let here = self.playlist.folder.as_deref();
                let shown: Vec<Folder> = library
                    .apply(items, Listing::Shelf(&self.shelf))
                    .into_iter()
                    .filter_map(|item| match item {
                        Item::Folder(folder)
                            if !library::is_placeholder_folder(folder)
                                && Some(folder.id.as_str()) != here =>
                        {
                            Some(library.folder(folder))
                        }
                        _ => None,
                    })
                    .collect();
                let (recents, rest) = arrange(shown, recent);
                let rows = |folders: Vec<Folder>| folders.into_iter().map(folder_row);
                let mut list = Column::new();
                match (recents.is_empty(), rest.is_empty()) {
                    (true, true) => list = list.push(note("No folders yet")),
                    (true, false) => list = list.extend(rows(rest)),
                    (false, _) => {
                        list = list.push(heading("Recent")).extend(rows(recents));
                        if !rest.is_empty() {
                            list = list.push(heading("All folders")).extend(rows(rest));
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
            container(text("Move to folder").size(13).color(style::TEXT_MUTED)).padding([4, 16]),
            new,
        ]
        .push(root)
        .push(list)
        .spacing(4);
        container(panel)
            .padding([8, 0])
            .width(WIDTH)
            .style(menu::panel)
            .into()
    }
}

/// The recent Folders, in the order they were last moved into and at most
/// [`RECENT_FOLDERS`], then the rest by name.
fn arrange(mut shown: Vec<Folder>, recent: &[String]) -> (Vec<Folder>, Vec<Folder>) {
    shown.sort_by_cached_key(|folder| folder.name.to_lowercase());
    let recents: Vec<Folder> = recent
        .iter()
        .take(RECENT_FOLDERS)
        .filter_map(|id| shown.iter().find(|folder| folder.id == *id).cloned())
        .collect();
    let rest = shown
        .into_iter()
        .filter(|folder| !recents.iter().any(|recent| recent.id == folder.id))
        .collect();
    (recents, rest)
}

/// A row that does something other than move into a listed Folder.
fn action<'a>(glyph: Icon, label: &'a str, message: Message) -> Element<'a, Message> {
    let line = row![
        icon(glyph, 18.0, style::TEXT_MUTED),
        text(label).size(14).color(style::TEXT_SECONDARY),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    button(line)
        .padding([10, 16])
        .width(Length::Fill)
        .style(menu::item_style)
        .on_press(message)
        .into()
}

/// A Folder to move into, with how many playlists it holds.
fn folder_row<'a>(folder: Folder) -> Element<'a, Message> {
    let line = row![
        icon(Icon::FolderOpen, 16.0, style::TEXT_MUTED),
        text(folder.name.clone())
            .size(14)
            .color(style::TEXT_SECONDARY)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
        text(folder.subtitle()).size(12).color(style::TEXT_DISABLED),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    button(container(line).clip(true))
        .padding([8, 16])
        .width(Length::Fill)
        .style(menu::item_style)
        .on_press(Message::Move(Some(folder)))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str) -> Folder {
        Folder {
            id: id.to_string(),
            name: name.to_string(),
            playlists: Some(0),
        }
    }

    fn names(folders: &[Folder]) -> Vec<&str> {
        folders.iter().map(|folder| folder.name.as_str()).collect()
    }

    #[test]
    fn the_recent_folders_go_first_in_their_order_and_the_rest_by_name() {
        let all = vec![
            folder("a", "running"),
            folder("b", "Focus"),
            folder("c", "Chill"),
            folder("d", "Work"),
        ];
        let recent = ["d".to_string(), "gone".to_string(), "b".to_string()];

        let (recents, rest) = arrange(all, &recent);

        assert_eq!(names(&recents), ["Work", "Focus"]);
        assert_eq!(names(&rest), ["Chill", "running"]);
    }
}
