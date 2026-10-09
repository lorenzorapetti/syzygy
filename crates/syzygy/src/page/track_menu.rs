//! The menu a track row opens on right-click: queueing, and where the
//! track leads. It's built from sections of items, so the Library's own
//! (liking, adding to a playlist) slot in as sections of their own.

use iced::widget::{Column, button, container, row, rule, text};
use iced::{Alignment, Border, Color, Element, Length, Shadow, Theme, Vector};
use iced_aw::ContextMenu;
use syzygy_catalog::Track;

use super::{Link, Preview, Route};
use crate::icons::{Icon, icon};
use crate::style;

const WIDTH: f32 = 240.0;
const ICON_SIZE: f32 = 18.0;

/// One thing the menu offers. Without a link it's shown disabled.
struct Item {
    icon: Icon,
    label: &'static str,
    link: Option<Link>,
}

/// `row`, opening the track's menu where it's right-clicked.
pub fn with_menu<'a>(row: Element<'a, Link>, track: &'a Track) -> Element<'a, Link> {
    ContextMenu::new(row, move || menu(track))
        .style(backdrop)
        .into()
}

/// No dimming behind the menu.
fn backdrop(
    _theme: &Theme,
    _status: iced_aw::style::Status,
) -> iced_aw::style::context_menu::Style {
    iced_aw::style::context_menu::Style {
        background: Color::TRANSPARENT.into(),
    }
}

/// The menu's sections, top to bottom, with a line between each.
fn sections(track: &Track) -> Vec<Vec<Item>> {
    let album = track.album.as_ref().map(|album| {
        Link::Open(Route::Album {
            id: album.id,
            preview: Some(Preview {
                title: album.title.clone(),
                cover: album.cover.clone(),
                artist: track.artists.first().map(|artist| artist.name.clone()),
            }),
        })
    });
    let artist = track.artists.first().map(|artist| {
        Link::Open(Route::Artist {
            id: artist.id,
            preview: None,
        })
    });
    vec![
        vec![
            Item {
                icon: Icon::ListEnd,
                label: "Play next",
                link: Some(Link::PlayNext(track.clone())),
            },
            Item {
                icon: Icon::ListPlus,
                label: "Add to queue",
                link: Some(Link::AddToQueue(track.clone())),
            },
        ],
        vec![
            Item {
                icon: Icon::Radio,
                label: "Go to Track radio",
                link: Some(Link::TrackRadio(track.clone())),
            },
            Item {
                icon: Icon::Disc3,
                label: "Go to album",
                link: album,
            },
            Item {
                icon: Icon::User,
                label: "Go to artist",
                link: artist,
            },
        ],
    ]
}

fn menu<'a>(track: &Track) -> Element<'a, Link> {
    let mut items = Column::new();
    for (i, section) in sections(track).into_iter().enumerate() {
        if i > 0 {
            items = items.push(container(rule::horizontal(1).style(divider)).padding([4, 0]));
        }
        items = items.extend(section.into_iter().map(item));
    }
    container(items)
        .padding([4, 0])
        .width(WIDTH)
        .style(panel)
        .into()
}

fn item<'a>(item: Item) -> Element<'a, Link> {
    let enabled = item.link.is_some();
    let (glyph, label) = if enabled {
        (style::TEXT_MUTED, style::TEXT_SECONDARY)
    } else {
        (style::TEXT_DISABLED, style::TEXT_DISABLED)
    };
    let line = row![
        icon(item.icon, ICON_SIZE, glyph),
        text(item.label).size(14).color(label),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    button(line)
        .padding([10, 16])
        .width(Length::Fill)
        .style(item_style)
        .on_press_maybe(item.link)
        .into()
}

fn item_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(style::HL_FAINT.into()),
        _ => None,
    };
    button::Style {
        background,
        text_color: style::TEXT_PRIMARY,
        ..button::Style::default()
    }
}

fn divider(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: style::BG_INSET,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_SURFACE.into()),
        border: Border {
            color: style::BORDER_SUBTLE,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.6),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}
