//! The signed-in Shell: the sidebar, the header, the current Page and the
//! Back stack, and the toasts over them.

mod back_stack;
pub mod toast;

use futures::StreamExt;
use futures::stream::{self, BoxStream};
use iced::task;
use iced::widget::{button, column, container, operation, row, scrollable, space, stack, text};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use syzygy_catalog::{Catalog, HomeFeed, Read};

use crate::app::{self, Services};
use crate::identity::DISPLAY_NAME;
use crate::page::{self, Action, Load, Page, PageId, Route};
use back_stack::{BackStack, Entry};
use toast::{Kind, ToastId, Toasts};

const SIDEBAR_WIDTH: f32 = 280.0;
const HEADER_HEIGHT: f32 = 64.0;
/// Pages stop growing past this on wide windows.
const MAX_PAGE_WIDTH: f32 = 1520.0;
/// The scrollable every Page draws in.
const PAGE_SCROLL: iced::widget::Id = iced::widget::Id::new("page");

pub struct Shell {
    back_stack: BackStack,
    current: Current,
    next_id: u64,
    toasts: Toasts,
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
    Scrolled(PageId, f32),
    DismissToast(ToastId),
}

impl Shell {
    /// The Shell at Home, with an empty Back stack.
    pub fn new(services: &Services) -> (Self, Task<app::Message>) {
        let (current, action) = Current::open(
            PageId(0),
            Entry {
                route: Route::home(),
                offset: 0.0,
            },
        );
        let mut shell = Self {
            back_stack: BackStack::default(),
            current,
            next_id: 1,
            toasts: Toasts::default(),
        };
        let task = shell.run_action(action, services);
        (shell, task)
    }

    /// Go somewhere new. The current Page goes on the Back stack.
    pub fn navigate(&mut self, route: Route, services: &Services) -> Task<app::Message> {
        if route == self.current.route {
            return scroll_to(0.0);
        }
        self.back_stack.push(self.current.entry());
        self.open(Entry { route, offset: 0.0 }, services)
    }

    pub fn back(&mut self, services: &Services) -> Task<app::Message> {
        let entry = self.back_stack.back(self.current.entry());
        self.open_maybe(entry, services)
    }

    pub fn forward(&mut self, services: &Services) -> Task<app::Message> {
        let entry = self.back_stack.forward(self.current.entry());
        self.open_maybe(entry, services)
    }

    /// A message for a Page. Dropped unless that Page is still showing.
    pub fn update_page(
        &mut self,
        id: PageId,
        message: page::Message,
        services: &Services,
    ) -> Task<app::Message> {
        if id != self.current.id {
            return Task::none();
        }
        let action = self.current.page.update(message);
        self.run_action(action, services)
    }

    /// The window came back into focus.
    pub fn focused(&mut self, services: &Services) -> Task<app::Message> {
        let action = self.current.page.focused();
        self.run_action(action, services)
    }

    pub fn update(&mut self, message: Message) -> Task<app::Message> {
        match message {
            Message::Scrolled(id, offset) => {
                if id == self.current.id {
                    self.current.offset = offset;
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

    fn open_maybe(&mut self, entry: Option<Entry>, services: &Services) -> Task<app::Message> {
        match entry {
            Some(entry) => self.open(entry, services),
            None => Task::none(),
        }
    }

    /// Show the Page for `entry` under a new id, replacing the current one
    /// and aborting its reads. The scrollable keeps the offset and clamps it
    /// to the content, so it lands once the Page's data is tall enough.
    fn open(&mut self, entry: Entry, services: &Services) -> Task<app::Message> {
        let offset = entry.offset;
        let (current, action) = Current::open(PageId(self.next_id), entry);
        self.current = current;
        self.next_id += 1;
        let task = self.run_action(action, services);
        Task::batch([task, scroll_to(offset)])
    }

    fn run_action(&mut self, action: Action, services: &Services) -> Task<app::Message> {
        match action {
            Action::None => Task::none(),
            Action::Navigate(route) => self.navigate(route, services),
            Action::Run(task) => {
                let id = self.current.id;
                task.map(move |message| app::Message::Page(id, message))
            }
            Action::Replace(route, load) => {
                self.current.route = route;
                self.current.offset = 0.0;
                let load = self.run_action(Action::Load(load), services);
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

    pub fn view(&self) -> Element<'_, app::Message> {
        let id = self.current.id;
        let page = container(
            self.current
                .page
                .view()
                .map(move |m| app::Message::Page(id, m)),
        )
        .max_width(MAX_PAGE_WIDTH)
        .width(Length::Fill);
        let page = scrollable(container(page).center_x(Length::Fill))
            .id(PAGE_SCROLL)
            .on_scroll(move |viewport| {
                app::Message::Shell(Message::Scrolled(id, viewport.absolute_offset().y))
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
        let back = button(text("‹").size(20))
            .on_press_maybe(self.back_stack.can_go_back().then_some(app::Message::Back))
            .style(button::text);
        let forward = button(text("›").size(20))
            .on_press_maybe(
                self.back_stack
                    .can_go_forward()
                    .then_some(app::Message::Forward),
            )
            .style(button::text);
        let search = container(text("Search").size(14).style(text::secondary))
            .padding([8, 14])
            .width(320)
            .style(search_box);
        let avatar = container(space()).width(32).height(32).style(avatar);

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
    fn open(id: PageId, entry: Entry) -> (Self, Action) {
        let (page, action) = Page::open(&entry.route);
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
    let home = button(text("Home"))
        .on_press(app::Message::Navigate(Route::home()))
        .style(button::text)
        .width(Length::Fill);
    let body = column![
        text(DISPLAY_NAME).size(22),
        home,
        text("Your Library").size(13).style(text::secondary),
    ]
    .spacing(16);
    container(body)
        .padding(16)
        .width(SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(sidebar_frame)
        .into()
}

fn sidebar_frame(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weakest.color.into()),
        ..container::Style::default()
    }
}

fn search_box(theme: &Theme) -> container::Style {
    placeholder(theme, 18.0)
}

fn avatar(theme: &Theme) -> container::Style {
    placeholder(theme, 16.0)
}

fn placeholder(theme: &Theme, radius: f32) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius.into(),
        },
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
