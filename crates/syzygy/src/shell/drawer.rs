//! The now-playing drawer. It slides up over everything above the player
//! bar, on an 80% black backdrop: the current track's cover on the left
//! (45%), and tabs on the right (55%): the Queue, the Suggested tracks, the
//! Lyrics and the Credits.
//!
//! Suggested, Lyrics and Credits are about the current track only. Each is
//! read while its tab shows, and again once the track changes.
//!
//! Synced lyrics keep the line sung in view: the tab measures where that
//! line is and scrolls there. A scroll that isn't one of its own is the
//! user's, and stops it until "Sync lyrics".

use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{
    Column, button, column, container, mouse_area, responsive, row, rule, scrollable, space, stack,
    text,
};
use iced::{
    Alignment, Animation, Color, Element, Font, Length, Rectangle, Task, Theme, Vector, animation,
    font, mouse,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_catalog::track::Credit;
use syzygy_catalog::{Lyrics, Read, Track};

use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::library::Library;
use crate::page::track_list::{self, Columns, Mark, ROW_HEIGHT};
use crate::page::{self, Link, Preview, Remote, Route, Viewport};
use crate::playback::{self, Entry, EntryId, PlayRequest, Playback, Radio, Slot, Start};
use crate::style;

/// How long the drawer takes to slide up or down.
const SLIDE: Duration = Duration::from_millis(250);
/// How dark the backdrop gets.
const BACKDROP: f32 = 0.8;
/// The cover's side of the drawer, and the tabs', out of 20: 45% and 55%.
const COVER_SHARE: u16 = 9;
const TABS_SHARE: u16 = 11;
/// Around the cover.
const COVER_PADDING: f32 = 40.0;
/// The largest cover the drawer draws.
const MAX_COVER: f32 = 640.0;
/// Between the cover and the title under it.
const COVER_GAP: f32 = 24.0;
/// The title and artists under the cover.
const CAPTION_HEIGHT: f32 = 56.0;
/// How much History the Queue tab shows.
const HISTORY_ROWS: usize = 10;
/// The grip a queued row is dragged by.
const GRIP_WIDTH: f32 = 24.0;
const TAB_PADDING: f32 = 24.0;
/// Above a tab's content.
const TAB_TOP: f32 = 16.0;
/// How tall the tab's viewport is taken to be until it's reported.
const UNKNOWN_HEIGHT: f32 = 4000.0;
/// The scrollable every tab draws in.
pub const SCROLL: Id = Id::new("drawer");
/// The synced line sung now, for [`measure`] to find.
const SUNG: Id = Id::new("drawer-sung");
/// How far the tab may sit from where it scrolled itself and still be
/// taken as there.
const LANDED: f32 = 1.0;
/// The Queue's and Suggested tracks' rows: a cover by the title.
const COLUMNS: Columns = Columns {
    cover: true,
    album: false,
    date_added: false,
};
const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};

pub struct Drawer {
    open: bool,
    slide: Animation<bool>,
    tab: Tab,
    /// The current track, which the tabs are about.
    track: Option<Track>,
    suggested: Option<TabRead<Option<Radio>>>,
    lyrics: Option<TabRead<Lyrics>>,
    credits: Option<TabRead<Vec<Credit>>>,
    /// Playback's position in the current track, in seconds.
    position: f32,
    follow: Follow,
    drag: Option<Drag>,
    /// The part of the tab in view, for the Queue's long lists.
    viewport: Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Queue,
    Suggested,
    Lyrics,
    Credits,
}

/// The Lyrics tab keeping the line sung in view.
struct Follow {
    /// Off once the user scrolls, until "Sync lyrics".
    on: bool,
    /// The line last brought into view, `None` meaning the top. Unset
    /// until one is, after the tab shows, the lyrics arrive or it syncs
    /// again.
    line: Option<Option<usize>>,
    /// Where it last scrolled the tab to.
    placed: Option<Placed>,
    /// How far down the tab could scroll when it last said.
    end: Option<f32>,
}

impl Default for Follow {
    fn default() -> Self {
        Self {
            on: true,
            line: None,
            placed: None,
            end: None,
        }
    }
}

/// A scroll the Lyrics tab made itself.
struct Placed {
    offset: f32,
    /// Whether the tab has said it's there. Until then a scroll elsewhere
    /// is one from before, not the user's.
    landed: bool,
}

/// Where the line sung is, from [`measure`].
#[derive(Debug, Clone, Copy)]
pub struct Measure {
    /// The tab's scrollable.
    viewport: Rectangle,
    /// What it scrolls.
    content: Rectangle,
    line: Rectangle,
}

/// A tab's read for one track. Dropping it aborts the read.
struct TabRead<T> {
    track_id: u64,
    remote: Remote<T>,
    load: Option<iced::task::Handle>,
}

impl<T> TabRead<T> {
    fn loading(track_id: u64) -> Self {
        Self {
            track_id,
            remote: Remote::Loading,
            load: None,
        }
    }

