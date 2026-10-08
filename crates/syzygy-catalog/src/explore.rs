//! Explore and the Pages under it: TIDAL's genres, moods, decades and
//! editorial pages, each read by its path. A section either links to more
//! of them or holds cards, as on Home.

use serde_json::Value;
use syzygy_tidal::models::{HomePageResponse, HomePageSection};

use crate::home_feed::{self, Card, items, string};

/// The path Explore itself is read from.
pub const ROOT: &str = "pages/explore";

/// One Explore Page's content.
#[derive(Debug, Clone, Default)]
pub struct ExplorePage {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    /// Links to more Explore Pages: genres, moods, decades.
    Links {
        title: String,
        /// The Page with all of them, by its path.
        view_all: Option<String>,
        links: Vec<PageLink>,
    },
    /// Albums, playlists, artists and so on.
    Cards {
        title: String,
        /// The Page with the whole section, by its path.
        view_all: Option<String>,
        cards: Vec<Card>,
    },
}

/// A link to another Explore Page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLink {
    pub title: String,
    pub path: String,
}

impl ExplorePage {
    /// Nothing but links, as on "All Genres": they read best as one list.
    pub fn only_links(&self) -> bool {
        !self.sections.is_empty()
            && self
                .sections
                .iter()
                .all(|section| matches!(section, Section::Links { .. }))
    }
}

impl From<HomePageResponse> for ExplorePage {
    fn from(response: HomePageResponse) -> Self {
        ExplorePage {
            sections: response.sections.iter().filter_map(section).collect(),
        }
    }
}

/// A section with something in it.
fn section(section: &HomePageSection) -> Option<Section> {
    let items = items(&section.items);
    if items.is_empty() {
        return None;
    }
    let title = section.title.clone();
    if !is_links(section, items) {
        return Some(Section::Cards {
            title,
            view_all: section.api_path.clone().filter(|_| section.has_more),
            cards: home_feed::cards(items, &section.section_type).collect(),
        });
    }
    let links: Vec<PageLink> = items
        .iter()
        .filter_map(|item| {
            Some(PageLink {
                title: string(item, "title")?.to_string(),
                path: string(item, "apiPath")?.to_string(),
            })
        })
        .collect();
    (!links.is_empty()).then(|| Section::Links {
        title,
        view_all: section.api_path.clone(),
        links,
    })
}

/// sone's `isNavLinkSection`: a links type, or items that lead to a path
/// and are nothing in themselves.
fn is_links(section: &HomePageSection, items: &[Value]) -> bool {
    matches!(
        section.section_type.as_str(),
        "PAGE_LINKS_CLOUD" | "PAGE_LINKS"
    ) || items.first().is_some_and(|item| {
        item.get("apiPath").is_some() && item.get("uuid").is_none() && item.get("id").is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home_feed::Target;
    use serde_json::json;

    fn section(
        title: &str,
        section_type: &str,
        api_path: Option<&str>,
        items: Value,
    ) -> HomePageSection {
        HomePageSection {
            title: title.to_string(),
            section_type: section_type.to_string(),
            items,
            has_more: api_path.is_some(),
            api_path: api_path.map(str::to_string),
        }
    }

    fn page(sections: Vec<HomePageSection>) -> ExplorePage {
        ExplorePage::from(HomePageResponse {
            tabs: vec![],
            sections,
            cursor: None,
        })
    }

    fn link(title: &str, path: &str) -> PageLink {
        PageLink {
            title: title.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn genres_link_to_their_pages_and_to_all_of_them() {
        let explore = page(vec![section(
            "Genres",
            "PAGE_LINKS_CLOUD",
            Some("pages/genre_page"),
            json!([
                { "apiPath": "pages/genre_hip_hop", "icon": "hiphop", "imageId": "hiphop", "title": "Hip-Hop" },
                { "apiPath": "pages/genre_pop", "icon": null, "imageId": null, "title": "Pop" },
            ]),
        )]);
        assert_eq!(
            explore.sections,
            [Section::Links {
                title: "Genres".to_string(),
                view_all: Some("pages/genre_page".to_string()),
                links: vec![
                    link("Hip-Hop", "pages/genre_hip_hop"),
                    link("Pop", "pages/genre_pop")
                ],
            }]
        );
        assert!(explore.only_links());
    }

    #[test]
    fn a_section_of_items_with_paths_and_no_ids_is_links_whatever_its_type() {
        let explore = page(vec![section(
            "",
            "HORIZONTAL_LIST",
            None,
            json!([{ "apiPath": "pages/explore_new_music", "title": "New" }]),
        )]);
        assert_eq!(
            explore.sections,
            [Section::Links {
                title: String::new(),
                view_all: None,
                links: vec![link("New", "pages/explore_new_music")],
            }]
        );
    }

    #[test]
    fn a_link_with_no_path_or_no_title_is_left_out() {
        let explore = page(vec![section(
            "Moods",
            "PAGE_LINKS_CLOUD",
            None,
            json!([
                { "title": "Nowhere" },
                { "apiPath": "pages/mood_nameless", "title": "" },
                { "apiPath": "pages/mood_party", "title": "Party" },
            ]),
        )]);
        let Section::Links { links, .. } = &explore.sections[0] else {
            panic!("links");
        };
        assert_eq!(links, &[link("Party", "pages/mood_party")]);
    }

    #[test]
    fn content_sections_hold_cards_and_view_all_when_there_is_more() {
        let explore = page(vec![
            section(
                "Playlists",
                "PLAYLIST_LIST",
                Some("pages/single-module-page/a/1"),
                json!([{ "uuid": "u-1", "title": "Pop Life", "squareImage": "sq-1", "creator": { "id": 0 } }]),
            ),
            section(
                "New Albums",
                "ALBUM_LIST",
                None,
                json!([{ "id": 42, "title": "Kid A", "cover": "aa-bb", "artists": [{ "name": "Radiohead" }] }]),
            ),
        ]);
        let Section::Cards {
            title,
            view_all,
            cards,
        } = &explore.sections[0]
        else {
            panic!("cards");
        };
        assert_eq!(title, "Playlists");
        assert_eq!(view_all.as_deref(), Some("pages/single-module-page/a/1"));
        assert_eq!(cards[0].target, Target::Playlist("u-1".to_string()));
        let Section::Cards {
            view_all, cards, ..
        } = &explore.sections[1]
        else {
            panic!("cards");
        };
        assert_eq!(view_all, &None);
        assert_eq!(cards[0].target, Target::Album(42));
        assert_eq!(cards[0].subtitle, "Radiohead");
        assert!(!explore.only_links());
    }

    #[test]
    fn empty_sections_are_left_out() {
        let explore = page(vec![
            section("Nothing", "ALBUM_LIST", None, json!([])),
            section("Nothing either", "PAGE_LINKS_CLOUD", None, Value::Null),
        ]);
        assert!(explore.sections.is_empty());
        assert!(!explore.only_links());
    }
}
