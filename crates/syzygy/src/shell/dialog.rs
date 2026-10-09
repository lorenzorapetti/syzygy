//! The Library's dialog: the form for a new or an Own playlist, and the
//! confirmation before one is deleted. One is open at a time, over the
//! Shell, and Escape or a click outside closes it.

use iced::widget::{
    button, center, column, container, mouse_area, opaque, row, space, text, text_input, toggler,
};
use iced::{Alignment, Element};
use syzygy_catalog::playlist::DESCRIPTION_LIMIT;
use syzygy_catalog::{Playlist, PlaylistFields};

use crate::library::{self, Ask, Library};
use crate::style;

const WIDTH: f32 = 440.0;

pub enum Dialog {
    /// A new playlist's fields, or an Own playlist's being edited.
    Playlist {
        editing: Option<Playlist>,
        fields: PlaylistFields,
    },
    /// Whether to delete an Own playlist.
    Delete(Playlist),
}

#[derive(Debug, Clone)]
pub enum Message {
    Title(String),
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
            Ask::NewPlaylist => Dialog::Playlist {
                editing: None,
                fields: PlaylistFields::new(),
            },
            Ask::EditPlaylist(editing) => Dialog::Playlist {
                fields: PlaylistFields::of(library.playlist(&editing)),
                editing: Some(editing),
            },
            Ask::DeletePlaylist(playlist) => Dialog::Delete(playlist),
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
            (Dialog::Playlist { editing, fields }, Message::Submit) => {
                if fields.title.trim().is_empty() {
                    return Outcome::None;
                }
                let fields = fields.trimmed();
                Outcome::Library(Box::new(match editing.take() {
                    Some(playlist) => library::Message::EditPlaylist(playlist, fields),
                    None => library::Message::CreatePlaylist(fields),
                }))
            }
            (Dialog::Delete(playlist), Message::Submit) => {
                Outcome::Library(Box::new(library::Message::DeletePlaylist(playlist.clone())))
            }
            (Dialog::Delete(_), _) => Outcome::None,
        }
    }

    /// The dialog, on a backdrop that darkens what's under it and takes
    /// its clicks. A click outside closes it.
    pub fn view(&self) -> Element<'_, Message> {
        let card = match self {
            Dialog::Playlist { editing, fields } => form(editing.is_some(), fields),
            Dialog::Delete(playlist) => confirm_delete(playlist),
        };
        let card = container(card)
            .padding(24)
            .max_width(WIDTH)
            .style(style::modal);
        let backdrop = center(opaque(card)).style(style::backdrop);
        opaque(mouse_area(backdrop).on_press(Message::Close))
    }
}

fn form<'a>(editing: bool, fields: &'a PlaylistFields) -> Element<'a, Message> {
    let heading = if editing {
        "Edit playlist"
    } else {
        "Create playlist"
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

fn confirm_delete(playlist: &Playlist) -> Element<'_, Message> {
    column![
        text("Delete playlist").size(20),
        text(format!(
            "Delete \u{201c}{}\u{201d}? This can't be undone.",
            library::short(&playlist.title)
        ))
        .size(14)
        .color(style::TEXT_SECONDARY),
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
