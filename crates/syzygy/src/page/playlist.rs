//! The Playlist Page: a playlist's tracks a page at a time, sorted by TIDAL
//! and filtered here, then the tracks TIDAL recommends for it.
//!
//! On an Own playlist, "Edit order" lets the user drag a track to another
//! place in the Own order, one Track reorder per drag. It shows the rows in
//! the Own order unfiltered, so a row's place is its index in the
//! playlist, and a drop goes between two loaded rows.

use iced::widget::{
    Column, button, column, container, mouse_area, row, space, stack, text, text_input,
};
use iced::{Alignment, Element, Length, Point, mouse};
use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::{Paged, Playlist, Read, Track, TrackOrder, TrackSort};

use super::paged::List;
use super::track_list::{self, Columns, Hover, Mark};
use super::{
    Action, Context, Link, Load, NowPlaying, PADDING, Preview, Remote, Route, Viewport, count,
    duration, header_actions, hero, menu, play_buttons,
};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::library::{At, Favorite, Library, Removal, Reorder};
use crate::playback::{Continuation, SourceRef, Start};
use crate::settings::Sort;
use crate::style;

const SPACING: f32 = 32.0;
const FILTER_HEIGHT: f32 = 40.0;
const FILTER_WIDTH: f32 = 360.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING + FILTER_HEIGHT + SPACING;
/// How far the pointer moves with the button down before a row is dragged.
const DRAG_THRESHOLD: f32 = 4.0;
/// How near the top or bottom of the viewport a drag scrolls the Page.
const EDGE: f32 = 64.0;
/// How far a drag at the very edge scrolls the Page each tick.
const EDGE_SPEED: f32 = 16.0;
/// How many recommendations show at a time, and how many are read at once.
const RECOMMENDATIONS_SHOWN: usize = 10;
const RECOMMENDATIONS_READ: usize = syzygy_catalog::PAGE_SIZE as usize;

pub struct State {
    uuid: String,
    preview: Option<Preview>,
    user_id: Option<u64>,
    playlist: Remote<Playlist>,
    sort: Option<TrackSort>,
    tracks: List<Track>,
    filter: String,
    /// The loaded tracks the pending removals leave, by their place in
    /// `tracks`. In the own order a row's place among them is its index in
    /// the playlist.
    rows: Vec<usize>,
    /// The places in `rows` the filter lets through.
    shown: Vec<usize>,
    recommendations: Recommendations,
    /// The track the pointer is over, by its place in `rows`.
    hover: Hover,
    /// The recommendation the pointer is over, by its place among those
    /// showing.
    recommendation_hover: Hover,
    /// "Edit order" is on. It's off whenever the Page opens.
    ordering: Option<Ordering>,
}

/// "Edit order" while it's on: where the pointer is and the row it holds.
#[derive(Debug, Default)]
struct Ordering {
    /// The pointer's height down the Page, as last seen over it.
    pointer: Option<f32>,
    /// The part of the Page in view, as of the pointer's last move or the
    /// last scroll.
    viewport: Viewport,
    grab: Option<Grab>,
}

/// A row the button went down on: dragged once the pointer has moved past
/// [`DRAG_THRESHOLD`].
#[derive(Debug, Clone, Copy)]
struct Grab {
    /// Its place in the rows.
    from: usize,
    /// The pointer's height down the Page as the button went down.
    start: f32,
    dragging: bool,
}

impl Ordering {
    /// The row being dragged, by its place.
    fn dragging(&self) -> Option<usize> {
        self.grab.filter(|grab| grab.dragging).map(|grab| grab.from)
    }

    /// The slot a drag would drop in among `rows` loaded rows: before the
    /// row at it, or after the last. Only loaded rows are dropped between.
    fn slot(&self, rows: usize) -> Option<usize> {
        self.dragging()?;
        let y = self.pointer? - LIST_TOP - track_list::HEADER_HEIGHT;
        Some(((y / track_list::ROW_HEIGHT).round().max(0.0) as usize).min(rows))
    }