    /// Take what the read for `track_id` brought, unless it's for another
    /// track.
    fn arrived(
        read: &mut Option<Self>,
        track_id: u64,
        result: Result<T, Arc<syzygy_catalog::Error>>,
        what: &str,
    ) {
        if let Some(read) = read.as_mut().filter(|read| read.track_id == track_id) {
            read.remote.apply(Read::Fresh(result), what);
            read.load = None;
        }
    }
}

/// A Manual queue entry or upcoming track being dragged.
struct Drag {
    entry: Entry,
    from: usize,
    /// The row of its section the pointer is over, and where it would go.
    over: Option<usize>,
}

/// The two lists of the Queue tab rows move within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Queued,
    Upcoming,
}

impl Section {
    fn of(entry: Entry) -> Option<Self> {
        match entry {
            Entry::Played(_) => None,
            Entry::Queued(_) => Some(Section::Queued),
            Entry::Upcoming(_) => Some(Section::Upcoming),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Close,
    /// A tab was clicked.
    Show(Tab),
    Maximize,
    /// The tab scrolled, and how far down it can go.
    Scrolled(Viewport, f32),
    Pick(Entry),
    Remove(Entry),
    Clear,
    /// A row's grip was pressed: the row at this place of its section.
    Grab(Entry, usize),
    /// The pointer went over the row at this place of a section.
    Over(Section, usize),
    /// The mouse button was let go, wherever it was.
    Dropped,
    /// Read the tab showing again, after it failed.
    Retry,
    Suggested(u64, Result<Option<Radio>, Arc<syzygy_catalog::Error>>),
    Lyrics(u64, Result<Lyrics, Arc<syzygy_catalog::Error>>),
    Credits(u64, Result<Vec<Credit>, Arc<syzygy_catalog::Error>>),
    /// Where the synced line at this index is.
    Measured(usize, Measure),
    /// "Sync lyrics": follow the line sung again.
    Sync,
    /// Play the Track radio the Suggested tab shows, from this track.
    PlaySuggested(usize),
    Link(Link),
}

/// What the drawer asks of the Shell.
pub enum Effect {
    None,
    Playback(playback::Message),
    Link(Link),
    Maximize,
    /// Read the track's Track radio, for [`Message::Suggested`].
    ReadSuggested(Track),
    /// Read the track's lyrics, for [`Message::Lyrics`].
    ReadLyrics(u64),
    /// Read the track's credits, for [`Message::Credits`].
    ReadCredits(u64),
    /// Find the line sung, at this index, with [`measure`].
    Measure(usize),
    /// Scroll the tab to this offset.
    ScrollTo(f32),
}

impl Default for Drawer {
    fn default() -> Self {
        Self {
            open: false,
            slide: Animation::new(false)
                .duration(SLIDE)
                .easing(animation::Easing::EaseOutCubic),
            tab: Tab::Queue,
            track: None,
            suggested: None,
            lyrics: None,
            credits: None,
            position: 0.0,
            follow: Follow::default(),
            drag: None,
            viewport: Viewport {
                offset: 0.0,
                height: UNKNOWN_HEIGHT,
            },
        }
    }
}

impl Drawer {
    pub fn update(&mut self, message: Message) -> Effect {
        match message {
            Message::Close => {
                self.close();
                Effect::None
            }
            Message::Show(tab) => self.show(tab),
            Message::Maximize => Effect::Maximize,
            Message::Scrolled(viewport, end) => {
                self.viewport = viewport;
                if self.tab == Tab::Lyrics {
                    self.scrolled(viewport.offset, end);
                }
                Effect::None
            }
            Message::Pick(entry) => Effect::Playback(playback::Message::Pick(entry)),
            Message::Remove(entry) => Effect::Playback(playback::Message::Remove(entry)),
            Message::Clear => Effect::Playback(playback::Message::Clear),
            Message::Grab(entry, from) => {
                self.drag = Some(Drag {
                    entry,
                    from,
                    over: None,
                });
                Effect::None
            }
            Message::Over(section, at) => {
                if let Some(drag) = &mut self.drag {
                    drag.over = (Section::of(drag.entry) == Some(section)).then_some(at);
                }
                Effect::None
            }
            Message::Dropped => match self.drag.take() {
                Some(Drag {
                    entry,
                    from,
                    over: Some(to),
                }) if to != from => Effect::Playback(playback::Message::Move(entry, to)),
                _ => Effect::None,
            },
            Message::Retry => {
                match self.tab {
                    Tab::Queue => {}
                    Tab::Suggested => self.suggested = None,
                    Tab::Lyrics => self.lyrics = None,
                    Tab::Credits => self.credits = None,
                }
                self.wanted()
            }
            Message::Suggested(track_id, result) => {
                TabRead::arrived(
                    &mut self.suggested,
                    track_id,
                    result,
                    "the Suggested tracks",
                );
                Effect::None
            }
            Message::Lyrics(track_id, result) => {
                TabRead::arrived(&mut self.lyrics, track_id, result, "the lyrics");
                self.follow.line = None;
                Effect::None
            }
            Message::Credits(track_id, result) => {
                TabRead::arrived(&mut self.credits, track_id, result, "the credits");
                Effect::None
            }
            Message::Measured(line, measure) => {
                if !self.follow.on || self.follow.line != Some(Some(line)) {
                    return Effect::None;
                }
                // The line in the middle, as far as the tab scrolls.
                let top = measure.line.y - measure.content.y;
                let offset = top + measure.line.height / 2.0 - measure.viewport.height / 2.0;
                let end = (measure.content.height - measure.viewport.height).max(0.0);
                self.scroll_to(offset.clamp(0.0, end))
            }
            Message::Sync => {
                self.follow = Follow::default();
                Effect::None
            }
            Message::PlaySuggested(index) => {
                let radio = self
                    .suggested
                    .as_ref()
                    .and_then(|read| read.remote.loaded())
                    .and_then(Option::as_ref);
                match radio {
                    Some(radio) => Effect::Playback(playback::Message::Start(PlayRequest {
                        source: radio.source.clone(),
                        first_page: radio.tracks.clone(),
                        start: Start::Track(index),
                        continuation: None,
                    })),
                    None => Effect::None,
                }
            }
            Message::Link(link) => Effect::Link(link),
        }
    }

    /// A player bar toggle: open on `tab`, or close if it's already
    /// showing.
    pub fn toggle(&mut self, tab: Tab) -> Effect {
        if self.showing() == Some(tab) {
            self.close();
            return Effect::None;
        }
        self.show(tab)
    }

    /// The tab showing, while the drawer is open.
    pub fn showing(&self) -> Option<Tab> {
        self.open.then_some(self.tab)
    }

    /// Slide it down. False if it wasn't open.
    pub fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.drag = None;
        self.slide.go_mut(false, Instant::now());
        true
    }

    fn show(&mut self, tab: Tab) -> Effect {
        if self.tab != tab || !self.open {
            // The Shell scrolls the new tab to its top.
            self.viewport.offset = 0.0;
            self.follow = Follow::default();
        }
        self.tab = tab;
        if !self.open {
            self.open = true;
            self.slide.go_mut(true, Instant::now());
        }
        self.wanted()
    }

    /// The current track is now `track`, `position` seconds in.
    pub fn playing(&mut self, track: Option<&Track>, position: f32) -> Effect {
        self.position = position;
        if self.track.as_ref().map(|track| track.id) == track.map(|track| track.id) {
            return Effect::None;
        }
        self.track = track.cloned();
        self.follow = Follow::default();
        self.wanted()
    }

    /// The scroll that brings the line sung into view, when the Lyrics tab
    /// follows it and it's moved on. Lyrics without synced lines go to
    /// their top, once.
    pub fn follow(&mut self) -> Effect {
        let Some(lyrics) = self.lyrics_showing() else {
            return Effect::None;
        };
        let line = lyrics.active(self.position);
        if !self.follow.on || self.follow.line == Some(line) {
            return Effect::None;
        }
        self.follow.line = Some(line);
        match line {
            Some(line) => Effect::Measure(line),
            None => self.scroll_to(0.0),
        }
    }

    /// The current track's lyrics, while their tab shows.
    fn lyrics_showing(&self) -> Option<&Lyrics> {
        let track = self.track.as_ref()?;
        self.lyrics
            .as_ref()
            .filter(|read| self.showing() == Some(Tab::Lyrics) && read.track_id == track.id)?
            .remote
            .loaded()
    }

    /// Whether the user scrolled synced lyrics away from the line sung.
    fn paused(&self) -> bool {
        !self.follow.on
            && self
                .lyrics_showing()
                .is_some_and(|lyrics| lyrics.synced.is_some())
    }

    fn scroll_to(&mut self, offset: f32) -> Effect {
        // The tab says where it is only when that changes.
        let landed = (self.viewport.offset - offset).abs() <= LANDED;
        self.follow.placed = Some(Placed { offset, landed });
        Effect::ScrollTo(offset)
    }

    /// The Lyrics tab is `offset` down, of at most `end`: if the tab
    /// didn't scroll there itself, the user did.
    fn scrolled(&mut self, offset: f32, end: f32) {
        // Resized: the line sung has moved, so it's measured again.
        let resized = self
            .follow
            .end
            .replace(end)
            .is_some_and(|before| (before - end).abs() > LANDED);
        if resized {
            self.follow.line = None;
        }
        let Some(placed) = self.follow.placed.as_mut() else {
            return;
        };
        let there = (placed.offset - offset).abs() <= LANDED;
        // A resize cut the tab's own scroll short.
        let cut = resized && offset < placed.offset && (end - offset).abs() <= LANDED;
        if there || cut {
            placed.offset = offset;
            placed.landed = true;
        } else if placed.landed {
            self.follow.on = false;
        }
    }

    /// The read the tab showing needs, when it has none for the current
    /// track.
    fn wanted(&mut self) -> Effect {
        let Some(track) = self.track.as_ref().filter(|_| self.open) else {
            return Effect::None;
        };
        let current = |id: Option<u64>| id == Some(track.id);
        match self.tab {
            Tab::Queue => Effect::None,
            Tab::Suggested if !current(self.suggested.as_ref().map(|read| read.track_id)) => {
                self.suggested = Some(TabRead::loading(track.id));
                Effect::ReadSuggested(track.clone())
            }
            Tab::Lyrics if !current(self.lyrics.as_ref().map(|read| read.track_id)) => {
                self.lyrics = Some(TabRead::loading(track.id));
                Effect::ReadLyrics(track.id)
            }
            Tab::Credits if !current(self.credits.as_ref().map(|read| read.track_id)) => {
                self.credits = Some(TabRead::loading(track.id));
                Effect::ReadCredits(track.id)
            }
            Tab::Suggested | Tab::Lyrics | Tab::Credits => Effect::None,
        }
    }

    /// The read [`Self::update`] or [`Self::playing`] just asked for runs
    /// as `handle`.
    pub fn reading(&mut self, handle: iced::task::Handle) {
        let load = match self.tab {
            Tab::Queue => None,
            Tab::Suggested => self.suggested.as_mut().map(|read| &mut read.load),
            Tab::Lyrics => self.lyrics.as_mut().map(|read| &mut read.load),
            Tab::Credits => self.credits.as_mut().map(|read| &mut read.load),
        };
        if let Some(load) = load {
            *load = Some(handle.abort_on_drop());
        }
    }

    /// Whether it's sliding, so frames are wanted.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.slide.is_animating(now)
    }

