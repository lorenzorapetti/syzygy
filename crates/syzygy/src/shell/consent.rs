//! The explicit-consent modal: what the user chose would play explicit
//! tracks, which the settings don't allow.

use iced::Element;
use iced::widget::{button, center, column, container, mouse_area, opaque, row, text};

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
    .style(style::modal);
    let backdrop = center(opaque(card)).style(style::backdrop);
    opaque(mouse_area(backdrop).on_press(Answer::Dismiss))
}