    /// How far a drag near an edge of the viewport scrolls the Page now:
    /// faster the nearer it is.
    fn scroll(&self) -> Option<f32> {
        self.dragging()?;
        let pointer = self.pointer? - self.viewport.offset;
        let height = self.viewport.height;
        if pointer < EDGE {
            Some(-EDGE_SPEED * (1.0 - pointer.max(0.0) / EDGE))
        } else if height > 0.0 && pointer > height - EDGE {
            Some(EDGE_SPEED * (1.0 - (height - pointer).max(0.0) / EDGE))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Playlist(Read<Playlist>),
    /// The first page of the tracks, in `sort`'s order.
    Tracks {
        sort: Option<TrackSort>,
        read: Read<Paged<Track>>,
    },
    More {
        sort: Option<TrackSort>,
        offset: usize,
        result: Result<Paged<Track>, Arc<syzygy_catalog::Error>>,
    },
    /// The end of the list is nearly in view.
    EndInView,
    /// Try loading more again after it failed.
    RetryMore,
    /// A column title was clicked: sort by it, or `None` for the own order.
    Sort(Option<TrackOrder>),
    Filter(String),
    Recommendations {
        offset: usize,
        result: Result<Vec<Track>, Arc<syzygy_catalog::Error>>,
    },
    /// Show the next recommendations.
    MoreRecommendations,
    /// Add a recommended track to this playlist, an Own playlist.
    AddRecommendation(Track),
    Link(Link),
    /// Play the loaded tracks, in the order shown, from a track by its
    /// place in the list or all of them.
    Play(Start),
    /// Play the recommendations showing, from one by its place among them.
    PlayRecommendation(usize),
    /// The pointer came over the recommendation at this place.
    RecommendationHovered(usize),
    /// The pointer left the recommendation at this place.
    RecommendationLeft(usize),
    TogglePlay,
    /// The pointer came over the row with this key.
    Hovered(usize),
    /// The pointer left the row with this key.
    Left(usize),
    Retry,
    /// Turn "Edit order" on or off.
    EditOrder,
    /// The pointer moved over the Page, while "Edit order" is on: where it
    /// is, and the part of the Page in view.
    Pointer(Point, Viewport),
    /// The button went down on the row at this place, while "Edit order"
    /// is on.
    Grab(usize),
    /// The mouse button was let go, wherever it was.
    Released,
}

impl State {
    pub fn new(uuid: String, preview: Option<Preview>, context: &Context) -> (Self, Action) {
        let sort = context.settings.track_sorts.get(&uuid).copied();
        let action = Action::Batch(vec![
            Action::Load(Load::Playlist(uuid.clone())),
            Action::Load(tracks(&uuid, sort)),
            Action::Load(recommendations(&uuid, 0)),
        ]);
        let state = Self {
            uuid,
            preview,
            user_id: context.user_id,
            playlist: Remote::Loading,
            sort,
            tracks: List::new(),
            filter: String::new(),
            rows: Vec::new(),
            shown: Vec::new(),
            recommendations: Recommendations::default(),
            hover: Hover::default(),
            recommendation_hover: Hover::default(),
            ordering: None,
        };
        (state, action)
    }

    pub fn update(&mut self, message: Message, library: &Library) -> Action {
        match message {
            Message::Playlist(read) => {
                self.playlist.apply(read, "a playlist");
                Action::None
            }
            Message::Tracks { sort, read } if sort == self.sort => {
                self.tracks.apply(read, "playlist's tracks");
                self.loaded(library)
            }
            Message::More {
                sort,
                offset,
                result,
            } if sort == self.sort => {
                self.tracks.more(offset, result);
                self.loaded(library)
            }
            // From before the sort changed.
            Message::Tracks { .. } | Message::More { .. } => Action::None,
            Message::EndInView => self.load_more(),
            Message::RetryMore => {
                let offset = self.tracks.retry();
                self.more_from(offset)
            }
            Message::Sort(order) => {
                let sort = TrackSort::clicked(self.sort, order);
                if sort == self.sort {
                    return Action::None;
                }
                self.sort = sort;
                self.reset_tracks();
                Action::Batch(vec![
                    Action::Load(tracks(&self.uuid, sort)),
                    Action::SaveSort(Sort::Playlist(self.uuid.clone(), sort)),
                ])
            }
            Message::Filter(filter) => {
                self.filter = filter;
                self.loaded(library)
            }
            Message::Recommendations { offset, result } => {
                match self.recommendations.arrived(offset, result) {
                    Some(offset) => Action::Load(recommendations(&self.uuid, offset)),
                    None => Action::None,
                }
            }
            Message::MoreRecommendations => match self.recommendations.next() {
                Some(offset) => Action::Load(recommendations(&self.uuid, offset)),
                None => Action::None,
            },
            Message::AddRecommendation(track) => {
                let Some(playlist) = self.playlist.loaded().filter(|_| self.is_own()) else {
                    return Action::None;
                };
                // It's no longer recommended, unless it doesn't go in.
                self.recommendations.take(track.id);
                Action::Library(crate::library::Message::AddRecommendation(
                    playlist.clone(),
                    track,
                ))
            }
            Message::Link(link) => link.follow(),
            Message::Play(start) => {
                let tracks: Vec<Track> = self
                    .rows
                    .iter()
                    .map(|&position| self.tracks.items()[position].clone())
                    .collect();
                if tracks.is_empty() {
                    return Action::None;
                }
                let source = self.source();
                let mut request = super::request(source.clone(), self.title(), &tracks, start);
                // The rest is read on from where the loaded pages end, the
                // rows still on their way out counted: TIDAL has them until
                // their removals land.
                request.continuation = self.tracks.has_more().then(|| Continuation {
                    source,
                    offset: self.tracks.items().len(),
                });
                Action::Play(request)
            }
            Message::PlayRecommendation(i) => Action::Play(super::request(
                SourceRef::PlaylistRecommendations(self.uuid.clone()),
                &format!("{}: Recommended", self.title()),
                self.recommendations.shown(),
                Start::Track(i),
            )),
            Message::RecommendationHovered(i) => {
                self.recommendation_hover.entered(i);
                Action::None
            }
            Message::RecommendationLeft(i) => {
                self.recommendation_hover.left(i);
                Action::None
            }
            Message::TogglePlay => Action::TogglePlay,
            Message::Hovered(key) => {
                self.hover.entered(key);
                Action::None
            }
            Message::Left(key) => {
                self.hover.left(key);
                Action::None
            }
            Message::EditOrder => self.edit_order(library),
            Message::Pointer(point, viewport) => {
                if let Some(ordering) = &mut self.ordering {
                    ordering.pointer = Some(point.y);
                    ordering.viewport = viewport;
                    if let Some(grab) = &mut ordering.grab
                        && (point.y - grab.start).abs() > DRAG_THRESHOLD
                    {
                        grab.dragging = true;
                    }
                }
                Action::None
            }
            Message::Grab(from) => {
                if let Some(ordering) = &mut self.ordering
                    && let Some(start) = ordering.pointer
                    && from < self.rows.len()
                {
                    ordering.grab = Some(Grab {
                        from,
                        start,
                        dragging: false,
                    });
                }
                Action::None
            }
            Message::Released => self.drop_row(),
            Message::Retry => {
                self.reset_tracks();
                let tracks = Action::Load(tracks(&self.uuid, self.sort));
                if self.playlist.loaded().is_some() {
                    return tracks;
                }
                self.playlist = Remote::Loading;
                Action::Batch(vec![
                    tracks,
                    Action::Load(Load::Playlist(self.uuid.clone())),
                ])
            }
        }
    }

    /// Turn "Edit order" on or off. On, the rows show in the Own order,
    /// which is saved as the playlist's sort so the old one doesn't come
    /// back, unfiltered, and the recommendations hide.
    fn edit_order(&mut self, library: &Library) -> Action {
        if self.ordering.take().is_some() {
            return Action::None;
        }
        if !self.is_own() {
            return Action::None;
        }
        self.ordering = Some(Ordering::default());
        self.filter.clear();
        self.hover = Hover::default();
        if self.sort.take().is_none() {
            self.refilter(library);
            return Action::None;
        }
        self.reset_tracks();
        Action::Batch(vec![
            Action::Load(tracks(&self.uuid, None)),
            Action::SaveSort(Sort::Playlist(self.uuid.clone(), None)),
        ])
    }

    /// The button was let go: a dragged row goes to the slot it's over.
    fn drop_row(&mut self) -> Action {
        let Some(ordering) = &mut self.ordering else {
            return Action::None;
        };
        let slot = ordering.slot(self.rows.len());
        let Some(grab) = ordering.grab.take() else {
            return Action::None;
        };
        let (Some(to), Some(playlist)) = (slot, self.playlist.loaded()) else {
            return Action::None;
        };
        let Some(&position) = self.rows.get(grab.from) else {
            return Action::None;
        };
        Action::Library(crate::library::Message::ReorderTrack(Reorder {
            playlist: playlist.clone(),
            track: self.tracks.items()[position].clone(),
            from: grab.from,
            to,
        }))
    }

    /// Escape: it cancels a drag, or else turns "Edit order" off. Whether
    /// it did either.
    pub fn escape(&mut self) -> bool {
        match &mut self.ordering {
            Some(ordering) if ordering.dragging().is_some() => {
                ordering.grab = None;
                true
            }
            Some(_) => {
                self.ordering = None;
                true
            }
            None => false,
        }
    }

    /// The Page scrolled: a pointer that stayed where it was on screen is
    /// over another part of the Page now.
    pub fn scrolled(&mut self, viewport: Viewport) {
        if let Some(ordering) = &mut self.ordering {
            if let Some(pointer) = &mut ordering.pointer {
                *pointer += viewport.offset - ordering.viewport.offset;
            }
            ordering.viewport = viewport;
        }
    }

    /// Whether a drag is near an edge of the viewport, so the Page
    /// scrolls.
    pub fn autoscrolls(&self) -> bool {
        self.ordering
            .as_ref()
            .is_some_and(|ordering| ordering.scroll().is_some())
    }

    /// How far to scroll the Page now, for a drag near an edge.
    pub fn autoscroll(&self) -> Option<f32> {
        self.ordering.as_ref()?.scroll()
    }

    /// The playlist's title, as far as it's known.
    fn title(&self) -> &str {
        match (&self.playlist, &self.preview) {
            (Remote::Loaded(playlist), _) => &playlist.title,
            (_, Some(preview)) => &preview.title,
            _ => "Playlist",
        }
    }

    /// The playlist as a Playback source, in the order it's shown in.
    fn source(&self) -> SourceRef {
        SourceRef::Playlist {
            uuid: self.uuid.clone(),
            sort: self.sort,
        }
    }

    /// Read the tracks again from the first page.
    fn reset_tracks(&mut self) {
        self.tracks = List::new();
        self.rows.clear();
        self.shown.clear();
    }

    /// The tracks or the filter changed: filter them again, and while a
    /// filter is on, load the rest so it sees every track.
    fn loaded(&mut self, library: &Library) -> Action {
        self.refilter(library);
        self.load_rest_if_filtering()
    }

    /// Lay the pending removals over the tracks, then the filter.
    fn refilter(&mut self, library: &Library) {
        let tracks = self.tracks.items();
        self.rows = library.rows(&self.uuid, tracks, self.sort.is_some());
        let rows = self.rows.iter().map(|&position| &tracks[position]);
        self.shown = track_list::matching(rows, &self.filter);
    }

    /// The Library's pending edits changed which rows are on their way
    /// out.
    pub fn library_changed(&mut self, library: &Library) {
        self.refilter(library);
    }

    /// TIDAL took the track at `index` of the own order out of this
    /// playlist: its row goes from what's loaded, so the pages read after
    /// follow on from TIDAL's.
    pub fn removed(&mut self, uuid: &str, track: &Track, index: usize, library: &Library) {
        if uuid != self.uuid {
            return;
        }
        let index = self.sort.is_none().then_some(index);
        if let Remote::Loaded(page) = &mut self.tracks.list {
            let rows: Vec<&Track> = page.items.iter().collect();
            if let Some(at) = crate::library::locate(&rows, track, index) {
                page.items.remove(at);
                page.total = page.total.map(|total| total.saturating_sub(1));
            }
        }
        self.refilter(library);
    }

    /// TIDAL took `track` from `from` to the slot `to` of this playlist's
    /// own order: what's loaded is reordered, so it stays a prefix of
    /// TIDAL's. A sorted list is read in TIDAL's sort and stays.
    pub fn reordered(
        &mut self,
        uuid: &str,
        track: &Track,
        (from, to): (usize, usize),
        library: &Library,
    ) {
        if uuid != self.uuid || self.sort.is_some() {
            return;
        }
        if let Remote::Loaded(page) = &mut self.tracks.list {
            let rows: Vec<&Track> = page.items.iter().collect();
            if let Some(at) = crate::library::locate(&rows, track, Some(from)) {
                crate::library::reordered(&mut page.items, at, to);
            }
        }
        self.refilter(library);
    }

    /// A track recommended for the playlist with this uuid didn't go in:
    /// it's recommended again.
    pub fn not_added(&mut self, uuid: &str, track: Track) {
        if uuid == self.uuid {
            self.recommendations.put_back(track);
        }
    }

    fn load_rest_if_filtering(&mut self) -> Action {
        if self.filter.trim().is_empty() {
            Action::None
        } else {
            self.load_more()
        }
    }

    fn load_more(&mut self) -> Action {
        let offset = self.tracks.next();
        self.more_from(offset)
    }

    fn more_from(&self, offset: Option<usize>) -> Action {
        match offset {
            Some(offset) => Action::Load(Load::MorePlaylistTracks {
                uuid: self.uuid.clone(),
                sort: self.sort,
                offset,
            }),
            None => Action::None,
        }
    }

    /// An edit of the playlist landed: what it is is read again. Its
    /// tracks stay as they are.
    pub fn refresh(&mut self, tags: &[String]) -> Action {
        if !tags.contains(&syzygy_catalog::playlist_tag(&self.uuid)) {
            return Action::None;
        }
        Action::Load(Load::Playlist(self.uuid.clone()))
    }

    fn is_own(&self) -> bool {
        self.playlist
            .loaded()
            .is_some_and(|playlist| playlist.is_own(self.user_id))
    }

    pub fn shows_track(&self, track_id: u64) -> bool {
        let tracks = self.tracks.items();
        self.rows.iter().any(|&row| tracks[row].id == track_id)
            || self
                .recommendations
                .shown()
                .iter()
                .any(|track| track.id == track_id)
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        viewport: Viewport,
        now_playing: Option<NowPlaying<'a>>,
        allow_explicit: bool,
    ) -> Element<'a, Message> {
        let hero = match (&self.playlist, &self.preview) {
            (Remote::Loaded(playlist), _) => Some(self.hero(library.playlist(playlist), images)),
            (Remote::Loading, Some(preview)) => {
                Some(hero::preview(images, "PLAYLIST", preview, false).map(Message::Link))
            }
            _ => None,
        };
        let ordering = self.ordering.as_ref();
        let filter = container(
            text_input("Filter playlist on title, artist or album", &self.filter)
                .on_input_maybe(ordering.is_none().then_some(Message::Filter))
                .padding([8, 14])
                .size(14)
                .width(FILTER_WIDTH)
                .style(style::filter_input),
        )
        .center_y(FILTER_HEIGHT);
        let buttons = play_buttons(
            &self.source(),
            now_playing,
            Message::Play,
            Message::TogglePlay,
        );
        // An Own playlist isn't a Favorite: it has no heart, and its menu
        // edits and deletes it.
        let actions = self.playlist.loaded().map(|playlist| {
            let playlist = library.playlist(playlist);
            let card = Card {
                title: playlist.title.clone(),
                subtitle: String::new(),
                cover: playlist.cover.clone(),
                target: Target::Playlist(self.uuid.clone()),
            };
            if self.is_own() {
                let playlist = playlist.clone();
                return menu::more(move || menu::own_playlist(&card, &playlist, None))
                    .map(Message::Link);
            }
            let favorite = Favorite::playlist(playlist);
            header_actions(card, Some(favorite), library).map(Message::Link)
        });
        let filter = row![buttons]
            .push(actions)
            .push(space::horizontal())
            .push(filter)
            .spacing(24)
            .align_y(Alignment::Center);
        let columns = Columns {
            cover: true,
            album: true,
            date_added: self.is_own(),
        };
        let body = self.tracks.list.view(Message::Retry, |_| {
            let tracks = self.tracks.items();
            if self.rows.is_empty() {
                return empty(
                    "This playlist is empty",
                    "Add tracks in TIDAL to see them here.",
                );
            }
            let header = match ordering {
                Some(_) => track_list::header(columns),
                None => track_list::sortable_header(columns, self.sort, Message::Sort),
            };
            let own = self.playlist.loaded().filter(|_| self.is_own());
            let list = track_list::view(self.shown.len(), LIST_TOP, viewport, header, |i| {
                let place = self.shown[i];
                let track = &tracks[self.rows[place]];
                if let Some(ordering) = ordering {
                    let mark = Mark::of(track, now_playing, false);
                    let line =
                        track_list::still(images, place + 1, track, columns, allow_explicit, mark);
                    return self.draggable(ordering, line, place);
                }
                let liked = library.liked(track);
                let number = place + 1;
                let mark = Mark::of(track, now_playing, self.hover.over(place));
                let row = match own {
                    Some(playlist) => {
                        let removal = Removal {
                            playlist: playlist.clone(),
                            track: track.clone(),
                            at: match self.sort {
                                None => At::Index(place),
                                Some(_) => At::Sorted,
                            },
                        };
                        track_list::own(
                            images,
                            number,
                            track,
                            columns,
                            allow_explicit,
                            mark,
                            liked,
                            removal,
                        )
                    }
                    None => track_list::marked(
                        images,
                        number,
                        track,
                        columns,
                        allow_explicit,
                        mark,
                        liked,
                    ),
                };
                track_list::playable_track(
                    row.map(Message::Link),
                    mark,
                    Message::Play(Start::Track(place)),
                    place,
                    Message::Hovered,
                    Message::Left,
                )
            });
            let nothing_matches = (self.shown.is_empty() && !self.filter.trim().is_empty())
                .then(|| empty("Nothing matches", "Try another title, artist or album."));
            let all_loaded =
                self.tracks.list.loaded().is_some_and(|page| !page.has_more) && ordering.is_none();
            Column::new()
                .push(list)
                .push(nothing_matches)
                .push(self.tracks.end(Message::EndInView, Message::RetryMore))
                .push(
                    all_loaded
                        .then(|| self.recommendations(images, library, now_playing, allow_explicit))
                        .flatten(),
                )
                .spacing(SPACING)
                .into()
        });
        // Without its details the Page has no hero, so their failure shows
        // in the tracks' place, with Retry, even when the tracks loaded.
        let body = match &self.playlist {
            Remote::NotFound | Remote::Failed(_) => {
                self.playlist.view(Message::Retry, |_| Column::new().into())
            }
            _ => body,
        };
        let page = Column::new()
            .push(hero)
            .push(filter)
            .push(body)
            .spacing(SPACING)
            .padding(PADDING);
        if ordering.is_none() {
            return page.into();
        }
        // Where the pointer is, for the drag.
        mouse_area(page)
            .on_move(move |point| Message::Pointer(point, viewport))
            .into()
    }

    /// A row while "Edit order" is on: the button going down on it grabs
    /// it. Dragged, it fades where it is, and a line marks the slot it
    /// would drop in.
    fn draggable<'a>(
        &self,
        ordering: &Ordering,
        line: Element<'a, Message>,
        place: usize,
    ) -> Element<'a, Message> {
        let dragged = ordering.dragging();
        let slot = ordering
            .slot(self.rows.len())
            .filter(|&slot| dragged.is_some_and(|from| slot != from && slot != from + 1));
        let edge = match slot {
            Some(slot) if slot == place => Some(Alignment::Start),
            Some(slot) if slot == place + 1 && slot == self.rows.len() => Some(Alignment::End),
            _ => None,
        };
        let marker = edge.map(|edge| {
            container(container(space()).width(Length::Fill).height(2).style(|_| {
                container::Style {
                    background: Some(style::ACCENT.into()),
                    border: style::rounded(1.0),
                    ..container::Style::default()
                }
            }))
            .height(track_list::ROW_HEIGHT)
            .align_y(edge)
        });
        let faded = (dragged == Some(place)).then(|| {
            container(space())
                .width(Length::Fill)
                .height(track_list::ROW_HEIGHT)
                .style(|_| container::Style {
                    background: Some(
                        iced::Color {
                            a: 0.6,
                            ..style::BG_BASE
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                })
        });
        let interaction = if dragged.is_some() {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        };
        mouse_area(stack![line].push(faded).push(marker))
            .on_press(Message::Grab(place))
            .interaction(interaction)
            .into()
    }

    /// "Edit order", pressed while it's on, when it reads "Done".
    fn edit_order_button<'a>(&self) -> Element<'a, Message> {
        let on = self.ordering.is_some();
        let label = if on { "Done" } else { "Edit order" };
        button(
            row![
                icon(Icon::ArrowUpDown, 14.0, style::TEXT_PRIMARY),
                text(label).size(13)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6, 14])
        .style(if on {
            style::selected_tab
        } else {
            style::pill_button
        })
        .on_press(Message::EditOrder)
        .into()
    }

    fn hero<'a>(&'a self, playlist: &'a Playlist, images: &'a Images) -> Element<'a, Message> {
        // sone names the user on their Own playlists; syzygy doesn't know
        // their name yet.
        let creator = if self.is_own() {
            Some("You")
        } else {
            playlist.creator_name()
        };
        let facts = format!(
            "{} · {}",
            count(playlist.tracks as usize, "Track"),
            duration(playlist.duration)
        );
        // A user's name leads to their Profile.
        let creator = creator.map(|creator| match playlist.profile() {
            Some(user_id) => super::link(
                text(creator).size(14),
                Link::Open(Route::Profile { user_id }),
            ),
            None => text(creator).size(14).color(style::TEXT_PRIMARY).into(),
        });
        let lines = [
            creator,
            playlist.description.as_deref().map(|description| {
                text(description)
                    .size(14)
                    .color(style::TEXT_MUTED)
                    .wrapping(text::Wrapping::None)
                    .into()
            }),
            Some(text(facts).size(12).color(style::TEXT_MUTED).into()),
        ];
        let lines = lines
            .into_iter()
            .flatten()
            .map(|line: Element<'a, Link>| line.map(Message::Link))
            .chain(self.is_own().then(|| self.edit_order_button()))
            .collect();
        hero::with_art(
            super::rounded_cover(images, playlist.cover.as_ref(), hero::HEIGHT, hero::RADIUS)
                .map(Message::Link),
            "PLAYLIST",
            &playlist.title,
            lines,
        )
    }