    /// Whether the Queue tab shows the current track's row, whose bars
    /// bounce while it plays.
    pub fn shows_current(&self) -> bool {
        self.showing() == Some(Tab::Queue)
    }

    /// The drawer while it's open or sliding, and there's a current track.
    /// `clock` is the seconds animations run on.
    pub fn view<'a>(
        &'a self,
        playback: &'a Playback,
        images: &'a Images,
        library: &'a Library,
        clock: f32,
        now: Instant,
    ) -> Option<Element<'a, Message>> {
        let track = playback.current()?;
        if !self.open && !self.slide.is_animating(now) {
            return None;
        }
        let shown = self.slide.interpolate(0.0, 1.0, now);
        let backdrop = mouse_area(
            container(space())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Color::BLACK.scale_alpha(BACKDROP * shown).into()),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::Close);
        let panel = responsive(move |size| {
            let panel = row![
                container(cover_side(track, images))
                    .width(Length::FillPortion(COVER_SHARE))
                    .height(Length::Fill),
                container(self.tabs(playback, images, library, clock))
                    .width(Length::FillPortion(TABS_SHARE))
                    .height(Length::Fill)
                    .style(tabs_side),
            ];
            let panel = container(panel)
                .height(size.height)
                .style(|_| container::Style {
                    background: Some(style::BG_BASE.into()),
                    ..container::Style::default()
                });
            // It slides up from below the edge it's clipped at, over the
            // backdrop.
            let below = size.height * (1.0 - shown);
            container(column![space().height(below), panel])
                .height(Length::Fill)
                .clip(true)
                .into()
        });
        Some(stack![backdrop, panel].into())
    }

    /// The tab pills, the maximize and close buttons, and the tab showing.
    fn tabs<'a>(
        &'a self,
        playback: &'a Playback,
        images: &'a Images,
        library: &'a Library,
        clock: f32,
    ) -> Element<'a, Message> {
        let pill = |tab: Tab, glyph: Icon, label: &'static str| {
            let on = self.tab == tab;
            let color = if on {
                style::TEXT_PRIMARY
            } else {
                style::TEXT_MUTED
            };
            button(
                row![icon(glyph, 14.0, color), text(label).size(13).color(color)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding([8, 16])
            .style(move |theme, status| tab_pill(theme, status, on))
            .on_press(Message::Show(tab))
        };
        let round = |glyph: Icon, message: Message| {
            button(container(icon(glyph, 18.0, style::TEXT_MUTED)).center(32))
                .padding(0)
                .style(style::icon_button)
                .on_press(message)
        };
        let bar = row![
            pill(Tab::Queue, Icon::ListMusic, "Play queue"),
            pill(Tab::Suggested, Icon::Sparkles, "Suggested tracks"),
            pill(Tab::Lyrics, Icon::MicVocal, "Lyrics"),
            pill(Tab::Credits, Icon::Users, "Credits"),
            space::horizontal(),
            round(Icon::Maximize2, Message::Maximize),
            round(Icon::X, Message::Close),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let content = match self.tab {
            Tab::Queue => self.queue(playback, images, library, clock),
            Tab::Suggested => self.suggested(playback, images, library),
            Tab::Lyrics => self.lyrics(playback),
            Tab::Credits => self.credits(playback),
        };
        let content = scrollable(container(content).padding([TAB_TOP, TAB_PADDING]))
            .id(SCROLL)
            .on_scroll(|viewport| {
                let height = viewport.bounds().height;
                let end = (viewport.content_bounds().height - height).max(0.0);
                Message::Scrolled(
                    Viewport {
                        offset: viewport.absolute_offset().y,
                        height,
                    },
                    end,
                )
            })
            .width(Length::Fill)
            .height(Length::Fill);
        let sync = self.paused().then(|| {
            let sync = button(text("Sync lyrics").size(13))
                .padding([8, 16])
                .style(style::accent_pill)
                .on_press(Message::Sync);
            container(sync)
                .center_x(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(24)
        });
        let content = stack![content].push(sync);
        column![
            container(bar).padding(iced::Padding::new(TAB_PADDING).top(20.0).bottom(8.0)),
            content
        ]
        .into()
    }

    /// Recent History, the current track, the Manual queue and what's left
    /// of the source, each under its heading. Every line is a row tall, so
    /// only the lines near the viewport are built.
    fn queue<'a>(
        &'a self,
        playback: &'a Playback,
        images: &'a Images,
        library: &'a Library,
        clock: f32,
    ) -> Element<'a, Message> {
        let lines = lines(playback);
        let range = track_list::window(lines.len(), TAB_TOP, self.viewport);
        let above = range.start as f32 * ROW_HEIGHT;
        let below = (lines.len() - range.end) as f32 * ROW_HEIGHT;
        let built = lines[range]
            .iter()
            .map(|line| self.line(line, playback, images, library, clock));
        column![
            space().height(above),
            Column::with_children(built),
            space().height(below)
        ]
        .into()
    }

    fn line<'a>(
        &'a self,
        line: &Line<'a>,
        playback: &'a Playback,
        images: &'a Images,
        library: &'a Library,
        clock: f32,
    ) -> Element<'a, Message> {
        let allow_explicit = playback.allow_explicit();
        let track_row = |number: usize, track: &'a Track, mark| {
            let liked = library.liked(track);
            track_list::marked(images, number, track, COLUMNS, allow_explicit, mark, liked)
                .map(Message::Link)
        };
        match *line {
            Line::Heading(heading) => self.heading(heading, playback),
            Line::Played(number, id, track) => track_list::playable(
                track_row(number, track, Mark::None),
                Message::Pick(Entry::Played(id)),
                false,
            ),
            Line::Current(track) => {
                let mark = match playback.status() {
                    playback::Status::Playing => Mark::Playing(clock),
                    _ => Mark::Current { hovered: false },
                };
                // Lit, as a playing row is, but there's nothing to pick.
                container(track_row(1, track, mark))
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(style::HL_MED.into()),
                        border: style::rounded(4.0),
                        ..container::Style::default()
                    })
                    .into()
            }
            Line::Queued(at, id, track) => {
                self.movable(Entry::Queued(id), at, track_row(at + 1, track, Mark::None))
            }
            Line::Upcoming(at, slot, track) => self.movable(
                Entry::Upcoming(slot),
                at,
                track_row(at + 1, track, Mark::None),
            ),
        }
    }

    /// A row that plays when clicked, dragged by its grip within its
    /// section and taken out by its cross.
    fn movable<'a>(
        &'a self,
        entry: Entry,
        at: usize,
        track_row: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let grip = mouse_area(
            container(icon(Icon::GripVertical, 14.0, style::TEXT_DISABLED))
                .center_x(GRIP_WIDTH)
                .center_y(ROW_HEIGHT),
        )
        .on_press(Message::Grab(entry, at))
        .interaction(mouse::Interaction::Grab);
        let remove = button(container(icon(Icon::X, 14.0, style::TEXT_MUTED)).center(28))
            .padding(0)
            .style(style::icon_button)
            .on_press(Message::Remove(entry));
        let line = row![
            grip,
            track_list::playable(track_row, Message::Pick(entry), false),
            remove
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let Some(drag) = self.drag.as_ref().filter(|drag| {
            Section::of(drag.entry) == Section::of(entry) && Section::of(entry).is_some()
        }) else {
            return line.into();
        };
        let dragged = drag.entry == entry;
        // Where it would land: above a row it's dragged up to, below one
        // it's dragged down to.
        let mark = match drag.over {
            Some(over) if over == at && at < drag.from => Some(Alignment::Start),
            Some(over) if over == at && at > drag.from => Some(Alignment::End),
            _ => None,
        };
        let marker = mark.map(|edge| {
            container(container(space()).width(Length::Fill).height(2).style(|_| {
                container::Style {
                    background: Some(style::ACCENT.into()),
                    border: style::rounded(1.0),
                    ..container::Style::default()
                }
            }))
            .padding(iced::Padding::new(0.0).left(GRIP_WIDTH))
            .height(ROW_HEIGHT)
            .align_y(edge)
        });
        let line = container(line).style(move |_| container::Style {
            background: dragged.then_some(style::HL_FAINT.into()),
            ..container::Style::default()
        });
        mouse_area(stack![line].push(marker))
            .on_enter(Message::Over(
                Section::of(entry).expect("only queued and upcoming rows move"),
                at,
            ))
            .into()
    }

    fn heading<'a>(&'a self, heading: Heading, playback: &'a Playback) -> Element<'a, Message> {
        let title = |label: String| text(label).size(13).font(BOLD).color(style::TEXT_MUTED);
        let (label, clear): (Element<'a, Message>, bool) = match heading {
            Heading::History => (title("HISTORY".to_string()).into(), false),
            Heading::NowPlaying => (title("NOW PLAYING".to_string()).into(), false),
            Heading::NextInQueue => (title("NEXT IN QUEUE".to_string()).into(), true),
            Heading::NextUp { clear } => {
                let source = playback.source();
                let route = source
                    .zip(playback.current())
                    .and_then(|(source, track)| page::source_route(source, track));
                let label: Element<'a, Message> = match (source, route) {
                    (Some(source), Some(route)) => row![
                        title("NEXT UP FROM".to_string()),
                        page::link(
                            text(source.name.to_uppercase()).size(13).font(BOLD),
                            Link::Open(route),
                        )
                        .map(Message::Link),
                    ]
                    .spacing(4)
                    .into(),
                    (Some(source), None) => {
                        title(format!("NEXT UP FROM {}", source.name.to_uppercase())).into()
                    }
                    (None, _) => title("NEXT UP".to_string()).into(),
                };
                (label, clear)
            }
        };
        let clear = clear.then(|| {
            button(text("Clear").size(11))
                .padding(0)
                .style(style::text_link)
                .on_press(Message::Clear)
        });
        container(
            row![container(label).clip(true), space::horizontal()]
                .push(clear)
                .align_y(Alignment::Center),
        )
        .padding(iced::Padding::new(0.0).bottom(12.0))
        .align_bottom(ROW_HEIGHT)
        .into()
    }

    /// The current track's Track radio, each track playing it from there.
    fn suggested<'a>(
        &'a self,
        playback: &'a Playback,
        images: &'a Images,
        library: &'a Library,
    ) -> Element<'a, Message> {
        let current = playback.current().map(|track| track.id);
        let read = self
            .suggested
            .as_ref()
            .filter(|read| Some(read.track_id) == current);
        let radio = match read.map(|read| &read.remote) {
            None | Some(Remote::Loading) => return loading(),
            Some(Remote::Failed(e)) => return failed(e),
            Some(Remote::NotFound) => None,
            Some(Remote::Loaded(radio)) => radio.as_ref(),
        };
        let Some(radio) = radio.filter(|radio| !radio.tracks.is_empty()) else {
            return empty(
                Icon::Sparkles,
                "No suggested tracks available for this track",
            );
        };
        let tracks = &radio.tracks;
        let range = track_list::window(tracks.len(), TAB_TOP, self.viewport);
        let above = range.start as f32 * ROW_HEIGHT;
        let below = (tracks.len() - range.end) as f32 * ROW_HEIGHT;
        let rows = range.map(|i| {
            let track = &tracks[i];
            let line = track_list::marked(
                images,
                i + 1,
                track,
                COLUMNS,
                playback.allow_explicit(),
                Mark::None,
                library.liked(track),
            )
            .map(Message::Link);
            track_list::playable(line, Message::PlaySuggested(i), Some(track.id) == current)
        });
        column![
            space().height(above),
            Column::with_children(rows),
            space().height(below)
        ]
        .into()
    }

    /// The lyrics: synced lines with the one sung lit, else plain text.
    /// Lines aren't clickable.
    fn lyrics<'a>(&'a self, playback: &'a Playback) -> Element<'a, Message> {
        let current = playback.current().map(|track| track.id);
        let read = self
            .lyrics
            .as_ref()
            .filter(|read| Some(read.track_id) == current);
        let lyrics = match read.map(|read| &read.remote) {
            None | Some(Remote::Loading) => return loading(),
            Some(Remote::Failed(e)) => return failed(e),
            Some(Remote::NotFound) => None,
            Some(Remote::Loaded(lyrics)) => Some(lyrics),
        };
        let no_lyrics = || empty(Icon::MicVocal, "No lyrics available");
        let Some(lyrics) = lyrics else {
            return no_lyrics();
        };
        let align = if lyrics.right_to_left {
            iced::alignment::Horizontal::Right
        } else {
            iced::alignment::Horizontal::Left
        };
        let words = |words: &'a str| {
            text(words)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill)
                .align_x(align)
        };
        let body: Element<'a, Message> = match (&lyrics.synced, &lyrics.plain) {
            (Some(lines), _) => {
                let sung = lyrics.active(self.position);
                let lines = lines.iter().enumerate().map(|(i, line)| {
                    let (color, sung) = match sung {
                        Some(sung) if sung == i => (style::TEXT_PRIMARY, true),
                        Some(sung) if i < sung => (style::TEXT_MUTED, false),
                        _ => (style::TEXT_DISABLED, false),
                    };
                    // A break between verses, which would be invisible lit.
                    let line = if line.text.is_empty() {
                        "\u{266A}"
                    } else {
                        line.text.as_str()
                    };
                    let line = container(words(line).size(24).font(BOLD).color(color));
                    if sung { line.id(SUNG) } else { line }.into()
                });
                Column::with_children(lines).spacing(16).into()
            }
            (None, Some(plain)) => words(plain)
                .size(16)
                .line_height(1.8)
                .color(style::TEXT_SECONDARY)
                .into(),
            (None, None) => return no_lyrics(),
        };
        let provider = lyrics.provider.as_ref().map(|provider| {
            text(format!("Lyrics provided by {provider}"))
                .width(Length::Fill)
                .align_x(align)
                .size(12)
                .color(style::TEXT_DISABLED)
        });
        column![body]
            .push(provider)
            .spacing(32)
            .padding(iced::Padding::new(0.0).bottom(64.0))
            .into()
    }

    /// What the current track is, then who did what on it.
    fn credits<'a>(&'a self, playback: &'a Playback) -> Element<'a, Message> {
        let Some(track) = playback.current() else {
            return space().into();
        };
        let read = self
            .credits
            .as_ref()
            .filter(|read| read.track_id == track.id);
        let credits = match read.map(|read| &read.remote) {
            None | Some(Remote::Loading) => None,
            Some(Remote::Failed(e)) => return failed(e),
            Some(Remote::NotFound) => Some(&[][..]),
            Some(Remote::Loaded(credits)) => Some(credits.as_slice()),
        };
        if credits.is_some_and(<[Credit]>::is_empty) {
            return empty(Icon::Users, "No credits available for this track");
        }
        let names = |names: Vec<Element<'a, Message>>| -> Element<'a, Message> {
            row(names).spacing(4).wrap().into()
        };
        let artists = track
            .artists
            .iter()
            .map(|artist| artist_link(&artist.name, Some(artist.id)))
            .collect();
        let mut lines = vec![
            credit_line("Title", text(&track.title).size(14).into(), true),
            credit_line("Artists", names(artists), false),
        ];
        if let Some(album) = &track.album {
            let link = page::link(
                text(&album.title).size(14),
                Link::Open(Route::Album {
                    id: album.id,
                    preview: Some(Preview {
                        title: album.title.clone(),
                        cover: album.cover.clone(),
                        artist: None,
                    }),
                }),
            )
            .map(Message::Link);
            lines.push(credit_line("Album", link, false));
        }
        match credits {
            Some(credits) => lines.extend(credits.iter().map(|credit| {
                let who = credit
                    .contributors
                    .iter()
                    .map(|contributor| artist_link(&contributor.name, contributor.artist_id))
                    .collect();
                credit_line(&credit.role, names(who), false)
            })),
            None => lines.push(loading()),
        }
        Column::with_children(lines).into()
    }
}

