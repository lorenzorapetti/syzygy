//! Seam 3: build a Library, send it messages and mutation results, and
//! check what `apply` renders, what the hearts say and which mutations go
//! out.

use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::library::Item;
use syzygy_catalog::{
    Direction, FavoriteId, FavoriteIds, Kind, LibraryOrder, LibrarySort, Read, Shelf, Track,
};

use super::*;
use crate::settings::Settings;

const USER: u64 = 7;

fn track(id: u64) -> Track {
    Track {
        id,
        title: format!("Track {id}"),
        artists: vec![],
        album: None,
        duration: 200,
        explicit: false,
        available: true,
        volume: 1,
        date_added: None,
        track_radio: None,
    }
}

fn album(id: u64) -> Card {
    Card {
        title: format!("Album {id}"),
        subtitle: String::new(),
        cover: None,
        target: Target::Album(id),
    }
}

fn liked_album(id: u64) -> Favorite {
    Favorite::card(&album(id)).unwrap()
}

fn albums_shelf() -> Shelf {
    Shelf {
        user_id: USER,
        kind: Kind::Albums,
        folder: None,
        sort: LibrarySort {
            order: LibraryOrder::DateAdded,
            direction: Direction::Descending,
        },
    }
}

fn failure() -> Arc<syzygy_catalog::Error> {
    Arc::new(syzygy_catalog::Error::UnknownUser)
}

/// A Library for `USER` whose Favorites have loaded as `tracks` and
/// `albums`.
fn library(tracks: &[u64], albums: &[u64]) -> Library {
    let (mut library, effects) = Library::new(Some(USER), &Settings::default());
    let Some(Effect::ReadFavorites { stamp, .. }) = effects.first().cloned() else {
        panic!("the Favorites are read as the Library starts: {effects:?}");
    };
    let ids = FavoriteIds {
        tracks: tracks.iter().copied().collect(),
        albums: albums.iter().copied().collect(),
        ..FavoriteIds::default()
    };
    library.update(Message::FavoriteIds(stamp, Read::Fresh(Ok(ids))));
    library
}

/// The one mutation `effects` asks for.
fn mutation(effects: &[Effect]) -> (EditId, Mutation) {
    let mutations: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Mutate(id, mutation) => Some((*id, mutation.clone())),
            _ => None,
        })
        .collect();
    match mutations.as_slice() {
        [one] => one.clone(),
        _ => panic!("one mutation, not {effects:?}"),
    }
}

fn mutations(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Mutate(..)))
        .count()
}

fn toasts(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Toast(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn loved_ids(library: &Library, server: &[Track]) -> Vec<u64> {
    library
        .apply(server, Listing::Loved)
        .iter()
        .map(|track| track.id)
        .collect()
}

fn shelf_titles(library: &Library, server: &[Item], shelf: &Shelf) -> Vec<String> {
    library
        .apply(server, Listing::Shelf(shelf))
        .iter()
        .map(|item| match item {
            Item::Card(card) => card.title.clone(),
            Item::Playlist(playlist) => playlist.title.clone(),
            Item::Folder(folder) => folder.name.clone(),
        })
        .collect()
}

fn like_track(library: &mut Library, id: u64, on: bool) -> Vec<Effect> {
    library.update(Message::Favorite(Favorite::track(&track(id)), on))
}

#[test]
fn hearts_cant_be_clicked_until_the_favorites_have_loaded() {
    let (mut library, _) = Library::new(Some(USER), &Settings::default());

    assert_eq!(library.favorite(&FavoriteId::Track(1)), None);
    assert!(like_track(&mut library, 1, true).is_empty());
}

#[test]
fn nothing_is_read_until_tidal_says_who_the_user_is() {
    let (mut library, effects) = Library::new(None, &Settings::default());
    assert!(effects.is_empty());

    let effects = library.user_known(USER);
    assert!(matches!(
        effects.as_slice(),
        [Effect::ReadFavorites { user_id: USER, .. }]
    ));
}

#[test]
fn the_cached_favorites_enable_the_hearts_until_tidals_arrive() {
    let (mut library, effects) = Library::new(Some(USER), &Settings::default());
    let Some(Effect::ReadFavorites { stamp, .. }) = effects.first().cloned() else {
        panic!("a read of the Favorites");
    };
    let cached = FavoriteIds {
        tracks: [1].into(),
        ..FavoriteIds::default()
    };
    library.update(Message::FavoriteIds(stamp, Read::Cached(cached)));
    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(true));

    let fresh = FavoriteIds::default();
    library.update(Message::FavoriteIds(stamp, Read::Fresh(Ok(fresh))));
    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(false));
}

