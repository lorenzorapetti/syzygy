//! Seam 3: build a Library, send it messages and mutation results, and
//! check what `apply` renders, what the hearts say and which mutations go
//! out.

use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::library::Item;
use syzygy_catalog::{
    Direction, FavoriteId, FavoriteIds, Kind, LibraryOrder, LibrarySort, Playlist, PlaylistFields,
    Read, Shelf, Track,
};

use super::*;
use crate::playback::SourceRef;
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

// Own playlists: create, edit and delete.

fn own(uuid: &str, title: &str) -> Playlist {
    PlaylistFields {
        title: title.to_string(),
        description: String::new(),
        public: false,
    }
    .playlist(uuid.to_string(), USER)
}

fn fields(title: &str) -> PlaylistFields {
    PlaylistFields {
        title: title.to_string(),
        ..PlaylistFields::new()
    }
}

fn root_shelf() -> Shelf {
    Shelf {
        kind: Kind::Playlists,
        ..albums_shelf()
    }
}

/// The root playlists as the user sees them, by title.
fn root_titles(library: &Library, server: &[Item]) -> Vec<String> {
    shelf_titles(library, server, &root_shelf())
}

fn playlists(server: &[Playlist]) -> Vec<Item> {
    server.iter().cloned().map(Item::Playlist).collect()
}

fn source_deleted(effects: &[Effect]) -> Vec<SourceRef> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SourceDeleted(source) => Some(source.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_new_playlist_shows_first_straight_away_and_cant_be_opened_yet() {
    let mut library = library(&[], &[]);
    let server = playlists(&[own("p-1", "Old")]);

    let effects = library.update(Message::CreatePlaylist(fields("New"), vec![]));

    assert_eq!(root_titles(&library, &server), vec!["New", "Old"]);
    let shown = library.apply(&server, Listing::Shelf(&root_shelf()));
    let Item::Playlist(placeholder) = shown[0] else {
        panic!("a playlist");
    };
    assert!(is_placeholder(placeholder));
    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::CreatePlaylist {
            user_id: USER,
            fields: fields("New"),
        }
    );
}

#[test]
fn a_made_playlist_is_the_one_tidal_made_until_a_later_read_lists_it() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::CreatePlaylist(fields("New"), vec![])));

    let effects = library.update(Message::Created(id, Ok(own("p-9", "New"))));

    assert!(effects.contains(&Effect::Refresh(vec!["folders".to_string()])));
    let shown = library.apply::<Item>(&[], Listing::Shelf(&root_shelf()));
    let [Item::Playlist(made)] = shown.as_slice() else {
        panic!("one playlist, not {shown:?}");
    };
    assert_eq!(made.uuid, "p-9");
    assert!(!is_placeholder(made));

    let after = library.start_read();
    library.update(Message::Fresh(after, root_shelf().tags()));
    assert_eq!(root_titles(&library, &[]), Vec::<String>::new());
}

#[test]
fn a_new_playlist_goes_only_to_the_top_level_of_the_playlists() {
    let mut library = library(&[], &[]);
    library.update(Message::CreatePlaylist(fields("New"), vec![]));

    let folder = Shelf {
        folder: Some("f-1".to_string()),
        ..root_shelf()
    };
    assert_eq!(shelf_titles(&library, &[], &folder), Vec::<String>::new());
    assert_eq!(
        shelf_titles(&library, &[], &albums_shelf()),
        Vec::<String>::new()
    );
}

#[test]
fn a_playlist_tidal_wont_make_goes_away_with_a_toast() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::CreatePlaylist(fields("New"), vec![])));

    let effects = library.update(Message::Created(id, Err(failure())));

    assert_eq!(root_titles(&library, &[]), Vec::<String>::new());
    assert_eq!(
        toasts(&effects),
        vec!["Couldn't create \u{201c}New\u{201d}"]
    );
}