    /// Ten of the recommendations, and Refresh for the next ten.
    fn recommendations<'a>(
        &'a self,
        images: &'a Images,
        library: &'a Library,
        now_playing: Option<NowPlaying<'a>>,
        allow_explicit: bool,
    ) -> Option<Element<'a, Message>> {
        let shown = self.recommendations.shown();
        if shown.is_empty() {
            return None;
        }
        let columns = Columns {
            cover: true,
            album: true,
            date_added: false,
        };
        // TIDAL only lets the user add to their Own playlists. sone
        // offers it on every playlist.
        let own = self.is_own();
        let rows = shown.iter().enumerate().map(|(i, track)| {
            let liked = library.liked(track);
            let mark = Mark::of(track, now_playing, self.recommendation_hover.over(i));
            let line =
                track_list::marked(images, i + 1, track, columns, allow_explicit, mark, liked);
            let row = track_list::playable_track(
                line.map(Message::Link),
                mark,
                Message::PlayRecommendation(i),
                i,
                Message::RecommendationHovered,
                Message::RecommendationLeft,
            );
            if own {
                track_list::with_add(row, Message::AddRecommendation(track.clone()))
            } else {
                row
            }
        });
        let header = if own {
            track_list::header_with_add(columns)
        } else {
            track_list::header(columns)
        };
        let refresh = button(
            row![
                icon(Icon::RefreshCw, 16.0, style::TEXT_PRIMARY),
                text("Refresh").size(14)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 20])
        .style(style::pill_button)
        .on_press_maybe((!self.recommendations.loading).then_some(Message::MoreRecommendations));
        let section = column![
            text("Recommended Tracks").size(18),
            column![header].extend(rows),
            container(refresh).align_right(Length::Fill),
        ]
        .spacing(16);
        Some(section.into())
    }
}

