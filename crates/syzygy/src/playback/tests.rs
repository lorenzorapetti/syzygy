//! Seam 1: messages in, state and effects out.

use super::*;
use syzygy_catalog::track::ArtistRef;

fn track(id: u64) -> Track {
    Track {
        id,
        title: format!("Track {id}"),
        artists: vec![ArtistRef {
            id: 1,
            name: "Artist".to_string(),
        }],
        album: None,
        duration: 200,
        explicit: false,
        volume: 1,
        date_added: None,
    }
}

/// An album of tracks 1..=n, started at track `start` (from 0).
fn album(id: u64, n: u64, start: usize) -> PlayRequest {
    PlayRequest {
        source: SourceRef::Album(id),
        first_page: (1..=n).map(|i| track(id * 100 + i)).collect(),
        start,
    }
}

fn ids<'a>(tracks: impl Iterator<Item = &'a Track>) -> Vec<u64> {
    tracks.map(|track| track.id).collect()
}

fn current(playback: &Playback) -> Option<u64> {
    playback.current().map(|track| track.id)
}

/// The token of the one Play among `effects`.
fn token(effects: &[Effect]) -> PlayToken {
    match effects {
        [Effect::Play { token, .. }] => *token,
        _ => panic!("expected one Play, got {effects:?}"),
    }
}

fn played(token: PlayToken) -> Message {
    Message::Played(token, Ok(()))
}

fn resolve_failed(token: PlayToken) -> Message {
    let error = syzygy_tidal::Error::Network("offline".to_string());
    Message::Played(token, Err(PlayError::Resolve(Arc::new(error))))
}

fn audio_failed(token: PlayToken) -> Message {
    let error = syzygy_audio::Error::Engine("no sink".to_string());
    Message::Played(token, Err(PlayError::Audio(Arc::new(error))))
}

/// Playing `request`, its first track started.
fn playing(request: PlayRequest) -> Playback {
    let mut playback = Playback::new(1.0);
    let effects = playback.update(Message::Start(request));
    playback.update(played(token(&effects)));
    playback
}

#[test]
fn a_track_plays_to_the_end_of_its_album_without_wrapping_around() {
    let mut playback = Playback::new(1.0);

    let effects = playback.update(Message::Start(album(1, 5, 2)));

    assert_eq!(
        effects,
        vec![Effect::Play {
            token: token(&effects),
            track_id: 103,
            from: None,
        }]
    );
    assert_eq!(current(&playback), Some(103));
    assert_eq!(ids(playback.upcoming()), vec![104, 105]);
}

#[test]
fn each_finished_track_starts_the_next_until_the_album_runs_out() {
    let mut playback = playing(album(1, 3, 1));

    let effects = playback.update(Message::TrackFinished);
    assert!(matches!(effects[..], [Effect::Play { track_id: 103, .. }]));
    assert_eq!(current(&playback), Some(103));
    assert_eq!(ids(playback.upcoming()), Vec::<u64>::new());
    playback.update(played(token(&effects)));

    let effects = playback.update(Message::TrackFinished);
    assert_eq!(effects, vec![]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(103), "the last track stays shown");
}

#[test]
fn the_current_track_changes_as_soon_as_it_is_chosen() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.update(Message::Start(album(2, 3, 1)));

    assert_eq!(current(&playback), Some(202));
    assert_eq!(playback.status(), Status::Loading(token(&effects)));
    assert_eq!(playback.position(), 0.0);
}

#[test]
fn a_play_result_for_an_older_choice_is_dropped() {
    let mut playback = Playback::new(1.0);
    let first = token(&playback.update(Message::Start(album(1, 3, 0))));
    let second = token(&playback.update(Message::Start(album(1, 3, 2))));

    assert_eq!(playback.update(played(first)), vec![]);
    assert_eq!(playback.status(), Status::Loading(second));
    assert_eq!(current(&playback), Some(103));

    playback.update(played(second));
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_failure_for_an_older_choice_rolls_nothing_back() {
    let mut playback = Playback::new(1.0);
    let first = token(&playback.update(Message::Start(album(1, 3, 0))));
    let second = token(&playback.update(Message::Start(album(2, 3, 0))));

    playback.update(resolve_failed(first));

    assert_eq!(current(&playback), Some(201));
    assert_eq!(playback.status(), Status::Loading(second));
}

#[test]
fn a_track_that_cant_be_resolved_rolls_back_to_what_still_plays() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Position(42.0));

    let effects = playback.update(Message::Start(album(2, 3, 1)));
    let effects = playback.update(resolve_failed(token(&effects)));

    assert_eq!(effects, vec![]);
    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(playback.status(), Status::Playing);
    assert_eq!(playback.position(), 42.0);
}

#[test]
fn a_track_the_engine_refuses_rolls_back_stopped() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Position(42.0));

    let effects = playback.update(Message::Start(album(2, 3, 1)));
    playback.update(audio_failed(token(&effects)));

    // The engine let go of the old track to start the new one.
    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 42.0);
}

