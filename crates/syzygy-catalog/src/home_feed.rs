//! The home feed as Home draws it: tabs, and sections of cards that know
//! where they lead. Built from TIDAL's loosely typed section items with
//! sone's rules (`itemHelpers.ts`).

use serde_json::Value;
use std::collections::HashSet;
use syzygy_tidal::models::{HomePageResponse, HomePageSection};

/// One Home feed tab's content.
#[derive(Debug, Clone, Default)]
pub struct HomeFeed {
    pub tabs: Vec<Tab>,
    pub sections: Vec<Section>,
    /// Where the next page of sections starts. `None` at the end.
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub name: String,
    /// What the feed is read by.
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub layout: Layout,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The quick-access grid at the top of the default tab.
    Shortcuts,
    /// A horizontal row of cards.
    Row,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub subtitle: String,
    pub cover: Option<Cover>,
    pub target: Target,
}

/// A card's picture, by the id TIDAL serves it under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cover {
    /// Album, playlist, mix and video images: 160, 320, 640 or 1280 square.
    Image(String),
    /// Artist pictures: 160, 320, 480 or 750 square.
    Artist(String),
    /// Featured promotions: landscape, 550x400 only.
    Promo(String),
    /// A ready-made URL.
    Url(String),
}

/// What clicking a card does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Album(u64),
    Artist(u64),
    Playlist(String),
    Mix(String),
    /// The user's Loved tracks.
    Favorites,
    Track(u64),
    Video(u64),
    /// Nothing syzygy can open.
    None,
}

impl From<HomePageResponse> for HomeFeed {
    fn from(response: HomePageResponse) -> Self {
        let sections = response.sections.iter().map(section).collect();
        let tabs = response
            .tabs
            .into_iter()
            .map(|tab| Tab {
                slug: tab.tab_type.to_lowercase(),
                name: tab.name,
            })
            .collect();
        HomeFeed {
            tabs,
            sections,
            cursor: response.cursor,
        }
    }
}

fn section(section: &HomePageSection) -> Section {
    let items = items(&section.items);
    // The section type comes from the first item, so it says nothing about
    // the rest of a row that mixes types.
    let types: HashSet<&str> = items
        .iter()
        .map(item_type)
        .filter(|t| !t.is_empty())
        .collect();
    let hint = (types.len() <= 1).then_some(section.section_type.as_str());
    let cards = items.iter().map(|item| card(item, hint));
    if section.section_type != "SHORTCUT_LIST" {
        return Section {
            title: section.title.clone(),
            layout: Layout::Row,
            cards: cards.collect(),
        };
    }
    // Loved tracks always comes first, in place of TIDAL's own "My Tracks".
    let loved = Card {
        title: "Loved Tracks".to_string(),
        subtitle: String::new(),
        cover: None,
        target: Target::Favorites,
    };
    Section {
        title: section.title.clone(),
        layout: Layout::Shortcuts,
        cards: std::iter::once(loved)
            .chain(cards.filter(|card| card.target != Target::Favorites))
            .take(SHORTCUTS)
            .collect(),
    }
}

/// How many quick-access cards Home shows.
const SHORTCUTS: usize = 8;

const MY_TRACKS: &str = "tidal://my-collection/tracks";

/// TIDAL's "My Tracks" shortcut: the user's Loved tracks.
fn is_my_tracks(item: &Value) -> bool {
    if string(item, "id") == Some(MY_TRACKS) {
        return true;
    }
    if is_deep_link(item) {
        return deep_link(item) == Some(MY_TRACKS);
    }
    title(item) == "My Tracks"
        && ["uuid", "mixId", "cover"]
            .iter()
            .all(|key| item.get(key).is_none_or(Value::is_null))
}

fn is_deep_link(item: &Value) -> bool {
    item_type(item) == "DEEP_LINK" || string(item, "type") == Some("DEEP_LINK")
}

fn deep_link(item: &Value) -> Option<&str> {
    string(&item["data"], "url").or_else(|| string(&item["data"], "id"))
}

fn is_magazine(item: &Value) -> bool {
    item_type(item) == "MAGAZINE" || string(item, "type") == Some("MAGAZINE")
}

