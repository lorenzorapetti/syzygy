//! The user's Library as the sidebar and the Library Pages read it: their
//! playlists and Folders, their Favorite albums, artists and mixes, each in
//! an order TIDAL applies, and their Loved tracks.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use syzygy_tidal::models::{
    PaginatedResponse, TidalAlbumDetail, TidalArtistDetail, TidalFavoriteMix,
};

use crate::favorites::{self, FavoriteId};
use crate::home_feed::{self, Card, Cover, Target, items, string};
use crate::paged::Paged;
use crate::playlist::{self, Direction, Playlist};
use crate::track;

/// The Folder TIDAL's own list starts from.
pub(crate) const ROOT: &str = "root";

/// The tag the playlists and Folders are read under: what a Folder edit
/// invalidates.
pub(crate) const FOLDERS: &str = "folders";

/// One type of thing in the Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// Playlists and Folders.
    Playlists,
    Albums,
    Artists,
    Mixes,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Playlists, Kind::Albums, Kind::Artists, Kind::Mixes];

    /// The orders this type can be read in, as sone offers them.
    pub fn orders(self) -> &'static [LibraryOrder] {
        use LibraryOrder::*;
        match self {
            Kind::Playlists => &[DateAdded, LastUpdated, Name],
            Kind::Albums => &[DateAdded, Name, Artist, ReleaseDate],
            Kind::Artists => &[DateAdded, Name],
            Kind::Mixes => &[DateAdded, Name, MixType],
        }
    }

    /// The order until the user picks one: playlists last updated first,
    /// the rest last added first.
    pub fn default_sort(self) -> LibrarySort {
        let order = match self {
            Kind::Playlists => LibraryOrder::LastUpdated,
            _ => LibraryOrder::DateAdded,
        };
        LibrarySort {
            order,
            direction: Direction::Descending,
        }
    }
}

/// The order a Library type is read in, applied by TIDAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySort {
    pub order: LibraryOrder,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryOrder {
    DateAdded,
    LastUpdated,
    Name,
    Artist,
    ReleaseDate,
    MixType,
}

impl LibrarySort {
    /// What picking `order` does (sone's `SortDropdown`): the order already
    /// picked turns the other way; another sorts names A to Z and the rest
    /// newest or most first.
    pub fn picked(current: LibrarySort, order: LibraryOrder) -> LibrarySort {
        let direction = if current.order == order {
            current.direction.flipped()
        } else if order == LibraryOrder::Name {
            Direction::Ascending
        } else {
            Direction::Descending
        };
        LibrarySort { order, direction }
    }

    /// TIDAL's `order` and `orderDirection`.
    pub(crate) fn params(self) -> (&'static str, &'static str) {
        let order = match self.order {
            LibraryOrder::DateAdded => "DATE",
            LibraryOrder::LastUpdated => "DATE_UPDATED",
            LibraryOrder::Name => "NAME",
            LibraryOrder::Artist => "ARTIST",
            LibraryOrder::ReleaseDate => "RELEASE_DATE",
            LibraryOrder::MixType => "MIX_TYPE",
        };
        (order, self.direction.param())
    }

    /// How the sort tells cache keys apart, as `"NAME:ASC"`.
    pub(crate) fn key(self) -> String {
        let (order, direction) = self.params();
        format!("{order}:{direction}")
    }
}

/// One list in the user's Library: a type in an order, or one Folder's
/// playlists. Reads carry the shelf they were for, so a read for a list
/// the user has moved on from can be told apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelf {
    pub user_id: u64,
    pub kind: Kind,
    /// The Folder whose playlists these are; `None` is the top level.
    /// Only playlists have Folders.
    pub folder: Option<String>,
    pub sort: LibrarySort,
}

impl Shelf {
    pub(crate) fn folder_id(&self) -> &str {
        self.folder.as_deref().unwrap_or(ROOT)
    }

    /// The tags of this shelf's reads: what its edits invalidate.
    pub fn tags(&self) -> Vec<String> {
        let kind: &[&str] = match self.kind {
            // Favorite playlists sit among the user's own, and in Folders.
            Kind::Playlists => &[FOLDERS, favorites::PLAYLISTS],
            Kind::Albums => &[favorites::ALBUMS],
            Kind::Artists => &[favorites::ARTISTS],
            Kind::Mixes => &[favorites::MIXES],
        };
        kind.iter()
            .map(|tag| tag.to_string())
            .chain([favorites::user_tag(self.user_id)])
            .collect()
    }

