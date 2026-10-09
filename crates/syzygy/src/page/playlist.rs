//! The Playlist Page: a playlist's tracks a page at a time, sorted by TIDAL
//! and filtered here, then the tracks TIDAL recommends for it.

use iced::widget::{Column, button, column, container, row, space, text, text_input};
use iced::{Alignment, Element, Length};
use std::sync::Arc;
use syzygy_catalog::{Paged, Playlist, Read, Track, TrackOrder, TrackSort};

use super::paged::List;
use super::track_list::{self, Columns};
use super::{
    Action, Context, Link, Load, NowPlaying, PADDING, Preview, Remote, Route, Viewport, count,
    duration, hero, play_buttons,
};
use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::playback::{SourceRef, Start};
use crate::settings::Sort;
use crate::style;

const SPACING: f32 = 32.0;
const FILTER_HEIGHT: f32 = 40.0;
const FILTER_WIDTH: f32 = 360.0;
/// Where the track list starts.
const LIST_TOP: f32 = PADDING + hero::HEIGHT + SPACING + FILTER_HEIGHT + SPACING;
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
    /// The loaded tracks the filter lets through, by their place in the
    /// list. In the own order that's the track's index in the playlist.
    shown: Vec<usize>,
    recommendations: Recommendations,
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
    Link(Link),
    /// Play the loaded tracks, in the order shown, from a track by its
    /// place in the list or all of them.
    Play(Start),
    TogglePlay,
    Retry,
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
            shown: Vec::new(),
            recommendations: Recommendations::default(),
        };
        (state, action)
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Playlist(read) => {
                self.playlist.apply(read, "a playlist");
                Action::None
            }
            Message::Tracks { sort, read } if sort == self.sort => {
                self.tracks.apply(read, "playlist's tracks");
                self.loaded()
            }
            Message::More {
                sort,
                offset,
                result,
            } if sort == self.sort => {
                self.tracks.more(offset, result);
                self.loaded()
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
                self.loaded()
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
            Message::Link(link) => link.follow(),
            Message::Play(start) => {
                let tracks = self.tracks.items();
                if tracks.is_empty() {
                    return Action::None;
                }
                let name = match (&self.playlist, &self.preview) {
                    (Remote::Loaded(playlist), _) => playlist.title.as_str(),
                    (_, Some(preview)) => preview.title.as_str(),
                    _ => "Playlist",
                };
                let request = super::request(self.source(), name, tracks, start);
                Action::Play(super::with_rest(request, self.tracks.has_more()))
            }
            Message::TogglePlay => Action::TogglePlay,
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
        self.shown.clear();
    }

    /// The tracks or the filter changed: filter them again, and while a
    /// filter is on, load the rest so it sees every track.
    fn loaded(&mut self) -> Action {
        self.shown = track_list::matching(self.tracks.items(), &self.filter);
        self.load_rest_if_filtering()
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

    fn is_own(&self) -> bool {
        self.playlist
            .loaded()
            .is_some_and(|playlist| playlist.is_own(self.user_id))
    }

    pub fn view<'a>(
        &'a self,
        images: &'a Images,
        viewport: Viewport,
        now_playing: Option<NowPlaying<'a>>,
    ) -> Element<'a, Message> {
        let hero = match (&self.playlist, &self.preview) {
            (Remote::Loaded(playlist), _) => Some(self.hero(playlist, images)),
            (Remote::Loading, Some(preview)) => {
                Some(hero::preview(images, "PLAYLIST", preview, false))
            }
            _ => None,
        };
        let filter = container(
            text_input("Filter playlist on title, artist or album", &self.filter)
                .on_input(Message::Filter)
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
        let filter = row![buttons, space::horizontal(), filter].align_y(Alignment::Center);
        let columns = Columns {
            cover: true,
            album: true,
            date_added: self.is_own(),
        };
        let body = self.tracks.list.view(Message::Retry, |_| {
            let tracks = self.tracks.items();
            if tracks.is_empty() {
                return empty(
                    "This playlist is empty",
                    "Add tracks in TIDAL to see them here.",
                );
            }
            let header = track_list::sortable_header(columns, self.sort, Message::Sort);
            let list = track_list::view(self.shown.len(), LIST_TOP, viewport, header, |i| {
                let position = self.shown[i];
                let track = &tracks[position];
                let row = track_list::track(images, position + 1, track, columns);
                track_list::playable_track(
                    row.map(Message::Link),
                    track,
                    now_playing,
                    Message::Play(Start::Track(position)),
                )
            });
            let nothing_matches = (self.shown.is_empty() && !self.filter.trim().is_empty())
                .then(|| empty("Nothing matches", "Try another title, artist or album."));
            let all_loaded = self.tracks.list.loaded().is_some_and(|page| !page.has_more);
            Column::new()
                .push(list)
                .push(nothing_matches)
                .push(self.tracks.end(Message::EndInView, Message::RetryMore))
                .push(all_loaded.then(|| self.recommendations(images)).flatten())
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
        Column::new()
            .push(hero.map(|hero| hero.map(Message::Link)))
            .push(filter)
            .push(body)
            .spacing(SPACING)
            .padding(PADDING)
            .into()
    }

    fn hero<'a>(&'a self, playlist: &'a Playlist, images: &'a Images) -> Element<'a, Link> {
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
        let lines = lines.into_iter().flatten().collect();
        hero::view(
            images,
            "PLAYLIST",
            &playlist.title,
            playlist.cover.as_ref(),
            false,
            lines,
        )
    }

    /// Ten of the recommendations, and Refresh for the next ten.
    fn recommendations<'a>(&'a self, images: &'a Images) -> Option<Element<'a, Message>> {
        let shown = self.recommendations.shown();
        if shown.is_empty() {
            return None;
        }
        let columns = Columns {
            cover: true,
            album: true,
            date_added: false,
        };
        let rows = shown
            .iter()
            .enumerate()
            .map(|(i, track)| track_list::track(images, i + 1, track, columns).map(Message::Link));
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
            column![track_list::header(columns)].extend(rows),
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
            volume: 1,
            date_added: None,
            track_radio: None,
        }
    }

    fn batch(n: usize) -> Vec<Track> {
        (0..n as u64).map(|id| track(id, "T", "A", "B")).collect()
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