#[test]
fn an_edit_shows_straight_away_where_the_playlist_is() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Old");
    let server = playlists(&[own("p-0", "Other"), playlist.clone()]);

    let effects = library.update(Message::EditPlaylist(playlist.clone(), fields("New")));

    assert_eq!(root_titles(&library, &server), vec!["Other", "New"]);
    assert_eq!(library.playlist(&playlist).title, "New");
    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::UpdatePlaylist {
            user_id: USER,
            uuid: "p-1".to_string(),
            fields: fields("New"),
        }
    );
}

#[test]
fn a_refused_edit_puts_the_playlist_back_as_it_was() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Old");
    let effects = library.update(Message::EditPlaylist(playlist.clone(), fields("New")));
    let (id, _) = mutation(&effects);

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(root_titles(&library, &playlists(&[playlist])), vec!["Old"]);
    assert_eq!(toasts(&effects), vec!["Couldn't save \u{201c}New\u{201d}"]);
}

#[test]
fn a_landed_edit_reads_the_playlist_and_the_lists_again() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Old");
    let (id, _) = mutation(&library.update(Message::EditPlaylist(playlist, fields("New"))));

    let effects = library.update(Message::Done(id, Ok(())));

    assert!(effects.contains(&Effect::Refresh(vec![
        "folders".to_string(),
        "playlist:p-1".to_string(),
    ])));
}

#[test]
fn only_own_playlists_are_edited_or_deleted() {
    let mut library = library(&[], &[]);
    let theirs = PlaylistFields::new().playlist("p-1".to_string(), USER + 1);

    assert!(
        library
            .update(Message::Ask(Ask::EditPlaylist(theirs.clone())))
            .is_empty()
    );
    assert!(
        library
            .update(Message::EditPlaylist(theirs.clone(), fields("New")))
            .is_empty()
    );
    assert!(
        library
            .update(Message::DeletePlaylist(theirs.clone()))
            .is_empty()
    );
    assert!(
        library
            .update(Message::Ask(Ask::DeletePlaylist(theirs)))
            .is_empty()
    );
}

#[test]
fn the_dialogs_open_for_own_playlists() {
    let mut library = library(&[], &[]);
    let mine = own("p-1", "Mine");

    assert_eq!(
        library.update(Message::Ask(Ask::NewPlaylist(vec![]))),
        vec![Effect::Ask(Ask::NewPlaylist(vec![]))]
    );
    assert_eq!(
        library.update(Message::Ask(Ask::DeletePlaylist(mine.clone()))),
        vec![Effect::Ask(Ask::DeletePlaylist(mine))]
    );
}

#[test]
fn a_placeholder_cant_be_edited_or_deleted() {
    let mut library = library(&[], &[]);
    library.update(Message::CreatePlaylist(fields("New"), vec![]));
    let shown = library.apply::<Item>(&[], Listing::Shelf(&root_shelf()));
    let Item::Playlist(placeholder) = shown[0].clone() else {
        panic!("a playlist");
    };

    assert!(
        library
            .update(Message::DeletePlaylist(placeholder.clone()))
            .is_empty()
    );
    assert!(
        library
            .update(Message::EditPlaylist(placeholder, fields("Other")))
            .is_empty()
    );
}

#[test]
fn a_deleted_playlist_is_gone_at_once_and_its_pages_with_it() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Mine");
    let server = playlists(&[own("p-0", "Other"), playlist.clone()]);

    let effects = library.update(Message::DeletePlaylist(playlist));

    assert_eq!(root_titles(&library, &server), vec!["Other"]);
    assert!(effects.contains(&Effect::Deleted("p-1".to_string())));
    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::DeletePlaylist {
            user_id: USER,
            uuid: "p-1".to_string(),
        }
    );
    // Playback hears of it once TIDAL has deleted it.
    assert!(source_deleted(&effects).is_empty());
}

