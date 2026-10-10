//! The menus tracks and cards open on right-click, and a Page header's
//! "…" button that opens its item's card menu on a left click. A menu is
//! built from sections of items. "Add to playlist" and "Move to folder"
//! open the Shell's popovers beside the item, so it knows where on screen
//! it is.

use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::{Column, button, container, mouse_area, row, rule, space, text};
use iced::{
    Alignment, Border, Color, Element, Event, Length, Rectangle, Shadow, Size, Theme, Vector,
};
use iced_aw::ContextMenu;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::library::Folder;
use syzygy_catalog::{Playlist, Track};

use super::{Link, Preview, Route, cards};
use crate::icons::{Icon, filled, icon};
use crate::library::{Ask, Favorite, Placed, Removal, Tracks};
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
    /// Under the label: why it's disabled.
    hint: Option<&'static str>,
    /// Its link is made from where the item is on screen, and it opens
    /// what comes next beside it.
    anchored: Option<Box<dyn Fn(Rectangle) -> Link>>,
}

impl Item {
    fn new(icon: Icon, label: &'static str, link: Option<Link>) -> Self {
        Self {
            icon,
            label,
            link,
            lit: false,
            hint: None,
            anchored: None,
        }
    }
}

/// "Add to playlist ▸": the picker of an Own playlist for `tracks`.
fn add_to_playlist(tracks: Tracks) -> Item {
    Item {
        anchored: Some(Box::new(Link::add_to_playlist(tracks))),
        ..Item::new(Icon::ListMusic, "Add to playlist", None)
    }
}

/// "Move to folder ▸": the Folders an Own playlist can go to.
fn move_to_folder(placed: Placed) -> Item {
    Item {
        anchored: Some(Box::new(Link::move_to_folder(placed))),
        ..Item::new(Icon::FolderInput, "Move to folder", None)
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
        vec![
            favorite(Favorite::track(track), liked),
            add_to_playlist(Tracks::These(vec![track.clone()])),
        ],
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

/// A track's menu in an Own playlist: [`track`]'s, and taking it out.
pub fn own_track(track: &Track, liked: Option<bool>, removal: &Removal) -> Vec<Vec<Item>> {
    let mut sections = self::track(track, liked);
    sections.push(vec![Item::new(
        Icon::Trash2,
        "Remove from playlist",
        Some(Link::RemoveTrack(Box::new(removal.clone()))),
    )]);
    sections
}

/// A card's menu: playing what it leads to, and liking or following it.
/// Empty for a card with nothing to play, such as a video's.
pub fn card(card: &Card, liked: Option<bool>) -> Vec<Vec<Item>> {
    if !cards::plays(card) {
        return vec![];
    }
    let like = Favorite::card(card).map(|favorite| self::favorite(favorite, liked));
    // A track's card plays on its own: the Loved tracks are liked from
    // its rows.
    let like = like.filter(|_| !matches!(card.target, Target::Track(_)));
    // An artist's tracks don't go into a playlist all at once.
    let add = (!matches!(card.target, Target::Artist(_)))
        .then(|| add_to_playlist(Tracks::Card(card.clone())));
    let library: Vec<Item> = like.into_iter().chain(add).collect();
    std::iter::once(playing(card))
        .chain((!library.is_empty()).then_some(library))
        .collect()
}

/// An Own playlist's menu: playing it, adding its tracks to another,
/// editing it, moving it to a Folder when it's listed in one (or at the
/// top level: `listed_in`), and deleting it. It isn't a Favorite, so it
/// has no like.
pub fn own_playlist(
    card: &Card,
    playlist: &Playlist,
    listed_in: Option<Option<String>>,
) -> Vec<Vec<Item>> {
    let ask = |ask| Some(Link::Ask(Box::new(ask)));
    let edit = Item::new(
        Icon::Pencil,
        "Edit playlist",
        ask(Ask::EditPlaylist(playlist.clone())),
    );
    let moving = listed_in.map(|folder| {
        move_to_folder(Placed {
            playlist: playlist.clone(),
            folder,
        })
    });
    let delete = Item::new(
        Icon::Trash2,
        "Delete playlist",
        ask(Ask::DeletePlaylist(playlist.clone())),
    );
    vec![
        playing(card),
        vec![add_to_playlist(Tracks::Card(card.clone()))],
        std::iter::once(edit)
            .chain(moving)
            .chain([delete])
            .collect(),
    ]
}

/// A Folder's menu: a new playlist in it, renaming it, and deleting it,
/// which only an empty Folder offers (as TIDAL's own app does).
pub fn folder(folder: &Folder, deletable: bool) -> Vec<Vec<Item>> {
    let ask = |ask| Some(Link::Ask(Box::new(ask)));
    let delete = Item::new(
        Icon::Trash2,
        "Delete folder",
        deletable.then(|| Link::Ask(Box::new(Ask::DeleteFolder(folder.clone())))),
    );
    let delete = Item {
        hint: (!deletable).then_some("Move or delete its playlists first"),
        ..delete
    };
    vec![
        vec![
            Item::new(
                Icon::Plus,
                "New playlist in folder",
                ask(Ask::NewPlaylistIn(folder.clone())),
            ),
            Item::new(
                Icon::Pencil,
                "Rename folder",
                ask(Ask::RenameFolder(folder.clone())),
            ),
        ],
        vec![delete],
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
        lit: on,
        ..Item::new(
            icon,
            label,
            liked.map(|liked| Link::Favorite(Box::new(favorite), !liked)),
        )
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
    let panel = container(items).padding([4, 0]).width(WIDTH).style(panel);
    // The whole panel is the menu's, dividers and padding too: where the
    // menu has no interaction of its own, the Page under it would take the
    // cursor and light up a row.
    mouse_area(panel)
        .interaction(mouse::Interaction::Idle)
        .into()
}

fn item<'a>(item: Item) -> Element<'a, Link> {
    if let Some(anchored) = item.anchored {
        let line = row![
            icon(item.icon, ICON_SIZE, style::TEXT_MUTED),
            text(item.label).size(14).color(style::TEXT_SECONDARY),
            space::horizontal(),
            icon(Icon::ChevronRight, 14.0, style::TEXT_MUTED),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let button = button(line)
            .padding([10, 16])
            .width(Length::Fill)
            .style(item_style)
            .on_press(());
        return anchor(button, anchored);
    }
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
    let words = Column::new()
        .push(text(item.label).size(14).color(label))
        .push(
            item.hint
                .map(|hint| text(hint).size(12).color(style::TEXT_MUTED)),
        )
        .spacing(2);
    let line = row![glyph, words].spacing(12).align_y(Alignment::Center);
    button(line)
        .padding([10, 16])
        .width(Length::Fill)
        .style(item_style)
        .on_press_maybe(item.link)
        .into()
}

pub fn item_style(_theme: &Theme, status: button::Status) -> button::Style {
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

pub fn panel(_theme: &Theme) -> container::Style {
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

/// `content`, whose press publishes the message `to` makes from where it
/// is on screen: a popover opened by it goes beside it.
pub fn anchor<'a, Message: 'a>(
    content: impl Into<Element<'a, ()>>,
    to: impl Fn(Rectangle) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(Anchor {
        content: content.into(),
        to: Box::new(to),
    })
}

struct Anchor<'a, Message> {
    content: Element<'a, ()>,
    to: Box<dyn Fn(Rectangle) -> Message + 'a>,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Anchor<'_, Message> {
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let mut pressed = Vec::new();
        let mut inner = Shell::new(&mut pressed);
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut inner,
            viewport,
        );
        let at = layout.bounds();
        shell.merge(inner, |()| (self.to)(at));
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}