/// The Queue tab's lines, top to bottom.
#[derive(Clone, Copy)]
enum Line<'a> {
    Heading(Heading),
    /// Its number in the History shown.
    Played(usize, EntryId, &'a Track),
    Current(&'a Track),
    /// Its place in the Manual queue.
    Queued(usize, EntryId, &'a Track),
    /// Its place in what's left of the source.
    Upcoming(usize, Slot, &'a Track),
}

#[derive(Clone, Copy)]
enum Heading {
    History,
    NowPlaying,
    NextInQueue,
    /// What's left of the source, with Clear when the Manual queue is
    /// empty and has no heading to carry it.
    NextUp {
        clear: bool,
    },
}

fn lines(playback: &Playback) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let history: Vec<_> = playback.history().collect();
    let recent = &history[history.len().saturating_sub(HISTORY_ROWS)..];
    if !recent.is_empty() {
        lines.push(Line::Heading(Heading::History));
        lines.extend(
            recent
                .iter()
                .enumerate()
                .map(|(i, &(id, track))| Line::Played(i + 1, id, track)),
        );
    }
    if let Some(track) = playback.current() {
        lines.push(Line::Heading(Heading::NowPlaying));
        lines.push(Line::Current(track));
    }
    let queued_from = lines.len();
    lines.extend(
        playback
            .queued()
            .enumerate()
            .map(|(at, (id, track))| Line::Queued(at, id, track)),
    );
    let queued = lines.len() > queued_from;
    if queued {
        lines.insert(queued_from, Line::Heading(Heading::NextInQueue));
    }
    let upcoming_from = lines.len();
    lines.extend(
        playback
            .upcoming()
            .enumerate()
            .map(|(at, (slot, track))| Line::Upcoming(at, slot, track)),
    );
    if lines.len() > upcoming_from {
        let heading = Heading::NextUp { clear: !queued };
        lines.insert(upcoming_from, Line::Heading(heading));
    }
    lines
}

/// The cover, as big as fits, over the title and artists.
fn cover_side<'a>(track: &'a Track, images: &'a Images) -> Element<'a, Message> {
    responsive(move |size| {
        let width = size.width - 2.0 * COVER_PADDING;
        let height = size.height - 2.0 * COVER_PADDING - COVER_GAP - CAPTION_HEIGHT;
        let side = width.min(height).clamp(0.0, MAX_COVER).floor();
        let cover = page::large_cover(images, track, side);
        let caption = column![
            text(&track.title)
                .size(22)
                .font(BOLD)
                .wrapping(text::Wrapping::None),
            container(page::artists(&track.artists, 15.0)).clip(true),
        ]
        .spacing(4)
        .align_x(Alignment::Center);
        let caption = container(caption)
            .clip(true)
            .center_x(side.max(1.0))
            .height(CAPTION_HEIGHT);
        let side = column![cover, caption]
            .spacing(COVER_GAP)
            .align_x(Alignment::Center);
        Element::from(container(side).center(Length::Fill).padding(COVER_PADDING))
            .map(Message::Link)
    })
    .into()
}

/// One line of the Credits tab: what, then who.
fn credit_line<'a>(label: &str, value: Element<'a, Message>, first: bool) -> Element<'a, Message> {
    let line = row![
        text(label.to_string())
            .size(14)
            .color(style::TEXT_MUTED)
            .width(Length::FillPortion(2)),
        container(value).width(Length::FillPortion(3)),
    ]
    .spacing(16);
    let line = container(line).padding([12, 0]);
    if first {
        line.into()
    } else {
        column![rule::horizontal(1).style(divider), line].into()
    }
}