#[test]
fn rapid_choices_roll_back_to_what_played_before_them_all() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Start(album(2, 3, 0)));
    let last = token(&playback.update(Message::Start(album(3, 3, 0))));

    playback.update(resolve_failed(last));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_next_track_that_fails_rolls_back_to_the_finished_one_stopped() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.update(Message::TrackFinished);
    playback.update(resolve_failed(token(&effects)));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 0.0);
}

#[test]
fn only_track_finished_ends_a_track() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.update(Message::Position(500.0));

    assert_eq!(effects, vec![]);
    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.position(), 500.0);
}

#[test]
fn a_track_finishing_while_another_loads_is_ignored() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.update(Message::Start(album(2, 3, 0))));

    assert_eq!(playback.update(Message::TrackFinished), vec![]);
    assert_eq!(current(&playback), Some(201));
    assert_eq!(playback.status(), Status::Loading(loading));
}

#[test]
fn the_position_moves_only_while_playing() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Position(10.0));
    playback.update(Message::TogglePlay);

    playback.update(Message::Position(11.0));

    assert_eq!(playback.position(), 10.0);
}

#[test]
fn play_and_pause_toggle_the_engine() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(playback.update(Message::TogglePlay), vec![Effect::Pause]);
    assert_eq!(playback.status(), Status::Paused);
    assert_eq!(playback.update(Message::TogglePlay), vec![Effect::Resume]);
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn play_while_stopped_starts_the_current_track_where_it_stopped() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Position(42.0));
    playback.update(Message::EngineFailed);
    assert_eq!(playback.status(), Status::Stopped);

    let effects = playback.update(Message::TogglePlay);

    assert_eq!(
        effects,
        vec![Effect::Play {
            token: token(&effects),
            track_id: 101,
            from: Some(42.0),
        }]
    );
    assert_eq!(playback.status(), Status::Loading(token(&effects)));
}

#[test]
fn play_after_the_album_ran_out_starts_its_last_track_again() {
    let mut playback = playing(album(1, 1, 0));
    playback.update(Message::TrackFinished);

    let effects = playback.update(Message::TogglePlay);

    assert!(matches!(
        effects[..],
        [Effect::Play {
            track_id: 101,
            from: None,
            ..
        }]
    ));
}

#[test]
fn play_and_pause_do_nothing_with_nothing_to_play_or_while_loading() {
    let mut playback = Playback::new(1.0);
    assert_eq!(playback.update(Message::TogglePlay), vec![]);

    playback.update(Message::Start(album(1, 3, 0)));
    assert_eq!(playback.update(Message::TogglePlay), vec![]);
}

#[test]
fn a_seek_goes_to_the_engine_and_moves_the_position() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(
        playback.update(Message::Seek(30.0)),
        vec![Effect::Seek(30.0)]
    );
    assert_eq!(playback.position(), 30.0);
}

#[test]
fn a_seek_while_stopped_is_where_play_starts() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::EngineFailed);

    assert_eq!(playback.update(Message::Seek(30.0)), vec![]);
    let effects = playback.update(Message::TogglePlay);

    assert!(matches!(
        effects[..],
        [Effect::Play {
            from: Some(30.0),
            ..
        }]
    ));
}

#[test]
fn the_volume_goes_to_the_engine() {
    let mut playback = Playback::new(1.0);

    assert_eq!(
        playback.update(Message::SetVolume(0.3)),
        vec![Effect::SetVolume(0.3)]
    );
    assert_eq!(playback.volume(), 0.3);
}

#[test]
fn unmuting_restores_the_volume_from_before() {
    let mut playback = Playback::new(0.8);

    assert_eq!(
        playback.update(Message::ToggleMute),
        vec![Effect::SetVolume(0.0)]
    );
    assert_eq!(playback.volume(), 0.0);
    assert_eq!(
        playback.update(Message::ToggleMute),
        vec![Effect::SetVolume(0.8)]
    );
}

#[test]
fn unmuting_with_no_volume_from_before_goes_to_half() {
    let mut playback = Playback::new(0.0);

    assert_eq!(
        playback.update(Message::ToggleMute),
        vec![Effect::SetVolume(0.5)]
    );
}

#[test]
fn a_track_that_ends_while_the_next_choice_loads_isnt_brought_back_playing() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.update(Message::Start(album(2, 3, 0))));

    playback.update(Message::TrackFinished);
    playback.update(resolve_failed(loading));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Stopped);
}

#[test]
fn a_track_that_breaks_while_the_next_choice_loads_isnt_brought_back_playing() {
    let mut playback = playing(album(1, 3, 0));
    playback.update(Message::Position(42.0));
    let loading = token(&playback.update(Message::Start(album(2, 3, 0))));

    playback.update(Message::EngineFailed);
    playback.update(resolve_failed(loading));

    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(
        playback.position(),
        42.0,
        "play starts again where it broke"
    );
}
