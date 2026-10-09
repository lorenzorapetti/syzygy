//! The user's Favorites by id: which tracks, albums, playlists and mixes
//! they like and which artists they follow, for the hearts to show, and
//! the tags each kind's reads and edits share.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use syzygy_tidal::models::AllFavoriteIds;

/// Something the user can make a Favorite, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FavoriteId {
    Track(u64),
    Album(u64),
    /// Followed, rather than liked.
    Artist(u64),
    Playlist(String),
    Mix(String),
}

impl FavoriteId {
    /// The tag of every read that lists this kind of Favorite: what liking
    /// or unliking one invalidates.
    pub fn tag(&self) -> &'static str {
        match self {
            FavoriteId::Track(_) => TRACKS,
            FavoriteId::Album(_) => ALBUMS,
            FavoriteId::Artist(_) => ARTISTS,
            FavoriteId::Playlist(_) => PLAYLISTS,
            FavoriteId::Mix(_) => MIXES,
        }
    }
}

pub(crate) const TRACKS: &str = "fav-tracks";
pub(crate) const ALBUMS: &str = "fav-albums";
pub(crate) const ARTISTS: &str = "fav-artists";
pub(crate) const PLAYLISTS: &str = "fav-playlists";
pub(crate) const MIXES: &str = "fav-mixes";

/// Every Favorite the user has, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteIds {
    pub tracks: HashSet<u64>,
    pub albums: HashSet<u64>,
    pub artists: HashSet<u64>,
    pub playlists: HashSet<String>,
    pub mixes: HashSet<String>,
}

impl FavoriteIds {
    pub(crate) fn new(ids: AllFavoriteIds, mixes: Vec<String>) -> Self {
        Self {
            tracks: ids.tracks.into_iter().collect(),
            albums: ids.albums.into_iter().collect(),
            artists: ids.artists.into_iter().collect(),
            playlists: ids.playlists.into_iter().collect(),
            mixes: mixes.into_iter().collect(),
        }
    }

    /// Put a Favorite in (`on`) or out of the sets.
    pub fn set(&mut self, id: &FavoriteId, on: bool) {
        fn toggle<T: std::hash::Hash + Eq + Clone>(set: &mut HashSet<T>, id: &T, on: bool) {
            if on {
                set.insert(id.clone());
            } else {
                set.remove(id);
            }
        }
        match id {
            FavoriteId::Track(id) => toggle(&mut self.tracks, id, on),
            FavoriteId::Album(id) => toggle(&mut self.albums, id, on),
            FavoriteId::Artist(id) => toggle(&mut self.artists, id, on),
            FavoriteId::Playlist(uuid) => toggle(&mut self.playlists, uuid, on),
            FavoriteId::Mix(id) => toggle(&mut self.mixes, id, on),
        }
    }

    pub fn contains(&self, id: &FavoriteId) -> bool {
        match id {
            FavoriteId::Track(id) => self.tracks.contains(id),
            FavoriteId::Album(id) => self.albums.contains(id),
            FavoriteId::Artist(id) => self.artists.contains(id),
            FavoriteId::Playlist(uuid) => self.playlists.contains(uuid),
            FavoriteId::Mix(id) => self.mixes.contains(id),
        }
    }
}

/// The tags of the read of every Favorite id: any Favorite's edit makes it
/// stale.
pub fn ids_tags(user_id: u64) -> Vec<String> {
    [TRACKS, ALBUMS, ARTISTS, PLAYLISTS, MIXES]
        .into_iter()
        .map(str::to_string)
        .chain([user_tag(user_id)])
        .collect()
}

/// The tags of the Loved tracks' reads.
pub fn loved_tags(user_id: u64) -> Vec<String> {
    vec![TRACKS.to_string(), user_tag(user_id)]
}

pub(crate) fn user_tag(user_id: u64) -> String {
    format!("user:{user_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_favorite_has_the_tag_its_lists_are_read_under() {
        assert_eq!(FavoriteId::Track(1).tag(), "fav-tracks");
        assert_eq!(FavoriteId::Album(1).tag(), "fav-albums");
        assert_eq!(FavoriteId::Artist(1).tag(), "fav-artists");
        assert_eq!(FavoriteId::Playlist("p".into()).tag(), "fav-playlists");
        assert_eq!(FavoriteId::Mix("m".into()).tag(), "fav-mixes");
    }

    #[test]
    fn the_id_sets_are_stale_after_any_favorite_edit() {
        let tags = ids_tags(7);
        for id in [
            FavoriteId::Track(1),
            FavoriteId::Album(1),
            FavoriteId::Artist(1),
            FavoriteId::Playlist("p".into()),
            FavoriteId::Mix("m".into()),
        ] {
            assert!(tags.iter().any(|tag| tag == id.tag()), "{id:?}");
        }
        assert!(tags.contains(&"user:7".to_string()));
    }

    #[test]
    fn the_sets_hold_tidals_ids_and_the_mixes_read_apart() {
        let ids = FavoriteIds::new(
            AllFavoriteIds {
                tracks: vec![1, 2],
                albums: vec![3],
                artists: vec![4],
                playlists: vec!["p".into()],
            },
            vec!["m".into()],
        );

        assert!(ids.contains(&FavoriteId::Track(2)));
        assert!(ids.contains(&FavoriteId::Album(3)));
        assert!(ids.contains(&FavoriteId::Artist(4)));
        assert!(ids.contains(&FavoriteId::Playlist("p".into())));
        assert!(ids.contains(&FavoriteId::Mix("m".into())));
        assert!(!ids.contains(&FavoriteId::Track(3)));
    }
}
