//! The signed-in Shell: the sidebar, the header with its search, the
//! current Page and the Back stack, the now-playing drawer and the
//! maximized player, and the modal and toasts over them.

mod back_stack;
mod consent;
mod dialog;
pub mod drawer;
mod maximized;
mod picker;
mod play_card;
pub mod player_bar;
pub mod search;
mod settings_modal;
pub mod sidebar;
pub mod toast;
mod unseen;

use futures::StreamExt;
use futures::stream::{self, BoxStream};
use iced::task;
use iced::widget::{button, column, container, operation, row, scrollable, space, stack};
use iced::{Alignment, Element, Length, Task};
use std::sync::Arc;
use std::time::Instant;
use syzygy_catalog::{Catalog, Feed, HomeFeed, Read, Track};

use crate::app::{self, Services};
use crate::icons::{Icon, icon};
use crate::images::{self, Images};
use crate::library::{self, Library, Stamp, short};
use crate::page::{
    self, Action, Context, Load, NowPlaying, Page, PageId, Route, Viewport, album, artist_tracks,
    artist_view_all, explore, favorites, feed, library as library_page, mix, playlist,
    search as search_page,
};
use crate::playback::{self, Notice, PlayRequest, Playback, Status};
use crate::radio;
use crate::settings::{Settings, Sort};
use crate::style;
use back_stack::{BackStack, Entry};
use dialog::Dialog;
use drawer::Drawer;
use picker::Picker;
use player_bar::PlayerBar;
use search::Search;
use settings_modal::{Hardware, SettingsModal};
use sidebar::Sidebar;
use toast::{Kind, ToastId, Toasts};
use unseen::Unseen;

const HEADER_HEIGHT: f32 = 64.0;
const HEADER_PADDING: f32 = 16.0;
const HEADER_SPACING: f32 = 8.0;
/// The back and forward buttons.
const STEP_SIZE: f32 = 32.0;
/// Pages stop growing past this on wide windows.
const MAX_PAGE_WIDTH: f32 = 1520.0;
/// How long TIDAL takes to make a playlist's cover after its tracks
/// change, as sone waits.
const COVER_DELAY: std::time::Duration = std::time::Duration::from_secs(3);
/// The scrollable every Page draws in.
const PAGE_SCROLL: iced::widget::Id = iced::widget::Id::new("page");
/// How tall the Page's viewport is taken to be until it's reported:
/// tall enough that a short list is built in full on any screen, since a
/// scrollable whose content fits never reports.
const UNKNOWN_HEIGHT: f32 = 4000.0;

pub struct Shell {
    /// Who's signed in, when TIDAL has said.
    user_id: Option<u64>,
    back_stack: BackStack,
    current: Current,
    sidebar: Sidebar,
    search: Search,
    player_bar: PlayerBar,
    drawer: Drawer,
    /// Whether the maximized player covers the window.
    maximized: bool,
    next_id: u64,
    toasts: Toasts,
    /// Whether the Feed has something the user hasn't seen.
    unseen: Unseen,
    /// How tall the Page's viewport is, as last reported.
    viewport_height: f32,
    /// A card's source being read to play. A newer play aborts it.
    card_play: Option<task::Handle>,
    /// A track being read for its Track radio. Going anywhere aborts it.
    radio_read: Option<task::Handle>,
    /// What waits on the explicit-consent modal, while it's open.
    consent: Option<playback::Pending>,
    /// The Settings modal, while it's open. Never on the Back stack.
    settings: Option<SettingsModal>,
    /// The Library's dialog, while one is open.
    dialog: Option<Dialog>,
    /// The "Add to playlist" popover, while it's open.
    picker: Option<Picker>,
    /// The user's Library: their Favorites, their root playlists and
    /// Folders, and their pending edits.
    library: Library,
}

/// The Page on screen and the reads it started. Dropping it aborts them.
struct Current {
    id: PageId,
    route: Route,
    page: Page,
    loads: Vec<task::Handle>,
    /// How far down the Page is scrolled, as last reported.
    offset: f32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Scrolled(PageId, Viewport),
    DismissToast(ToastId),
    /// Escape: it closes the modal, or else the maximized player, the
    /// drawer or the search dropdown.
    Escape,
    /// The explicit-consent modal was answered.
    Consent(consent::Answer),
    /// The header's settings button.
    OpenSettings,
    Settings(settings_modal::Message),
    Dialog(dialog::Message),
    Picker(picker::Message),
    Sidebar(sidebar::Message),
    Search(search::Message),
    PlayerBar(player_bar::Message),
    Drawer(drawer::Message),
    Maximized(maximized::Message),
    /// The avatar: the user's own Profile.
    OpenProfile,
    /// The Feed check after login.
    FeedChecked(Result<Feed, Arc<syzygy_catalog::Error>>),
    /// The Feed was marked seen, or not.
    FeedSeen(Result<(), Arc<syzygy_catalog::Error>>),
    /// A card's source was read, to play.
    CardRead(Result<Option<PlayRequest>, Arc<syzygy_catalog::Error>>),
    /// A track was read for its Track radio.
    TrackRead(Track, Result<Track, Arc<syzygy_catalog::Error>>),
    /// A card's source was read, to queue: next, or at the end.
    CardQueued {
        next: bool,
        result: Result<Option<PlayRequest>, Arc<syzygy_catalog::Error>>,
    },
    Library(library::Message),
    /// What's read under these tags is stale, a while after an edit
    /// landed.
    Reread(Vec<String>),
}