/// A name, leading to the artist's Page when it has one.
fn artist_link<'a>(name: &str, id: Option<u64>) -> Element<'a, Message> {
    let label = text(name.to_string()).size(14);
    match id {
        Some(id) => page::link(
            label,
            Link::Open(Route::Artist {
                id,
                preview: Some(Preview {
                    title: name.to_string(),
                    cover: None,
                    artist: None,
                }),
            }),
        )
        .map(Message::Link),
        None => label.into(),
    }
}

fn loading<'a>() -> Element<'a, Message> {
    container(text("Loading…").size(14).color(style::TEXT_MUTED))
        .padding([12, 0])
        .into()
}

fn failed<'a>(e: &syzygy_catalog::Error) -> Element<'a, Message> {
    column![
        text("Couldn't load this").size(16),
        text(e.to_string()).size(13).color(style::TEXT_MUTED),
        button(text("Retry")).on_press(Message::Retry),
    ]
    .spacing(12)
    .into()
}

/// An empty tab: a large faint icon over what's missing.
fn empty<'a>(glyph: Icon, label: &'a str) -> Element<'a, Message> {
    container(
        column![
            icon(glyph, 40.0, style::TEXT_DISABLED),
            text(label).size(14).color(style::TEXT_DISABLED),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([64, 0])
    .into()
}

/// The tabs' side: a hairline along its left.
fn tabs_side(_theme: &Theme) -> container::Style {
    container::Style {
        border: iced::Border {
            color: style::BORDER_SUBTLE,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

fn divider(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: style::BORDER_SUBTLE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// A tab pill: lit while its tab shows, faintly under the pointer.
fn tab_pill(_theme: &Theme, status: button::Status, on: bool) -> button::Style {
    let background = match status {
        _ if on => Some(style::HL_MED.into()),
        button::Status::Hovered | button::Status::Pressed => Some(style::HL_FAINT.into()),
        _ => None,
    };
    button::Style {
        background,
        text_color: style::TEXT_PRIMARY,
        border: style::rounded(999.0),
        ..button::Style::default()
    }
}

/// Find where the line sung is in the tab's scrollable.
pub fn measure() -> Task<Measure> {
    iced::advanced::widget::operate(Measuring::default())
}

#[derive(Default)]
struct Measuring {
    scrollable: Option<(Rectangle, Rectangle)>,
    line: Option<Rectangle>,
}

impl Operation<Measure> for Measuring {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Measure>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&SUNG) {
            self.line = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&SCROLL) {
            self.scrollable = Some((bounds, content_bounds));
        }
    }

    fn finish(&self) -> operation::Outcome<Measure> {
        match (self.scrollable, self.line) {
            (Some((viewport, content)), Some(line)) => operation::Outcome::Some(Measure {
                viewport,
                content,
                line,
            }),
            _ => operation::Outcome::None,
        }
    }
}
