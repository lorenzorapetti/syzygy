//! Search: the results of a submitted query, by type, and the suggestions
//! the header's dropdown shows while the user types. Videos are left out.

use syzygy_tidal::models::{
    DirectHitItem, SuggestionsResponse, TidalArtist, TidalPlaylist, TidalSearchResults,
};

use crate::home_feed::{Card, Cover, Target, count_label};
use crate::library;
use crate::track::{AlbumRef, Track};

/// What a search found, each type in TIDAL's order of relevance.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    /// The best matches of any type, best first.
    pub top_hits: Vec<Hit>,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Card>,
    pub albums: Vec<Card>,
    pub artists: Vec<Card>,
}

/// One of the best matches: a track, or a card for anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Track(Track),
    Card(Card),
}

/// What the dropdown offers for what the user has typed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// Queries to search for instead.
    pub queries: Vec<Suggestion>,
    /// Things to open straight away.
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub query: String,
    /// From the searches TIDAL remembers the user making.
    pub searched_before: bool,
}

impl From<SuggestionsResponse> for Suggestions {
    fn from(response: SuggestionsResponse) -> Self {
        Suggestions {
            queries: response
                .text_suggestions
                .into_iter()
                .map(|item| Suggestion {
                    searched_before: item.source == "history",
                    query: item.query,
                })
                .collect(),
            hits: hits(response.direct_hits),
        }
    }
}

impl SearchResults {
    /// Nothing of any type was found.
    pub fn is_empty(&self) -> bool {
        self.top_hits.is_empty()
            && self.tracks.is_empty()
            && self.playlists.is_empty()
            && self.albums.is_empty()
            && self.artists.is_empty()
    }
}

impl From<TidalSearchResults> for SearchResults {
    fn from(results: TidalSearchResults) -> Self {
        SearchResults {
            top_hits: hits(results.top_hits),
            tracks: results.tracks.into_iter().map(Track::from).collect(),
            playlists: results.playlists.into_iter().map(playlist_card).collect(),
            albums: results
                .albums
                .into_iter()
                .map(library::album_card)
                .collect(),
            artists: results.artists.into_iter().map(artist_card).collect(),
        }
    }
}

/// TIDAL's direct hits in its order, as the dropdown writes them (sone's
/// `SearchBar`): what each is, then by whom. Videos are left out.
fn hits(items: Vec<DirectHitItem>) -> Vec<Hit> {
    items.into_iter().filter_map(hit).collect()
}

fn hit(item: DirectHitItem) -> Option<Hit> {
    let title = item.title.clone().unwrap_or_default();
    let card = match item.hit_type.as_str() {
        "ARTISTS" => Card {
            title: item.name.unwrap_or_default(),
            subtitle: "Artist".to_string(),
            cover: library::artist_picture(
                item.artwork_id,
                item.picture,
                item.selected_album_cover_fallback,
            ),
            target: Target::Artist(item.id?),
        },
        "ALBUMS" => Card {
            title,
            subtitle: kind_by("Album", item.artist_name.as_deref().unwrap_or("Unknown")),
            cover: item.cover.map(Cover::Image),
            target: Target::Album(item.id?),
        },
        "PLAYLISTS" => {
            let tracks = item.number_of_tracks.filter(|&n| n > 0);
            Card {
                title,
                subtitle: match tracks {
                    Some(n) => kind_by("Playlist", &count_label(n.into(), 0)),
                    None => "Playlist".to_string(),
                },
                cover: item.image.map(Cover::Image),
                target: Target::Playlist(item.uuid?),
            }
        }
        "TRACKS" => return Some(Hit::Track(track(item)?)),
        _ => return None,
    };
    Some(Hit::Card(card))
}

/// "Album · Björk".
fn kind_by(kind: &str, by: &str) -> String {
    format!("{kind} · {by}")
}

