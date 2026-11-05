//! A playlist's Page: what it is and who made it, its tracks a page at a
//! time in the order the user picked, and its recommendations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use syzygy_tidal::models::{PaginatedTracks, TidalPlaylist, TidalPlaylistRaw};

use crate::home_feed::Cover;
use crate::paged::Paged;
use crate::track::Track;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub cover: Option<Cover>,
    pub creator: Option<Creator>,
    pub tracks: u32,
    pub videos: u32,
    /// In seconds.
    pub duration: u32,
    /// Anyone can find it. Otherwise it's unlisted: only its link leads
    /// to it.
    #[serde(default)]
    pub public: bool,
}

/// Who made a playlist. TIDAL's own playlists have no id, or id 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Creator {
    pub id: Option<u64>,
    pub name: Option<String>,
    /// A user, rather than an artist or TIDAL.
    #[serde(default)]
    pub user: bool,
}

impl Playlist {
    /// One of the user's Own playlists: they made it.
    pub fn is_own(&self, user_id: Option<u64>) -> bool {
        let creator = self.creator.as_ref().and_then(|c| c.id);
        matches!((creator, user_id), (Some(creator), Some(user)) if creator == user)
    }

    pub fn creator_name(&self) -> Option<&str> {
        self.creator.as_ref()?.name.as_deref()
    }

    /// The user whose Profile the creator is: only a user's playlist has
    /// one. An artist's or TIDAL's is made by someone with no profile.
    pub fn profile(&self) -> Option<u64> {
        let creator = self.creator.as_ref().filter(|creator| creator.user)?;
        creator.id.filter(|&id| id != 0)
    }
}

/// A playlist from its details, `None` if it isn't one.
pub(crate) fn from_details(details: &Value) -> Option<Playlist> {
    let raw: TidalPlaylistRaw = serde_json::from_value(details.clone()).ok()?;
    Some(from_tidal(TidalPlaylist::from(raw)))
}

/// A playlist as one of TIDAL's lists gives it.
pub(crate) fn from_tidal(playlist: TidalPlaylist) -> Playlist {
    Playlist {
        uuid: playlist.uuid,
        title: playlist.title,
        description: playlist
            .description
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        cover: playlist.image.map(Cover::Image),
        creator: playlist.creator.map(|c| Creator {
            id: c.id,
            name: c.name.filter(|n| !n.is_empty()),
            user: playlist.playlist_type.as_deref() == Some("USER"),
        }),
        tracks: playlist.number_of_tracks.unwrap_or(0),
        videos: playlist.number_of_videos.unwrap_or(0),
        duration: playlist.duration.unwrap_or(0),
        public: playlist.access_type.as_deref() == Some("PUBLIC"),
    }
}

/// Tracks added to a playlist, as TIDAL counted them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    /// How many were to be added.
    pub asked: usize,
    /// How many the playlist didn't have already.
    pub new: usize,
    /// How many tracks the playlist has now.
    pub tracks: u32,
}

impl Added {
    /// How many it had already, which TIDAL skipped.
    pub fn skipped(&self) -> usize {
        self.asked.saturating_sub(self.new)
    }
}

/// What the user sets on an Own playlist: its title, its description
/// (at most [`DESCRIPTION_LIMIT`] characters) and who can find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistFields {
    pub title: String,
    pub description: String,
    pub public: bool,
}

/// The most characters a playlist's description has.
pub const DESCRIPTION_LIMIT: usize = 500;

impl PlaylistFields {
    /// A new playlist's: unlisted until the user says otherwise.
    pub fn new() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            public: false,
        }
    }

    /// What `playlist` has now.
    pub fn of(playlist: &Playlist) -> Self {
        Self {
            title: playlist.title.clone(),
            description: playlist.description.clone().unwrap_or_default(),
            public: playlist.public,
        }
    }

    /// TIDAL's `accessType`.
    pub(crate) fn access(&self) -> &'static str {
        if self.public { "PUBLIC" } else { "UNLISTED" }
    }

    /// The title and description trimmed, the description cut to its
    /// limit.
    pub fn trimmed(&self) -> Self {
        Self {
            title: self.title.trim().to_string(),
            description: self
                .description
                .trim()
                .chars()
                .take(DESCRIPTION_LIMIT)
                .collect(),
            public: self.public,
        }
    }

    /// `playlist` with these set on it.
    pub fn applied(&self, playlist: &Playlist) -> Playlist {
        let fields = self.trimmed();
        Playlist {
            title: fields.title,
            description: Some(fields.description).filter(|d| !d.is_empty()),
            public: fields.public,
            ..playlist.clone()
        }
    }

    /// The Own playlist of `user_id` these make, empty, under `uuid`.
    pub fn playlist(&self, uuid: String, user_id: u64) -> Playlist {
        let empty = Playlist {
            uuid,
            title: String::new(),
            description: None,
            cover: None,
            creator: Some(Creator {
                id: Some(user_id),
                name: None,
                user: true,
            }),
            tracks: 0,
            videos: 0,
            duration: 0,
            public: false,
        };
        self.applied(&empty)
    }
}

