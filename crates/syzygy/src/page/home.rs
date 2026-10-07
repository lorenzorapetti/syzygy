//! The Home Page: TIDAL's home feed, one tab at a time, with more sections
//! loaded as the user scrolls.

use iced::widget::{Text, button, column, container, row, scrollable, sensor, space, text};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_catalog::home_feed::{Card, Layout, Section, Tab, Target};
use syzygy_catalog::{HomeFeed, Read};

use super::{Action, Load, Preview, Remote, Route, cover_placeholder};

/// The feed tab Home opens on.
pub const DEFAULT_TAB: &str = "static";

/// Coming back to the window refreshes Home at most this often.
const FOCUS_REFRESH: Duration = Duration::from_secs(5 * 60);

/// Start loading more this far before the end comes into view.
const LOAD_MORE_AHEAD: f32 = 200.0;

const CARD_WIDTH: f32 = 160.0;
const SHORTCUT_HEIGHT: f32 = 56.0;
const SHORTCUTS_PER_ROW: usize = 4;

pub struct State {
    tab: String,
    /// The tabs as last read, kept while a tab loads or fails so the user
    /// can always switch.
    tabs: Vec<Tab>,
    feed: Remote<HomeFeed>,
    more: More,
    /// Sections were added after the first page. A new first page would
    /// drop them, so it's no longer taken.
    paginated: bool,
    /// When the feed was last read or refreshed.
    loaded_at: Instant,
}

/// Loading the sections after the first page.
enum More {
    Idle,
    Loading,
    /// This cursor failed and isn't asked for again.
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A read of the first page of a tab.
    Feed {
        tab: String,
        read: Read<HomeFeed>,
    },
    /// The sections after `cursor`.
    More {
        cursor: String,
        result: Result<HomeFeed, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the sections is nearly in view.
    EndInView,
    SelectTab(String),
    Open(Route),
    Retry,
}

impl State {
    pub fn new(tab: String) -> (Self, Action) {
        let state = Self {
            tab,
            tabs: Vec::new(),
            feed: Remote::Loading,
            more: More::Idle,
            paginated: false,
            loaded_at: Instant::now(),
        };
        let action = state.load();
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Feed { tab, read } => {
                if tab != self.tab {
                    // A read for the tab the user just left.
                } else if self.paginated {
                    log::debug!("Keeping the paged home feed over a new first page");
                } else {
                    self.feed.apply(read, "the home feed");
                    if let Remote::Loaded(feed) = &self.feed
                        && !feed.tabs.is_empty()
                    {
                        self.tabs = feed.tabs.clone();
                    }
                }
                Action::None
            }
            Message::More { cursor, result } => {
                // Only sections that follow on from the page on screen; a new
                // first page may have landed since they were asked for.
                let Remote::Loaded(feed) = &mut self.feed else {
                    return Action::None;
                };
                if feed.cursor.as_ref() != Some(&cursor) {
                    return Action::None;
                }
                match result {
                    Ok(more) => {
                        self.more = More::Idle;
                        feed.sections.extend(more.sections);
                        feed.cursor = more.cursor;
                        self.paginated = true;
                    }
                    Err(e) => {
                        log::warn!("Could not load more of the home feed: {e}");
                        self.more = More::Failed(cursor);
                    }
                }
                Action::None
            }
            Message::EndInView => self.load_more(),
            Message::SelectTab(tab) if tab != self.tab => {
                // The tab is part of the route: this entry changes, and no
                // step is added to the Back stack.
                self.tab = tab;
                self.feed = Remote::Loading;
                self.more = More::Idle;
                self.paginated = false;
                self.loaded_at = Instant::now();
                Action::Replace(
                    Route::Home {
                        tab: self.tab.clone(),
                    },
                    Load::HomeFeed(self.tab.clone()),
                )
            }
            Message::SelectTab(_) => Action::None,
            Message::Open(route) => Action::Navigate(route),
            Message::Retry => {
                self.feed = Remote::Loading;
                self.load()
            }
        }
    }

    /// Refresh from TIDAL if it's been a while, unless more sections were
    /// added or are on their way, which a refresh would drop.
    pub fn focused(&mut self) -> Action {
        let loaded = matches!(self.feed, Remote::Loaded(_));
        let paging = self.paginated || matches!(self.more, More::Loading);
        if !loaded || paging || self.loaded_at.elapsed() < FOCUS_REFRESH {
            return Action::None;
        }
        self.loaded_at = Instant::now();
        Action::Load(Load::RefreshHomeFeed(self.tab.clone()))
    }

    fn load(&self) -> Action {
        Action::Load(Load::HomeFeed(self.tab.clone()))
    }