fn empty<'a>(title: &'a str, line: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(18),
            text(line).size(14).color(style::TEXT_MUTED),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([48, 0])
    .into()
}

/// The first page of a playlist's tracks.
fn tracks(uuid: &str, sort: Option<TrackSort>) -> Load {
    Load::PlaylistTracks {
        uuid: uuid.to_string(),
        sort,
    }
}

fn recommendations(uuid: &str, offset: usize) -> Load {
    Load::PlaylistRecommendations {
        uuid: uuid.to_string(),
        offset,
    }
}

/// The tracks TIDAL recommends, read a batch at a time and shown ten at a
/// time, as in sone: Refresh shows the next ten, reads the next batch when
/// these run out, and starts over when TIDAL has no more.
#[derive(Default)]
struct Recommendations {
    batch: Vec<Track>,
    /// Where the batch starts in TIDAL's list.
    offset: usize,
    /// Which ten of the batch show.
    page: usize,
    loading: bool,
}

impl Recommendations {
    fn shown(&self) -> &[Track] {
        let start = (self.page * RECOMMENDATIONS_SHOWN).min(self.batch.len());
        let end = (start + RECOMMENDATIONS_SHOWN).min(self.batch.len());
        &self.batch[start..end]
    }

    /// Take the track with this id out: the ones after it move up.
    fn take(&mut self, id: u64) {
        self.batch.retain(|track| track.id != id);
        // The last of the last ten went: the ten before show.
        if self.page > 0 && self.page * RECOMMENDATIONS_SHOWN >= self.batch.len() {
            self.page -= 1;
        }
    }

