//! The Library's dialog: the form for a new or an Own playlist, a
//! Folder's name, and the confirmation before a playlist or a Folder is
//! deleted. One is open at a time, over the Shell, and Escape or a click
//! outside closes it.

use iced::widget::{
    button, center, column, container, mouse_area, opaque, row, space, text, text_input, toggler,
};
use iced::{Alignment, Element};
use syzygy_catalog::library::Folder;
use syzygy_catalog::playlist::DESCRIPTION_LIMIT;
use syzygy_catalog::{Playlist, PlaylistFields, Track};

use crate::library::{self, Ask, Library, Placed};
use crate::style;

const WIDTH: f32 = 440.0;

pub enum Dialog {
    /// A new playlist's fields, or an Own playlist's being edited. A new
    /// one gets `tracks`, or goes into `folder`.
    Playlist {
        editing: Option<Playlist>,
        fields: PlaylistFields,
        tracks: Vec<Track>,
        folder: Option<Folder>,
    },
    /// Whether to delete an Own playlist.
    Delete(Playlist),
    /// A new Folder's name, with `playlist` going into it, or a Folder's
    /// new name.
    Folder {
        renaming: Option<Folder>,
        name: String,
        playlist: Option<Placed>,
    },
    /// Whether to delete an empty Folder, as read, by its name as shown.
    DeleteFolder { folder: Folder, name: String },
}

#[derive(Debug, Clone)]
pub enum Message {
    Title(String),
    /// A Folder's name.
    Name(String),
    Description(String),
    Public(bool),
    Submit,
    Close,
}

/// What an answer to the dialog does.
pub enum Outcome {
    /// It stays open.
    None,
    Close,
    /// It closes, and the Library makes the edit.
    Library(Box<library::Message>),
}

impl Dialog {
    /// The dialog the Library asked for. An Own playlist's form starts
    /// from the playlist as the user last edited it.
    pub fn new(ask: Ask, library: &Library) -> Self {
        match ask {
            Ask::NewPlaylist(tracks) => Dialog::Playlist {
                editing: None,
                fields: PlaylistFields::new(),
                tracks,
                folder: None,
            },
            Ask::NewPlaylistIn(folder) => Dialog::Playlist {
                editing: None,
                fields: PlaylistFields::new(),
                tracks: vec![],
                folder: Some(library.folder(&folder)),
            },
            Ask::EditPlaylist(editing) => Dialog::Playlist {
                fields: PlaylistFields::of(library.playlist(&editing)),
                editing: Some(editing),
                tracks: vec![],
                folder: None,
            },
            Ask::DeletePlaylist(playlist) => Dialog::Delete(playlist),
            Ask::NewFolder(playlist) => Dialog::Folder {
                renaming: None,
                name: String::new(),
                playlist,
            },
            Ask::RenameFolder(folder) => Dialog::Folder {
                name: library.folder(&folder).name,
                renaming: Some(folder),
                playlist: None,
            },
            Ask::DeleteFolder(folder) => Dialog::DeleteFolder {
                name: library.folder(&folder).name,
                folder,
            },
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match (self, message) {
            (_, Message::Close) => Outcome::Close,
            (Dialog::Playlist { fields, .. }, Message::Title(title)) => {
                fields.title = title;
                Outcome::None
            }
            (Dialog::Playlist { fields, .. }, Message::Description(description)) => {
                fields.description = description.chars().take(DESCRIPTION_LIMIT).collect();
                Outcome::None
            }
            (Dialog::Playlist { fields, .. }, Message::Public(public)) => {
                fields.public = public;
                Outcome::None
            }
            (
                Dialog::Playlist {
                    editing,
                    fields,
                    tracks,
                    folder,
                },
                Message::Submit,
            ) => {
                if fields.title.trim().is_empty() {
                    return Outcome::None;
                }
                let fields = fields.trimmed();
                Outcome::Library(Box::new(match (editing.take(), folder.take()) {
                    (Some(playlist), _) => library::Message::EditPlaylist(playlist, fields),
                    (None, Some(folder)) => library::Message::CreatePlaylistIn(fields, folder),
                    (None, None) => {
                        library::Message::CreatePlaylist(fields, std::mem::take(tracks))
                    }
                }))
            }
            (Dialog::Folder { name, .. }, Message::Name(typed)) => {
                *name = typed;
                Outcome::None
            }
            (
                Dialog::Folder {
                    renaming,
                    name,
                    playlist,
                },
                Message::Submit,
            ) => {
                if name.trim().is_empty() {
                    return Outcome::None;
                }
                let name = name.trim().to_string();
                Outcome::Library(Box::new(match renaming.take() {
                    Some(folder) => library::Message::RenameFolder(folder, name),
                    None => library::Message::CreateFolder(name, playlist.take()),
                }))
            }
            (Dialog::Delete(playlist), Message::Submit) => {
                Outcome::Library(Box::new(library::Message::DeletePlaylist(playlist.clone())))
            }
            (Dialog::DeleteFolder { folder, .. }, Message::Submit) => {
                Outcome::Library(Box::new(library::Message::DeleteFolder(folder.clone())))
            }
            (Dialog::Playlist { .. }, Message::Name(_))
            | (Dialog::Folder { .. }, _)
            | (Dialog::Delete(_) | Dialog::DeleteFolder { .. }, _) => Outcome::None,
        }
    }

    /// The dialog, on a backdrop that darkens what's under it and takes
    /// its clicks. A click outside closes it.
    pub fn view(&self) -> Element<'_, Message> {
        let card = match self {
            Dialog::Playlist {
                editing,
                fields,
                folder,
                ..
            } => form(editing.is_some(), fields, folder.as_ref()),
            Dialog::Delete(playlist) => confirm_delete(
                "Delete playlist",
                format!(
                    "Delete \u{201c}{}\u{201d}? This can't be undone.",
                    library::short(&playlist.title)
                ),
            ),
            Dialog::Folder { renaming, name, .. } => folder_form(renaming.is_some(), name),
            Dialog::DeleteFolder { name, .. } => confirm_delete(
                "Delete folder",
                format!("Delete \u{201c}{}\u{201d}?", library::short(name)),
            ),
        };
        let card = container(card)
            .padding(24)
            .max_width(WIDTH)
            .style(style::modal);
        let backdrop = center(opaque(card)).style(style::backdrop);
        opaque(mouse_area(backdrop).on_press(Message::Close))
    }
}