/// The whole track TIDAL sent, else what the hit's own fields say of it.
fn track(item: DirectHitItem) -> Option<Track> {
    if let Some(track) = item.track {
        return Some(Track::from(track));
    }
    let album = item.album_id.map(|id| AlbumRef {
        id,
        title: item.album_title.unwrap_or_default(),
        cover: item.album_cover.map(Cover::Image),
    });
    Some(Track {
        id: item.id?,
        title: item.title.unwrap_or_default(),
        // The bare hit names the artist without an id to link to.
        artists: Vec::new(),
        album,
        duration: item.duration.unwrap_or(0),
        explicit: false,
        volume: 1,
        date_added: None,
    })
}

fn artist_card(artist: TidalArtist) -> Card {
    Card {
        title: artist.name,
        subtitle: "Artist".to_string(),
        cover: library::artist_picture(
            artist.artwork_id,
            artist.picture,
            artist.selected_album_cover_fallback,
        ),
        target: Target::Artist(artist.id),
    }
}

/// Who made it and how much is in it, as Home's cards write it.
fn playlist_card(playlist: TidalPlaylist) -> Card {
    let by =
        playlist.creator.and_then(
            |creator| match creator.name.filter(|name| !name.is_empty()) {
                Some(name) => Some(format!("By {name}")),
                None if creator.id == Some(0) => Some("By TIDAL".to_string()),
                None => None,
            },
        );
    let counts = count_label(
        playlist.number_of_tracks.unwrap_or(0).into(),
        playlist.number_of_videos.unwrap_or(0).into(),
    );
    Card {
        title: playlist.title,
        subtitle: by
            .into_iter()
            .chain([counts])
            .collect::<Vec<_>>()
            .join(" · "),
        cover: playlist.image.map(Cover::Image),
        target: Target::Playlist(playlist.uuid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home_feed::{Cover, Target};
    use serde_json::{Value, json};
    use syzygy_tidal::models::{DirectHitItem, SuggestionTextItem};

    /// The items of one of a search response's lists.
    fn list<T: serde::de::DeserializeOwned>(body: &Value, key: &str) -> Vec<T> {
        serde_json::from_value(body[key]["items"].clone()).unwrap_or_default()
    }

    fn results(body: Value) -> SearchResults {
        SearchResults::from(TidalSearchResults {
            artists: list(&body, "artists"),
            albums: list(&body, "albums"),
            tracks: list(&body, "tracks"),
            playlists: list(&body, "playlists"),
            videos: Vec::new(),
            top_hit_type: None,
            top_hits: Vec::new(),
        })
    }

    #[test]
    fn each_type_of_result_is_kept_in_tidals_order() {
        let found = results(json!({
            "tracks": { "items": [
                { "id": 11, "title": "Jóga", "duration": 305,
                  "artists": [{ "id": 1, "name": "Björk" }],
                  "album": { "id": 9, "title": "Homogenic", "cover": "aa-bb" } },
            ] },
            "albums": { "items": [
                { "id": 9, "title": "Homogenic", "cover": "aa-bb",
                  "artists": [{ "id": 1, "name": "Björk" }] },
                { "id": 8, "title": "Post", "version": "Deluxe",
                  "artist": { "id": 1, "name": "Björk" } },
            ] },
            "artists": { "items": [
                { "id": 1, "name": "Björk", "picture": "pp-qq" },
            ] },
            "playlists": { "items": [
                { "uuid": "u-1", "title": "Björk Essentials", "image": "ii-jj",
                  "numberOfTracks": 40, "creator": { "id": 0 } },
            ] },
        }));

        assert_eq!(found.tracks[0].title, "Jóga");
        let albums: Vec<(&str, &str)> = found
            .albums
            .iter()
            .map(|card| (card.title.as_str(), card.subtitle.as_str()))
            .collect();
        assert_eq!(albums, [("Homogenic", "Björk"), ("Post (Deluxe)", "Björk")]);
        assert_eq!(found.albums[0].target, Target::Album(9));
        assert_eq!(found.artists[0].cover, Some(Cover::Artist("pp-qq".into())));
        assert_eq!(found.artists[0].target, Target::Artist(1));
        let playlist = &found.playlists[0];
        assert_eq!(playlist.subtitle, "By TIDAL · 40 Tracks");
        assert_eq!(playlist.cover, Some(Cover::Image("ii-jj".into())));
        assert_eq!(playlist.target, Target::Playlist("u-1".into()));
    }

    #[test]
    fn a_search_with_nothing_in_it_is_empty() {
        assert!(results(json!({})).is_empty());
        assert!(
            !results(json!({ "artists": { "items": [{ "id": 1, "name": "Björk" }] } })).is_empty()
        );
    }

    /// A search's top hits, from TIDAL's direct hits.
    fn hits(hits: Value) -> Vec<Hit> {
        let found = SearchResults::from(TidalSearchResults {
            artists: Vec::new(),
            albums: Vec::new(),
            tracks: Vec::new(),
            playlists: Vec::new(),
            videos: Vec::new(),
            top_hit_type: None,
            top_hits: DirectHitItem::parse_array(hits.as_array().unwrap()),
        });
        found.top_hits
    }

    #[test]
    fn hits_keep_tidals_order_and_leave_out_videos() {
        let hits = hits(json!([
            { "type": "ARTISTS", "value": { "id": 1, "name": "Björk", "picture": "pp-qq" } },
            { "type": "VIDEOS", "value": { "id": 5, "title": "Army of Me" } },
            { "type": "ALBUMS", "value": { "id": 9, "title": "Homogenic", "cover": "aa-bb",
                                           "artists": [{ "id": 1, "name": "Björk" }] } },
            { "type": "PLAYLISTS", "value": { "uuid": "u-1", "title": "Björk Essentials",
                                              "squareImage": "ii-jj", "numberOfTracks": 1 } },
        ]));

        let cards: Vec<(&str, &str, &Target)> = hits
            .iter()
            .map(|hit| match hit {
                Hit::Card(card) => (card.title.as_str(), card.subtitle.as_str(), &card.target),
                Hit::Track(_) => panic!("no tracks here"),
            })
            .collect();
        assert_eq!(
            cards,
            [
                ("Björk", "Artist", &Target::Artist(1)),
                ("Homogenic", "Album · Björk", &Target::Album(9)),
                (
                    "Björk Essentials",
                    "Playlist · 1 Track",
                    &Target::Playlist("u-1".into())
                ),
            ]
        );
    }

    #[test]
    fn a_track_hit_is_the_whole_track() {
        let hits = hits(json!([
            { "type": "TRACKS", "value": {
                "id": 11, "title": "Jóga", "duration": 305, "explicit": true,
                "artists": [{ "id": 1, "name": "Björk" }],
                "album": { "id": 9, "title": "Homogenic", "cover": "aa-bb" },
            } },
        ]));

        let [Hit::Track(track)] = hits.as_slice() else {
            panic!("one track: {hits:?}");
        };
        assert_eq!(track.id, 11);
        assert!(track.explicit);
        assert_eq!(track.artists[0].name, "Björk");
        assert_eq!(track.album.as_ref().map(|a| a.id), Some(9));
    }

    #[test]
    fn suggestions_say_which_queries_the_user_searched_before() {
        let suggestions = Suggestions::from(SuggestionsResponse {
            text_suggestions: vec![
                SuggestionTextItem {
                    query: "björk".into(),
                    source: "history".into(),
                },
                SuggestionTextItem {
                    query: "björk jóga".into(),
                    source: "suggestion".into(),
                },
            ],
            direct_hits: Vec::new(),
        });

        assert_eq!(
            suggestions.queries,
            [
                Suggestion {
                    query: "björk".into(),
                    searched_before: true
                },
                Suggestion {
                    query: "björk jóga".into(),
                    searched_before: false
                },
            ]
        );
    }
}