impl Shell {
    /// The Shell at Home, with an empty Back stack.
    pub fn new(services: &Services, context: &Context) -> (Self, Task<app::Message>) {
        let (current, action) = Current::open(
            PageId(0),
            Entry {
                route: Route::home(),
                offset: 0.0,
            },
            context,
        );
        let (mut library, favorites) = Library::new(context.user_id, context.settings);
        let (sidebar, read) = Sidebar::new(context.user_id, context.settings, &mut library);
        let mut shell = Self {
            user_id: context.user_id,
            back_stack: BackStack::default(),
            current,
            sidebar,
            search: Search::default(),
            player_bar: PlayerBar::default(),
            drawer: Drawer::default(),
            maximized: false,
            next_id: 1,
            toasts: Toasts::default(),
            unseen: Unseen::default(),
            viewport_height: UNKNOWN_HEIGHT,
            card_play: None,
            radio_read: None,
            consent: None,
            settings: None,
            dialog: None,
            picker: None,
            library,
        };
        let task = shell.run_action(action, services, context);
        let read = shell.run_sidebar(read, services, context);
        let favorites = shell.run_library(favorites, services, context);
        let check = shell.check_feed(context.user_id, services);
        (shell, Task::batch([task, read, favorites, check]))
    }

    /// TIDAL said who's signed in.
    pub fn user_known(
        &mut self,
        user_id: u64,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        self.user_id = Some(user_id);
        // The Feed opened before TIDAL said whose it was.
        let seen = match self.current.route {
            Route::Feed => self.feed_seen(user_id, services),
            _ => Task::none(),
        };
        let favorites = self.library.user_known(user_id);
        let favorites = self.run_library(favorites, services, context);
        let effect = self.sidebar.user_known(user_id, &mut self.library);
        let read = self.run_sidebar(effect, services, context);
        Task::batch([
            read,
            favorites,
            seen,
            self.check_feed(Some(user_id), services),
        ])
    }

    /// Read the Feed once, for the sidebar's dot.
    fn check_feed(&mut self, user_id: Option<u64>, services: &Services) -> Task<app::Message> {
        match self.unseen.check(user_id) {
            Some(user_id) => Task::perform(services.catalog.feed(user_id), |result| {
                app::Message::Shell(Message::FeedChecked(result))
            }),
            None => Task::none(),
        }
    }

    /// Go somewhere new. The current Page goes on the Back stack.
    pub fn navigate(
        &mut self,
        route: Route,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        self.radio_read = None;
        self.close_overlays();
        if route == self.current.route {
            return scroll_to(0.0);
        }
        self.back_stack.push(self.current.entry());
        self.open(Entry { route, offset: 0.0 }, services, context)
    }

    pub fn back(&mut self, services: &Services, context: &Context) -> Task<app::Message> {
        let entry = self.back_stack.back(self.current.entry());
        self.open_maybe(entry, services, context)
    }

    pub fn forward(&mut self, services: &Services, context: &Context) -> Task<app::Message> {
        let entry = self.back_stack.forward(self.current.entry());
        self.open_maybe(entry, services, context)
    }

    /// A message for a Page. Dropped unless that Page is still showing.
    pub fn update_page(
        &mut self,
        id: PageId,
        message: page::Message,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        if id != self.current.id {
            return Task::none();
        }
        let action = self.current.page.update(message, &self.library);
        self.run_action(action, services, context)
    }

    /// The window came back into focus.
    pub fn focused(&mut self, services: &Services, context: &Context) -> Task<app::Message> {
        let action = self.current.page.focused();
        self.run_action(action, services, context)
    }

