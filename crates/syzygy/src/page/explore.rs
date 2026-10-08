//! Explore, and the Pages under it that it links to: genres, moods,
//! decades and TIDAL's editorial pages. Each is read by its path, and its
//! sections either link on to more of them or hold cards.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Element, Theme};
use syzygy_catalog::explore::{self, PageLink, Section};
use syzygy_catalog::{ExplorePage, Read};

use super::cards::{self, Rows};
use super::{Action, Link, Load, PADDING, Remote, Route};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::style;

/// The width of each link in a list of nothing but links.
const LINK_WIDTH: f32 = 200.0;
/// The width of each shortcut under Explore's links.
const SHORTCUT_WIDTH: f32 = 240.0;

pub struct State {
    /// Where the Page is read from.
    path: String,
    /// `None` for Explore itself.
    title: Option<String>,
    page: Remote<ExplorePage>,
    /// The card rows' scroll state, by section index.
    rows: Rows,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Read<ExplorePage>),
    Cards(cards::Message),
    Link(Link),
    Retry,
}

impl State {
    /// Explore itself.
    pub fn root() -> (Self, Action) {
        Self::new(explore::ROOT.to_string(), None)
    }

    /// The Explore Page at `path`, called `title`.
    pub fn page(path: String, title: String) -> (Self, Action) {
        Self::new(path, Some(title))
    }

    fn new(path: String, title: Option<String>) -> (Self, Action) {
        let action = Action::Load(Load::Explore(path.clone()));
        let state = Self {
            path,
            title,
            page: Remote::Loading,
            rows: Rows::default(),
        };
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Loaded(read) => {
                self.page.apply(read, "an Explore Page");
                Action::None
            }
            Message::Cards(message) => {
                let page = self.page.loaded();
                let cards = |index: usize| match page?.sections.get(index)? {
                    Section::Cards { cards, .. } => Some(cards.len()),
                    Section::Links { .. } => None,
                };
                self.rows.update(message, cards, |message| {
                    super::Message::Explore(Message::Cards(message))
                })
            }
            Message::Link(link) => link.follow(),
            Message::Retry => {
                self.page = Remote::Loading;
                Action::Load(Load::Explore(self.path.clone()))
            }
        }
    }

    pub fn view<'a>(&'a self, images: &'a Images) -> Element<'a, Message> {
        let title = text(self.title.as_deref().unwrap_or("Explore")).size(32);
        let body = self.page.view(Message::Retry, |page| {
            if page.sections.is_empty() {
                return text("Nothing to show here yet")
                    .style(text::secondary)
                    .into();
            }
            // Explore itself is links too, but in its own sections.
            if self.title.is_some() && page.only_links() {
                return all_links(page);
            }
            // A whole section on a Page of its own, as "View all" opens:
            // untitled, since the Page's title is the section's.
            if let [Section::Cards { title, cards, .. }] = page.sections.as_slice()
                && title.is_empty()
            {
                return cards::grid(cards, images).map(Message::Link);
            }
            // The shortcuts go under everything else, as in sone.
            let (shortcuts, sections): (Vec<_>, Vec<_>) = page
                .sections
                .iter()
                .enumerate()
                .partition(|(_, section)| is_shortcuts(section));
            let sections = sections
                .into_iter()
                .chain(shortcuts)
                .map(|(index, section)| self.section(index, section, images));
            column(sections).spacing(40).into()
        });
        column![title, body].spacing(32).padding(PADDING).into()
    }

    fn section<'a>(
        &'a self,
        index: usize,
        section: &'a Section,
        images: &'a Images,
    ) -> Element<'a, Message> {
        match section {
            Section::Links { links, .. } if is_shortcuts(section) => shortcuts(links),
            Section::Links {
                title,
                view_all,
                links,
            } => {
                let view_all = view_all.as_ref().map(|path| {
                    super::link(text("View all").size(13), open(path, title)).map(Message::Link)
                });
                let header = row![text(title).size(22), space::horizontal()]
                    .push(view_all)
                    .align_y(Alignment::Center);
                let pills = row(links.iter().map(pill)).spacing(12).wrap();
                column![header, pills.vertical_spacing(12)]
                    .spacing(16)
                    .into()
            }
            Section::Cards {
                title,
                view_all,
                cards,
            } => {
                let view_all = view_all.as_ref().map(|path| Route::ExplorePage {
                    path: path.clone(),
                    title: title.clone(),
                });
                self.rows
                    .view(index, title, cards, view_all, images)
                    .map(Message::Cards)
            }
        }
    }
}

/// Untitled links, drawn with icons.
fn is_shortcuts(section: &Section) -> bool {
    matches!(section, Section::Links { title, .. } if title.is_empty())
}

fn open(path: &str, title: &str) -> Link {
    Link::Open(Route::ExplorePage {
        path: path.to_string(),
        title: title.to_string(),
    })
}

/// A genre, mood or decade, as a pill.
fn pill(link: &PageLink) -> Element<'_, Message> {
    button(text(&link.title).size(15))
        .padding([12, 24])
        .style(pill_style)
        .on_press(Message::Link(open(&link.path, &link.title)))
        .into()
}

/// The untitled links under Explore's (New, Top, HiRes…), each with its
/// icon, as in sone.
fn shortcuts(links: &[PageLink]) -> Element<'_, Message> {
    let shortcuts = links.iter().map(|link| {
        let label = row![
            icon(shortcut_icon(&link.title), 24.0, style::TEXT_MUTED),
            text(&link.title).size(16),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        button(label)
            .padding([8, 0])
            .width(SHORTCUT_WIDTH)
            .style(style::text_link)
            .on_press(Message::Link(open(&link.path, &link.title)))
            .into()
    });
    row(shortcuts)
        .spacing(48)
        .wrap()
        .vertical_spacing(16)
        .into()
}

/// sone's `ICON_MAP`; anything else gets a note.
fn shortcut_icon(title: &str) -> Icon {
    match title {
        "New" => Icon::Calendar,
        "Top" => Icon::Trophy,
        "Videos" => Icon::SquarePlay,
        "HiRes" => Icon::ChartNoAxesColumn,
        "Clean Content" => Icon::ShieldCheck,
        "Staff Picks" => Icon::Star,
        "Creator Hub" => Icon::Users,
        _ => Icon::Music,
    }
}

/// A Page of nothing but links, as "All Genres": one list of them.
fn all_links(page: &ExplorePage) -> Element<'_, Message> {
    let links = page.sections.iter().flat_map(|section| match section {
        Section::Links { links, .. } => links.as_slice(),
        Section::Cards { .. } => &[],
    });
    let links = links.map(|link| {
        container(
            button(text(&link.title).size(15))
                .padding(0)
                .style(style::text_link)
                .on_press(Message::Link(open(&link.path, &link.title))),
        )
        .width(LINK_WIDTH)
        .into()
    });
    row(links).spacing(40).wrap().vertical_spacing(24).into()
}

fn pill_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => style::BG_SURFACE_HOVER,
        _ => style::BG_INSET,
    };
    button::Style {
        background: Some(background.into()),
        text_color: style::TEXT_PRIMARY,
        border: style::rounded(8.0),
        ..button::Style::default()
    }
}