#[test]
fn source_deleted_goes_out_once_the_playlist_is_deleted() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::DeletePlaylist(own("p-1", "Mine"))));

    let effects = library.update(Message::Done(id, Ok(())));

    assert_eq!(
        source_deleted(&effects),
        vec![SourceRef::Playlist {
            uuid: "p-1".to_string(),
            sort: None,
        }]
    );
    assert!(effects.contains(&Effect::Refresh(vec![
        "folders".to_string(),
        "playlist:p-1".to_string(),
    ])));
}

#[test]
fn a_refused_deletion_brings_the_playlist_back_and_leaves_playback_alone() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Mine");
    let (id, _) = mutation(&library.update(Message::DeletePlaylist(playlist.clone())));

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(root_titles(&library, &playlists(&[playlist])), vec!["Mine"]);
    assert_eq!(
        toasts(&effects),
        vec!["Couldn't delete \u{201c}Mine\u{201d}"]
    );
    assert!(source_deleted(&effects).is_empty());
}

#[test]
fn a_deletion_waits_for_an_edit_of_the_same_playlist() {
    let mut library = library(&[], &[]);
    let playlist = own("p-1", "Mine");
    let (edit, _) =
        mutation(&library.update(Message::EditPlaylist(playlist.clone(), fields("New"))));

    let effects = library.update(Message::DeletePlaylist(playlist));
    assert_eq!(mutations(&effects), 0);

    let effects = library.update(Message::Done(edit, Ok(())));
    let (_, next) = mutation(&effects);
    assert!(matches!(next, Mutation::DeletePlaylist { .. }));
}

// Adding tracks to Own playlists.

fn own_with(uuid: &str, title: &str, tracks: u32) -> Playlist {
    Playlist {
        tracks,
        ..own(uuid, title)
    }
}

fn informed(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Inform(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn tracks(ids: &[u64]) -> Vec<Track> {
    ids.iter().copied().map(track).collect()
}

/// The playlists "Add to playlist" offers, as `(title, tracks)`.
fn offered(library: &Library, server: &[Item]) -> Vec<(String, u32)> {
    library
        .apply(server, Listing::Own)
        .iter()
        .filter_map(|item| match item {
            Item::Playlist(playlist) => Some((playlist.title.clone(), playlist.tracks)),
            _ => None,
        })
        .collect()
}

fn duplicate() -> Arc<syzygy_catalog::Error> {
    Arc::new(syzygy_catalog::Error::Tidal(syzygy_tidal::Error::Api {
        status: 409,
        body: String::new(),
    }))
}

#[test]
fn one_track_goes_in_as_it_is_and_the_playlist_counts_it_at_once() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 10);
    let server = playlists(std::slice::from_ref(&playlist));

    let effects = library.update(Message::AddTracks(playlist, tracks(&[5])));

    assert_eq!(offered(&library, &server), vec![("Mix".to_string(), 11)]);
    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::AddTrack {
            user_id: USER,
            uuid: "p-1".to_string(),
            track: 5,
        }
    );
    assert!(effects.contains(&Effect::Recent("p-1".to_string())));
}

#[test]
fn a_track_already_in_the_playlist_is_told_and_rolled_back() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 10);
    let (id, _) = mutation(&library.update(Message::AddTracks(playlist.clone(), tracks(&[5]))));

    let effects = library.update(Message::Done(id, Err(duplicate())));

    assert_eq!(informed(&effects), vec!["Track already in this playlist"]);
    assert!(toasts(&effects).is_empty());
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 10)]
    );
}

#[test]
fn an_added_track_is_told_and_its_playlist_read_again_now_and_for_its_cover() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 10);
    let (id, _) = mutation(&library.update(Message::AddTracks(playlist, tracks(&[5]))));

    let effects = library.update(Message::Done(id, Ok(())));

    assert_eq!(
        informed(&effects),
        vec!["Added \u{201c}Track 5\u{201d} to playlist"]
    );
    assert!(effects.contains(&Effect::Refresh(vec![
        "folders".to_string(),
        "playlist:p-1".to_string(),
    ])));
    assert!(effects.contains(&Effect::Cover("p-1".to_string())));
}