    pub fn update(
        &mut self,
        message: Message,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        match message {
            Message::Scrolled(id, viewport) => {
                self.viewport_height = viewport.height;
                if id == self.current.id {
                    self.current.offset = viewport.offset;
                }
            }
            Message::DismissToast(id) => self.toasts.dismiss(id),
            Message::Escape => {
                let closed = self.consent.take().is_some()
                    || self.settings.take().is_some()
                    || self.dialog.take().is_some()
                    || self.picker.take().is_some()
                    || std::mem::take(&mut self.maximized)
                    || self.drawer.close();
                if !closed {
                    let effect = self.search.update(search::Message::Escape);
                    return self.run_search(effect, services, context);
                }
            }
            Message::Consent(answer) => {
                let Some(pending) = self.consent.take() else {
                    return Task::none();
                };
                let to_playback = |message| Task::done(app::Message::Playback(message));
                return match answer {
                    consent::Answer::Allow => to_playback(playback::Message::AllowExplicit(true))
                        .chain(to_playback(pending.into())),
                    consent::Answer::Without => {
                        to_playback(playback::Message::WithoutExplicit(pending))
                    }
                    consent::Answer::Dismiss => Task::none(),
                };
            }
            Message::OpenSettings => {
                self.settings = Some(SettingsModal::default());
                // Listing waits on GStreamer, for up to two seconds.
                return Task::perform(tokio::task::spawn_blocking(Hardware::list), |listed| {
                    let hardware = listed.unwrap_or_else(|e| Hardware {
                        devices: Err(e.to_string()),
                        gapless_supported: true,
                    });
                    app::Message::Shell(Message::Settings(settings_modal::Message::Listed(
                        hardware,
                    )))
                });
            }
            Message::Settings(settings_modal::Message::Close) => self.settings = None,
            Message::Settings(message) => {
                if let Some(message) = self
                    .settings
                    .as_mut()
                    .and_then(|modal| modal.update(message))
                {
                    return Task::done(message);
                }
            }
            Message::Dialog(message) => {
                let Some(dialog) = &mut self.dialog else {
                    return Task::none();
                };
                match dialog.update(message) {
                    dialog::Outcome::None => {}
                    dialog::Outcome::Close => self.dialog = None,
                    dialog::Outcome::Library(message) => {
                        self.dialog = None;
                        return self.update_library(*message, services, context);
                    }
                }
            }
            Message::Picker(message) => {
                let Some(picker) = &mut self.picker else {
                    return Task::none();
                };
                match picker.update(message) {
                    picker::Outcome::None => {}
                    picker::Outcome::Close => self.picker = None,
                    picker::Outcome::Library(message) => {
                        self.picker = None;
                        return self.update_library(*message, services, context);
                    }
                    picker::Outcome::Failed(text) => {
                        self.picker = None;
                        return self.toast(Kind::Error, text);
                    }
                }
            }
            Message::Sidebar(message) => {
                let effect = self.sidebar.update(message, &mut self.library);
                return self.run_sidebar(effect, services, context);
            }
            Message::Search(message) => {
                let effect = self.search.update(message);
                return self.run_search(effect, services, context);
            }
            Message::PlayerBar(message) => {
                return match self.player_bar.update(message) {
                    player_bar::Effect::None => Task::none(),
                    player_bar::Effect::Playback(message) => {
                        Task::done(app::Message::Playback(message))
                    }
                    player_bar::Effect::SaveVolume => Task::done(app::Message::SaveVolume),
                    player_bar::Effect::Drawer(tab) => {
                        let effect = self.drawer.toggle(tab);
                        let read = self.run_drawer(effect, services, context);
                        let follow = self.follow_lyrics(services, context);
                        Task::batch([read, drawer_to_top().chain(follow)])
                    }
                    player_bar::Effect::Maximize => {
                        self.maximized = true;
                        Task::none()
                    }
                    player_bar::Effect::Link(link) => {
                        self.run_action(link.follow(), services, context)
                    }
                };
            }
            Message::Drawer(message) => {
                let top = match message {
                    drawer::Message::Show(_) => drawer_to_top(),
                    _ => Task::none(),
                };
                let effect = self.drawer.update(message);
                let task = self.run_drawer(effect, services, context);
                let follow = self.follow_lyrics(services, context);
                return Task::batch([task, top.chain(follow)]);
            }
            Message::Maximized(maximized::Message::Minimize) => self.maximized = false,
            Message::Maximized(maximized::Message::PlayerBar(message)) => {
                return self.update(Message::PlayerBar(message), services, context);
            }
            Message::OpenProfile => {
                if let Some(user_id) = self.user_id {
                    return self.navigate(Route::Profile { user_id }, services, context);
                }
            }
            Message::FeedChecked(Ok(feed)) => self.unseen.arrived(feed.unseen),
            Message::FeedChecked(Err(e)) => log::warn!("Could not check the Feed: {e}"),
            Message::FeedSeen(result) => {
                if let Err(e) = result {
                    log::warn!("Could not mark the Feed seen: {e}");
                }
            }
            Message::TrackRead(track, result) => {
                self.radio_read = None;
                let mix_id = match result {
                    Ok(read) => read.track_radio,
                    Err(e) => {
                        log::warn!("Could not read track {} for its radio: {e}", track.id);
                        None
                    }
                };
                return match mix_id {
                    Some(mix_id) => {
                        self.navigate(page::track_radio_route(&track, mix_id), services, context)
                    }
                    None => self.toast(Kind::Info, "Track radio unavailable"),
                };
            }
            Message::Library(message) => return self.update_library(message, services, context),
            Message::Reread(tags) => return self.reread(&tags, services, context),
            Message::CardQueued { next, result } => {
                return match result {
                    Ok(Some(request)) => self.queue(
                        request.first_page,
                        request.source,
                        next,
                        context.settings.allow_explicit,
                    ),
                    Ok(None) => Task::none(),
                    Err(e) => {
                        log::warn!("Could not read what to queue: {e}");
                        self.toast(Kind::Error, format!("Couldn't queue it: {e}"))
                    }
                };
            }
            Message::CardRead(result) => {
                self.card_play = None;
                return match result {
                    Ok(Some(request)) => play(request),
                    Ok(None) => Task::none(),
                    Err(e) => {
                        log::warn!("Could not read what to play: {e}");
                        self.toast(Kind::Error, format!("Couldn't play it: {e}"))
                    }
                };
            }
        }
        Task::none()
    }

    /// The current track is now `track`, `position` seconds in: the
    /// drawer's tabs follow it.
    pub fn playing(
        &mut self,
        track: Option<&Track>,
        position: f32,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let effect = self.drawer.playing(track, position);
        let read = self.run_drawer(effect, services, context);
        Task::batch([read, self.follow_lyrics(services, context)])
    }

    /// Keep the line sung in view, should the Lyrics tab show.
    fn follow_lyrics(&mut self, services: &Services, context: &Context) -> Task<app::Message> {
        let effect = self.drawer.follow();
        self.run_drawer(effect, services, context)
    }

    /// Navigating anywhere shows where it went.
    fn close_overlays(&mut self) {
        self.drawer.close();
        self.maximized = false;
        self.settings = None;
        self.dialog = None;
        self.picker = None;
    }

    /// Playback chose explicit tracks while they aren't allowed: ask.
    pub fn ask_explicit_consent(&mut self, pending: playback::Pending) -> Task<app::Message> {
        self.consent = Some(pending);
        Task::none()
    }

    /// Tell the user what playback has to say.
    pub fn notify(&mut self, notice: Notice) -> Task<app::Message> {
        let (kind, text) = match notice {
            Notice::TooManyFailures => (
                Kind::Error,
                "Multiple tracks failed to play \u{2014} stopped".to_string(),
            ),
            Notice::Unavailable => (Kind::Info, "Track unavailable".to_string()),
            Notice::RateLimited(secs) => (
                Kind::Info,
                format!("Too many requests \u{2014} resuming in {secs}s"),
            ),
            Notice::DeviceBusy => (
                Kind::Info,
                "The audio device is busy \u{2014} trying again".to_string(),
            ),
            Notice::AudioError(error) => (Kind::Error, cut(&error, AUDIO_ERROR_LENGTH)),
        };
        self.toast(kind, text)
    }

    /// Exclusive output is resampling, so it isn't bit-perfect.
    pub fn resampled(&mut self, from: u32, to: u32) -> Task<app::Message> {
        self.toast(
            Kind::Info,
            format!("Resampling {} \u{2192} {} kHz", khz(from), khz(to)),
        )
    }

    /// Bit-perfect output widened the samples for the DAC.
    pub fn bit_depth_changed(&mut self, from: &str, to: &str) -> Task<app::Message> {
        self.toast(
            Kind::Info,
            format!("Bit depth changed: {from} \u{2192} {to}"),
        )
    }

