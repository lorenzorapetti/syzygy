//! A user's Profile Page, the signed-in user's or anyone's, read-only: who
//! they are, and a row of their public playlists.

use iced::Element;
use iced::widget::{Column, container, text};
use syzygy_catalog::{Profile, Read};

use super::cards::{self, Rows};
use super::{Action, Link, Load, PADDING, Remote, Route, hero, no_picture, rounded_cover};
use crate::images::Images;
use crate::library::Library;
use crate::style;

/// How wide the bio runs.
const BIO_WIDTH: f32 = 820.0;

pub struct State {
    user_id: u64,
    profile: Remote<Profile>,
    /// The playlists row's scroll state.
    rows: Rows,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<Profile>),
    Cards(cards::Message),
    Link(Link),
    Retry,
}

impl State {
    pub fn new(user_id: u64) -> (Self, Action) {
        let state = Self {
            user_id,
            profile: Remote::Loading,
            rows: Rows::default(),
        };
        (state, Action::Load(load(user_id)))
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.profile.apply(read, "a profile");
                Action::None
            }
            Message::Cards(message) => {
                let playlists = self.profile.loaded().map(|p| p.playlists.len());
                self.rows.update(
                    message,
                    |_| playlists,
                    |message| super::Message::Profile(Message::Cards(message)),
                )
            }
            Message::Link(link) => link.follow(),
            Message::Retry => {
                self.profile = Remote::Loading;
                Action::Load(load(self.user_id))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images, library: &'a Library) -> Element<'a, Message> {
        self.profile.view(Message::Retry, |profile| {
            let art = match &profile.picture {
                Some(picture) => {
                    rounded_cover(images, Some(picture), hero::HEIGHT, hero::HEIGHT / 2.0)
                }
                None => no_picture(hero::HEIGHT),
            };
            let facts = profile.facts();
            let lines = (!facts.is_empty())
                .then(|| text(facts).size(14).color(style::TEXT_SECONDARY).into())
                .into_iter()
                .collect();
            let hero = hero::with_art(art, "PROFILE", profile.called(), lines).map(Message::Link);
            let bio = profile.bio.as_deref().map(|bio| {
                container(text(bio).size(14).color(style::TEXT_SECONDARY)).max_width(BIO_WIDTH)
            });
            let playlists = (!profile.playlists.is_empty()).then(|| {
                let all = Route::ProfilePlaylists {
                    user_id: self.user_id,
                };
                self.rows
                    .view(
                        0,
                        "Public playlists",
                        &profile.playlists,
                        Some(all),
                        images,
                        library,
                    )
                    .map(Message::Cards)
            });
            Column::new()
                .push(hero)
                .push(bio)
                .push(playlists)
                .spacing(32)
                .padding(PADDING)
                .into()
        })
    }
}

fn load(user_id: u64) -> Load {
    Load::Profile {
        user_id,
        then: |read| super::Message::Profile(Message::Loaded(read)),
    }
}
