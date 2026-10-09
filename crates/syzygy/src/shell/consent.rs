//! The explicit-consent modal: what the user chose would play explicit
//! tracks, which the settings don't allow.

use iced::widget::{button, center, column, container, mouse_area, opaque, row, text};
use iced::{Color, Element, Theme};

use crate::style;

/// What the user answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Allow explicit content, and play what was chosen.
    Allow,
    /// Play what was chosen, skipping the explicit tracks.
    Without,
    /// Escape or a click outside: play nothing.
    Dismiss,
}

/// The modal, on a backdrop that darkens what's under it and takes its
/// clicks. A click outside the modal dismisses it.
pub fn view<'a>() -> Element<'a, Answer> {
    let card = container(
        column![
            text("Explicit content").size(20),
            text(
                "This includes tracks marked explicit, which your settings don't \
                 allow. Allowing them changes the setting.",
            )
            .size(14)
            .color(style::TEXT_SECONDARY),
            row![
                button(text("Play without them").size(14))
                    .padding([8, 16])
                    .style(style::pill_button)
                    .on_press(Answer::Without),
                button(text("Allow explicit content").size(14))
                    .padding([8, 16])
                    .style(style::accent_pill)
                    .on_press(Answer::Allow),
            ]
            .spacing(8),
        ]
        .spacing(16),
    )
    .padding(24)
    .max_width(440)
    .style(card);
    let backdrop = center(opaque(card)).style(|_theme| container::Style {
        background: Some(
            Color {
                a: 0.8,
                ..Color::BLACK
            }
            .into(),
        ),
        ..container::Style::default()
    });
    opaque(mouse_area(backdrop).on_press(Answer::Dismiss))
}

fn card(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_ELEVATED.into()),
        text_color: Some(style::TEXT_PRIMARY),
        border: iced::Border {
            color: style::BORDER_SUBTLE,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}
