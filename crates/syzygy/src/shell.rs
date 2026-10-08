//! The signed-in Shell: the sidebar, the header, the current Page and the
//! Back stack, and the toasts over them.

mod back_stack;
pub mod toast;

use futures::StreamExt;
use futures::stream::{self, BoxStream};
use iced::task;
use iced::widget::{button, column, container, operation, row, scrollable, space, stack, text};
use iced::{Alignment, Element, Length, Task, Theme};
use syzygy_catalog::{Catalog, HomeFeed, Read};

use crate::app::{self, Services};
use crate::icons::{Icon, icon};
use crate::identity::DISPLAY_NAME;
use crate::images::{self, Images};
use crate::page::{
    self, Action, Context, Load, Page, PageId, Route, Viewport, album, artist_tracks,
    artist_view_all, mix, playlist,
};
use crate::style;
use back_stack::{BackStack, Entry};
use toast::{Kind, ToastId, Toasts};

const SIDEBAR_WIDTH: f32 = 280.0;
const HEADER_HEIGHT: f32 = 64.0;
/// Pages stop growing past this on wide windows.
const MAX_PAGE_WIDTH: f32 = 1520.0;
/// The scrollable every Page draws in.
const PAGE_SCROLL: iced::widget::Id = iced::widget::Id::new("page");
/// How tall the Page's viewport is taken to be until it's reported:
/// tall enough that a short list is built in full on any screen, since a
/// scrollable whose content fits never reports.
const UNKNOWN_HEIGHT: f32 = 4000.0;

pub struct Shell {
    back_stack: BackStack,
    current: Current,
    next_id: u64,
    toasts: Toasts,
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
        let mut shell = Self {
            back_stack: BackStack::default(),
            current,
            next_id: 1,
            toasts: Toasts::default(),
            viewport_height: UNKNOWN_HEIGHT,
        };
        let task = shell.run_action(action, services, context);
        (shell, task)
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

    pub fn update(&mut self, message: Message) -> Task<app::Message> {
        match message {
            Message::Scrolled(id, viewport) => {
                self.viewport_height = viewport.height;
                if id == self.current.id {
                    self.current.offset = viewport.offset;
                }
            }
            Message::DismissToast(id) => self.toasts.dismiss(id),
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
        let (current, action) = Current::open(PageId(self.next_id), entry, context);
        self.current = current;
        self.next_id += 1;
        let task = self.run_action(action, services, context);
        Task::batch([task, scroll_to(offset)])
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
            Action::SaveTrackSort(uuid, sort) => Task::done(app::Message::TrackSort(uuid, sort)),
            Action::Navigate(route) => self.navigate(route, services, context),
            Action::Run(task) => {
                let id = self.current.id;
                task.map(move |message| app::Message::Page(id, message))
            }
            Action::Replace(route, load) => {
                self.current.route = route;
                self.current.offset = 0.0;
                let load = self.run_action(Action::Load(load), services, context);
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

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, app::Message> {
        let id = self.current.id;
        let viewport = Viewport {
            offset: self.current.offset,
            height: self.viewport_height,
        };
        let page = container(
            self.current
                .page
                .view(images, viewport)
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

        let main = row![sidebar(), column![self.header(), page]];
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
            button(container(icon(glyph, 20.0, color)).center(32))
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
        let search = container(
            row![
                icon(Icon::Search, 16.0, style::TEXT_MUTED),
                text("Search").size(14).color(style::TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 14])
        .width(320)
        .style(search_box);
        let avatar = images::placeholder(32.0, 16.0);

        container(
            row![back, forward, search, space::horizontal(), avatar]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding([0, 16])
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

/// The sidebar frame. The Library lists land here.
fn sidebar<'a>() -> Element<'a, app::Message> {
    let home = button(
        row![
            icon(Icon::House, 20.0, style::TEXT_PRIMARY),
            text("Home").size(14)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .on_press(app::Message::Navigate(Route::home()))
    .style(nav_item)
    .width(Length::Fill);
    let library = row![
        icon(Icon::Library, 20.0, style::TEXT_SECONDARY),
        text("Your Library").size(13).color(style::TEXT_SECONDARY),
    ]
    .spacing(12)
    .padding([0, 12])
    .align_y(Alignment::Center);
    let body = column![text(DISPLAY_NAME).size(22), home, library].spacing(16);
    container(body)
        .padding(16)
        .width(SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(sidebar_frame)
        .into()
}

fn sidebar_frame(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_SIDEBAR.into()),
        ..container::Style::default()
    }
}

fn nav_item(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(style::HL_FAINT.into()),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background,
        text_color: style::TEXT_PRIMARY,
        border: style::rounded(6.0),
        ..button::Style::default()
    }
}

fn search_box(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_INSET.into()),
        border: style::rounded(18.0),
        ..container::Style::default()
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
