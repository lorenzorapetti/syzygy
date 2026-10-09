//! The maximized player: the whole window for what's playing, its cover as
//! large as fits over its title, artists and the player bar's transport.

use iced::widget::{button, column, container, responsive, row, space, text};
use iced::{Alignment, Element, Font, Length, font};

use super::player_bar::{self, PlayerBar};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::page;
use crate::playback::Playback;
use crate::style;

/// The largest cover it draws.
const MAX_COVER: f32 = 640.0;
/// What the cover leaves of the window's height: the title, the artists
/// and the transport under it, and the margins.
const BELOW_COVER: f32 = 280.0;
const PADDING: f32 = 32.0;
const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};

#[derive(Debug, Clone)]
pub enum Message {
    Minimize,
    PlayerBar(player_bar::Message),
}

/// What's playing, filling the window. None with nothing to play.
pub fn view<'a>(
    player_bar: &'a PlayerBar,
    playback: &'a Playback,
    images: &'a Images,
) -> Option<Element<'a, Message>> {
    let track = playback.current()?;
    let minimize = button(container(icon(Icon::Minimize2, 20.0, style::TEXT_MUTED)).center(36))
        .padding(0)
        .style(style::icon_button)
        .on_press(Message::Minimize);
    let body = responsive(move |size| {
        let side = (size.width - 2.0 * PADDING)
            .min(size.height - BELOW_COVER)
            .clamp(0.0, MAX_COVER)
            .floor();
        let cover = page::large_cover(images, track, side);
        let caption = column![
            text(&track.title)
                .size(28)
                .font(BOLD)
                .wrapping(text::Wrapping::None),
            page::artists(&track.artists, 16.0),
        ]
        .spacing(6)
        .align_x(Alignment::Center);
        let top = column![cover, caption]
            .spacing(24)
            .align_x(Alignment::Center);
        let top =
            Element::from(top).map(|link| Message::PlayerBar(player_bar::Message::Link(link)));
        let controls = player_bar.controls(playback).map(Message::PlayerBar);
        container(
            column![
                top,
                container(controls)
                    .width(Length::Fill)
                    .center_x(Length::Fill)
            ]
            .spacing(32)
            .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    });
    let screen = column![
        row![space::horizontal(), minimize],
        container(body).height(Length::Fill)
    ]
    .padding(PADDING);
    Some(
        container(screen)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(style::BG_BASE.into()),
                ..container::Style::default()
            })
            .into(),
    )
}