    /// Show a toast that dismisses itself.
    pub fn toast(&mut self, kind: Kind, text: impl Into<String>) -> Task<app::Message> {
        self.toasts
            .push(kind, text.into())
            .map(|id| app::Message::Shell(Message::DismissToast(id)))
    }

    fn open_maybe(
        &mut self,
        entry: Option<Entry>,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        match entry {
            Some(entry) => self.open(entry, services, context),
            None => Task::none(),
        }
    }

    /// Show the Page for `entry` under a new id, replacing the current one
    /// and aborting its reads. The scrollable keeps the offset and clamps it
    /// to the content, so it lands once the Page's data is tall enough.
    fn open(&mut self, entry: Entry, services: &Services, context: &Context) -> Task<app::Message> {
        self.close_overlays();
        let offset = entry.offset;
        if let Route::Search { query, .. } = &entry.route {
            self.search.showing(query);
        }
        let seen = match (&entry.route, context.user_id) {
            (Route::Feed, Some(user_id)) => self.feed_seen(user_id, services),
            _ => Task::none(),
        };
        let (current, action) = Current::open(PageId(self.next_id), entry, context);
        self.current = current;
        self.next_id += 1;
        let task = self.run_action(action, services, context);
        Task::batch([task, seen, scroll_to(offset)])
    }

    /// The Feed opened: everything in it is seen. Not one of the Page's
    /// reads, so leaving the Page doesn't abort it.
    fn feed_seen(&mut self, user_id: u64, services: &Services) -> Task<app::Message> {
        self.unseen.opened();
        Task::perform(services.catalog.mark_feed_seen(user_id), |result| {
            app::Message::Shell(Message::FeedSeen(result))
        })
    }

    fn run_action(
        &mut self,
        action: Action,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        match action {
            Action::None => Task::none(),
            Action::Batch(actions) => Task::batch(
                actions
                    .into_iter()
                    .map(|action| self.run_action(action, services, context))
                    .collect::<Vec<_>>(),
            ),
            Action::FetchImages(urls) => {
                Task::done(app::Message::Images(images::Message::Wanted(urls)))
            }
            Action::SaveSort(sort) => {
                // The sidebar lists each Library type in the same order.
                let resort = match &sort {
                    Sort::Library(kind, library_sort) => {
                        let effect = self.sidebar.sorted(*kind, *library_sort, &mut self.library);
                        self.run_sidebar(effect, services, context)
                    }
                    _ => Task::none(),
                };
                Task::batch([Task::done(app::Message::Sort(sort)), resort])
            }
            Action::Navigate(route) => self.navigate(route, services, context),
            Action::Play(request) => {
                // The newer choice wins over a card still being read.
                self.card_play = None;
                play(request)
            }
            Action::PlayCard(card) => {
                let (task, handle) = Task::perform(
                    play_card::read(card, &services.catalog, context),
                    |result| app::Message::Shell(Message::CardRead(result)),
                )
                .abortable();
                self.card_play = Some(handle.abort_on_drop());
                task
            }
            Action::Queue { track, next } => {
                let tag = page::queue_tag(&track);
                self.queue(vec![track], tag, next, context.settings.allow_explicit)
            }
            Action::QueueCard { card, next } => Task::perform(
                play_card::read(card, &services.catalog, context),
                move |result| app::Message::Shell(Message::CardQueued { next, result }),
            ),
            Action::Library(message) => self.update_library(message, services, context),
            Action::TrackRadio(track) => match track.track_radio.clone() {
                Some(mix_id) => {
                    self.navigate(page::track_radio_route(&track, mix_id), services, context)
                }
                // Many lists don't say; the track read on its own does.
                None => {
                    let (task, handle) =
                        Task::perform(services.catalog.track(track.id), move |result| {
                            app::Message::Shell(Message::TrackRead(track.clone(), result))
                        })
                        .abortable();
                    self.radio_read = Some(handle.abort_on_drop());
                    task
                }
            },
            Action::TogglePlay => Task::done(app::Message::Playback(playback::Message::TogglePlay)),
            Action::Run(task) => {
                let id = self.current.id;
                task.map(move |message| app::Message::Page(id, message))
            }
            Action::Replace(route, load) => {
                self.current.route = route;
                self.current.offset = 0.0;
                let load = match load {
                    Some(load) => self.run_action(Action::Load(load), services, context),
                    None => Task::none(),
                };
                Task::batch([load, scroll_to(0.0)])
            }
            Action::Load(load) => {
                let id = self.current.id;
                let note = load.tags().map(|tags| (self.library.start_read(), tags));
                let reads = noting(read(load, &services.catalog), page_fresh, note, move |m| {
                    app::Message::Page(id, m)
                });
                let (task, handle) = Task::run(reads, std::convert::identity).abortable();
                self.current.loads.push(handle.abort_on_drop());
                task
            }
        }
    }

    /// Put tracks in the Manual queue under `tag`, and say so.
    fn queue(
        &mut self,
        tracks: Vec<Track>,
        tag: playback::Source,
        next: bool,
        allow_explicit: bool,
    ) -> Task<app::Message> {
        let what = match tracks.as_slice() {
            [track] => short(&track.title),
            _ => short(&tag.name),
        };
        // Explicit tracks that aren't allowed ask first instead.
        let asks = !allow_explicit && tracks.iter().any(|track| track.explicit);
        let message = if next {
            playback::Message::PlayNext(tracks, tag)
        } else {
            playback::Message::AddToQueue(tracks, tag)
        };
        let toast = match (asks, next) {
            (true, _) => Task::none(),
            (false, true) => {
                self.toast(Kind::Info, format!("\u{201c}{what}\u{201d} will play next"))
            }
            (false, false) => self.toast(
                Kind::Info,
                format!("Added \u{201c}{what}\u{201d} to the queue"),
            ),
        };
        Task::batch([Task::done(app::Message::Playback(message)), toast])
    }

