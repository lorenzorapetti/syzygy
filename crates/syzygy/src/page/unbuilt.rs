//! Playlist and Favorites until their Pages land: the hero drawn from the
//! card's Preview.

use iced::Element;
use iced::widget::{column, text};

use super::{Link, PADDING, Preview, hero};
use crate::images::Images;

pub struct State {
    preview: Option<Preview>,
}

impl State {
    pub fn new(preview: Option<Preview>) -> Self {
        Self { preview }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Link> {
        let (title, artist, art) = match &self.preview {
            Some(preview) => (
                preview.title.as_str(),
                preview.artist.as_deref(),
                preview.cover.as_ref(),
            ),
            None => ("", None, None),
        };
        let lines = artist
            .map(|artist| text(artist).style(text::secondary).into())
            .into_iter()
            .chain([text("This Page isn't built yet.")
                .size(13)
                .style(text::secondary)
                .into()])
            .collect();
        column![hero::view(images, "", title, art, false, lines)]
            .padding(PADDING)
            .into()
    }
}