fn items(items: &Value) -> &[Value] {
    items.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn card(item: &Value, hint: Option<&str>) -> Card {
    Card {
        title: title(item).to_string(),
        subtitle: subtitle(item),
        cover: cover(item),
        target: target(item, hint),
    }
}

/// The item's own type: `_itemType` from a v2 wrapper, else `type`.
fn item_type(item: &Value) -> &str {
    string(item, "_itemType")
        .or_else(|| string(item, "type"))
        .unwrap_or_default()
}

fn is_artist(item: &Value, hint: Option<&str>) -> bool {
    hint == Some("ARTIST_LIST")
        || item_type(item) == "ARTIST"
        || (item.get("picture").is_some()
            && ["cover", "album", "images", "mixType"]
                .iter()
                .all(|key| item.get(key).is_none_or(Value::is_null)))
}

fn is_mix(item: &Value, hint: Option<&str>) -> bool {
    hint == Some("MIX_LIST")
        || item_type(item) == "MIX"
        || item.get("mixType").is_some()
        || item.get("mixImages").is_some()
}

fn is_video(item: &Value, hint: Option<&str>) -> bool {
    hint == Some("VIDEO_LIST") || matches!(item_type(item), "VIDEO" | "Music Video")
}

fn is_track(item: &Value, hint: Option<&str>) -> bool {
    hint == Some("TRACK_LIST")
        || item_type(item) == "TRACK"
        || (item.get("duration").is_some()
            && (item.get("artist").is_some() || item.get("artists").is_some())
            && item.get("album").is_some())
}

/// sone's `handleItemClick` order.
fn target(item: &Value, hint: Option<&str>) -> Target {
    let id = item.get("id").and_then(Value::as_u64);
    if is_my_tracks(item) {
        return Target::Favorites;
    }
    // Other tidal:// links have nowhere to go yet.
    if is_deep_link(item) {
        return Target::None;
    }
    if is_magazine(item) {
        let data = &item["data"];
        return match (string(data, "type"), string(data, "artifactId")) {
            (Some("PLAYLIST"), Some(uuid)) => Target::Playlist(uuid.to_string()),
            _ => Target::None,
        };
    }
    // Featured promotions name their content by `artifactId` and `type`.
    if let (Some(artifact), Some(kind)) = (string(item, "artifactId"), string(item, "type")) {
        let number = artifact.parse::<u64>().ok();
        match kind {
            "PLAYLIST" => return Target::Playlist(artifact.to_string()),
            "VIDEO" => return number.map_or(Target::None, Target::Video),
            "ALBUM" => return number.map_or(Target::None, Target::Album),
            "ARTIST" => return number.map_or(Target::None, Target::Artist),
            _ => {}
        }
    }
    if is_video(item, hint) {
        return id.map_or(Target::None, Target::Video);
    }
    if is_track(item, hint) {
        return id.map_or(Target::None, Target::Track);
    }
    if is_mix(item, hint) {
        let mix_id = string(item, "mixId")
            .map(str::to_string)
            .or_else(|| string(item, "id").map(str::to_string))
            .or_else(|| id.map(|id| id.to_string()));
        return mix_id.map_or(Target::None, Target::Mix);
    }
    if is_artist(item, hint) {
        return id.map_or(Target::None, Target::Artist);
    }
    if let Some(uuid) = string(item, "uuid") {
        return Target::Playlist(uuid.to_string());
    }
    id.map_or(Target::None, Target::Album)
}

fn title(item: &Value) -> &str {
    if is_magazine(item) {
        return string(&item["data"], "shortHeader").unwrap_or_default();
    }
    if is_deep_link(item)
        && let Some(title) = string(&item["data"], "title")
    {
        return title;
    }
    string(item, "title")
        .or_else(|| string(item, "shortHeader"))
        .or_else(|| string(item, "header"))
        .or_else(|| string(item, "name"))
        .or_else(|| text(item, "titleTextInfo"))
        .unwrap_or_default()
}

/// sone's `getItemImage` order, at card size.
fn cover(item: &Value) -> Option<Cover> {
    if is_magazine(item) {
        return string(&item["data"], "imageURL").map(|url| Cover::Url(url.to_string()));
    }
    if is_deep_link(item) {
        return None;
    }
    if string(item, "artifactId").is_some() {
        return string(item, "imageId").map(|id| Cover::Promo(id.to_string()));
    }
    let image = |key| string(item, key).map(|id| Cover::Image(id.to_string()));
    let artist = |key| string(item, key).map(|id| Cover::Artist(id.to_string()));
    let url = |url: Option<&str>| url.map(|url| Cover::Url(url.to_string()));
    let first = |key| {
        items(&item[key])
            .first()
            .and_then(|image| string(image, "url"))
    };
    url(sized(&item["images"], &["SMALL", "MEDIUM", "LARGE"]))
        .or_else(|| url(first("mixImages")))
        .or_else(|| url(sized(&item["detailImages"], &["MEDIUM", "SMALL"])))
        .or_else(|| url(first("detailMixImages")))
        .or_else(|| image("cover"))
        .or_else(|| image("squareImage"))
        .or_else(|| image("image"))
        .or_else(|| artist("artworkId"))
        .or_else(|| artist("picture"))
        .or_else(|| image("selectedAlbumCoverFallback"))
        .or_else(|| image("albumCoverFallback"))
        .or_else(|| string(&item["album"], "cover").map(|id| Cover::Image(id.to_string())))
        .or_else(|| url(string(item, "imageUrl")))
        .or_else(|| image("imageId"))
}

/// The URL of the first of `sizes` in a `{ SIZE: { url } }` map.
fn sized<'a>(images: &'a Value, sizes: &[&str]) -> Option<&'a str> {
    sizes
        .iter()
        .find_map(|size| images.get(size).and_then(|image| string(image, "url")))
}

