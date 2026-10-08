//! The signed-in Shell: the sidebar, the header with its search, the
//! current Page and the Back stack, and the toasts over them.

mod back_stack;
pub mod player_bar;
pub mod search;
pub mod sidebar;
pub mod toast;
mod unseen;

use futures::StreamExt;
use futures::stream::{self, BoxStream};
use iced::task;
use iced::widget::{button, column, container, operation, row, scrollable, space, stack};
use iced::{Alignment, Element, Length, Task};
use std::sync::Arc;
use syzygy_catalog::{Catalog, Feed, HomeFeed, Read};

use crate::app::{self, Services};
use crate::icons::{Icon, icon};
use crate::images::{self, Images};
use crate::page::{
    self, Action, Context, Load, NowPlaying, Page, PageId, Route, Viewport, album, artist_tracks,
    artist_view_all, explore, favorites, feed, library, mix, playlist, search as search_page,
};
use crate::playback::{self, Playback, Status};
use crate::settings::Sort;
use crate::style;
use back_stack::{BackStack, Entry};
use player_bar::PlayerBar;
use search::Search;
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
    next_id: u64,
    toasts: Toasts,
    /// Whether the Feed has something the user hasn't seen.
    unseen: Unseen,
    /// How tall the Page's viewport is, as last reported.
    viewport_height: f32,
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
    Sidebar(sidebar::Message),
    Search(search::Message),
    PlayerBar(player_bar::Message),
    /// The avatar: the user's own Profile.
    OpenProfile,
    /// The Feed check after login.
    FeedChecked(Result<Feed, Arc<syzygy_catalog::Error>>),
    /// The Feed was marked seen, or not.
    FeedSeen(Result<(), Arc<syzygy_catalog::Error>>),
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
        let (sidebar, read) = Sidebar::new(context.user_id, context.settings);
        let mut shell = Self {
            user_id: context.user_id,
            back_stack: BackStack::default(),
            current,
            sidebar,
            search: Search::default(),
            player_bar: PlayerBar::default(),
            next_id: 1,
            toasts: Toasts::default(),
            unseen: Unseen::default(),
            viewport_height: UNKNOWN_HEIGHT,
        };
        let task = shell.run_action(action, services, context);
        let read = shell.run_sidebar(read, services, context);
        let check = shell.check_feed(context.user_id, services);
        (shell, Task::batch([task, read, check]))
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
        let effect = self.sidebar.user_known(user_id);
        let read = self.run_sidebar(effect, services, context);
        Task::batch([read, seen, self.check_feed(Some(user_id), services)])
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
        let action = self.current.page.update(message);
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
            Message::Sidebar(message) => {
                let effect = self.sidebar.update(message);
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
                    player_bar::Effect::Link(link) => {
                        self.run_action(link.follow(), services, context)
                    }
                };
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
        }
        Task::none()
    }

    /// Show a toast that dismisses itself.
    #[expect(dead_code, reason = "Library edits and playback show the first toasts")]
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
                        let effect = self.sidebar.sorted(*kind, *library_sort);
                        self.run_sidebar(effect, services, context)
                    }
                    _ => Task::none(),
                };
                Task::batch([Task::done(app::Message::Sort(sort)), resort])
            }
            Action::Navigate(route) => self.navigate(route, services, context),
            Action::Play(request) => {
                Task::done(app::Message::Playback(playback::Message::Start(request)))
            }
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
                let (task, handle) = Task::run(read(load, &services.catalog), move |message| {
                    app::Message::Page(id, message)
                })
                .abortable();
                self.current.loads.push(handle.abort_on_drop());
                task
            }
        }
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
                Task::run(services.catalog.library(&shelf), move |read| {
                    to_shell(sidebar::Message::Items {
                        shelf: shelf.clone(),
                        read,
                    })
                })
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
    pub fn shows_track(&self, track_id: u64) -> bool {
        self.current.page.shows_track(track_id)
    }

    /// `past_searches` are the user's, for the search dropdown. `clock` is
    /// the seconds animations run on.
    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        past_searches: &'a [String],
        playback: &'a Playback,
        clock: f32,
    ) -> Element<'a, app::Message> {
        let id = self.current.id;
        let now_playing = playback.current().map(|track| NowPlaying {
            track_id: track.id,
            playing: (playback.status() == Status::Playing).then_some(clock),
        });
        let viewport = Viewport {
            offset: self.current.offset,
            height: self.viewport_height,
        };
        let page = container(
            self.current
                .page
                .view(images, viewport, now_playing)
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
            .view(images, &self.current.route, self.unseen.any())
            .map(|message| app::Message::Shell(Message::Sidebar(message)));
        // The dropdown hangs over the Page, under the search field.
        let dropdown = self.search.view(past_searches, images).map(|dropdown| {
            let left = HEADER_PADDING + 2.0 * (STEP_SIZE + HEADER_SPACING);
            container(dropdown.map(|message| app::Message::Shell(Message::Search(message))))
                .padding(iced::Padding::new(0.0).left(left))
        });
        // Only once there's something to play.
        let player_bar = playback.current().map(|_| {
            self.player_bar
                .view(playback, images)
                .map(|message| app::Message::Shell(Message::PlayerBar(message)))
        });
        let main = column![
            row![sidebar, column![self.header(), stack![page].push(dropdown)]].height(Length::Fill),
        ]
        .push(player_bar);
        let toasts = self
            .toasts
            .view()
            .map(|id| app::Message::Shell(Message::DismissToast(id)));
        stack![main, toasts].into()
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
        let avatar = button(page::no_picture(STEP_SIZE))
            .padding(0)
            .style(style::icon_button)
            .on_press_maybe(
                self.user_id
                    .map(|_| app::Message::Shell(Message::OpenProfile)),
            );

        container(
            row![back, forward, search, space::horizontal(), avatar]
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
                page::Message::Library(library::Message::Items {
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
                page::Message::Library(library::Message::More {
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