impl Default for PlaylistFields {
    fn default() -> Self {
        Self::new()
    }
}

/// The order a playlist's tracks are read in, applied by TIDAL. No sort is
/// the playlist's own order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSort {
    pub order: TrackOrder,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackOrder {
    Title,
    Artist,
    Album,
    DateAdded,
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Ascending,
    Descending,
}

impl TrackSort {
    /// What a click on `clicked`'s column title does (sone's
    /// `handleHeaderClick`): the same column turns the other way, another
    /// column sorts ascending, and `None` goes back to the own order.
    pub fn clicked(current: Option<TrackSort>, clicked: Option<TrackOrder>) -> Option<TrackSort> {
        let order = clicked?;
        let direction = match current {
            Some(current) if current.order == order => current.direction.flipped(),
            _ => Direction::Ascending,
        };
        Some(TrackSort { order, direction })
    }

    /// TIDAL's `order` and `orderDirection`.
    pub(crate) fn params(self) -> (&'static str, &'static str) {
        let order = match self.order {
            TrackOrder::Title => "NAME",
            TrackOrder::Artist => "ARTIST",
            TrackOrder::Album => "ALBUM",
            TrackOrder::DateAdded => "DATE",
            TrackOrder::Duration => "LENGTH",
        };
        (order, self.direction.param())
    }

    /// How the sort tells cache keys apart, as `"NAME:ASC"`.
    pub(crate) fn key(sort: Option<TrackSort>) -> String {
        match sort {
            Some(sort) => {
                let (order, direction) = sort.params();
                format!("{order}:{direction}")
            }
            None => "own".to_string(),
        }
    }
}

impl Direction {
    pub(crate) fn flipped(self) -> Direction {
        match self {
            Direction::Ascending => Direction::Descending,
            Direction::Descending => Direction::Ascending,
        }
    }

    /// TIDAL's `orderDirection`.
    pub(crate) fn param(self) -> &'static str {
        match self {
            Direction::Ascending => "ASC",
            Direction::Descending => "DESC",
        }
    }
}

/// A page of a playlist's tracks. TIDAL counts tracks and videos alike.
pub(crate) fn tracks_page(page: PaginatedTracks) -> Paged<Track> {
    let end = page.offset as usize + page.items.len();
    Paged {
        has_more: !page.items.is_empty() && end < page.total_number_of_items as usize,
        items: page.items.into_iter().map(Track::from).collect(),
        cursor: None,
        total: Some(page.total_number_of_items as usize),
    }
}

pub(crate) fn encode(playlist: &Playlist) -> Option<Vec<u8>> {
    serde_json::to_vec(playlist).ok()
}

pub(crate) fn decode(bytes: &[u8]) -> Option<Playlist> {
    serde_json::from_slice(bytes).ok()
}

pub(crate) fn encode_page(page: &PaginatedTracks) -> Option<Vec<u8>> {
    serde_json::to_vec(page).ok()
}

