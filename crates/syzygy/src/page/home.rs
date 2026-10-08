//! The Home Page: TIDAL's home feed, one tab at a time, with more sections
//! loaded as the user scrolls.

use iced::widget::{button, column, container, hover, row, sensor, space, text};
use iced::{Alignment, Color, Element, Length, Theme};
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_catalog::home_feed::{Card, Layout, Section, Tab};
use syzygy_catalog::{HomeFeed, Read};

use super::cards::{self, Rows};
use super::{Action, Link, Load, PADDING, Remote, Route, cover};
use crate::icons::{Icon, filled};
use crate::images::Images;
use crate::style;

/// The feed tab Home opens on.
pub const DEFAULT_TAB: &str = "static";

/// Coming back to the window refreshes Home at most this often.
const FOCUS_REFRESH: Duration = Duration::from_secs(5 * 60);

/// Start loading more this far before the end comes into view.
const LOAD_MORE_AHEAD: f32 = 200.0;

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
    /// The card rows' scroll state, by section index.
    rows: Rows,
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
    Cards(cards::Message),
    SelectTab(String),
    Link(Link),
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
            rows: Rows::default(),
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
            Message::Cards(message) => {
                let feed = self.feed.loaded();
                let cards = |index: usize| {
                    feed.and_then(|feed| feed.sections.get(index))
                        .map(|section| section.cards.len())
                };
                self.rows.update(message, cards, |message| {
                    super::Message::Home(Message::Cards(message))
                })
            }
            Message::SelectTab(tab) if tab != self.tab => {
                // The tab is part of the route: this entry changes, and no
                // step is added to the Back stack.
                self.tab = tab;
                self.feed = Remote::Loading;
                self.more = More::Idle;
                self.paginated = false;
                self.rows.clear();
                self.loaded_at = Instant::now();
                Action::Replace(
                    Route::Home {
                        tab: self.tab.clone(),
                    },
                    Some(Load::HomeFeed(self.tab.clone())),
                )
            }
            Message::SelectTab(_) => Action::None,
            Message::Link(link) => link.follow(),
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

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let body = self.feed.view(Message::Retry, |feed| {
            if feed.sections.is_empty() {
                return text("Nothing to show here yet")
                    .style(text::secondary)
                    .into();
            }
            let sections = feed
                .sections
                .iter()
                .enumerate()
                .map(|(index, s)| self.section(index, s, images));
            let mut page = column(sections).spacing(32);
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
            .padding(PADDING)
            .into()
    }

    fn section<'a>(
        &'a self,
        index: usize,
        section: &'a Section,
        images: &'a Images,
    ) -> Element<'a, Message> {
        match section.layout {
            Layout::Shortcuts => shortcuts(&section.cards, images),
            Layout::Row => self
                .rows
                .view(index, &section.title, &section.cards, None, images)
                .map(Message::Cards),
        }
    }
}

fn tab_bar<'a>(tabs: &'a [Tab], selected: &str) -> Element<'a, Message> {
    let pills = tabs.iter().map(|tab| {
        let style = if tab.slug == selected {
            style::selected_tab
        } else {
            style::unselected_tab
        };
        button(text(&tab.name).size(14))
            .padding([8, 16])
            .style(style)
            .on_press(Message::SelectTab(tab.slug.clone()))
            .into()
    });
    row(pills).spacing(8).into()
}

/// The quick-access grid.
fn shortcuts<'a>(cards: &'a [Card], images: &'a Images) -> Element<'a, Message> {
    let rows = cards.chunks(SHORTCUTS_PER_ROW).map(|chunk| {
        let mut tiles: Vec<Element<'_, Message>> =
            chunk.iter().map(|card| shortcut(card, images)).collect();
        // Keep the last row's tiles the width of the others.
        tiles.extend((chunk.len()..SHORTCUTS_PER_ROW).map(|_| space::horizontal().into()));
        row(tiles).spacing(12).into()
    });
    column(rows).spacing(12).into()
}

fn shortcut<'a>(card: &'a Card, images: &'a Images) -> Element<'a, Message> {
    let title = text(&card.title).size(13).wrapping(text::Wrapping::None);
    let art = cover(images, card.cover.as_ref(), SHORTCUT_HEIGHT);
    // As in sone: under the pointer the cover dims behind a small play.
    let art = if cards::plays(card) {
        let play =
            button(container(filled(Icon::Play, 14.0, Color::WHITE)).center(SHORTCUT_HEIGHT))
                .padding(0)
                .style(cover_play)
                .on_press(Link::PlayCard(card.clone()));
        hover(art, play)
    } else {
        art
    };
    button(
        row![
            art.map(Message::Link),
            container(title).clip(true).padding([0, 12]),
        ]
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fill)
    .style(shortcut_tile)
    .on_press_maybe(cards::open(card).map(Message::Link))
    .into()
}

/// A shortcut's play, over its cover: a 40% black veil.
fn cover_play(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.4).into()),
        border: style::rounded(4.0),
        ..button::Style::default()
    }
}

fn shortcut_tile(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => style::BG_SURFACE_HOVER,
        _ => style::BG_SURFACE,
    };
    button::Style {
        background: Some(background.into()),
        text_color: style::TEXT_PRIMARY,
        border: style::rounded(4.0),
        ..button::Style::default()
    }
}
