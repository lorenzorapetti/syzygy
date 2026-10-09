//! The menus tracks and cards open on right-click, and a Page header's
//! "…" button that opens its item's card menu on a left click. A menu is
//! built from sections of items, so later Library items (adding to a
//! playlist, moving to a Folder) slot in as sections of their own.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::{Column, button, container, row, rule, text};
use iced::{
    Alignment, Border, Color, Element, Event, Length, Rectangle, Shadow, Size, Theme, Vector,
};
use iced_aw::ContextMenu;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Playlist, Track};

use super::{Link, Preview, Route, cards};
use crate::icons::{Icon, filled, icon};
use crate::library::{Ask, Favorite};
use crate::style;

const WIDTH: f32 = 240.0;
const ICON_SIZE: f32 = 18.0;
/// The header's "…" button.
const MORE_SIZE: f32 = 40.0;

/// One thing a menu offers. Without a link it's shown disabled.
pub struct Item {
    icon: Icon,
    label: &'static str,
    link: Option<Link>,
    /// Drawn filled in the accent, as a Favorite's heart.
    lit: bool,
}

impl Item {
    fn new(icon: Icon, label: &'static str, link: Option<Link>) -> Self {
        Self {
            icon,
            label,
            link,
            lit: false,
        }
    }
}

/// `underlay`, opening the menu `sections` builds where it's
/// right-clicked.
pub fn with_menu<'a>(
    underlay: Element<'a, Link>,
    sections: impl Fn() -> Vec<Vec<Item>> + 'a,
) -> Element<'a, Link> {
    ContextMenu::new(underlay, move || menu(sections()))
        .style(backdrop)
        .into()
}

/// A round "…" button opening the menu `sections` builds.
pub fn more<'a>(sections: impl Fn() -> Vec<Vec<Item>> + 'a) -> Element<'a, Link> {
    let dots = container(icon(Icon::Ellipsis, 22.0, style::TEXT_SECONDARY))
        .center(MORE_SIZE)
        .style(|_| container::Style {
            background: Some(style::BG_BUTTON.into()),
            border: style::rounded(MORE_SIZE / 2.0),
            ..container::Style::default()
        });
    Element::new(LeftClick {
        content: with_menu(dots.into(), sections),
    })
}

/// A Favorite's heart, or an artist's follow button: lit while it's a
/// Favorite, and disabled while `liked` is unknown.
pub fn heart<'a>(favorite: Favorite, liked: Option<bool>, size: f32) -> Element<'a, Link> {
    let artist = matches!(favorite.id(), syzygy_catalog::FavoriteId::Artist(_));
    let glyph = match (artist, liked) {
        (true, Some(true)) => icon(Icon::UserCheck, size, style::ACCENT),
        (true, _) => icon(Icon::UserPlus, size, style::TEXT_SECONDARY),
        (false, Some(true)) => filled(Icon::Heart, size, style::ACCENT),
        (false, Some(false)) => icon(Icon::Heart, size, style::TEXT_SECONDARY),
        (false, None) => icon(Icon::Heart, size, style::TEXT_DISABLED),
    };
    button(container(glyph).center(size + 16.0))
        .padding(0)
        .style(style::icon_button)
        .on_press_maybe(liked.map(|liked| Link::Favorite(Box::new(favorite), !liked)))
        .into()
}

/// A track's menu: queueing, liking, and where the track leads.
pub fn track(track: &Track, liked: Option<bool>) -> Vec<Vec<Item>> {
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
            Item::new(
                Icon::ListEnd,
                "Play next",
                Some(Link::PlayNext(track.clone())),
            ),
            Item::new(
                Icon::ListPlus,
                "Add to queue",
                Some(Link::AddToQueue(track.clone())),
            ),
        ],
        vec![favorite(Favorite::track(track), liked)],
        vec![
            Item::new(
                Icon::Radio,
                "Go to Track radio",
                Some(Link::TrackRadio(track.clone())),
            ),
            Item::new(Icon::Disc3, "Go to album", album),
            Item::new(Icon::User, "Go to artist", artist),
        ],
    ]
}