pub(crate) fn decode_page(bytes: &[u8]) -> Option<PaginatedTracks> {
    serde_json::from_slice(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use syzygy_tidal::models::TidalTrack;

    fn track(id: u64) -> TidalTrack {
        serde_json::from_value(json!({
            "id": id,
            "title": format!("Track {id}"),
            "duration": 200,
            "dateAdded": "2024-03-05T10:00:00.000+0000",
        }))
        .unwrap()
    }

    fn page(offset: u32, ids: std::ops::Range<u64>, total: u32) -> PaginatedTracks {
        PaginatedTracks {
            items: ids.map(track).collect(),
            total_number_of_items: total,
            offset,
            limit: 50,
        }
    }

    #[test]
    fn details_give_the_square_cover_and_who_made_it() {
        let playlist = from_details(&json!({
            "uuid": "u-1",
            "title": "Mine",
            "description": "  ",
            "image": "wide-image",
            "squareImage": "sq-image",
            "numberOfTracks": 12,
            "numberOfVideos": 1,
            "duration": 3000,
            "creator": { "id": 7, "name": "" },
        }))
        .unwrap();

        assert_eq!(playlist.uuid, "u-1");
        assert_eq!(playlist.title, "Mine");
        assert_eq!(playlist.description, None);
        assert_eq!(playlist.cover, Some(Cover::Image("sq-image".to_string())));
        assert_eq!(
            playlist.creator,
            Some(Creator {
                id: Some(7),
                name: None,
                user: false,
            })
        );
        assert_eq!(
            (playlist.tracks, playlist.videos, playlist.duration),
            (12, 1, 3000)
        );
    }

    #[test]
    fn a_playlist_is_public_only_when_tidal_says_so() {
        let public = |flag: Value| {
            from_details(&json!({ "uuid": "u", "title": "T", "publicPlaylist": flag }))
                .unwrap()
                .public
        };

        assert!(public(json!(true)));
        assert!(!public(json!(false)));
        assert!(!public(Value::Null));
    }

    #[test]
    fn a_new_playlist_is_unlisted() {
        let fields = PlaylistFields::new();

        assert!(!fields.public);
        assert!(!fields.playlist("u-1".to_string(), 7).public);
    }

    #[test]
    fn a_description_is_cut_to_500_characters() {
        let fields = PlaylistFields {
            title: "  Mine ".to_string(),
            description: "é".repeat(600),
            public: true,
        };

        let trimmed = fields.trimmed();

        assert_eq!(trimmed.title, "Mine");
        assert_eq!(trimmed.description.chars().count(), DESCRIPTION_LIMIT);
        assert!(trimmed.public);
    }

    #[test]
    fn a_made_playlist_is_the_users_own_and_empty() {
        let fields = PlaylistFields {
            title: "Mine".to_string(),
            description: " ".to_string(),
            public: false,
        };

        let playlist = fields.playlist("u-1".to_string(), 7);

        assert!(playlist.is_own(Some(7)));
        assert_eq!((playlist.title.as_str(), playlist.tracks), ("Mine", 0));
        assert_eq!(playlist.description, None);
    }

    #[test]
    fn a_playlist_is_own_only_when_the_signed_in_user_made_it() {
        let made_by = |id: Option<u64>| {
            from_details(&json!({ "uuid": "u", "title": "T", "creator": { "id": id } })).unwrap()
        };

        assert!(made_by(Some(7)).is_own(Some(7)));
        assert!(!made_by(Some(8)).is_own(Some(7)));
        assert!(!made_by(None).is_own(Some(7)));
        assert!(!made_by(Some(7)).is_own(None));
    }

    #[test]
    fn only_a_users_playlist_leads_to_its_creators_profile() {
        let playlist = |kind: &str, id: u64| {
            from_details(&json!({
                "uuid": "u",
                "title": "T",
                "type": kind,
                "creator": { "id": id, "name": "Ada" },
            }))
            .unwrap()
        };

        assert_eq!(playlist("USER", 7).profile(), Some(7));
        assert_eq!(playlist("USER", 0).profile(), None);
        assert_eq!(playlist("EDITORIAL", 7).profile(), None);
        assert_eq!(playlist("ARTIST", 7).profile(), None);
        let untyped = from_details(&json!({ "uuid": "u", "title": "T", "creator": { "id": 7 } }));
        assert_eq!(untyped.unwrap().profile(), None);
    }

    #[test]
    fn something_that_isnt_a_playlist_gives_none() {
        assert!(from_details(&json!({ "status": 404 })).is_none());
    }

    #[test]
    fn a_page_has_more_until_the_total_is_reached() {
        assert!(tracks_page(page(0, 0..50, 120)).has_more);
        assert!(tracks_page(page(50, 50..100, 120)).has_more);
        assert!(!tracks_page(page(100, 100..120, 120)).has_more);
    }

    #[test]
    fn an_empty_page_ends_the_list_whatever_the_total_says() {
        assert!(!tracks_page(page(100, 0..0, 120)).has_more);
    }

    #[test]
    fn tracks_keep_the_date_they_were_added() {
        let tracks = tracks_page(page(0, 0..1, 1)).items;

        assert_eq!(
            tracks[0].date_added.as_deref(),
            Some("2024-03-05T10:00:00.000+0000")
        );
    }

    #[test]
    fn a_column_title_sorts_ascending_then_flips() {
        let sort = |order, direction| Some(TrackSort { order, direction });
        let title = Some(TrackOrder::Title);

        let first = TrackSort::clicked(None, title);
        assert_eq!(first, sort(TrackOrder::Title, Direction::Ascending));
        let second = TrackSort::clicked(first, title);
        assert_eq!(second, sort(TrackOrder::Title, Direction::Descending));
        assert_eq!(
            TrackSort::clicked(second, title),
            sort(TrackOrder::Title, Direction::Ascending)
        );
    }

    #[test]
    fn another_column_sorts_ascending_and_none_goes_back_to_the_own_order() {
        let by_title = Some(TrackSort {
            order: TrackOrder::Title,
            direction: Direction::Descending,
        });

        assert_eq!(
            TrackSort::clicked(by_title, Some(TrackOrder::Duration)),
            Some(TrackSort {
                order: TrackOrder::Duration,
                direction: Direction::Ascending
            })
        );
        assert_eq!(TrackSort::clicked(by_title, None), None);
    }

    #[test]
    fn sorts_go_to_tidal_as_sone_sends_them() {
        let params = |order| {
            TrackSort {
                order,
                direction: Direction::Descending,
            }
            .params()
        };

        assert_eq!(params(TrackOrder::Title), ("NAME", "DESC"));
        assert_eq!(params(TrackOrder::Artist), ("ARTIST", "DESC"));
        assert_eq!(params(TrackOrder::Album), ("ALBUM", "DESC"));
        assert_eq!(params(TrackOrder::DateAdded), ("DATE", "DESC"));
        assert_eq!(params(TrackOrder::Duration), ("LENGTH", "DESC"));
    }
}