    /// Recommend a track taken out again, after the others, unless it's
    /// there already.
    fn put_back(&mut self, track: Track) {
        if !self
            .batch
            .iter()
            .any(|recommended| recommended.id == track.id)
        {
            self.batch.push(track);
        }
    }

    /// Show the next ten. The offset to read from when that needs a new
    /// batch.
    fn next(&mut self) -> Option<usize> {
        if self.loading {
            return None;
        }
        if (self.page + 1) * RECOMMENDATIONS_SHOWN < self.batch.len() {
            self.page += 1;
            return None;
        }
        self.loading = true;
        Some(self.offset + RECOMMENDATIONS_READ)
    }

    /// A batch read from `offset` arrived. The offset to read from next
    /// when TIDAL had nothing there, to start over.
    fn arrived(
        &mut self,
        offset: usize,
        result: Result<Vec<Track>, Arc<syzygy_catalog::Error>>,
    ) -> Option<usize> {
        self.loading = false;
        match result {
            Ok(batch) if batch.is_empty() && offset > 0 => {
                self.loading = true;
                Some(0)
            }
            Ok(batch) => {
                self.batch = batch;
                self.offset = offset;
                self.page = 0;
                None
            }
            Err(e) => {
                log::warn!("Could not load the playlist's recommendations: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syzygy_catalog::track::{AlbumRef, ArtistRef};

    fn track(id: u64, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id,
            title: title.to_string(),
            artists: vec![ArtistRef {
                id: 1,
                name: artist.to_string(),
            }],
            album: Some(AlbumRef {
                id: 2,
                title: album.to_string(),
                cover: None,
            }),
            duration: 200,
            explicit: false,
            available: true,
            volume: 1,
            date_added: None,
            track_radio: None,
        }
    }

    fn batch(n: usize) -> Vec<Track> {
        (0..n as u64).map(|id| track(id, "T", "A", "B")).collect()
    }

    #[test]
    fn the_recommendations_showing_animate_and_the_others_do_not() {
        let settings = crate::settings::Settings::default();
        let context = Context {
            user_id: None,
            settings: &settings,
        };
        let (mut state, _) = State::new("uuid".to_string(), None, &context);
        state.recommendations.arrived(0, Ok(batch(50)));

        assert!(state.shows_track(9));
        assert!(!state.shows_track(10));

        state.recommendations.next();

        assert!(!state.shows_track(9));
        assert!(state.shows_track(10));
    }

    #[test]
    fn a_recommendation_plays_the_ones_showing_as_their_own_source() {
        let settings = crate::settings::Settings::default();
        let context = Context {
            user_id: None,
            settings: &settings,
        };
        let preview = Preview {
            title: "Mine".to_string(),
            cover: None,
            artist: None,
        };
        let (mut state, _) = State::new("uuid".to_string(), Some(preview), &context);
        state.recommendations.arrived(0, Ok(batch(50)));
        state.recommendations.next();

        let (library, _) = Library::new(None, &settings);
        let Action::Play(request) = state.update(Message::PlayRecommendation(3), &library) else {
            panic!("a click on a recommendation plays");
        };

        assert_eq!(
            request.source.kind,
            SourceRef::PlaylistRecommendations("uuid".to_string())
        );
        assert_eq!(request.source.name, "Mine: Recommended");
        let ids: Vec<u64> = request.first_page.iter().map(|track| track.id).collect();
        assert_eq!(ids, (10..20).collect::<Vec<u64>>());
        assert!(matches!(request.start, Start::Track(3)));
        assert!(request.continuation.is_none());
    }

    const USER: u64 = 7;

    fn by_title() -> TrackSort {
        TrackSort {
            order: TrackOrder::Title,
            direction: syzygy_catalog::Direction::Ascending,
        }
    }

    /// The Page of an Own playlist with four tracks loaded, in `sort`'s
    /// order, and the Library it shows.
    fn own_page(sort: Option<TrackSort>) -> (State, Library) {
        let mut settings = crate::settings::Settings::default();
        if let Some(sort) = sort {
            settings.save_sort(Sort::Playlist("p-1".to_string(), Some(sort)));
        }
        let context = Context {
            user_id: Some(USER),
            settings: &settings,
        };
        let (mut state, _) = State::new("p-1".to_string(), None, &context);
        let (library, _) = Library::new(Some(USER), &settings);
        let playlist = syzygy_catalog::PlaylistFields::new().playlist("p-1".to_string(), USER);
        state.update(Message::Playlist(Read::Fresh(Ok(playlist))), &library);
        let page = Paged {
            items: batch(4),
            has_more: false,
            cursor: None,
            total: Some(4),
        };
        state.update(
            Message::Tracks {
                sort,
                read: Read::Fresh(Ok(page)),
            },
            &library,
        );
        (state, library)
    }

    /// Where the row at `place` sits down the Page, halfway down it.
    fn row_y(place: f32) -> f32 {
        LIST_TOP + track_list::HEADER_HEIGHT + (place + 0.5) * track_list::ROW_HEIGHT
    }

    fn pointer(y: f32) -> Message {
        Message::Pointer(
            Point::new(100.0, y),
            Viewport {
                offset: 0.0,
                height: 10_000.0,
            },
        )
    }

    #[test]
    fn edit_order_clears_the_sort_and_saves_the_own_order_and_the_filter() {
        let (mut state, library) = own_page(Some(by_title()));
        state.update(Message::Filter("T".to_string()), &library);

        let Action::Batch(actions) = state.update(Message::EditOrder, &library) else {
            panic!("the own order is read and saved");
        };

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::Load(Load::PlaylistTracks { sort: None, .. })
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            Action::SaveSort(Sort::Playlist(uuid, None)) if uuid == "p-1"
        )));
        assert_eq!(state.sort, None);
        assert_eq!(state.filter, "");
    }