    /// Hand the Library a message, and let the Page know its edits moved.
    fn update_library(
        &mut self,
        message: library::Message,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let effects = self.library.update(message);
        self.current.page.library_changed(&self.library);
        self.run_library(effects, services, context)
    }

    fn run_library(
        &mut self,
        effects: Vec<library::Effect>,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let to_library = |message| app::Message::Shell(Message::Library(message));
        let mut tasks = Vec::new();
        for effect in effects {
            let task = match effect {
                library::Effect::Mutate(edit, mutation) => {
                    let done = move |result| to_library(library::Message::Done(edit, result));
                    match mutation {
                        library::Mutation::Favorite { user_id, id, on } => {
                            Task::perform(services.catalog.set_favorite(user_id, id, on), done)
                        }
                        library::Mutation::CreatePlaylist { user_id, fields } => Task::perform(
                            services.catalog.create_playlist(user_id, fields),
                            move |result| to_library(library::Message::Created(edit, result)),
                        ),
                        library::Mutation::UpdatePlaylist {
                            user_id,
                            uuid,
                            fields,
                        } => Task::perform(
                            services.catalog.update_playlist(user_id, &uuid, fields),
                            done,
                        ),
                        library::Mutation::DeletePlaylist { user_id, uuid } => {
                            Task::perform(services.catalog.delete_playlist(user_id, &uuid), done)
                        }
                        library::Mutation::AddTrack {
                            user_id,
                            uuid,
                            track,
                        } => Task::perform(services.catalog.add_track(user_id, &uuid, track), done),
                        library::Mutation::AddTracks {
                            user_id,
                            uuid,
                            tracks,
                        } => Task::perform(
                            services.catalog.add_tracks(user_id, &uuid, tracks),
                            move |result| to_library(library::Message::Added(edit, result)),
                        ),
                    }
                }
                library::Effect::ReadFavorites { user_id, stamp } => {
                    Task::run(services.catalog.favorite_ids(user_id), move |read| {
                        to_library(library::Message::FavoriteIds(stamp, read))
                    })
                }
                library::Effect::Refresh(tags) => self.reread(&tags, services, context),
                library::Effect::Toast(text) => self.toast(Kind::Error, text),
                library::Effect::Inform(text) => self.toast(Kind::Info, text),
                library::Effect::Recent(uuid) => Task::done(app::Message::RecentPlaylist(uuid)),
                library::Effect::Cover(uuid) => match self.user_id {
                    Some(user_id) => {
                        let forget = services.catalog.forget_playlist(user_id, &uuid);
                        Task::perform(
                            async move {
                                tokio::time::sleep(COVER_DELAY).await;
                                forget.await;
                            },
                            move |()| {
                                app::Message::Shell(Message::Reread(library::playlist_tags(&uuid)))
                            },
                        )
                    }
                    None => Task::none(),
                },
                library::Effect::Pick { tracks, at } => self.open_picker(tracks, at, services),
                library::Effect::Ask(ask) => {
                    self.picker = None;
                    self.dialog = Some(Dialog::new(ask, &self.library));
                    Task::none()
                }
                library::Effect::Deleted(uuid) => self.playlist_deleted(&uuid, services, context),
                library::Effect::SourceDeleted(source) => Task::done(app::Message::Playback(
                    playback::Message::SourceDeleted(source),
                )),
            };
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Open "Add to playlist" beside `at`, reading the Own playlists and
    /// a card's tracks.
    fn open_picker(
        &mut self,
        tracks: library::Tracks,
        at: iced::Rectangle,
        services: &Services,
    ) -> Task<app::Message> {
        let Some(user_id) = self.user_id else {
            return Task::none();
        };
        let to_picker = |message| app::Message::Shell(Message::Picker(message));
        let (mut picker, card) = match tracks {
            library::Tracks::These(tracks) => (Picker::new(Some(tracks), at), None),
            library::Tracks::Card(card) => (Picker::new(None, at), Some(card)),
        };
        let (playlists, handle) = Task::run(services.catalog.own_playlists(user_id), move |read| {
            to_picker(picker::Message::Playlists(read))
        })
        .abortable();
        picker.reading(handle);
        let tracks = match card {
            Some(card) => {
                let (task, handle) = Task::perform(
                    play_card::tracks(card, &services.catalog, Some(user_id)),
                    move |result| to_picker(picker::Message::Tracks(result)),
                )
                .abortable();
                picker.reading(handle);
                task
            }
            None => Task::none(),
        };
        self.picker = Some(picker);
        Task::batch([playlists, tracks, Picker::focus()])
    }

    /// What's read under `tags` is stale: the lists on screen read it
    /// again.
    fn reread(
        &mut self,
        tags: &[String],
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let mut reads = Vec::new();
        for effect in self.sidebar.refresh(tags, &mut self.library) {
            reads.push(self.run_sidebar(effect, services, context));
        }
        let action = self.current.page.refresh(tags);
        reads.push(self.run_action(action, services, context));
        Task::batch(reads)
    }

    /// A playlist is gone: its Pages leave back and forward, and if it's
    /// on screen, Home takes its place.
    fn playlist_deleted(
        &mut self,
        uuid: &str,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let shows =
            |route: &Route| matches!(route, Route::Playlist { uuid: shown, .. } if shown == uuid);
        self.back_stack.remove(shows);
        if !shows(&self.current.route) {
            return Task::none();
        }
        let home = Entry {
            route: Route::home(),
            offset: 0.0,
        };
        self.open(home, services, context)
    }

    fn run_drawer(
        &mut self,
        effect: drawer::Effect,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let to_drawer = |message| app::Message::Shell(Message::Drawer(message));
        let (task, handle) = match effect {
            drawer::Effect::None => return Task::none(),
            drawer::Effect::Playback(message) => {
                return Task::done(app::Message::Playback(message));
            }
            drawer::Effect::Link(link) => return self.run_action(link.follow(), services, context),
            drawer::Effect::Maximize => {
                self.maximized = true;
                return Task::none();
            }
            drawer::Effect::ReadSuggested(track) => {
                let id = track.id;
                Task::perform(
                    radio::fetch(services.catalog.clone(), track),
                    move |result| to_drawer(drawer::Message::Suggested(id, result)),
                )
                .abortable()
            }
            drawer::Effect::ReadLyrics(id) => {
                Task::perform(services.catalog.lyrics(id), move |result| {
                    to_drawer(drawer::Message::Lyrics(id, result))
                })
                .abortable()
            }
            drawer::Effect::Measure(line) => {
                return drawer::measure()
                    .map(move |measure| to_drawer(drawer::Message::Measured(line, measure)));
            }
            drawer::Effect::ScrollTo(offset) => {
                return operation::scroll_to(
                    drawer::SCROLL,
                    scrollable::AbsoluteOffset { x: 0.0, y: offset },
                );
            }
            drawer::Effect::ReadCredits(id) => {
                Task::perform(services.catalog.credits(id), move |result| {
                    to_drawer(drawer::Message::Credits(id, result))
                })
                .abortable()
            }
        };
        self.drawer.reading(handle);
        task
    }

    fn run_sidebar(
        &mut self,
        effect: sidebar::Effect,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let to_shell = |message| app::Message::Shell(Message::Sidebar(message));
        match effect {
            sidebar::Effect::None => Task::none(),
            sidebar::Effect::Read(shelf) => {
                let note = Some((self.library.start_read(), shelf.tags()));
                let reads = services.catalog.library(&shelf);
                let reads = noting(reads, is_fresh, note, move |read| {
                    to_shell(sidebar::Message::Items {
                        shelf: shelf.clone(),
                        read,
                    })
                });
                Task::run(reads, std::convert::identity)
            }
            sidebar::Effect::More {
                shelf,
                offset,
                cursor,
            } => Task::perform(
                services.catalog.more_library(&shelf, offset, cursor),
                move |result| {
                    to_shell(sidebar::Message::More {
                        shelf: shelf.clone(),
                        offset,
                        result,
                    })
                },
            ),
            sidebar::Effect::Link(link) => self.run_action(link.follow(), services, context),
        }
    }

    fn run_search(
        &mut self,
        effect: search::Effect,
        services: &Services,
        context: &Context,
    ) -> Task<app::Message> {
        let to_shell = |message| app::Message::Shell(Message::Search(message));
        match effect {
            search::Effect::None => Task::none(),
            search::Effect::Wait(typed) => {
                Task::perform(tokio::time::sleep(search::DEBOUNCE), move |()| {
                    to_shell(search::Message::Waited(typed))
                })
            }
            search::Effect::Fetch(query) => {
                let (task, handle) =
                    Task::perform(services.catalog.suggestions(&query), move |suggestions| {
                        to_shell(search::Message::Arrived {
                            query: query.clone(),
                            suggestions,
                        })
                    })
                    .abortable();
                self.search.fetching(handle);
                task
            }
            search::Effect::Search(query) => {
                let route = Route::Search {
                    query: query.clone(),
                    tab: search_page::Tab::All,
                };
                let open = self.navigate(route, services, context);
                Task::batch([Task::done(app::Message::Searched(query)), open])
            }
            search::Effect::Forget(query) => Task::done(app::Message::ForgetSearch(query)),
            search::Effect::Follow(link) => self.run_action(link.follow(), services, context),
            search::Effect::Focus => Search::focus(),
        }
    }

    /// Whether the Page shows the track in a list, where its row animates
    /// while it plays.
    /// `track_id` is the current track's: the drawer's Queue tab shows it
    /// too.
    pub fn shows_track(&self, track_id: u64) -> bool {
        self.drawer.shows_current() || self.current.page.shows_track(track_id)
    }

    /// Whether the drawer slides, so frames are wanted.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.drawer.is_animating(now)
    }

    /// `settings` has the user's past searches, for the search dropdown.
    /// `clock` is the seconds animations run on, and `now` the last frame.
    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        settings: &'a Settings,
        playback: &'a Playback,
        clock: f32,
        now: Instant,
    ) -> Element<'a, app::Message> {
        let id = self.current.id;
        let now_playing = playback
            .current()
            .zip(playback.playing_from())
            .map(|(track, from)| NowPlaying {
                track_id: track.id,
                playing: (playback.status() == Status::Playing).then_some(clock),
                source: &from.kind,
            });
        let viewport = Viewport {
            offset: self.current.offset,
            height: self.viewport_height,
        };
        let page = container(
            self.current
                .page
                .view(
                    images,
                    &self.library,
                    viewport,
                    now_playing,
                    playback.allow_explicit(),
                )
                .map(move |m| app::Message::Page(id, m)),
        )
        .max_width(MAX_PAGE_WIDTH)
        .width(Length::Fill);
        let page = scrollable(container(page).center_x(Length::Fill))
            .id(PAGE_SCROLL)
            .on_scroll(move |viewport| {
                app::Message::Shell(Message::Scrolled(
                    id,
                    Viewport {
                        offset: viewport.absolute_offset().y,
                        height: viewport.bounds().height,
                    },
                ))
            })
            .width(Length::Fill)
            .height(Length::Fill);

