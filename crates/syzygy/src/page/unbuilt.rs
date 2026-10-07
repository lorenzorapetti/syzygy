//! Album, Artist, Playlist, Mix and Favorites until their Pages land: the
//! hero drawn from the card's Preview.

use iced::Element;
use iced::widget::{column, container, row, text};

use super::{Message, Preview, cover_placeholder};

pub struct State {
    preview: Option<Preview>,
}

impl State {
    pub fn new(preview: Option<Preview>) -> Self {
        Self { preview }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let (title, artist) = match &self.preview {
            Some(preview) => (preview.title.as_str(), preview.artist.as_deref()),
            None => ("", None),
        };
        let details = column![text(title).size(32)]
            .push(artist.map(|artist| text(artist).style(text::secondary)))
            .push(
                text("This Page isn't built yet.")
                    .size(13)
                    .style(text::secondary),
            )
            .spacing(8);
        container(
            row![cover_placeholder(200.0), details]
                .spacing(24)
                .align_y(iced::Alignment::End),
        )
        .padding(24)
        .into()
    }
}
