//! The 90px player bar along the bottom, split 30/40/30: what's playing on
//! the left, play and pause over the seek bar in the middle, the volume on
//! the right.

use iced::widget::slider::{Handle, HandleShape, Rail};
use iced::widget::{Space, button, column, container, row, slider, space, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme, font};

use crate::icons::{Icon, filled, icon};
use crate::images::Images;
use crate::page::{self, Link};
use crate::playback::{self, Playback, Status};
use crate::style;

const HEIGHT: f32 = 90.0;
const COVER_SIZE: f32 = 64.0;
/// The play and pause disc.
const PLAY_SIZE: f32 = 36.0;
/// The seek bar stops growing past this.
const SEEK_WIDTH: f32 = 600.0;
const TIME_WIDTH: f32 = 40.0;
const VOLUME_WIDTH: f32 = 100.0;
const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

/// The seek bar while it's dragged.
#[derive(Default)]
pub struct PlayerBar {
    /// Where the handle is, until it's let go.
    scrubbing: Option<f32>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePlay,
    /// The seek bar's handle moved.
    Scrub(f32),
    /// The seek bar's handle was let go.
    Scrubbed,
    Volume(f32),
    /// The volume slider was let go.
    VolumeSet,
    ToggleMute,
    Link(Link),
}

pub enum Effect {
    None,
    Playback(playback::Message),
    /// Remember the volume.
    SaveVolume,
    Link(Link),
}

impl PlayerBar {
    pub fn update(&mut self, message: Message) -> Effect {
        match message {
            Message::TogglePlay => Effect::Playback(playback::Message::TogglePlay),
            Message::Scrub(position) => {
                self.scrubbing = Some(position);
                Effect::None
            }
            // Only now does the engine seek, so dragging doesn't stutter.
            Message::Scrubbed => match self.scrubbing.take() {
                Some(position) => Effect::Playback(playback::Message::Seek(position)),
                None => Effect::None,
            },
            Message::Volume(volume) => Effect::Playback(playback::Message::SetVolume(volume)),
            Message::VolumeSet => Effect::SaveVolume,
            Message::ToggleMute => Effect::Playback(playback::Message::ToggleMute),
            Message::Link(link) => Effect::Link(link),
        }
    }

    pub fn view<'a>(&'a self, playback: &'a Playback, images: &'a Images) -> Element<'a, Message> {
        let bar = row![
            container(now_playing(playback, images))
                .width(Length::FillPortion(3))
                .align_left(Length::Fill),
            container(self.controls(playback))
                .width(Length::FillPortion(4))
                .center_x(Length::Fill),
            container(volume(playback))
                .width(Length::FillPortion(3))
                .align_right(Length::Fill),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        container(bar)
            .padding([0, 16])
            .width(Length::Fill)
            .center_y(HEIGHT)
            .style(player_bar)
            .into()
    }

    /// Play or pause over the seek bar, between the time played and the
    /// track's length.
    fn controls<'a>(&self, playback: &'a Playback) -> Element<'a, Message> {
        let glyph = match playback.status() {
            Status::Playing => Icon::Pause,
            _ => Icon::Play,
        };
        let play = button(container(filled(glyph, 17.0, style::BG_BASE)).center(PLAY_SIZE))
            .padding(0)
            .style(play_button)
            .on_press_maybe(playback.current().map(|_| Message::TogglePlay));

        let length = playback.current().map_or(0, |track| track.duration) as f32;
        let at = self
            .scrubbing
            .unwrap_or(playback.position())
            .clamp(0.0, length);
        let time = |seconds: f32| {
            text(page::duration(seconds as u32))
                .size(11)
                .color(style::TEXT_SECONDARY)
                .width(TIME_WIDTH)
        };
        let seek: Element<'a, Message> = if length > 0.0 {
            slider(0.0..=length, at, Message::Scrub)
                .on_release(Message::Scrubbed)
                .step(0.25_f32)
                .height(17)
                .style(scrubber)
                .into()
        } else {
            Space::new().width(Length::Fill).height(17).into()
        };
        let seek = row![time(at).align_x(Alignment::End), seek, time(length)]
            .spacing(8)
            .align_y(Alignment::Center);
        column![play, seek]
            .spacing(4)
            .align_x(Alignment::Center)
            .max_width(SEEK_WIDTH)
            .into()
    }
}

/// The cover, the title and the artists of what's playing.
fn now_playing<'a>(playback: &'a Playback, images: &'a Images) -> Element<'a, Message> {
    // The Shell shows the bar only with a current track.
    let Some(track) = playback.current() else {
        return space().into();
    };
    let cover = track.album.as_ref().and_then(|album| album.cover.as_ref());
    let details = column![
        text(&track.title)
            .size(13)
            .font(SEMIBOLD)
            .wrapping(text::Wrapping::None),
        page::artists(&track.artists, 11.0),
    ]
    .spacing(2);
    let line = row![
        page::rounded_cover(images, cover, COVER_SIZE, 6.0),
        container(details).clip(true)
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    Element::from(line).map(Message::Link)
}

/// Mute, and the volume. Its icon says how loud: crossed out at 0, one
/// wave below half, two above.
fn volume(playback: &Playback) -> Element<'_, Message> {
    let level = playback.volume();
    let glyph = if level == 0.0 {
        Icon::VolumeX
    } else if level < 0.5 {
        Icon::Volume1
    } else {
        Icon::Volume2
    };
    let mute = button(container(icon(glyph, 16.0, style::TEXT_SECONDARY)).center(30))
        .padding(0)
        .style(style::icon_button)
        .on_press(Message::ToggleMute);
    let slider = slider(0.0..=1.0, level, Message::Volume)
        .on_release(Message::VolumeSet)
        .step(0.01_f32)
        .width(VOLUME_WIDTH)
        .style(scrubber);
    row![space::horizontal(), mute, slider]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

/// The player bar's play and pause: a white disc.
fn play_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => style::TEXT_SECONDARY,
        _ => style::TEXT_PRIMARY,
    };
    button::Style {
        background: Some(background.into()),
        text_color: style::BG_BASE,
        border: style::rounded(999.0),
        ..button::Style::default()
    }
}

/// The seek bar and the volume: a thin rail that thickens, turns the accent
/// and shows its handle under the pointer.
fn scrubber(_theme: &Theme, status: slider::Status) -> slider::Style {
    let hot = !matches!(status, slider::Status::Active);
    slider::Style {
        rail: Rail {
            backgrounds: (
                Background::Color(if hot {
                    style::ACCENT
                } else {
                    style::SLIDER_FILL
                }),
                Background::Color(style::SLIDER_TRACK),
            ),
            width: if hot { 5.0 } else { 3.0 },
            border: style::rounded(999.0),
        },
        handle: Handle {
            shape: HandleShape::Circle {
                radius: if hot { 6.0 } else { 0.0 },
            },
            background: Background::Color(style::TEXT_PRIMARY),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// The player bar: elevated, with a hairline along its top.
fn player_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_ELEVATED.into()),
        border: Border {
            color: style::BORDER_SUBTLE,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}