#[test]
fn a_refused_track_says_so() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::AddTracks(own("p-1", "Mix"), tracks(&[5]))));

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(
        toasts(&effects),
        vec!["Couldn't add \u{201c}Track 5\u{201d} to \u{201c}Mix\u{201d}"]
    );
}

#[test]
fn a_selection_skips_duplicates_and_says_how_many_were_there() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 10);
    let server = playlists(std::slice::from_ref(&playlist));
    let effects = library.update(Message::AddTracks(playlist, tracks(&[1, 2, 3])));
    let (id, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::AddTracks {
            user_id: USER,
            uuid: "p-1".to_string(),
            tracks: vec![1, 2, 3],
        }
    );
    assert_eq!(offered(&library, &server), vec![("Mix".to_string(), 13)]);

    let added = syzygy_catalog::Added {
        asked: 3,
        new: 1,
        tracks: 11,
    };
    let effects = library.update(Message::Added(id, Ok(added)));

    assert_eq!(informed(&effects), vec!["Added 1 (2 already in playlist)"]);
    // TIDAL's count, not the one guessed.
    assert_eq!(offered(&library, &server), vec![("Mix".to_string(), 11)]);
    assert!(effects.contains(&Effect::Cover("p-1".to_string())));
}

#[test]
fn a_selection_all_new_says_how_many_went_in() {
    let mut library = library(&[], &[]);
    let effects = library.update(Message::AddTracks(own("p-1", "Mix"), tracks(&[1, 2])));
    let (id, _) = mutation(&effects);

    let added = syzygy_catalog::Added {
        asked: 2,
        new: 2,
        tracks: 2,
    };
    let effects = library.update(Message::Added(id, Ok(added)));

    assert_eq!(
        informed(&effects),
        vec!["Added 2 tracks to \u{201c}Mix\u{201d}"]
    );
}

#[test]
fn tracks_go_only_into_own_playlists_that_tidal_has_made() {
    let mut library = library(&[], &[]);
    let theirs = PlaylistFields::new().playlist("p-1".to_string(), USER + 1);
    assert!(
        library
            .update(Message::AddTracks(theirs, tracks(&[1])))
            .is_empty()
    );

    library.update(Message::CreatePlaylist(fields("New"), vec![]));
    let shown = library.apply::<Item>(&[], Listing::Own);
    let Item::Playlist(placeholder) = shown[0].clone() else {
        panic!("a playlist");
    };
    assert!(
        library
            .update(Message::AddTracks(placeholder, tracks(&[1])))
            .is_empty()
    );
    assert!(
        library
            .update(Message::AddTracks(own("p-2", "Mine"), vec![]))
            .is_empty()
    );
}

#[test]
fn adds_to_one_playlist_run_in_order() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 10);
    let (first, _) = mutation(&library.update(Message::AddTracks(playlist.clone(), tracks(&[1]))));

    let effects = library.update(Message::AddTracks(playlist.clone(), tracks(&[2])));
    assert_eq!(mutations(&effects), 0);
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 12)]
    );

    let (_, next) = mutation(&library.update(Message::Done(first, Ok(()))));
    assert!(matches!(next, Mutation::AddTrack { track: 2, .. }));
}

#[test]
fn a_new_playlist_with_tracks_is_made_then_filled() {
    let mut library = library(&[], &[]);
    let effects = library.update(Message::CreatePlaylist(fields("New"), tracks(&[1, 2])));
    let (id, create) = mutation(&effects);
    assert!(matches!(create, Mutation::CreatePlaylist { .. }));
    assert_eq!(offered(&library, &[]), vec![("New".to_string(), 2)]);

    let effects = library.update(Message::Created(id, Ok(own("p-9", "New"))));

    let (_, add) = mutation(&effects);
    assert_eq!(
        add,
        Mutation::AddTracks {
            user_id: USER,
            uuid: "p-9".to_string(),
            tracks: vec![1, 2],
        }
    );
    assert!(effects.contains(&Effect::Recent("p-9".to_string())));
    // Still listed first while TIDAL's lists don't have it.
    assert_eq!(root_titles(&library, &[]), vec!["New"]);
    assert_eq!(offered(&library, &[]), vec![("New".to_string(), 2)]);
}

