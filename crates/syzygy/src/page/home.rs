//! The Home Page: TIDAL's home feed. For now it lists the section titles.

use iced::Element;
use iced::widget::{column, text};
use syzygy_catalog::Read;
use syzygy_tidal::models::HomePageResponse;

use super::{Action, Load, Remote};

/// The feed tab Home opens on.
const DEFAULT_FEED: &str = "static";

pub struct State {
    feed: Remote<HomePageResponse>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Feed(Read<HomePageResponse>),
    Retry,
}

impl State {
    pub fn new() -> (Self, Action) {
        let state = Self {
            feed: Remote::Loading,
        };
        (state, load_feed())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Feed(read) => {
                self.feed.apply(read, "the home feed");
                Action::None
            }
            Message::Retry => {
                self.feed = Remote::Loading;
                load_feed()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        self.feed.view(Message::Retry, |home| {
            let titles = home
                .sections
                .iter()
                .filter(|section| !section.title.is_empty())
                .map(|section| text(&section.title).size(18).into());
            column![text("Home").size(32), column(titles).spacing(16)]
                .spacing(24)
                .padding(24)
                .into()
        })
    }
}

fn load_feed() -> Action {
    Action::Load(Load::HomeFeed(DEFAULT_FEED.to_string()))
}