/// A card's menu: playing what it leads to, and liking or following it.
/// Empty for a card with nothing to play, such as a video's.
pub fn card(card: &Card, liked: Option<bool>) -> Vec<Vec<Item>> {
    if !cards::plays(card) {
        return vec![];
    }
    let like = Favorite::card(card).map(|favorite| vec![self::favorite(favorite, liked)]);
    // A track's card plays on its own: the Loved tracks are liked from
    // its rows.
    let like = like.filter(|_| !matches!(card.target, Target::Track(_)));
    std::iter::once(playing(card)).chain(like).collect()
}

/// An Own playlist's menu: playing it, editing it and deleting it. It
/// isn't a Favorite, so it has no like.
pub fn own_playlist(card: &Card, playlist: &Playlist) -> Vec<Vec<Item>> {
    let ask = |ask| Some(Link::Ask(Box::new(ask)));
    vec![
        playing(card),
        vec![
            Item::new(
                Icon::Pencil,
                "Edit playlist",
                ask(Ask::EditPlaylist(playlist.clone())),
            ),
            Item::new(
                Icon::Trash2,
                "Delete playlist",
                ask(Ask::DeletePlaylist(playlist.clone())),
            ),
        ],
    ]
}

/// Play now, Play next and Add to queue, for all of what a card leads to.
pub fn playing(card: &Card) -> Vec<Item> {
    let queue = |next| {
        Some(Link::QueueCard {
            card: card.clone(),
            next,
        })
    };
    vec![
        Item::new(Icon::Play, "Play now", Some(Link::PlayCard(card.clone()))),
        Item::new(Icon::ListEnd, "Play next", queue(true)),
        Item::new(Icon::ListPlus, "Add to queue", queue(false)),
    ]
}

/// Like or unlike, follow or unfollow: disabled until it's known which.
fn favorite(favorite: Favorite, liked: Option<bool>) -> Item {
    let artist = matches!(favorite.id(), syzygy_catalog::FavoriteId::Artist(_));
    let track = matches!(favorite, Favorite::Track(_));
    let on = liked == Some(true);
    let (icon, label) = match (artist, track, on) {
        (true, _, true) => (Icon::UserCheck, "Unfollow artist"),
        (true, _, false) => (Icon::UserPlus, "Follow artist"),
        (false, true, true) => (Icon::Heart, "Remove from Loved tracks"),
        (false, true, false) => (Icon::Heart, "Add to Loved tracks"),
        (false, false, true) => (Icon::Heart, "Remove from my library"),
        (false, false, false) => (Icon::Heart, "Add to my library"),
    };
    Item {
        icon,
        label,
        link: liked.map(|liked| Link::Favorite(Box::new(favorite), !liked)),
        lit: on,
    }
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

/// The sections, top to bottom, with a line between each.
fn menu<'a>(sections: Vec<Vec<Item>>) -> Element<'a, Link> {
    let mut items = Column::new();
    for (i, section) in sections.into_iter().enumerate() {
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
    let glyph = match (enabled, item.lit) {
        (true, true) => filled(item.icon, ICON_SIZE, style::ACCENT),
        (true, false) => icon(item.icon, ICON_SIZE, style::TEXT_MUTED),
        (false, _) => icon(item.icon, ICON_SIZE, style::TEXT_DISABLED),
    };
    let label = if enabled {
        style::TEXT_SECONDARY
    } else {
        style::TEXT_DISABLED
    };
    let line = row![glyph, text(item.label).size(14).color(label)]
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

/// Its content, with a left click on it taken as a right click: a context
/// menu that opens like a button, when the click is released.
struct LeftClick<'a> {
    content: Element<'a, Link>,
}

impl Widget<Link, Theme, iced::Renderer> for LeftClick<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Link>,
        viewport: &Rectangle,
    ) {
        // The menu opens as the click ends, not as it starts: an open menu
        // closes on a left button's release, which would be this click's.
        let over = cursor.is_over(layout.bounds());
        let event = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => return,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if over => {
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
            }
            _ => event,
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Link, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