fn form<'a>(
    editing: bool,
    fields: &'a PlaylistFields,
    folder: Option<&Folder>,
) -> Element<'a, Message> {
    let heading = match (editing, folder) {
        (true, _) => "Edit playlist".to_string(),
        (false, Some(folder)) => format!(
            "Create playlist in \u{201c}{}\u{201d}",
            library::short(&folder.name)
        ),
        (false, None) => "Create playlist".to_string(),
    };
    let title = text_input("Title", &fields.title)
        .on_input(Message::Title)
        .on_submit(Message::Submit)
        .padding([8, 14])
        .size(14)
        .style(style::filter_input);
    let description = text_input("Description (optional)", &fields.description)
        .on_input(Message::Description)
        .on_submit(Message::Submit)
        .padding([8, 14])
        .size(14)
        .style(style::filter_input);
    let count = text(format!(
        "{}/{DESCRIPTION_LIMIT}",
        fields.description.chars().count()
    ))
    .size(12)
    .color(style::TEXT_MUTED);
    let (access, explained) = if fields.public {
        ("Public", "Anyone can find it on TIDAL.")
    } else {
        ("Unlisted", "Only people with the link can open it.")
    };
    let access = row![
        column![
            text(access).size(14),
            text(explained).size(12).color(style::TEXT_MUTED),
        ]
        .spacing(2),
        space::horizontal(),
        toggler(fields.public).size(20).on_toggle(Message::Public),
    ]
    .align_y(Alignment::Center);
    let can_save = !fields.title.trim().is_empty();
    let save = if editing { "Save" } else { "Create" };
    column![
        text(heading).size(20),
        title,
        column![description, row![space::horizontal(), count]].spacing(4),
        access,
        buttons(save, can_save),
    ]
    .spacing(16)
    .into()
}

/// A new Folder's name, or a Folder's new name.
fn folder_form(renaming: bool, name: &str) -> Element<'_, Message> {
    let (heading, answer) = if renaming {
        ("Rename folder", "Save")
    } else {
        ("New folder", "Create")
    };
    let input = text_input("Name", name)
        .on_input(Message::Name)
        .on_submit(Message::Submit)
        .padding([8, 14])
        .size(14)
        .style(style::filter_input);
    column![
        text(heading).size(20),
        input,
        buttons(answer, !name.trim().is_empty()),
    ]
    .spacing(16)
    .into()
}

fn confirm_delete<'a>(heading: &'a str, question: String) -> Element<'a, Message> {
    column![
        text(heading).size(20),
        text(question).size(14).color(style::TEXT_SECONDARY),
        buttons("Delete", true),
    ]
    .spacing(16)
    .into()
}

/// Cancel, and the button that answers.
fn buttons<'a>(answer: &'a str, enabled: bool) -> Element<'a, Message> {
    row![
        space::horizontal(),
        button(text("Cancel").size(14))
            .padding([8, 16])
            .style(style::pill_button)
            .on_press(Message::Close),
        button(text(answer).size(14))
            .padding([8, 16])
            .style(style::accent_pill)
            .on_press_maybe(enabled.then_some(Message::Submit)),
    ]
    .spacing(8)
    .into()
}