    fn load_more(&mut self) -> Action {
        let Remote::Loaded(HomeFeed {
            cursor: Some(cursor),
            ..
        }) = &self.feed
        else {
            return Action::None;
        };
        match &self.more {
            More::Loading => Action::None,
            More::Failed(failed) if failed == cursor => Action::None,
            More::Idle | More::Failed(_) => {
                self.more = More::Loading;
                Action::Load(Load::MoreHomeFeed {
                    tab: self.tab.clone(),
                    cursor: cursor.clone(),
                })
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body = self.feed.view(Message::Retry, |feed| {
            if feed.sections.is_empty() {
                return text("Nothing to show here yet")
                    .style(text::secondary)
                    .into();
            }
            let mut page = column(feed.sections.iter().map(section)).spacing(32);
            if matches!(self.more, More::Loading) {
                page = page.push(text("Loading…").style(text::secondary));
            }
            // Keyed by the cursor, so it fires again for each new page when
            // the end is still in view.
            if let Some(cursor) = &feed.cursor {
                page = page.push(
                    sensor(space().height(1))
                        .key(cursor.clone())
                        .anticipate(LOAD_MORE_AHEAD)
                        .on_show(|_| Message::EndInView),
                );
            }
            page.into()
        });
        column![tab_bar(&self.tabs, &self.tab), body]
            .spacing(32)
            .padding(24)
            .into()
    }
}

fn tab_bar<'a>(tabs: &'a [Tab], selected: &str) -> Element<'a, Message> {
    let pills = tabs.iter().map(|tab| {
        let style = if tab.slug == selected {
            selected_tab
        } else {
            unselected_tab
        };
        button(text(&tab.name).size(14))
            .padding([8, 16])
            .style(style)
            .on_press(Message::SelectTab(tab.slug.clone()))
            .into()
    });
    row(pills).spacing(8).into()
}

fn section(section: &Section) -> Element<'_, Message> {
    match section.layout {
        Layout::Shortcuts => shortcuts(&section.cards),
        Layout::Row => {
            let cards = row(section.cards.iter().map(card)).spacing(16);
            column![
                text(&section.title).size(22),
                scrollable(cards).direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new()
                        .width(4)
                        .scroller_width(4)
                        .spacing(8),
                )),
            ]
            .spacing(16)
            .into()
        }
    }
}

/// The quick-access grid.
fn shortcuts(cards: &[Card]) -> Element<'_, Message> {
    let rows = cards.chunks(SHORTCUTS_PER_ROW).map(|chunk| {
        let mut tiles: Vec<Element<'_, Message>> = chunk.iter().map(shortcut).collect();
        // Keep the last row's tiles the width of the others.
        tiles.extend((chunk.len()..SHORTCUTS_PER_ROW).map(|_| space::horizontal().into()));
        row(tiles).spacing(12).into()
    });
    column(rows).spacing(12).into()
}

fn shortcut(card: &Card) -> Element<'_, Message> {
    let title = text(&card.title).size(13).wrapping(text::Wrapping::None);
    button(
        row![
            cover_placeholder(SHORTCUT_HEIGHT),
            container(title).clip(true).padding([0, 12]),
        ]
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .style(shortcut_tile)
    .on_press_maybe(route(card).map(Message::Open))
    .into()
}

fn card<'a>(card: &'a Card) -> Element<'a, Message> {
    let line = |line: Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(CARD_WIDTH)
            .clip(true)
    };
    button(
        column![
            cover_placeholder(CARD_WIDTH),
            line(text(&card.title).size(14)),
            line(text(&card.subtitle).size(12).style(text::secondary)),
        ]
        .spacing(6),
    )
    .padding(0)
    .style(card_button)
    .on_press_maybe(route(card).map(Message::Open))
    .into()
}

/// Where a card leads, with what its Page can draw straight away. Tracks
/// and videos play once there's playback.
fn route(card: &Card) -> Option<Route> {
    // Only an album card's subtitle is its artist.
    let preview = |artist: bool| {
        Some(Preview {
            title: card.title.clone(),
            cover: card.cover.clone(),
            artist: (artist && !card.subtitle.is_empty()).then(|| card.subtitle.clone()),
        })
    };
    match &card.target {
        Target::Album(id) => Some(Route::Album {
            id: *id,
            preview: preview(true),
        }),
        Target::Artist(id) => Some(Route::Artist {
            id: *id,
            preview: preview(false),
        }),
        Target::Playlist(uuid) => Some(Route::Playlist {
            uuid: uuid.clone(),
            preview: preview(false),
        }),
        Target::Mix(id) => Some(Route::Mix {
            id: id.clone(),
            preview: preview(false),
        }),
        Target::Favorites => Some(Route::Favorites),
        Target::Track(_) | Target::Video(_) | Target::None => None,
    }
}

fn selected_tab(theme: &Theme, _status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    pill(palette.background.base.text, palette.background.base.color)
}

fn unselected_tab(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.strong.color,
        _ => palette.background.weak.color,
    };
    pill(background, palette.background.base.text)
}

fn pill(background: Color, text_color: Color) -> button::Style {
    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            radius: 18.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn shortcut_tile(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.strong.color,
        _ => palette.background.weakest.color,
    };
    button::Style {
        background: Some(background.into()),
        text_color: palette.background.base.text,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn card_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => palette.primary.base.color,
        _ => palette.background.base.text,
    };
    button::Style {
        background: None,
        text_color,
        ..button::Style::default()
    }
}