/// sone's `getItemSubtitle`.
fn subtitle(item: &Value) -> String {
    let given = string(item, "subTitle")
        .or_else(|| string(item, "shortSubtitle"))
        .or_else(|| text(item, "subtitleTextInfo"))
        .or_else(|| text(item, "subTitleTextInfo"))
        .or_else(|| text(item, "shortSubtitleTextInfo"));
    if let Some(given) = given {
        return given.to_string();
    }
    let names: Vec<&str> = items(&item["artists"])
        .iter()
        .filter_map(|artist| string(artist, "name"))
        .collect();
    if !names.is_empty() {
        return names.join(", ");
    }
    if let Some(name) = string(&item["artist"], "name") {
        return name.to_string();
    }
    let creator = &item["creator"];
    if creator.is_object() {
        let by = match string(creator, "name") {
            Some(name) => Some(format!("By {name}")),
            None if creator["id"].as_u64() == Some(0) => Some("By TIDAL".to_string()),
            None => None,
        };
        let counts = (item.get("numberOfTracks").is_some() || item.get("numberOfVideos").is_some())
            .then(|| {
                count_label(
                    item["numberOfTracks"].as_u64().unwrap_or(0),
                    item["numberOfVideos"].as_u64().unwrap_or(0),
                )
            });
        let parts: Vec<String> = by.into_iter().chain(counts).collect();
        if !parts.is_empty() {
            return parts.join(" · ");
        }
    }
    string(item, "description").unwrap_or_default().to_string()
}

/// "2 Videos · 12 Tracks", as sone's `playlistCountLabel`.
fn count_label(tracks: u64, videos: u64) -> String {
    let plural = |n: u64, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if videos > 0 {
        parts.push(plural(videos, "Video"));
    }
    if tracks > 0 || videos == 0 {
        parts.push(plural(tracks, "Track"));
    }
    parts.join(" · ")
}