#[test]
fn a_new_playlist_whose_tracks_dont_go_in_is_kept_with_a_toast() {
    let mut library = library(&[], &[]);
    let (id, _) =
        mutation(&library.update(Message::CreatePlaylist(fields("New"), tracks(&[1, 2]))));
    let (add, _) = mutation(&library.update(Message::Created(id, Ok(own("p-9", "New")))));

    let effects = library.update(Message::Added(add, Err(failure())));

    assert_eq!(
        toasts(&effects),
        vec!["Created \u{201c}New\u{201d}, but couldn't add the tracks"]
    );
    assert_eq!(offered(&library, &[]), vec![("New".to_string(), 0)]);
    assert_eq!(mutations(&effects), 0);
}

#[test]
fn the_own_playlists_show_the_users_edits_but_not_their_likes() {
    let mut library = library(&[], &[]);
    let server = playlists(&[own("p-1", "Kept"), own("p-2", "Gone"), own("p-3", "Old")]);
    library.update(Message::DeletePlaylist(own("p-2", "Gone")));
    library.update(Message::EditPlaylist(own("p-3", "Old"), fields("Renamed")));
    library.update(Message::CreatePlaylist(fields("New"), vec![]));
    let theirs = PlaylistFields::new().playlist("p-4".to_string(), USER + 1);
    library.update(Message::Favorite(Favorite::playlist(&theirs), true));
    library.update(Message::Favorite(
        Favorite::playlist(&own("p-1", "Kept")),
        false,
    ));

    let titles: Vec<String> = offered(&library, &server)
        .into_iter()
        .map(|(title, _)| title)
        .collect();

    assert_eq!(titles, vec!["New", "Kept", "Renamed"]);
}

#[test]
fn the_picker_opens_once_tidal_has_said_who_the_user_is() {
    let at = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(240.0, 40.0));
    let pick = || Message::Pick {
        tracks: Tracks::These(tracks(&[1])),
        at,
    };
    let (mut unknown, _) = Library::new(None, &Settings::default());
    assert!(unknown.update(pick()).is_empty());

    let mut library = library(&[], &[]);
    assert_eq!(
        library.update(pick()),
        vec![Effect::Pick {
            tracks: Tracks::These(tracks(&[1])),
            at,
        }]
    );
}

#[test]
fn a_new_playlists_one_track_refused_as_a_dupe_still_says_the_track_didnt_go_in() {
    let mut library = library(&[], &[]);
    let (id, _) = mutation(&library.update(Message::CreatePlaylist(fields("New"), tracks(&[1]))));
    let (add, _) = mutation(&library.update(Message::Created(id, Ok(own("p-9", "New")))));

    let effects = library.update(Message::Done(add, Err(duplicate())));

    assert_eq!(
        toasts(&effects),
        vec!["Created \u{201c}New\u{201d}, but couldn't add the tracks"]
    );
    assert!(informed(&effects).is_empty());
}

// Removing tracks from Own playlists.

/// A track as an Own playlist lists it: when it was added tells apart two
/// of the same track.
fn entry(id: u64, added: &str) -> Track {
    Track {
        date_added: Some(added.to_string()),
        ..track(id)
    }
}

/// An Own playlist's rows, in its own order.
fn rows() -> Vec<Track> {
    vec![
        entry(1, "2024-01-01"),
        entry(2, "2024-01-02"),
        entry(3, "2024-01-03"),
        entry(4, "2024-01-04"),
    ]
}

/// The ids of the rows `library` leaves of `tracks`, in the order shown.
fn shown_ids(library: &Library, uuid: &str, tracks: &[Track], sorted: bool) -> Vec<u64> {
    library
        .rows(uuid, tracks, sorted)
        .into_iter()
        .map(|position| tracks[position].id)
        .collect()
}

