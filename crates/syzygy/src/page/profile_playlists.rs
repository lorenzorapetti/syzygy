//! A user's public playlists in full, as a grid. They come from the
//! profile's read, as on the Profile Page.

use iced::widget::{Column, container, text};
use iced::{Element, Length};
use syzygy_catalog::{Profile, Read};

use super::{Action, Link, Load, PADDING, Remote, cards};
use crate::images::Images;
use crate::style;

pub struct State {
    user_id: u64,
    profile: Remote<Profile>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Profile>),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(user_id: u64) -> (Self, Action) {
        let state = Self {
            user_id,
            profile: Remote::Loading,
        };
        (state, Action::Load(load(user_id)))
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.profile.apply(read, "a profile");
                Action::None
            }
            Message::Link(link) => link.follow(),
            Message::Retry => {
                self.profile = Remote::Loading;
                Action::Load(load(self.user_id))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let header = |line: String| {
            Column::new()
                .push(text("Public playlists").size(32))
                .push(text(line).size(14).color(style::TEXT_MUTED))
                .spacing(4)
        };
        let body = self.profile.view(Message::Retry, |profile| {
            let count = profile.playlists.len();
            let line = format!(
                "{} · {count} playlist{}",
                profile.called(),
                if count == 1 { "" } else { "s" }
            );
            let grid: Element<'a, Message> = if count == 0 {
                text("No public playlists").style(text::secondary).into()
            } else {
                cards::grid(&profile.playlists, images).map(Message::Link)
            };
            Column::new()
                .push(header(line))
                .push(grid)
                .spacing(32)
                .width(Length::Fill)
                .into()
        });
        container(body).padding(PADDING).into()
    }
}

fn load(user_id: u64) -> Load {
    Load::Profile {
        user_id,
        then: |read| super::Message::ProfilePlaylists(Message::Loaded(read)),
    }
}