    #[test]
    fn edit_order_is_only_for_own_playlists() {
        let settings = crate::settings::Settings::default();
        let context = Context {
            user_id: Some(USER),
            settings: &settings,
        };
        let (mut state, _) = State::new("p-1".to_string(), None, &context);
        let (library, _) = Library::new(Some(USER), &settings);
        let theirs = syzygy_catalog::PlaylistFields::new().playlist("p-1".to_string(), USER + 1);
        state.update(Message::Playlist(Read::Fresh(Ok(theirs))), &library);

        state.update(Message::EditOrder, &library);

        assert!(state.ordering.is_none());
    }

    #[test]
    fn a_dragged_row_is_reordered_to_the_slot_it_is_let_go_over() {
        let (mut state, library) = own_page(None);
        state.update(Message::EditOrder, &library);

        state.update(pointer(row_y(0.0)), &library);
        state.update(Message::Grab(0), &library);
        // Between the third row and the fourth.
        state.update(pointer(row_y(2.6)), &library);
        let action = state.update(Message::Released, &library);

        let Action::Library(crate::library::Message::ReorderTrack(reorder)) = action else {
            panic!("the drop reorders the track");
        };
        assert_eq!(reorder.track.id, 0);
        assert_eq!((reorder.from, reorder.to), (0, 3));
    }