fn remove(playlist: &Playlist, track: Track, at: At) -> Message {
    Message::RemoveTrack(Removal {
        playlist: playlist.clone(),
        track,
        at,
    })
}

#[test]
fn a_removal_in_the_own_order_goes_by_its_index_and_shows_at_once() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let server = playlists(std::slice::from_ref(&playlist));

    let effects = library.update(remove(&playlist, entry(2, "2024-01-02"), At::Index(1)));

    let (_, mutation) = mutation(&effects);
    assert_eq!(
        mutation,
        Mutation::RemoveTrack {
            user_id: USER,
            uuid: "p-1".to_string(),
            index: 1,
        }
    );
    assert_eq!(offered(&library, &server), vec![("Mix".to_string(), 3)]);
    assert_eq!(shown_ids(&library, "p-1", &rows(), false), vec![1, 3, 4]);
}

#[test]
fn rows_after_a_removal_move_up_and_the_next_removal_waits_with_its_new_index() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let (first, _) =
        mutation(&library.update(remove(&playlist, entry(2, "2024-01-02"), At::Index(1))));

    // Track 4 was fourth; with track 2 on its way out it's third.
    let effects = library.update(remove(&playlist, entry(4, "2024-01-04"), At::Index(2)));
    assert_eq!(mutations(&effects), 0);
    assert_eq!(shown_ids(&library, "p-1", &rows(), false), vec![1, 3]);
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 2)]
    );

    let (_, next) = mutation(&library.update(Message::Done(first, Ok(()))));
    assert_eq!(
        next,
        Mutation::RemoveTrack {
            user_id: USER,
            uuid: "p-1".to_string(),
            index: 2,
        }
    );
}

#[test]
fn a_landed_removal_leaves_the_row_to_the_page_and_reads_the_playlist_again() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let (id, _) =
        mutation(&library.update(remove(&playlist, entry(2, "2024-01-02"), At::Index(1))));

    let effects = library.update(Message::Done(id, Ok(())));

    assert!(effects.contains(&Effect::Removed {
        uuid: "p-1".to_string(),
        track: entry(2, "2024-01-02"),
        index: 1,
    }));
    assert!(effects.contains(&Effect::Refresh(vec![
        "folders".to_string(),
        "playlist:p-1".to_string(),
    ])));
    // The Page has dropped it from its rows, so they're shown as they are.
    let dropped: Vec<Track> = rows().into_iter().filter(|track| track.id != 2).collect();
    assert_eq!(shown_ids(&library, "p-1", &dropped, false), vec![1, 3, 4]);
    // The count stays down until TIDAL's lists have it.
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 3)]
    );
}

#[test]
fn a_refused_removal_brings_the_row_and_the_count_back_with_a_toast() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let (id, _) =
        mutation(&library.update(remove(&playlist, entry(2, "2024-01-02"), At::Index(1))));
    library.update(remove(&playlist, entry(4, "2024-01-04"), At::Index(2)));

    let effects = library.update(Message::Done(id, Err(failure())));

    assert_eq!(
        toasts(&effects),
        vec!["Couldn't remove \u{201c}Track 2\u{201d} from \u{201c}Mix\u{201d}"]
    );
    assert_eq!(mutations(&effects), 0);
    assert_eq!(shown_ids(&library, "p-1", &rows(), false), vec![1, 2, 3, 4]);
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 4)]
    );
}

#[test]
fn tracks_come_out_only_of_own_playlists_that_tidal_has_made() {
    let mut library = library(&[], &[]);
    let theirs = PlaylistFields::new().playlist("p-1".to_string(), USER + 1);

    let effects = library.update(remove(&theirs, entry(1, "2024-01-01"), At::Index(0)));

    assert!(effects.is_empty());
    assert_eq!(shown_ids(&library, "p-1", &rows(), false), vec![1, 2, 3, 4]);
}