#[test]
fn a_like_shows_straight_away_and_its_mutation_goes_out() {
    let mut library = library(&[], &[]);

    let effects = like_track(&mut library, 1, true);

    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(true));
    assert_eq!(loved_ids(&library, &[track(2)]), vec![1, 2]);
    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::Favorite {
            user_id: USER,
            id: FavoriteId::Track(1),
            on: true,
        }
    );
}

#[test]
fn an_unlike_hides_the_favorite_straight_away() {
    let mut library = library(&[1, 2], &[]);

    like_track(&mut library, 1, false);

    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(false));
    assert_eq!(loved_ids(&library, &[track(1), track(2)]), vec![2]);
}

#[test]
fn liking_what_is_already_liked_does_nothing() {
    let mut library = library(&[1], &[]);

    assert!(like_track(&mut library, 1, true).is_empty());
    assert_eq!(loved_ids(&library, &[track(1)]), vec![1]);
}

#[test]
fn a_refused_edit_rolls_back_with_a_toast() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&like_track(&mut library, 1, true));

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(false));
    assert_eq!(loved_ids(&library, &[]), Vec::<u64>::new());
    assert_eq!(
        toasts(&effects),
        vec!["Couldn't add \u{201c}Track 1\u{201d} to your Favorites"]
    );
}

#[test]
fn a_refused_unfollow_says_so_by_the_artists_name() {
    let mut library = library(&[], &[]);
    let artist = Card {
        title: "Björk".to_string(),
        subtitle: "Artist".to_string(),
        cover: None,
        target: Target::Artist(5),
    };
    let stamp = library.start_read();
    library.update(Message::FavoriteIds(
        stamp,
        Read::Fresh(Ok(FavoriteIds {
            artists: [5].into(),
            ..FavoriteIds::default()
        })),
    ));
    let effects = library.update(Message::Favorite(Favorite::card(&artist).unwrap(), false));
    let (id, _) = mutation(&effects);

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(toasts(&effects), vec!["Couldn't unfollow Björk"]);
    assert_eq!(library.favorite(&FavoriteId::Artist(5)), Some(true));
}

#[test]
fn a_landed_edit_reads_its_lists_again_and_stays_until_a_later_fresh_read() {
    let mut library = library(&[], &[]);
    let before = library.start_read();
    let (id, _) = mutation(&library.update(Message::Favorite(liked_album(3), true)));

    let effects = library.update(Message::Done(id, Ok(())));
    assert!(effects.contains(&Effect::Refresh(vec!["fav-albums".to_string()])));

    // A read from before the success doesn't show it yet: the edit stays.
    library.update(Message::Fresh(before, vec!["fav-albums".to_string()]));
    let shelf = albums_shelf();
    assert_eq!(shelf_titles(&library, &[], &shelf), vec!["Album 3"]);

    // One that started after does: the server is the truth from then on.
    let after = library.start_read();
    library.update(Message::Fresh(after, vec!["fav-albums".to_string()]));
    assert_eq!(shelf_titles(&library, &[], &shelf), Vec::<String>::new());
    let server = [Item::Card(album(3))];
    assert_eq!(shelf_titles(&library, &server, &shelf), vec!["Album 3"]);
}

#[test]
fn a_fresh_read_of_other_tags_leaves_a_landed_edit_in_place() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::Favorite(liked_album(3), true)));
    library.update(Message::Done(id, Ok(())));

    let after = library.start_read();
    library.update(Message::Fresh(after, vec!["fav-tracks".to_string()]));

    assert_eq!(
        shelf_titles(&library, &[], &albums_shelf()),
        vec!["Album 3"]
    );
}

#[test]
fn a_running_edit_stays_whatever_fresh_reads_arrive() {
    let mut library = library(&[], &[]);
    library.update(Message::Favorite(liked_album(3), true));

    let after = library.start_read();
    library.update(Message::Fresh(after, vec!["fav-albums".to_string()]));

    assert_eq!(
        shelf_titles(&library, &[], &albums_shelf()),
        vec!["Album 3"]
    );
}

#[test]
fn the_hearts_keep_a_landed_edit_over_a_read_from_before_it() {
    let mut library = library(&[], &[]);
    let before = library.start_read();
    let (id, _) = mutation(&like_track(&mut library, 1, true));
    library.update(Message::Done(id, Ok(())));

    // TIDAL's answer to a read that started before the like landed.
    library.update(Message::FavoriteIds(
        before,
        Read::Fresh(Ok(FavoriteIds::default())),
    ));

    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(true));
}

