//! An artist's Page, from TIDAL's v2 artist page or the older v1 one
//! (sone's `parseArtistPageResponse`).

use serde_json::Value;

use crate::Paged;
use crate::home_feed::{self, Card, Cover, items, string};
use crate::track::Track;

#[derive(Debug, Clone, Default)]
pub struct Artist {
    pub name: String,
    pub picture: Option<Cover>,
    pub bio: Option<String>,
    pub followers: Option<u64>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub content: Content,
    /// Where the whole section is read, for its "View all" Page.
    pub view_all: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Content {
    Tracks(Vec<Track>),
    Cards(Vec<Card>),
}

impl From<Value> for Artist {
    fn from(page: Value) -> Self {
        if page.get("item").is_some() {
            v2(&page)
        } else {
            v1(&page)
        }
    }
}

fn v1(page: &Value) -> Artist {
    let mut artist = Artist::default();
    let modules = items(&page["rows"])
        .iter()
        .flat_map(|row| items(&row["modules"]));
    for module in modules {
        let kind = string(module, "type").unwrap_or_default();
        if kind == "ARTIST_HEADER" {
            let data = &module["artist"];
            artist.name = string(data, "name").unwrap_or_default().to_string();
            artist.picture = picture(data);
            artist.bio = string(&module["bio"], "text").map(strip_bio);
            continue;
        }
        let items = items(&module["pagedList"]["items"]);
        if items.is_empty() {
            continue;
        }
        let title = match string(module, "title") {
            None if kind == "TRACK_LIST" => "Popular tracks",
            title => title.unwrap_or_default(),
        };
        artist.sections.push(section(
            title,
            kind,
            items,
            string(&module["showMore"], "apiPath"),
        ));
    }
    artist
}

/// A page of an artist's top tracks, asked for `limit` at a time.
pub(crate) fn tracks_page(page: &Value, limit: usize) -> Paged<Track> {
    let items = items(&page["items"]);
    Paged {
        items: items.iter().filter_map(Track::from_value).collect(),
        // TIDAL sends no total here: a full page probably has more after it.
        has_more: items.len() >= limit,
    }
}

/// A page of one of an artist's sections, asked for `limit` at a time.
pub(crate) fn cards_page(page: &Value, limit: usize) -> Paged<Card> {
    let raw = items(&page["items"]);
    let items = match unwrap_v2(raw) {
        Some((section_type, items)) => home_feed::cards(&items, section_type).collect(),
        None => Vec::new(),
    };
    Paged {
        items,
        has_more: raw.len() >= limit,
    }
}

/// v2 items come as `{ type, data }`. The section type the first one's
/// type stands for, and the items out of their wrappers.
fn unwrap_v2(items: &[Value]) -> Option<(&str, Vec<Value>)> {
    let kind = items.first().and_then(|item| string(item, "type"))?;
    let section_type = match kind {
        "TRACK" => "TRACK_LIST",
        "ALBUM" => "ALBUM_LIST",
        "ARTIST" => "ARTIST_LIST",
        "PLAYLIST" => "PLAYLIST_LIST",
        "MIX" => "MIX_LIST",
        "VIDEO" => "VIDEO_LIST",
        other => other,
    };
    let items = items
        .iter()
        .map(|item| item.get("data").unwrap_or(item).clone())
        .collect();
    Some((section_type, items))
}

fn v2(page: &Value) -> Artist {
    let data = &page["item"]["data"];
    let header = &page["header"];
    let sections = items(&page["items"])
        .iter()
        .filter_map(|module| {
            let (section_type, items) = unwrap_v2(items(&module["items"]))?;
            if section_type == "TRACK_CREDITS" {
                return None;
            }
            Some(section(
                string(module, "title").unwrap_or_default(),
                section_type,
                &items,
                string(module, "viewAll"),
            ))
        })
        .collect();
    Artist {
        name: string(data, "name").unwrap_or_default().to_string(),
        picture: picture(data),
        bio: string(&header["biography"], "text").map(strip_bio),
        followers: header["followersAmount"].as_u64(),
        sections,
    }
}

fn section(title: &str, section_type: &str, items: &[Value], view_all: Option<&str>) -> Section {
    let content = if section_type == "TRACK_LIST" {
        Content::Tracks(items.iter().filter_map(Track::from_value).collect())
    } else {
        Content::Cards(home_feed::cards(items, section_type).collect())
    };
    Section {
        title: title.to_string(),
        content,
        view_all: view_all.map(str::to_string),
    }
}

/// The artist's own artwork, else their picture, else the album cover
/// TIDAL falls back to (sone's `getArtistImage`).
fn picture(artist: &Value) -> Option<Cover> {
    string(artist, "artworkId")
        .or_else(|| string(artist, "picture"))
        .map(|id| Cover::Artist(id.to_string()))
        .or_else(|| {
            string(artist, "selectedAlbumCoverFallback").map(|id| Cover::Image(id.to_string()))
        })
}

/// The bio without TIDAL's `[wimpLink …]` and HTML markup.
fn strip_bio(bio: &str) -> String {
    let mut out = String::with_capacity(bio.len());
    let mut closer = None;
    for c in bio.chars() {
        match (closer, c) {
            (None, '[') => closer = Some(']'),
            (None, '<') => closer = Some('>'),
            (Some(end), c) if c == end => closer = None,
            (Some(_), _) => {}
            (None, c) => out.push(c),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home_feed::Target;
    use serde_json::json;

    fn track(id: u64, title: &str) -> Value {
        json!({
            "id": id,
            "title": title,
            "duration": 200,
            "artists": [{ "id": 1, "name": "Björk" }],
            "album": { "id": 9, "title": "Homogenic", "cover": "aa-bb" },
        })
    }

    #[test]
    fn a_v2_page_names_the_artist_and_keeps_its_sections_in_order() {
        let artist = Artist::from(json!({
            "item": { "data": { "name": "Björk", "picture": "pp-qq" } },
            "header": { "followersAmount": 1200 },
            "items": [
                {
                    "title": "Top tracks",
                    "viewAll": "artist/ARTIST_TOP_TRACKS/view-all?artistId=1",
                    "items": [
                        { "type": "TRACK", "data": track(11, "Jóga") },
                        { "type": "TRACK", "data": track(12, "Bachelorette") },
                    ],
                },
                {
                    "title": "Albums",
                    "viewAll": "artist/ARTIST_ALBUMS/view-all?artistId=1",
                    "items": [
                        { "type": "ALBUM", "data": { "id": 9, "title": "Homogenic", "cover": "aa-bb" } },
                    ],
                },
            ],
        }));

        assert_eq!(artist.name, "Björk");
        assert_eq!(artist.picture, Some(Cover::Artist("pp-qq".to_string())));
        assert_eq!(artist.followers, Some(1200));
        assert_eq!(artist.sections.len(), 2);

        let top = &artist.sections[0];
        assert_eq!(top.title, "Top tracks");
        let Content::Tracks(tracks) = &top.content else {
            panic!("top tracks should be tracks");
        };
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Jóga", "Bachelorette"]);

        let albums = &artist.sections[1];
        assert_eq!(
            albums.view_all.as_deref(),
            Some("artist/ARTIST_ALBUMS/view-all?artistId=1")
        );
        let Content::Cards(cards) = &albums.content else {
            panic!("albums should be cards");
        };
        assert_eq!(cards[0].target, Target::Album(9));
    }

    #[test]
    fn a_v1_page_reads_the_header_module_and_the_paged_lists() {
        let artist = Artist::from(json!({
            "rows": [
                { "modules": [{
                    "type": "ARTIST_HEADER",
                    "artist": { "name": "Björk", "picture": "pp-qq" },
                    "bio": { "text": "Icelandic singer." },
                }] },
                { "modules": [{
                    "type": "TRACK_LIST",
                    "title": "",
                    "pagedList": { "items": [track(11, "Jóga")] },
                }] },
                { "modules": [{
                    "type": "ALBUM_LIST",
                    "title": "Albums",
                    "pagedList": { "items": [{ "id": 9, "title": "Homogenic" }] },
                    "showMore": { "apiPath": "pages/data/albums?artistId=1" },
                }] },
            ],
        }));

        assert_eq!(artist.name, "Björk");
        assert_eq!(artist.bio.as_deref(), Some("Icelandic singer."));
        let titles: Vec<&str> = artist.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Popular tracks", "Albums"]);
        assert!(matches!(&artist.sections[0].content, Content::Tracks(t) if t.len() == 1));
        assert_eq!(
            artist.sections[1].view_all.as_deref(),
            Some("pages/data/albums?artistId=1")
        );
    }

    #[test]
    fn a_full_page_of_top_tracks_may_have_more() {
        let full = json!({ "items": (0..3).map(|id| json!({ "type": "TRACK", "data": track(id, "Song") })).collect::<Vec<_>>() });
        let page = tracks_page(&full, 3);
        assert_eq!(page.items.len(), 3);
        assert!(page.has_more);

        let short = json!({ "items": [{ "type": "TRACK", "data": track(1, "Song") }] });
        assert!(!tracks_page(&short, 3).has_more);
    }

    #[test]
    fn a_view_all_page_is_cards_of_the_wrapped_type() {
        let page = cards_page(
            &json!({ "items": [
                { "type": "ARTIST", "data": { "id": 5, "name": "Sigur Rós" } },
            ] }),
            50,
        );
        assert_eq!(page.items[0].target, Target::Artist(5));
        assert!(!page.has_more);
    }

    #[test]
    fn credits_and_empty_modules_are_not_sections() {
        let artist = Artist::from(json!({
            "item": { "data": { "name": "Björk" } },
            "items": [
                { "title": "Credits", "items": [{ "type": "TRACK_CREDITS", "data": {} }] },
                { "title": "Nothing", "items": [] },
            ],
        }));
        assert!(artist.sections.is_empty());
    }

    #[test]
    fn the_bio_loses_tidals_link_and_html_markup() {
        let artist = Artist::from(json!({
            "item": { "data": { "name": "Björk" } },
            "header": { "biography": {
                "text": "Worked with [wimpLink artistId=\"3346\"]Thom Yorke[/wimpLink].<br/> ",
            } },
        }));
        assert_eq!(artist.bio.as_deref(), Some("Worked with Thom Yorke."));
    }
}