    #[test]
    fn a_press_that_hardly_moves_drags_nothing() {
        let (mut state, library) = own_page(None);
        state.update(Message::EditOrder, &library);

        state.update(pointer(row_y(1.0)), &library);
        state.update(Message::Grab(1), &library);
        state.update(pointer(row_y(1.0) + 2.0), &library);

        assert!(matches!(
            state.update(Message::Released, &library),
            Action::None
        ));
    }

    #[test]
    fn escape_cancels_a_drag_then_turns_edit_order_off() {
        let (mut state, library) = own_page(None);
        state.update(Message::EditOrder, &library);
        state.update(pointer(row_y(0.0)), &library);
        state.update(Message::Grab(0), &library);
        state.update(pointer(row_y(3.0)), &library);

        assert!(state.escape());
        assert!(state.ordering.is_some());
        assert!(matches!(
            state.update(Message::Released, &library),
            Action::None
        ));

        assert!(state.escape());
        assert!(state.ordering.is_none());
        assert!(!state.escape());
    }

    #[test]
    fn a_drag_near_the_bottom_of_the_viewport_scrolls_down() {
        let (mut state, library) = own_page(None);
        state.update(Message::EditOrder, &library);
        let viewport = Viewport {
            offset: 0.0,
            height: 800.0,
        };
        state.update(Message::Pointer(Point::new(0.0, 500.0), viewport), &library);
        state.update(Message::Grab(0), &library);
        state.update(Message::Pointer(Point::new(0.0, 790.0), viewport), &library);

        assert!(state.autoscroll().is_some_and(|by| by > 0.0));
        state.update(Message::Pointer(Point::new(0.0, 400.0), viewport), &library);
        assert_eq!(state.autoscroll(), None);
    }