/// The one own-order read `effects` asks for, before a sorted removal.
fn located(effects: &[Effect]) -> EditId {
    let reads: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ReadOrder(id, uuid) if uuid == "p-1" => Some(*id),
            _ => None,
        })
        .collect();
    match reads.as_slice() {
        [one] => *one,
        _ => panic!("one read of the own order, not {effects:?}"),
    }
}

#[test]
fn a_sorted_removal_reads_the_own_order_and_goes_by_the_row_it_finds() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    // Sorted by title, last first.
    let sorted: Vec<Track> = rows().into_iter().rev().collect();

    let effects = library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted));

    assert_eq!(mutations(&effects), 0);
    let id = located(&effects);
    assert_eq!(shown_ids(&library, "p-1", &sorted, true), vec![4, 2, 1]);

    // Track 3 is there twice, added apart.
    let order = vec![
        entry(3, "2023-12-31"),
        entry(1, "2024-01-01"),
        entry(2, "2024-01-02"),
        entry(3, "2024-01-03"),
        entry(4, "2024-01-04"),
    ];
    let (_, mutation) = mutation(&library.update(Message::Order(id, Ok(order))));
    assert_eq!(
        mutation,
        Mutation::RemoveTrack {
            user_id: USER,
            uuid: "p-1".to_string(),
            index: 3,
        }
    );
}

#[test]
fn a_sorted_removal_whose_row_isnt_there_is_refused_with_a_toast() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let id = located(&library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted)));

    let order = vec![entry(1, "2024-01-01"), entry(3, "2024-02-01")];
    let effects = library.update(Message::Order(id, Ok(order)));

    assert_eq!(mutations(&effects), 0);
    assert_eq!(
        toasts(&effects),
        vec!["Couldn't find \u{201c}Track 3\u{201d} in \u{201c}Mix\u{201d}"]
    );
    assert_eq!(shown_ids(&library, "p-1", &rows(), true), vec![1, 2, 3, 4]);
    assert_eq!(
        offered(&library, &playlists(&[playlist])),
        vec![("Mix".to_string(), 4)]
    );
}

#[test]
fn a_sorted_removal_matching_several_rows_is_refused_with_a_toast() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let id = located(&library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted)));

    let order = vec![entry(3, "2024-01-03"), entry(3, "2024-01-03")];
    let effects = library.update(Message::Order(id, Ok(order)));

    assert_eq!(mutations(&effects), 0);
    assert_eq!(
        toasts(&effects),
        vec![
            "Couldn't tell which \u{201c}Track 3\u{201d} to remove. \
             Sort the playlist by # and remove it there"
        ]
    );
    assert_eq!(shown_ids(&library, "p-1", &rows(), true), vec![1, 2, 3, 4]);
}

#[test]
fn a_sorted_removal_waits_for_the_one_before_it_to_read_the_own_order() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let (first, _) =
        mutation(&library.update(remove(&playlist, entry(1, "2024-01-01"), At::Index(0))));

    let effects = library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted));
    assert!(effects.is_empty());

    located(&library.update(Message::Done(first, Ok(()))));
}

#[test]
fn a_sorted_removal_whose_read_fails_says_so() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let id = located(&library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted)));

    let effects = library.update(Message::Order(id, Err(failure())));

    assert_eq!(
        toasts(&effects),
        vec!["Couldn't remove \u{201c}Track 3\u{201d} from \u{201c}Mix\u{201d}"]
    );
}

#[test]
fn a_refused_sorted_removal_drops_the_removals_waiting_behind_it() {
    let mut library = library(&[], &[]);
    let playlist = own_with("p-1", "Mix", 4);
    let id = located(&library.update(remove(&playlist, entry(3, "2024-01-03"), At::Sorted)));
    library.update(remove(&playlist, entry(1, "2024-01-01"), At::Sorted));

    let effects = library.update(Message::Order(id, Ok(vec![])));

    assert_eq!(toasts(&effects).len(), 1);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::ReadOrder(..)))
    );
    assert_eq!(shown_ids(&library, "p-1", &rows(), true), vec![1, 2, 3, 4]);
}