#[test]
fn a_favorites_read_leaves_the_lists_the_edits_they_still_need() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&like_track(&mut library, 1, true));
    library.update(Message::Done(id, Ok(())));

    // The id sets catch up before the Loved tracks' own read does.
    let stamp = library.start_read();
    library.update(Message::FavoriteIds(
        stamp,
        Read::Fresh(Ok(FavoriteIds {
            tracks: [1].into(),
            ..FavoriteIds::default()
        })),
    ));

    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(true));
    assert_eq!(loved_ids(&library, &[]), vec![1]);
}

#[test]
fn edits_of_one_target_run_in_order() {
    let mut library = library(&[], &[]);
    let (first, _) = mutation(&like_track(&mut library, 1, true));

    // Unliking while the like runs waits for it.
    let effects = like_track(&mut library, 1, false);
    assert_eq!(mutations(&effects), 0);
    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(false));

    let effects = library.update(Message::Done(first, Ok(())));
    let (_, next) = mutation(&effects);
    assert_eq!(
        next,
        Mutation::Favorite {
            user_id: USER,
            id: FavoriteId::Track(1),
            on: false,
        }
    );
}

#[test]
fn edits_of_different_targets_run_at_once() {
    let mut library = library(&[], &[]);

    assert_eq!(mutations(&like_track(&mut library, 1, true)), 1);
    assert_eq!(mutations(&like_track(&mut library, 2, true)), 1);
}

#[test]
fn a_failure_drops_the_edits_queued_behind_it_with_one_toast() {
    let mut library = library(&[], &[]);
    let (first, _) = mutation(&like_track(&mut library, 1, true));
    like_track(&mut library, 1, false);
    like_track(&mut library, 1, true);
    // Another target's edit is left alone.
    like_track(&mut library, 2, true);

    let effects = library.update(Message::Done(first, Err(failure())));

    assert_eq!(toasts(&effects).len(), 1);
    assert_eq!(mutations(&effects), 0);
    assert_eq!(library.favorite(&FavoriteId::Track(1)), Some(false));
    assert_eq!(library.favorite(&FavoriteId::Track(2)), Some(true));
    assert_eq!(loved_ids(&library, &[]), vec![2]);
}

#[test]
fn a_result_for_an_edit_already_dropped_is_ignored() {
    let mut library = library(&[], &[]);
    let (first, _) = mutation(&like_track(&mut library, 1, true));
    library.update(Message::Done(first, Err(failure())));

    assert!(library.update(Message::Done(first, Ok(()))).is_empty());
}

#[test]
fn the_latest_like_goes_first_and_a_liked_item_already_listed_keeps_its_place() {
    let mut library = library(&[], &[2]);
    library.update(Message::Favorite(liked_album(3), true));
    library.update(Message::Favorite(liked_album(4), true));

    let server = [Item::Card(album(2))];
    let shelf = albums_shelf();
    assert_eq!(
        shelf_titles(&library, &server, &shelf),
        vec!["Album 4", "Album 3", "Album 2"]
    );

    // TIDAL's list now has album 3: it's not listed twice.
    let server = [Item::Card(album(2)), Item::Card(album(3))];
    assert_eq!(
        shelf_titles(&library, &server, &shelf),
        vec!["Album 4", "Album 2", "Album 3"]
    );
}

#[test]
fn a_like_goes_only_into_lists_of_its_kind_at_the_top_level() {
    let mut library = library(&[], &[]);
    library.update(Message::Favorite(liked_album(3), true));
    like_track(&mut library, 1, true);

    let artists = Shelf {
        kind: Kind::Artists,
        ..albums_shelf()
    };
    assert_eq!(shelf_titles(&library, &[], &artists), Vec::<String>::new());
    assert_eq!(loved_ids(&library, &[]), vec![1]);

    let playlist = |uuid: &str| Card {
        title: uuid.to_string(),
        subtitle: String::new(),
        cover: None,
        target: Target::Playlist(uuid.to_string()),
    };
    library.update(Message::Favorite(
        Favorite::card(&playlist("p-1")).unwrap(),
        true,
    ));
    let root = Shelf {
        kind: Kind::Playlists,
        ..albums_shelf()
    };
    let folder = Shelf {
        folder: Some("f-1".to_string()),
        ..root.clone()
    };
    assert_eq!(shelf_titles(&library, &[], &root), vec!["p-1"]);
    assert_eq!(shelf_titles(&library, &[], &folder), Vec::<String>::new());
}