    #[test]
    fn a_still_pointer_keeps_scrolling_and_drops_where_it_is_after_the_scroll() {
        let (mut state, library) = own_page(None);
        state.update(Message::EditOrder, &library);
        let at = |offset| Viewport {
            offset,
            height: 800.0,
        };
        state.update(Message::Pointer(Point::new(0.0, 500.0), at(0.0)), &library);
        state.update(Message::Grab(0), &library);
        // Near the bottom edge, then the Page scrolls under the pointer.
        state.update(Message::Pointer(Point::new(0.0, 790.0), at(0.0)), &library);
        state.scrolled(at(200.0));

        assert!(state.autoscroll().is_some_and(|by| by > 0.0));
        assert_eq!(state.ordering.as_ref().unwrap().pointer, Some(990.0));
    }

    #[test]
    fn refresh_shows_the_next_ten_of_the_batch() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(50)));

        assert_eq!(recommendations.shown()[0].id, 0);
        assert_eq!(recommendations.next(), None);
        assert_eq!(recommendations.shown()[0].id, 10);
        assert_eq!(recommendations.shown().len(), 10);
    }

    #[test]
    fn refresh_past_the_batch_reads_the_next_one() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(15)));

        assert_eq!(recommendations.next(), None);
        assert_eq!(recommendations.shown().len(), 5);
        assert_eq!(recommendations.next(), Some(50));
        // Not again while it loads.
        assert_eq!(recommendations.next(), None);

        recommendations.arrived(50, Ok(batch(50)));
        assert_eq!(recommendations.offset, 50);
        assert_eq!(recommendations.shown().len(), 10);
    }

    #[test]
    fn an_empty_batch_past_the_start_starts_over() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(10)));
        assert_eq!(recommendations.next(), Some(50));

        assert_eq!(recommendations.arrived(50, Ok(Vec::new())), Some(0));
        // What was showing stays until the start arrives.
        assert_eq!(recommendations.shown().len(), 10);
    }

    #[test]
    fn a_taken_recommendation_makes_way_for_the_next() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(15)));

        recommendations.take(3);

        let shown: Vec<u64> = recommendations.shown().iter().map(|t| t.id).collect();
        assert_eq!(shown, vec![0, 1, 2, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn taking_the_last_shown_shows_the_ten_before() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(11)));
        recommendations.next();

        recommendations.take(10);

        assert_eq!(recommendations.shown().len(), 10);
    }

    #[test]
    fn a_recommendation_put_back_goes_last_and_only_once() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(5)));
        recommendations.take(2);

        recommendations.put_back(track(2, "T", "A", "B"));
        recommendations.put_back(track(2, "T", "A", "B"));

        let shown: Vec<u64> = recommendations.shown().iter().map(|t| t.id).collect();
        assert_eq!(shown, vec![0, 1, 3, 4, 2]);
    }

    #[test]
    fn a_failed_read_keeps_what_was_showing() {
        let mut recommendations = Recommendations::default();
        recommendations.arrived(0, Ok(batch(10)));
        recommendations.next();

        let failed = syzygy_tidal::Error::Parse("nope".to_string());
        recommendations.arrived(50, Err(Arc::new(failed.into())));

        assert_eq!(recommendations.shown().len(), 10);
        assert!(!recommendations.loading);
    }
}