    /// Whether this shelf lists that kind of Favorite.
    pub fn lists(&self, id: &FavoriteId) -> bool {
        matches!(
            (self.kind, id),
            (Kind::Playlists, FavoriteId::Playlist(_))
                | (Kind::Albums, FavoriteId::Album(_))
                | (Kind::Artists, FavoriteId::Artist(_))
                | (Kind::Mixes, FavoriteId::Mix(_))
        )
    }
}

/// One thing on a shelf.
#[derive(Debug, Clone)]
pub enum Item {
    Folder(Folder),
    Playlist(Playlist),
    /// A Favorite album, artist or mix.
    Card(Card),
}

/// A named group of playlists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// How many playlists it holds, when TIDAL says.
    pub playlists: Option<u32>,
}

impl Folder {
    /// "3 playlists", or "Folder" when the count is unknown (sone's
    /// `folderSubtitle`).
    pub fn subtitle(&self) -> String {
        match self.playlists {
            Some(1) => "1 playlist".to_string(),
            Some(n) => format!("{n} playlists"),
            None => "Folder".to_string(),
        }
    }
}

/// Under a playlist in the Library: who made it ("You" for an Own
/// playlist, "TIDAL" for TIDAL's own) and how much is in it, as sone's
/// sidebar writes it.
pub fn playlist_subtitle(playlist: &Playlist, user_id: Option<u64>) -> String {
    let creator = playlist.creator.as_ref();
    let by = if playlist.is_own(user_id) {
        Some("You".to_string())
    } else if let Some(name) = playlist.creator_name() {
        Some(name.to_string())
    } else if creator.and_then(|c| c.id) == Some(0) {
        Some("TIDAL".to_string())
    } else {
        None
    };
    let counts = home_feed::count_label(playlist.tracks.into(), playlist.videos.into());
    by.into_iter()
        .chain([counts])
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A page of a Folder's contents. TIDAL pages these by cursor; its total
/// is the Folder's, which sone shows though it's not always right.
pub(crate) fn folder_page(page: &Value) -> Paged<Item> {
    let items: Vec<Item> = items(&page["items"]).iter().filter_map(entry).collect();
    let cursor = string(page, "cursor").map(str::to_string);
    Paged {
        has_more: cursor.is_some() && !items.is_empty(),
        items,
        cursor,
        total: page["totalNumberOfItems"].as_u64().map(|n| n as usize),
    }
}

/// A Folder or a playlist from a Folder's list.
fn entry(item: &Value) -> Option<Item> {
    let data = &item["data"];
    if string(item, "itemType") == Some("FOLDER") {
        let trn = string(item, "trn")?;
        return Some(Item::Folder(Folder {
            id: trn.trim_start_matches("trn:folder:").to_string(),
            name: string(item, "name").unwrap_or_default().to_string(),
            playlists: data["totalNumberOfItems"].as_u64().map(|n| n as u32),
        }));
    }
    playlist::from_details(data).map(Item::Playlist)
}

/// How TIDAL names a Folder in its Folder calls.
pub(crate) fn folder_trn(id: &str) -> String {
    format!("trn:folder:{id}")
}

/// How TIDAL names a playlist in its Folder calls.
pub(crate) fn playlist_trn(uuid: &str) -> String {
    format!("trn:playlist:{uuid}")
}

/// A page of the user's Favorite albums.
pub(crate) fn albums_page(page: PaginatedResponse<TidalAlbumDetail>) -> Paged<Item> {
    paged(page, album_card)
}

/// An album as a card: its title with its version, under its artists.
pub(crate) fn album_card(album: TidalAlbumDetail) -> Card {
    let artists = track::artists(album.artists, album.artist);
    Card {
        title: track::with_version(album.title, album.version.as_deref()),
        subtitle: artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        cover: album.cover.map(Cover::Image),
        target: Target::Album(album.id),
    }
}

/// An artist's artwork, else their picture, else the album cover TIDAL
/// falls back to (sone's `getArtistImage`).
pub(crate) fn artist_picture(
    artwork: Option<String>,
    picture: Option<String>,
    album_cover: Option<String>,
) -> Option<Cover> {
    artwork
        .or(picture)
        .map(Cover::Artist)
        .or_else(|| album_cover.map(Cover::Image))
}

/// A page of the artists the user follows.
pub(crate) fn artists_page(page: PaginatedResponse<TidalArtistDetail>) -> Paged<Item> {
    paged(page, |artist| Card {
        title: artist.name,
        subtitle: "Artist".to_string(),
        cover: artist_picture(
            artist.artwork_id,
            artist.picture,
            artist.selected_album_cover_fallback,
        ),
        target: Target::Artist(artist.id),
    })
}

/// A page of the user's Favorite mixes. TIDAL sends no total for these: a
/// full page probably has more after it.
pub(crate) fn mixes_page(page: PaginatedResponse<TidalFavoriteMix>) -> Paged<Item> {
    let full = page.items.len() >= page.limit as usize;
    let mut paged = paged(page, |mix| {
        let images = mix.images.as_ref();
        let url = images
            .and_then(|images| images.medium.as_ref().or(images.small.as_ref()))
            .map(|image| Cover::Url(image.url.clone()));
        Card {
            title: mix.title.unwrap_or_default(),
            subtitle: mix
                .sub_title
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Mix".to_string()),
            cover: url,
            target: Target::Mix(mix.id),
        }
    });
    paged.has_more = full && !paged.items.is_empty();
    paged.total = None;
    paged
}

/// A page of cards, with more after it until TIDAL's total is reached.
fn paged<T>(page: PaginatedResponse<T>, card: impl Fn(T) -> Card) -> Paged<Item> {
    let end = page.offset as usize + page.items.len();
    let total = page.total_number_of_items as usize;
    Paged {
        has_more: !page.items.is_empty() && end < total,
        items: page.items.into_iter().map(card).map(Item::Card).collect(),
        cursor: None,
        total: Some(total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sort(order: LibraryOrder, direction: Direction) -> LibrarySort {
        LibrarySort { order, direction }
    }

    #[test]
    fn playlists_start_last_updated_first_and_the_rest_last_added_first() {
        use LibraryOrder::*;
        let descending = |order| sort(order, Direction::Descending);

        assert_eq!(Kind::Playlists.default_sort(), descending(LastUpdated));
        assert_eq!(Kind::Albums.default_sort(), descending(DateAdded));
        assert_eq!(Kind::Artists.default_sort(), descending(DateAdded));
        assert_eq!(Kind::Mixes.default_sort(), descending(DateAdded));
    }

    #[test]
    fn each_type_offers_its_own_orders() {
        use LibraryOrder::*;

        assert_eq!(Kind::Playlists.orders(), &[DateAdded, LastUpdated, Name]);
        assert_eq!(
            Kind::Albums.orders(),
            &[DateAdded, Name, Artist, ReleaseDate]
        );
        assert_eq!(Kind::Artists.orders(), &[DateAdded, Name]);
        assert_eq!(Kind::Mixes.orders(), &[DateAdded, Name, MixType]);
    }

    #[test]
    fn picking_the_same_order_turns_it_the_other_way() {
        let by_date = sort(LibraryOrder::DateAdded, Direction::Descending);

        let flipped = LibrarySort::picked(by_date, LibraryOrder::DateAdded);
        assert_eq!(flipped, sort(LibraryOrder::DateAdded, Direction::Ascending));
        assert_eq!(
            LibrarySort::picked(flipped, LibraryOrder::DateAdded),
            by_date
        );
    }

    #[test]
    fn picking_another_order_sorts_names_a_to_z_and_the_rest_newest_first() {
        let by_name = sort(LibraryOrder::Name, Direction::Descending);
        let by_date = sort(LibraryOrder::DateAdded, Direction::Ascending);

        assert_eq!(
            LibrarySort::picked(by_date, LibraryOrder::Name),
            sort(LibraryOrder::Name, Direction::Ascending)
        );
        assert_eq!(
            LibrarySort::picked(by_name, LibraryOrder::ReleaseDate),
            sort(LibraryOrder::ReleaseDate, Direction::Descending)
        );
    }

    #[test]
    fn sorts_go_to_tidal_as_sone_sends_them() {
        use LibraryOrder::*;
        let params = |order| sort(order, Direction::Ascending).params();

        assert_eq!(params(DateAdded), ("DATE", "ASC"));
        assert_eq!(params(LastUpdated), ("DATE_UPDATED", "ASC"));
        assert_eq!(params(Name), ("NAME", "ASC"));
        assert_eq!(params(Artist), ("ARTIST", "ASC"));
        assert_eq!(params(ReleaseDate), ("RELEASE_DATE", "ASC"));
        assert_eq!(params(MixType), ("MIX_TYPE", "ASC"));
    }

    fn folder_response(cursor: Value) -> Value {
        json!({
            "totalNumberOfItems": 7,
            "cursor": cursor,
            "items": [
                {
                    "trn": "trn:folder:f-1",
                    "itemType": "FOLDER",
                    "name": "Running",
                    "parent": null,
                    "data": { "totalNumberOfItems": 3 },
                },
                {
                    "trn": "trn:playlist:p-1",
                    "itemType": "PLAYLIST",
                    "name": "Mine",
                    "data": {
                        "uuid": "p-1",
                        "title": "Mine",
                        "squareImage": "sq-1",
                        "numberOfTracks": 12,
                        "creator": { "id": 7, "name": null },
                    },
                },
            ],
        })
    }

    #[test]
    fn a_folder_lists_its_folders_and_playlists() {
        let page = folder_page(&folder_response(json!(null)));

        let Item::Folder(folder) = &page.items[0] else {
            panic!("a Folder first");
        };
        assert_eq!(
            folder,
            &Folder {
                id: "f-1".to_string(),
                name: "Running".to_string(),
                playlists: Some(3),
            }
        );
        let Item::Playlist(playlist) = &page.items[1] else {
            panic!("then a playlist");
        };
        assert_eq!(playlist.uuid, "p-1");
        assert_eq!(playlist.cover, Some(Cover::Image("sq-1".to_string())));
        assert_eq!(page.total, Some(7));
    }

    #[test]
    fn folders_and_playlists_are_named_by_trn() {
        assert_eq!(folder_trn("f-1"), "trn:folder:f-1");
        assert_eq!(playlist_trn("p-1"), "trn:playlist:p-1");
    }

    #[test]
    fn a_folder_has_more_while_tidal_sends_a_cursor() {
        let more = folder_page(&folder_response(json!("next-page")));
        assert!(more.has_more);
        assert_eq!(more.cursor.as_deref(), Some("next-page"));

        let last = folder_page(&folder_response(json!(null)));
        assert!(!last.has_more);
        assert_eq!(last.cursor, None);
    }

    #[test]
    fn an_empty_folder_page_ends_the_list_whatever_the_cursor() {
        let page = folder_page(&json!({ "items": [], "cursor": "again" }));
        assert!(!page.has_more);
    }

    #[test]
    fn a_folder_count_reads_as_sone_writes_it() {
        let folder = |playlists| Folder {
            id: "f".to_string(),
            name: "F".to_string(),
            playlists,
        };

        assert_eq!(folder(Some(1)).subtitle(), "1 playlist");
        assert_eq!(folder(Some(0)).subtitle(), "0 playlists");
        assert_eq!(folder(Some(4)).subtitle(), "4 playlists");
        assert_eq!(folder(None).subtitle(), "Folder");
    }

    fn playlist(creator: Value, tracks: u32, videos: u32) -> Playlist {
        playlist::from_details(&json!({
            "uuid": "p",
            "title": "P",
            "creator": creator,
            "numberOfTracks": tracks,
            "numberOfVideos": videos,
        }))
        .unwrap()
    }

    #[test]
    fn an_own_playlist_is_by_you() {
        let mine = playlist(json!({ "id": 7, "name": "Lorenzo" }), 12, 0);
        assert_eq!(playlist_subtitle(&mine, Some(7)), "You · 12 Tracks");
    }

    #[test]
    fn someone_elses_playlist_names_its_creator_or_tidal() {
        let theirs = playlist(json!({ "id": 8, "name": "Ana" }), 1, 2);
        assert_eq!(
            playlist_subtitle(&theirs, Some(7)),
            "Ana · 2 Videos · 1 Track"
        );

        let tidals = playlist(json!({ "id": 0 }), 30, 0);
        assert_eq!(playlist_subtitle(&tidals, Some(7)), "TIDAL · 30 Tracks");

        let unknown = playlist(json!({}), 0, 0);
        assert_eq!(playlist_subtitle(&unknown, Some(7)), "0 Tracks");
    }

    fn response<T>(items: Vec<T>, offset: u32, total: u32) -> PaginatedResponse<T> {
        PaginatedResponse {
            items,
            total_number_of_items: total,
            offset,
            limit: 50,
        }
    }

    fn cards(page: Paged<Item>) -> Vec<Card> {
        page.items
            .into_iter()
            .map(|item| match item {
                Item::Card(card) => card,
                other => panic!("a card, not {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_favorite_album_leads_to_the_album_and_names_its_artists() {
        let album: TidalAlbumDetail = serde_json::from_value(json!({
            "id": 42,
            "title": "Kid A",
            "version": "Deluxe",
            "cover": "aa-bb",
            "artists": [{ "id": 1, "name": "Radiohead" }, { "id": 2, "name": "Guest" }],
        }))
        .unwrap();

        let cards = cards(albums_page(response(vec![album], 0, 1)));

        assert_eq!(
            cards,
            vec![Card {
                title: "Kid A (Deluxe)".to_string(),
                subtitle: "Radiohead, Guest".to_string(),
                cover: Some(Cover::Image("aa-bb".to_string())),
                target: Target::Album(42),
            }]
        );
    }

    #[test]
    fn a_followed_artist_shows_their_picture_or_an_album_cover() {
        let artist = |value| serde_json::from_value::<TidalArtistDetail>(value).unwrap();
        let page = response(
            vec![
                artist(json!({ "id": 1, "name": "Björk", "picture": "pic-1" })),
                artist(json!({ "id": 2, "name": "Sade", "selectedAlbumCoverFallback": "alb-2" })),
            ],
            0,
            2,
        );

        let cards = cards(artists_page(page));

        assert_eq!(cards[0].cover, Some(Cover::Artist("pic-1".to_string())));
        assert_eq!(cards[0].subtitle, "Artist");
        assert_eq!(cards[0].target, Target::Artist(1));
        assert_eq!(cards[1].cover, Some(Cover::Image("alb-2".to_string())));
    }

    #[test]
    fn favorite_albums_and_artists_have_more_until_the_total_is_reached() {
        let artist = |id| {
            serde_json::from_value::<TidalArtistDetail>(json!({ "id": id, "name": "A" })).unwrap()
        };

        assert!(artists_page(response(vec![artist(1)], 0, 2)).has_more);
        let last = artists_page(response(vec![artist(2)], 1, 2));
        assert!(!last.has_more);
        assert_eq!(last.total, Some(2));
    }

    fn mix(id: &str, sub_title: Option<&str>) -> TidalFavoriteMix {
        serde_json::from_value(json!({
            "id": id,
            "title": "My Mix",
            "subTitle": sub_title,
            "images": { "SMALL": { "url": "small.jpg" }, "MEDIUM": { "url": "medium.jpg" } },
        }))
        .unwrap()
    }

    #[test]
    fn a_favorite_mix_leads_to_the_mix() {
        let cards = cards(mixes_page(response(vec![mix("m-1", None)], 0, 1)));

        assert_eq!(
            cards,
            vec![Card {
                title: "My Mix".to_string(),
                subtitle: "Mix".to_string(),
                cover: Some(Cover::Url("medium.jpg".to_string())),
                target: Target::Mix("m-1".to_string()),
            }]
        );
    }

    #[test]
    fn favorite_mixes_have_more_while_pages_come_back_full() {
        let full = response(
            (0..50).map(|n| mix(&n.to_string(), Some("S"))).collect(),
            0,
            51,
        );
        let page = mixes_page(full);
        assert!(page.has_more);
        assert_eq!(page.total, None);

        let short = response(vec![mix("m", Some("S"))], 50, 51);
        assert!(!mixes_page(short).has_more);
    }
}
