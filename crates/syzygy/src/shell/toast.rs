//! Toasts: short info and error messages in the corner that go away on
//! their own.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Border, Element, Length, Task, Theme};
use std::time::Duration;

/// How long a toast stays up.
const DURATION: Duration = Duration::from_secs(3);
/// The most toasts on screen; a new one pushes the oldest out.
const MAX_SHOWN: usize = 4;

#[expect(dead_code, reason = "Library edits and playback show the first toasts")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastId(u64);

struct Toast {
    id: ToastId,
    kind: Kind,
    text: String,
}

#[derive(Default)]
pub struct Toasts {
    next_id: u64,
    shown: Vec<Toast>,
}

impl Toasts {
    /// Show a toast. The task finishes with its id once it's time to
    /// dismiss it.
    pub fn push(&mut self, kind: Kind, text: String) -> Task<ToastId> {
        let id = ToastId(self.next_id);
        self.next_id += 1;
        self.shown.push(Toast { id, kind, text });
        if self.shown.len() > MAX_SHOWN {
            self.shown.remove(0);
        }
        Task::perform(tokio::time::sleep(DURATION), move |()| id)
    }

    pub fn dismiss(&mut self, id: ToastId) {
        self.shown.retain(|toast| toast.id != id);
    }

    /// The stack, bottom right. Each toast's close button dismisses it.
    pub fn view(&self) -> Element<'_, ToastId> {
        let toasts = self.shown.iter().map(|toast| {
            let close = button(text("×")).on_press(toast.id).style(button::text);
            let kind = toast.kind;
            container(
                row![text(&toast.text).size(14).width(Length::Fill), close]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(380)
            .style(move |theme: &Theme| style(theme, kind))
            .into()
        });
        container(column(toasts).spacing(8))
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .padding(16)
            .into()
    }
}

fn style(theme: &Theme, kind: Kind) -> container::Style {
    let palette = theme.extended_palette();
    let accent = match kind {
        Kind::Info => palette.background.strong.color,
        Kind::Error => palette.danger.base.color,
    };
    container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(palette.background.weak.text),
        border: Border {
            color: accent,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}