        let sidebar = self
            .sidebar
            .view(
                images,
                &self.library,
                &self.current.route,
                self.unseen.any(),
            )
            .map(|message| app::Message::Shell(Message::Sidebar(message)));
        // The dropdown hangs over the Page, under the search field.
        let dropdown = self
            .search
            .view(&settings.search_history, images)
            .map(|dropdown| {
                let left = HEADER_PADDING + 2.0 * (STEP_SIZE + HEADER_SPACING);
                container(dropdown.map(|message| app::Message::Shell(Message::Search(message))))
                    .padding(iced::Padding::new(0.0).left(left))
            });
        // Only once there's something to play.
        let player_bar = playback.current().map(|_| {
            self.player_bar
                .view(playback, images, &self.library, self.drawer.showing())
                .map(|message| app::Message::Shell(Message::PlayerBar(message)))
        });
        // The drawer covers everything above the player bar.
        let drawer = self
            .drawer
            .view(playback, images, &self.library, clock, now)
            .map(|drawer| drawer.map(|message| app::Message::Shell(Message::Drawer(message))));
        let main = column![
            stack![row![
                sidebar,
                column![self.header(), stack![page].push(dropdown)]
            ]]
            .push(drawer)
            .height(Length::Fill),
        ]
        .push(player_bar);
        let maximized = self
            .maximized
            .then(|| maximized::view(&self.player_bar, playback, images))
            .flatten()
            .map(|view| view.map(|message| app::Message::Shell(Message::Maximized(message))));
        let modal = match (&self.consent, &self.settings, &self.dialog) {
            (Some(_), _, _) => {
                Some(consent::view().map(|answer| app::Message::Shell(Message::Consent(answer))))
            }
            (None, Some(modal), _) => Some(
                modal
                    .view(settings, playback)
                    .map(|message| app::Message::Shell(Message::Settings(message))),
            ),
            (None, None, Some(dialog)) => Some(
                dialog
                    .view()
                    .map(|message| app::Message::Shell(Message::Dialog(message))),
            ),
            (None, None, None) => None,
        };
        let toasts = self
            .toasts
            .view()
            .map(|id| app::Message::Shell(Message::DismissToast(id)));
        let picker = self.picker.as_ref().map(|picker| {
            picker
                .view(&self.library, &settings.recent_playlists)
                .map(|message| app::Message::Shell(Message::Picker(message)))
        });
        stack![main]
            .push(maximized)
            .push(picker)
            .push(modal)
            .push(toasts)
            .into()
    }

    /// Back and forward, search and the avatar.
    fn header(&self) -> Element<'_, app::Message> {
        let step = |glyph, message: Option<app::Message>| {
            let color = match message {
                Some(_) => style::TEXT_PRIMARY,
                None => style::TEXT_DISABLED,
            };
            button(container(icon(glyph, 20.0, color)).center(STEP_SIZE))
                .padding(0)
                .on_press_maybe(message)
                .style(style::icon_button)
        };
        let back = step(
            Icon::ChevronLeft,
            self.back_stack.can_go_back().then_some(app::Message::Back),
        );
        let forward = step(
            Icon::ChevronRight,
            self.back_stack
                .can_go_forward()
                .then_some(app::Message::Forward),
        );
        let search = self
            .search
            .field()
            .map(|message| app::Message::Shell(Message::Search(message)));
        let settings =
            button(container(icon(Icon::Settings, 20.0, style::TEXT_SECONDARY)).center(STEP_SIZE))
                .padding(0)
                .style(style::icon_button)
                .on_press(app::Message::Shell(Message::OpenSettings));
        let avatar = button(page::no_picture(STEP_SIZE))
            .padding(0)
            .style(style::icon_button)
            .on_press_maybe(
                self.user_id
                    .map(|_| app::Message::Shell(Message::OpenProfile)),
            );

        container(
            row![back, forward, search, space::horizontal(), settings, avatar]
                .spacing(HEADER_SPACING)
                .align_y(Alignment::Center),
        )
        .padding([0.0, HEADER_PADDING])
        .width(Length::Fill)
        .center_y(HEADER_HEIGHT)
        .into()
    }
}

