//! The Home Page: TIDAL's home feed, one tab at a time, with more sections
//! loaded as the user scrolls.

use iced::widget::{
    self as widget, Text, button, column, container, operation, row, scrollable, sensor, space,
    text,
};
use iced::{Alignment, Color, Element, Length, Theme};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_catalog::home_feed::{Card, Layout, Section, Tab, Target};
use syzygy_catalog::{HomeFeed, Read};

use super::{Action, Load, Preview, Remote, Route, cover};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::style;

/// The feed tab Home opens on.
pub const DEFAULT_TAB: &str = "static";

/// Coming back to the window refreshes Home at most this often.
const FOCUS_REFRESH: Duration = Duration::from_secs(5 * 60);

/// Start loading more this far before the end comes into view.
const LOAD_MORE_AHEAD: f32 = 200.0;

const CARD_WIDTH: f32 = 160.0;
const CARD_GAP: f32 = 16.0;
/// A row this close to an end counts as at that end.
const SCROLL_SLACK: f32 = 10.0;
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
    rows: HashMap<usize, RowScroll>,
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
    /// A card row scrolled to this offset.
    RowScrolled(usize, f32),
    /// A card row is this wide on screen.
    RowSized(usize, f32),
    /// An arrow: a page of cards left (-1) or right (1).
    ScrollRow(usize, f32),
    SelectTab(String),
    /// A card's cover came into view.
    CoverWanted(String),
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
            rows: HashMap::new(),
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
            Message::RowScrolled(index, offset) => {
                self.rows.entry(index).or_default().offset = offset;
                Action::None
            }
            Message::RowSized(index, width) => {
                self.rows.entry(index).or_default().width = width;
                Action::None
            }
            Message::ScrollRow(index, step) => {
                let Remote::Loaded(feed) = &self.feed else {
                    return Action::None;
                };
                let Some(section) = feed.sections.get(index) else {
                    return Action::None;
                };
                // A scroll operation isn't reported through `on_scroll`, so
                // the new offset is worked out and kept here.
                let row = self.rows.entry(index).or_default();
                let end = (row_width(section.cards.len()) - row.width).max(0.0);
                row.offset = (row.offset + step * row_page(row.width)).clamp(0.0, end);
                let offset = scrollable::AbsoluteOffset {
                    x: row.offset,
                    y: 0.0,
                };
                Action::Run(operation::scroll_to(row_id(index), offset))
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
                    Load::HomeFeed(self.tab.clone()),
                )
            }
            Message::SelectTab(_) => Action::None,
            Message::Open(route) => Action::Navigate(route),
            Message::CoverWanted(url) => Action::FetchImages(vec![url]),
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
                .map(|(index, s)| section(index, s, self.rows.get(&index), images));
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

fn section<'a>(
    index: usize,
    section: &'a Section,
    scroll: Option<&RowScroll>,
    images: &'a Images,
) -> Element<'a, Message> {
    match section.layout {
        Layout::Shortcuts => shortcuts(&section.cards, images),
        Layout::Row => {
            let scroll = scroll.copied().unwrap_or_default();
            let content = row_width(section.cards.len());
            // Until the row has been measured, assume there's more to the right.
            let can_left = scroll.offset > SCROLL_SLACK;
            let can_right =
                scroll.width == 0.0 || scroll.offset + scroll.width < content - SCROLL_SLACK;
            let arrow = |glyph, step, enabled: bool| {
                let color = if enabled {
                    style::TEXT_PRIMARY
                } else {
                    style::TEXT_DISABLED
                };
                button(container(icon(glyph, 18.0, color)).center(32))
                    .padding(0)
                    .style(row_arrow)
                    .on_press_maybe(enabled.then_some(Message::ScrollRow(index, step)))
            };
            let header = row![
                text(&section.title).size(22),
                space::horizontal(),
                arrow(Icon::ChevronLeft, -1.0, can_left),
                arrow(Icon::ChevronRight, 1.0, can_right),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            let cards = section.cards.iter().map(|c| card(c, images));
            let cards = scrollable(row(cards).spacing(CARD_GAP))
                .id(row_id(index))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::hidden(),
                ))
                .on_scroll(move |viewport| {
                    Message::RowScrolled(index, viewport.absolute_offset().x)
                });
            let cards = sensor(cards)
                .on_show(move |size| Message::RowSized(index, size.width))
                .on_resize(move |size| Message::RowSized(index, size.width));
            column![header, cards].spacing(16).into()
        }
    }
}

/// How far a card row is scrolled, and how wide it is on screen.
#[derive(Debug, Clone, Copy, Default)]
struct RowScroll {
    offset: f32,
    width: f32,
}

/// Each card row's scrollable, by its section's place in the feed.
fn row_id(index: usize) -> widget::Id {
    widget::Id::from(format!("home-row-{index}"))
}

/// The width of a row of `cards` cards.
fn row_width(cards: usize) -> f32 {
    (cards as f32 * (CARD_WIDTH + CARD_GAP) - CARD_GAP).max(0.0)
}

/// How far an arrow scrolls a row: as many whole cards as fit, so the row
/// lands on a card's edge.
fn row_page(width: f32) -> f32 {
    let stride = CARD_WIDTH + CARD_GAP;
    ((width + CARD_GAP) / stride).floor().max(1.0) * stride
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
    button(
        row![
            cover(
                images,
                card.cover.as_ref(),
                SHORTCUT_HEIGHT,
                Message::CoverWanted
            ),
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

fn card<'a>(card: &'a Card, images: &'a Images) -> Element<'a, Message> {
    let line = |line: Text<'a>| {
        container(line.wrapping(text::Wrapping::None))
            .width(CARD_WIDTH)
            .clip(true)
    };
    button(
        column![
            cover(
                images,
                card.cover.as_ref(),
                CARD_WIDTH,
                Message::CoverWanted
            ),
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

fn selected_tab(_theme: &Theme, _status: button::Status) -> button::Style {
    pill(style::TEXT_PRIMARY, style::BG_BASE)
}

fn unselected_tab(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => style::BG_BUTTON_HOVER,
        _ => style::BG_BUTTON,
    };
    pill(background, style::TEXT_PRIMARY)
}

fn pill(background: Color, text_color: Color) -> button::Style {
    button::Style {
        background: Some(background.into()),
        text_color,
        border: style::rounded(18.0),
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

fn card_button(_theme: &Theme, status: button::Status) -> button::Style {
    let text_color = match status {
        button::Status::Hovered | button::Status::Pressed => style::ACCENT,
        _ => style::TEXT_PRIMARY,
    };
    button::Style {
        background: None,
        text_color,
        ..button::Style::default()
    }
}

fn row_arrow(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Disabled => None,
        button::Status::Hovered | button::Status::Pressed => Some(style::BG_BUTTON_HOVER.into()),
        button::Status::Active => Some(style::BG_BUTTON.into()),
    };
    button::Style {
        background,
        border: style::rounded(16.0),
        ..button::Style::default()
    }
}