/// A non-empty string field.
fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// The text of a v2 `{ "text": … }` field.
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|info| string(info, "text"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feed(sections: Vec<(&str, &str, Value)>) -> HomeFeed {
        HomeFeed::from(HomePageResponse {
            tabs: vec![],
            sections: sections
                .into_iter()
                .map(|(title, section_type, items)| HomePageSection {
                    title: title.to_string(),
                    section_type: section_type.to_string(),
                    items,
                    has_more: false,
                    api_path: None,
                })
                .collect(),
            cursor: None,
        })
    }

    fn cards(section_type: &str, items: Value) -> Vec<Card> {
        let feed = feed(vec![("Section", section_type, items)]);
        feed.sections.into_iter().next().expect("one section").cards
    }

    #[test]
    fn an_album_card_leads_to_the_album_and_names_its_artists() {
        let cards = cards(
            "ALBUM_LIST",
            json!([{
                "_itemType": "ALBUM",
                "id": 42,
                "title": "Kid A",
                "cover": "aa-bb-cc",
                "artists": [{ "name": "Radiohead" }, { "name": "Guest" }],
            }]),
        );
        assert_eq!(
            cards,
            [Card {
                title: "Kid A".to_string(),
                subtitle: "Radiohead, Guest".to_string(),
                cover: Some(Cover::Image("aa-bb-cc".to_string())),
                target: Target::Album(42),
            }]
        );
    }

    #[test]
    fn an_artist_card_leads_to_the_artist_with_their_picture() {
        let cards = cards(
            "ARTIST_LIST",
            json!([{ "id": 7, "name": "Björk", "picture": "pp-qq" }]),
        );
        assert_eq!(cards[0].title, "Björk");
        assert_eq!(cards[0].cover, Some(Cover::Artist("pp-qq".to_string())));
        assert_eq!(cards[0].target, Target::Artist(7));
    }

    #[test]
    fn a_mix_card_leads_to_the_mix_with_its_image_url() {
        let cards = cards(
            "MIX_LIST",
            json!([{
                "id": "0123abc",
                "title": "My Daily Discovery",
                "subTitle": "Fresh picks",
                "images": {
                    "SMALL": { "url": "https://img/small.jpg" },
                    "LARGE": { "url": "https://img/large.jpg" },
                },
            }]),
        );
        assert_eq!(
            cards,
            [Card {
                title: "My Daily Discovery".to_string(),
                subtitle: "Fresh picks".to_string(),
                cover: Some(Cover::Url("https://img/small.jpg".to_string())),
                target: Target::Mix("0123abc".to_string()),
            }]
        );
    }

    #[test]
    fn a_playlist_card_leads_to_the_playlist_and_says_who_made_it() {
        let cards = cards(
            "PLAYLIST_LIST",
            json!([{
                "uuid": "u-1",
                "title": "New Arrivals",
                "squareImage": "sq-1",
                "creator": { "id": 0 },
                "numberOfTracks": 12,
            }]),
        );
        assert_eq!(cards[0].subtitle, "By TIDAL · 12 Tracks");
        assert_eq!(cards[0].cover, Some(Cover::Image("sq-1".to_string())));
        assert_eq!(cards[0].target, Target::Playlist("u-1".to_string()));
    }

    #[test]
    fn a_track_row_holds_tracks_but_a_mixed_row_typed_as_tracks_does_not() {
        let track = json!({ "_itemType": "TRACK", "id": 1, "title": "Song", "duration": 200,
            "artists": [], "album": { "cover": "al-1" } });
        let album = json!({ "_itemType": "ALBUM", "id": 2, "title": "Record", "cover": "al-2" });
        let video = json!({ "_itemType": "VIDEO", "id": 3, "title": "Clip", "imageId": "im-3" });

        let tracks = cards("TRACK_LIST", json!([track.clone(), track.clone()]));
        assert_eq!(tracks[0].target, Target::Track(1));
        assert_eq!(tracks[0].cover, Some(Cover::Image("al-1".to_string())));

        let mixed = cards("TRACK_LIST", json!([track, album, video]));
        let targets: Vec<_> = mixed.into_iter().map(|card| card.target).collect();
        assert_eq!(
            targets,
            [Target::Track(1), Target::Album(2), Target::Video(3)]
        );
    }

    #[test]
    fn shortcuts_open_with_loved_tracks_and_hold_eight() {
        let mut items = vec![json!({ "id": "tidal://my-collection/tracks", "title": "My Tracks" })];
        items.extend(
            (1..=9).map(|id| json!({ "id": id, "title": format!("Album {id}"), "cover": "c" })),
        );
        let feed = feed(vec![("Shortcuts", "SHORTCUT_LIST", Value::Array(items))]);

        let section = &feed.sections[0];
        assert_eq!(section.layout, Layout::Shortcuts);
        let targets: Vec<_> = section
            .cards
            .iter()
            .map(|card| card.target.clone())
            .collect();
        assert_eq!(
            targets,
            [
                Target::Favorites,
                Target::Album(1),
                Target::Album(2),
                Target::Album(3),
                Target::Album(4),
                Target::Album(5),
                Target::Album(6),
                Target::Album(7)
            ]
        );
        assert_eq!(section.cards[0].title, "Loved Tracks");
    }

    #[test]
    fn tabs_are_read_by_their_lowercased_type() {
        let feed = HomeFeed::from(HomePageResponse {
            tabs: vec![syzygy_tidal::models::HomeTab {
                name: "For You".to_string(),
                tab_type: "STATIC".to_string(),
            }],
            sections: vec![],
            cursor: Some("next".to_string()),
        });
        assert_eq!(
            feed.tabs,
            [Tab {
                name: "For You".to_string(),
                slug: "static".to_string()
            }]
        );
        assert_eq!(feed.cursor.as_deref(), Some("next"));
    }
}