impl Current {
    fn open(id: PageId, entry: Entry, context: &Context) -> (Self, Action) {
        let (page, action) = Page::open(&entry.route, context);
        let current = Self {
            id,
            route: entry.route,
            page,
            loads: Vec::new(),
            offset: entry.offset,
        };
        (current, action)
    }

    /// Where the user is, to come back to.
    fn entry(&self) -> Entry {
        Entry {
            route: self.route.clone(),
            offset: self.offset,
        }
    }
}

/// A sample rate in kHz, as 44.1 or 96.
fn khz(hz: u32) -> String {
    format!("{}", f64::from(hz) / 1000.0)
}

/// The most of an audio error a toast shows.
const AUDIO_ERROR_LENGTH: usize = 80;

/// `text`, cut to `max` characters with an ellipsis if it's longer.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let cut: String = text.chars().take(max - 1).collect();
        format!("{cut}\u{2026}")
    } else {
        text.to_string()
    }
}

fn play(request: PlayRequest) -> Task<app::Message> {
    Task::done(app::Message::Playback(playback::Message::Start(request)))
}

/// The drawer's tab, back at its top.
fn drawer_to_top() -> Task<app::Message> {
    operation::scroll_to(
        drawer::SCROLL,
        scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
    )
}

fn scroll_to(offset: f32) -> Task<app::Message> {
    operation::scroll_to(
        PAGE_SCROLL,
        scrollable::AbsoluteOffset { x: 0.0, y: offset },
    )
}

