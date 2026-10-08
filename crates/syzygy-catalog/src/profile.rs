//! A user's profile, read-only: who they are, and their public playlists.

use syzygy_tidal::models::{self, ProfileArtFile, ProfilePlaylist};

use crate::artist::strip_bio;
use crate::home_feed::{Card, Cover, Target, count_label};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub user_id: u64,
    /// Empty when the user never set one.
    pub name: String,
    pub handle: Option<String>,
    pub bio: Option<String>,
    pub fans: Option<u32>,
    /// Square, for a round picture.
    pub picture: Option<Cover>,
    pub playlists: Vec<Card>,
}

/// The picture is drawn at most this wide, before HiDPI.
const PICTURE_SIZE: u32 = 160;

impl Profile {
    /// What the profile is called: its name, else its handle.
    pub fn called(&self) -> &str {
        match (&self.name, &self.handle) {
            (name, _) if !name.is_empty() => name,
            (_, Some(handle)) => handle,
            _ => "Profile",
        }
    }

    /// "@handle · 1.2K fans", or as much of it as there is.
    pub fn facts(&self) -> String {
        let handle = self.handle.as_ref().map(|handle| format!("@{handle}"));
        let fans = self
            .fans
            .map(|n| format!("{} fan{}", compact(n), if n == 1 { "" } else { "s" }));
        handle
            .into_iter()
            .chain(fans)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl From<models::Profile> for Profile {
    fn from(profile: models::Profile) -> Self {
        Profile {
            user_id: profile.user_id,
            name: profile.name,
            handle: profile.handle.filter(|handle| !handle.is_empty()),
            bio: profile
                .bio
                .map(|bio| strip_bio(&bio))
                .filter(|bio| !bio.is_empty()),
            fans: profile.fan_count,
            picture: picture(&profile.picture_files).map(Cover::Url),
            playlists: profile.public_playlists.into_iter().map(card).collect(),
        }
    }
}

/// TIDAL serves the picture square and wide, in several sizes. A square
/// one is wanted, the smallest that's sharp on HiDPI, else the widest
/// (sone's `pickProfileAvatarHref` and `pickProfileHeroImage`).
fn picture(files: &[ProfileArtFile]) -> Option<String> {
    let squares: Vec<&ProfileArtFile> = files
        .iter()
        .filter(|file| file.width.is_some() && file.width == file.height)
        .collect();
    let pool = if squares.is_empty() {
        files.iter().collect()
    } else {
        squares
    };
    let sized = pool.iter().filter_map(|file| Some((file.width?, *file)));
    let sharp = sized
        .clone()
        .filter(|(width, _)| *width >= PICTURE_SIZE * 2)
        .min_by_key(|(width, _)| *width);
    sharp
        .or_else(|| sized.max_by_key(|(width, _)| *width))
        .map(|(_, file)| file)
        .or_else(|| pool.first().copied())
        .map(|file| file.href.clone())
}

fn card(playlist: ProfilePlaylist) -> Card {
    Card {
        subtitle: playlist
            .number_of_tracks
            .map(|tracks| count_label(u64::from(tracks), 0))
            .unwrap_or_default(),
        cover: playlist.cover_url.map(Cover::Url),
        target: Target::Playlist(playlist.id),
        title: playlist.title,
    }
}

/// 999, 1K, 1.3K, 15.3K, 2.5M: one decimal at most, rounded half up, as
/// `Intl.NumberFormat`'s compact notation.
fn compact(n: u32) -> String {
    let n = u64::from(n);
    let tenths = |unit: u64| (n * 10 + unit / 2) / unit;
    let (tenths, suffix) = match n {
        ..1000 => return n.to_string(),
        _ if tenths(1000) < 10_000 => (tenths(1000), "K"),
        _ => (tenths(1_000_000), "M"),
    };
    if tenths % 10 == 0 {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> models::Profile {
        models::Profile {
            user_id: 7,
            artist_id: Some(70),
            name: "Ada".to_string(),
            handle: Some("ada".to_string()),
            bio: None,
            bio_id: None,
            picture_files: vec![],
            artwork_id: None,
            blur_hash: None,
            palette: vec![],
            external_links: vec![],
            fan_count: None,
            public_playlists: vec![],
        }
    }

    fn file(width: u32, height: u32) -> ProfileArtFile {
        ProfileArtFile {
            href: format!("https://img/{width}x{height}.jpg"),
            width: Some(width),
            height: Some(height),
        }
    }

    fn picture(files: Vec<ProfileArtFile>) -> Option<Cover> {
        Profile::from(models::Profile {
            picture_files: files,
            ..profile()
        })
        .picture
    }

    fn url(url: &str) -> Option<Cover> {
        Some(Cover::Url(url.to_string()))
    }

    #[test]
    fn the_picture_is_the_smallest_square_sharp_enough() {
        let files = vec![
            file(1280, 720),
            file(1280, 1280),
            file(640, 640),
            file(320, 320),
            file(160, 160),
        ];
        assert_eq!(picture(files), url("https://img/320x320.jpg"));
    }

    #[test]
    fn small_squares_give_the_widest() {
        let files = vec![file(1280, 720), file(160, 160), file(80, 80)];
        assert_eq!(picture(files), url("https://img/160x160.jpg"));
    }

    #[test]
    fn with_no_square_any_picture_will_do() {
        assert_eq!(
            picture(vec![file(1280, 720), file(640, 360)]),
            url("https://img/640x360.jpg")
        );
        let no_size = ProfileArtFile {
            href: "https://img/unsized.jpg".to_string(),
            width: None,
            height: None,
        };
        assert_eq!(picture(vec![no_size]), url("https://img/unsized.jpg"));
        assert_eq!(picture(vec![]), None);
    }

    fn facts(handle: Option<&str>, fans: Option<u32>) -> String {
        Profile::from(models::Profile {
            handle: handle.map(str::to_string),
            fan_count: fans,
            ..profile()
        })
        .facts()
    }

    #[test]
    fn facts_name_the_handle_and_count_fans_compactly() {
        assert_eq!(facts(Some("ada"), Some(1)), "@ada · 1 fan");
        assert_eq!(facts(Some("ada"), Some(0)), "@ada · 0 fans");
        assert_eq!(facts(None, Some(999)), "999 fans");
        assert_eq!(facts(None, Some(1000)), "1K fans");
        assert_eq!(facts(None, Some(1250)), "1.3K fans");
        assert_eq!(facts(None, Some(15_340)), "15.3K fans");
        assert_eq!(facts(None, Some(999_960)), "1M fans");
        assert_eq!(facts(None, Some(2_540_000)), "2.5M fans");
        assert_eq!(facts(Some("ada"), None), "@ada");
        assert_eq!(facts(None, None), "");
    }

    #[test]
    fn a_profile_is_called_by_its_name_else_its_handle() {
        let called = |name: &str, handle: Option<&str>| {
            Profile::from(models::Profile {
                name: name.to_string(),
                handle: handle.map(str::to_string),
                ..profile()
            })
            .called()
            .to_string()
        };
        assert_eq!(called("Ada", Some("ada")), "Ada");
        assert_eq!(called("", Some("ada")), "ada");
        assert_eq!(called("", None), "Profile");
    }

    #[test]
    fn the_bio_loses_its_markup_and_an_empty_one_is_none() {
        let bio = |bio: &str| {
            Profile::from(models::Profile {
                bio: Some(bio.to_string()),
                ..profile()
            })
            .bio
        };
        assert_eq!(
            bio("Fan of [wimpLink artistId=\"1\"]Björk[/wimpLink]."),
            Some("Fan of Björk.".to_string())
        );
        assert_eq!(bio("  "), None);
    }

    #[test]
    fn public_playlists_are_cards_that_open_the_playlist() {
        let profile = Profile::from(models::Profile {
            public_playlists: vec![
                ProfilePlaylist {
                    id: "u-1".to_string(),
                    title: "Game OST".to_string(),
                    access_type: Some("PUBLIC".to_string()),
                    number_of_tracks: Some(17),
                    cover_url: Some("https://img/cover.jpg".to_string()),
                },
                ProfilePlaylist {
                    id: "u-2".to_string(),
                    title: "Empty".to_string(),
                    access_type: None,
                    number_of_tracks: None,
                    cover_url: None,
                },
            ],
            ..profile()
        });
        assert_eq!(
            profile.playlists,
            [
                Card {
                    title: "Game OST".to_string(),
                    subtitle: "17 Tracks".to_string(),
                    cover: url("https://img/cover.jpg"),
                    target: Target::Playlist("u-1".to_string()),
                },
                Card {
                    title: "Empty".to_string(),
                    subtitle: String::new(),
                    cover: None,
                    target: Target::Playlist("u-2".to_string()),
                },
            ]
        );
        assert_eq!(profile.name, "Ada");
        assert_eq!(profile.user_id, 7);
    }
}