/// Run a Page's Catalog read, as that Page's messages.
fn read(load: Load, catalog: &Catalog) -> BoxStream<'static, page::Message> {
    match load {
        Load::HomeFeed(tab) => home_feed(catalog.home_feed(&tab), tab),
        Load::RefreshHomeFeed(tab) => home_feed(catalog.refresh_home_feed(&tab), tab),
        Load::MoreHomeFeed { tab, cursor } => {
            let more = catalog.more_home_feed(&tab, &cursor);
            stream::once(more)
                .map(move |result| {
                    page::Message::Home(page::home::Message::More {
                        cursor: cursor.clone(),
                        result,
                    })
                })
                .boxed()
        }
        Load::Album(id) => catalog
            .album(id)
            .map(|read| page::Message::Album(album::Message::Loaded(read)))
            .boxed(),
        Load::Artist { id, then } => catalog.artist(id).map(then).boxed(),
        Load::ArtistTracks(id) => catalog
            .artist_tracks(id)
            .map(|read| page::Message::ArtistTracks(artist_tracks::Message::Tracks(read)))
            .boxed(),
        Load::MoreArtistTracks { id, offset } => {
            stream::once(catalog.more_artist_tracks(id, offset))
                .map(move |result| {
                    page::Message::ArtistTracks(artist_tracks::Message::More { offset, result })
                })
                .boxed()
        }
        Load::ArtistViewAll { id, section } => catalog
            .artist_view_all(id, &section)
            .map(move |read| {
                page::Message::ArtistViewAll(artist_view_all::Message::Cards {
                    section: section.clone(),
                    read,
                })
            })
            .boxed(),
        Load::MoreArtistViewAll {
            id,
            section,
            offset,
        } => stream::once(catalog.more_artist_view_all(id, &section, offset))
            .map(move |result| {
                page::Message::ArtistViewAll(artist_view_all::Message::More {
                    section: section.clone(),
                    offset,
                    result,
                })
            })
            .boxed(),
        Load::Mix(id) => catalog
            .mix(&id)
            .map(|read| page::Message::Mix(mix::Message::Loaded(read)))
            .boxed(),
        Load::Playlist(uuid) => catalog
            .playlist(&uuid)
            .map(|read| page::Message::Playlist(playlist::Message::Playlist(read)))
            .boxed(),
        Load::PlaylistTracks { uuid, sort } => catalog
            .playlist_tracks(&uuid, sort)
            .map(move |read| page::Message::Playlist(playlist::Message::Tracks { sort, read }))
            .boxed(),
        Load::MorePlaylistTracks { uuid, sort, offset } => {
            stream::once(catalog.more_playlist_tracks(&uuid, sort, offset))
                .map(move |result| {
                    page::Message::Playlist(playlist::Message::More {
                        sort,
                        offset,
                        result,
                    })
                })
                .boxed()
        }
        Load::PlaylistRecommendations { uuid, offset } => {
            stream::once(catalog.playlist_recommendations(&uuid, offset))
                .map(move |result| {
                    page::Message::Playlist(playlist::Message::Recommendations { offset, result })
                })
                .boxed()
        }
        Load::Library(shelf) => catalog
            .library(&shelf)
            .map(move |read| {
                page::Message::Library(library_page::Message::Items {
                    shelf: shelf.clone(),
                    read,
                })
            })
            .boxed(),
        Load::MoreLibrary {
            shelf,
            offset,
            cursor,
        } => stream::once(catalog.more_library(&shelf, offset, cursor))
            .map(move |result| {
                page::Message::Library(library_page::Message::More {
                    shelf: shelf.clone(),
                    offset,
                    result,
                })
            })
            .boxed(),
        Load::LovedTracks { user_id, sort } => catalog
            .loved_tracks(user_id, sort)
            .map(move |read| page::Message::Favorites(favorites::Message::Tracks { sort, read }))
            .boxed(),
        Load::MoreLovedTracks {
            user_id,
            sort,
            offset,
        } => stream::once(catalog.more_loved_tracks(user_id, sort, offset))
            .map(move |result| {
                page::Message::Favorites(favorites::Message::More {
                    sort,
                    offset,
                    result,
                })
            })
            .boxed(),
        Load::Search(query) => stream::once(catalog.search(&query))
            .map(|result| page::Message::Search(search_page::Message::Loaded(result)))
            .boxed(),
        Load::Explore(path) => catalog
            .explore(&path)
            .map(|read| page::Message::Explore(explore::Message::Loaded(read)))
            .boxed(),
        Load::Feed(user_id) => stream::once(catalog.feed(user_id))
            .map(|result| page::Message::Feed(feed::Message::Loaded(result)))
            .boxed(),
        Load::Profile { user_id, then } => catalog.profile(user_id).map(then).boxed(),
    }
}

/// A read's values as app messages through `wrap`. With a `note` (the
/// read's stamp and tags), each value `fresh` says is TIDAL's answer is
/// followed by word to the Library, which settles the edits it shows.
fn noting<T: Send + 'static>(
    reads: BoxStream<'static, T>,
    fresh: fn(&T) -> bool,
    note: Option<(Stamp, Vec<String>)>,
    wrap: impl Fn(T) -> app::Message + Send + 'static,
) -> BoxStream<'static, app::Message> {
    reads
        .flat_map(move |value| {
            let settled = note
                .as_ref()
                .filter(|_| fresh(&value))
                .map(|(stamp, tags)| {
                    app::Message::Shell(Message::Library(library::Message::Fresh(
                        *stamp,
                        tags.clone(),
                    )))
                });
            stream::iter(std::iter::once(wrap(value)).chain(settled))
        })
        .boxed()
}

/// Whether a read brought TIDAL's answer.
fn is_fresh<T>(read: &Read<T>) -> bool {
    matches!(read, Read::Fresh(Ok(_)))
}

/// Whether a Page's message brings TIDAL's answer to one of its Library
/// reads.
fn page_fresh(message: &page::Message) -> bool {
    match message {
        page::Message::Library(library_page::Message::Items { read, .. }) => is_fresh(read),
        page::Message::Favorites(favorites::Message::Tracks { read, .. }) => is_fresh(read),
        _ => false,
    }
}

/// A Home feed tab's reads, as Home's messages for that tab.
fn home_feed(
    reads: BoxStream<'static, Read<HomeFeed>>,
    tab: String,
) -> BoxStream<'static, page::Message> {
    reads
        .map(move |read| {
            page::Message::Home(page::home::Message::Feed {
                tab: tab.clone(),
                read,
            })
        })
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_audio_error_is_cut_to_80_characters() {
        let error = "x".repeat(100);

        let shown = cut(&error, AUDIO_ERROR_LENGTH);

        assert_eq!(shown.chars().count(), 80);
        assert!(shown.ends_with('\u{2026}'));
        assert_eq!(cut("no sink", AUDIO_ERROR_LENGTH), "no sink");
    }
}
