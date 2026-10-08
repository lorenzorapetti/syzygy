//! Seam 1: messages in, state and effects out.

use super::*;
use rand::SeedableRng;
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

/// Playback with nothing playing, Shuffle and Repeat off, and the same
/// shuffles every run.
fn new(volume: f32) -> Playback {
    Playback::new(
        Preferences {
            volume,
            shuffle: false,
            repeat: Repeat::Off,
        },
        SmallRng::seed_from_u64(7),
    )
}

fn source(kind: SourceRef) -> Source {
    Source {
        name: format!("{kind:?}"),
        kind,
    }
}

/// `n` tracks numbered from `id * 100 + 1`, started as `start` says.
fn request(kind: SourceRef, id: u64, n: u64, start: Start) -> PlayRequest {
    PlayRequest {
        source: source(kind),
        first_page: (1..=n).map(|i| track(id * 100 + i)).collect(),
        start,
    }
}

/// An album of tracks 1..=n, started at track `start` (from 0).
fn album(id: u64, n: u64, start: usize) -> PlayRequest {
    request(SourceRef::Album(id), id, n, Start::Track(start))
}

fn playlist(id: u64, n: u64, start: Start) -> PlayRequest {
    request(SourceRef::Playlist(format!("playlist-{id}")), id, n, start)
}

/// What plays now and after, in order.
fn play_order(playback: &Playback) -> Vec<u64> {
    current(playback)
        .into_iter()
        .chain(ids(playback.upcoming()))
        .collect()
}

fn sorted(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids
}

fn history(playback: &Playback) -> Vec<u64> {
    ids(playback.history())
}

fn playing_from(playback: &Playback) -> Option<SourceRef> {
    playback.playing_from().map(|source| source.kind.clone())
}

/// Whether the one Play among `effects` asks for album gain.
fn album_gain(effects: &[Effect]) -> bool {
    match effects {
        [Effect::Play { album_gain, .. }] => *album_gain,
        _ => panic!("expected one Play, got {effects:?}"),
    }
}

fn shuffled(mut playback: Playback) -> Playback {
    playback.update(Message::ToggleShuffle);
    playback
}

/// Send `message` and let the play it starts succeed.
fn and_play(playback: &mut Playback, message: Message) -> Vec<Effect> {
    let effects = playback.update(message);
    if let [Effect::Play { token, .. }] = effects[..] {
        playback.update(played(token));
    }
    effects
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
    playing_with(new(1.0), request)
}

/// `playback`, now playing `request`.
fn playing_with(mut playback: Playback, request: PlayRequest) -> Playback {
    and_play(&mut playback, Message::Start(request));
    playback
}

#[test]
fn a_track_plays_to_the_end_of_its_album_without_wrapping_around() {
    let mut playback = new(1.0);

    let effects = playback.update(Message::Start(album(1, 5, 2)));

    assert_eq!(
        effects,
        vec![Effect::Play {
            token: token(&effects),
            track_id: 103,
            from: None,
            album_gain: true,
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
    let mut playback = new(1.0);
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
    let mut playback = new(1.0);
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
            album_gain: true,
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
    let mut playback = new(1.0);
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
    let mut playback = new(1.0);

    assert_eq!(
        playback.update(Message::SetVolume(0.3)),
        vec![Effect::SetVolume(0.3)]
    );
    assert_eq!(playback.volume(), 0.3);
}

#[test]
fn unmuting_restores_the_volume_from_before() {
    let mut playback = new(0.8);

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
    let mut playback = new(0.0);

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

// Playback sources and "Playing from".

#[test]
fn a_play_remembers_the_source_it_came_from() {
    let playback = playing(album(1, 3, 1));

    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
    assert_eq!(playback.playing_from().unwrap().name, "Album(1)");
}

#[test]
fn a_track_played_on_its_own_still_has_a_source() {
    let mut playback = new(1.0);
    playback.update(Message::CycleRepeat);

    let mut playback = playing_with(
        playback,
        request(SourceRef::Track(5), 5, 1, Start::Track(0)),
    );
    assert_eq!(playing_from(&playback), Some(SourceRef::Track(5)));

    let effects = playback.update(Message::TrackFinished);
    assert!(matches!(effects[..], [Effect::Play { track_id: 501, .. }]));
    assert_eq!(playing_from(&playback), Some(SourceRef::Track(5)));
}

#[test]
fn nothing_plays_from_anywhere_before_the_first_play() {
    assert_eq!(playing_from(&new(1.0)), None);
}

// Starting a source.

#[test]
fn play_starts_a_source_at_its_top() {
    let playback = playing(playlist(1, 4, Start::All));

    assert_eq!(play_order(&playback), vec![101, 102, 103, 104]);
}

#[test]
fn play_respects_shuffle() {
    let playback = playing_with(shuffled(new(1.0)), playlist(1, 10, Start::All));

    let order = play_order(&playback);
    assert_eq!(sorted(order.clone()), (101..=110).collect::<Vec<_>>());
    assert_ne!(order, sorted(order.clone()), "the whole source is shuffled");
    assert!(playback.shuffle());
}

#[test]
fn a_track_chosen_with_shuffle_on_plays_first_then_the_rest_after_it_shuffled() {
    let playback = playing_with(shuffled(new(1.0)), playlist(1, 10, Start::Track(3)));

    assert_eq!(current(&playback), Some(104));
    let upcoming = ids(playback.upcoming());
    assert_eq!(sorted(upcoming.clone()), (105..=110).collect::<Vec<_>>());
    assert_ne!(upcoming, sorted(upcoming.clone()));
}

#[test]
fn shuffle_play_shuffles_once_without_turning_shuffle_on() {
    let playback = playing(playlist(1, 10, Start::Shuffled));

    let order = play_order(&playback);
    assert_eq!(sorted(order.clone()), (101..=110).collect::<Vec<_>>());
    assert_ne!(order, sorted(order.clone()));
    assert!(!playback.shuffle());
}

#[test]
fn the_same_seed_shuffles_the_same_way() {
    let a = playing(playlist(1, 10, Start::Shuffled));
    let b = playing(playlist(1, 10, Start::Shuffled));

    assert_eq!(play_order(&a), play_order(&b));
}

#[test]
fn a_start_past_the_end_of_the_source_is_ignored() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(playback.update(Message::Start(album(2, 3, 3))), vec![]);
    assert_eq!(current(&playback), Some(101));
}

// Shuffle.

#[test]
fn turning_shuffle_on_shuffles_only_what_is_left() {
    let mut playback = playing(playlist(1, 10, Start::All));
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);

    assert_eq!(playback.update(Message::ToggleShuffle), vec![]);

    assert!(playback.shuffle());
    assert_eq!(current(&playback), Some(103));
    assert_eq!(history(&playback), vec![101, 102]);
    let upcoming = ids(playback.upcoming());
    assert_eq!(sorted(upcoming.clone()), (104..=110).collect::<Vec<_>>());
    assert_ne!(upcoming, sorted(upcoming.clone()));
}

#[test]
fn turning_shuffle_off_puts_what_is_left_back_in_source_order() {
    let mut playback = playing_with(shuffled(new(1.0)), playlist(1, 10, Start::All));
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);
    let played = history(&playback);
    let now = current(&playback).unwrap();

    playback.update(Message::ToggleShuffle);

    assert!(!playback.shuffle());
    assert_eq!(current(&playback), Some(now));
    let rest: Vec<u64> = (101..=110)
        .filter(|id| *id != now && !played.contains(id))
        .collect();
    assert_eq!(ids(playback.upcoming()), rest);
}

#[test]
fn turning_shuffle_off_never_brings_back_tracks_before_the_one_chosen() {
    let mut playback = playing_with(shuffled(new(1.0)), playlist(1, 10, Start::Track(6)));

    playback.update(Message::ToggleShuffle);

    assert_eq!(ids(playback.upcoming()), vec![108, 109, 110]);
}

#[test]
fn shuffle_with_nothing_playing_only_changes_the_mode() {
    let mut playback = new(1.0);

    assert_eq!(playback.update(Message::ToggleShuffle), vec![]);
    assert!(playback.shuffle());
    assert_eq!(current(&playback), None);
}

// Next.

#[test]
fn next_plays_what_comes_next_and_puts_the_current_track_in_history() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.update(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 102, .. }]));
    assert_eq!(current(&playback), Some(102));
    assert_eq!(ids(playback.upcoming()), vec![103]);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn next_on_the_last_track_stops() {
    let mut playback = playing(album(1, 2, 1));

    assert_eq!(playback.update(Message::Next), vec![Effect::Stop]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 0.0);
    assert_eq!(current(&playback), Some(102), "the last track stays shown");
    assert_eq!(history(&playback), Vec::<u64>::new());
}

#[test]
fn next_with_nothing_playing_does_nothing() {
    assert_eq!(new(1.0).update(Message::Next), vec![]);
}

#[test]
fn rapid_nexts_each_move_on() {
    let mut playback = playing(album(1, 4, 0));

    playback.update(Message::Next);
    let last = token(&playback.update(Message::Next));

    assert_eq!(current(&playback), Some(103));
    assert_eq!(history(&playback), vec![101, 102]);
    assert_eq!(playback.status(), Status::Loading(last));
}

#[test]
fn a_next_that_fails_rolls_history_back_too() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.update(Message::Next);
    playback.update(resolve_failed(token(&effects)));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(history(&playback), Vec::<u64>::new());
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_new_source_puts_the_current_track_in_history() {
    let mut playback = playing(album(1, 3, 0));

    and_play(&mut playback, Message::Start(album(2, 3, 0)));

    assert_eq!(history(&playback), vec![101]);
}

// Repeat.

#[test]
fn repeat_cycles_off_all_one() {
    let mut playback = new(1.0);
    assert_eq!(playback.repeat(), Repeat::Off);

    assert_eq!(playback.update(Message::CycleRepeat), vec![]);
    assert_eq!(playback.repeat(), Repeat::All);
    playback.update(Message::CycleRepeat);
    assert_eq!(playback.repeat(), Repeat::One);
    playback.update(Message::CycleRepeat);
    assert_eq!(playback.repeat(), Repeat::Off);
}

fn repeating(repeat: Repeat) -> Playback {
    let mut playback = new(1.0);
    while playback.repeat() != repeat {
        playback.update(Message::CycleRepeat);
    }
    playback
}

#[test]
fn repeat_one_replays_the_track_when_it_ends() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 3, 0));

    let effects = playback.update(Message::TrackFinished);

    assert!(matches!(
        effects[..],
        [Effect::Play {
            track_id: 101,
            from: None,
            ..
        }]
    ));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(history(&playback), Vec::<u64>::new());
}

#[test]
fn next_moves_on_under_repeat_one() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 3, 0));

    let effects = playback.update(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 102, .. }]));
}

#[test]
fn next_on_the_last_track_stops_under_repeat_one() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 2, 1));

    assert_eq!(playback.update(Message::Next), vec![Effect::Stop]);
}

#[test]
fn repeat_all_starts_the_source_over_and_keeps_history() {
    let mut playback = playing_with(repeating(Repeat::All), album(1, 3, 1));
    and_play(&mut playback, Message::TrackFinished);

    let effects = playback.update(Message::TrackFinished);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(play_order(&playback), vec![101, 102, 103]);
    assert_eq!(history(&playback), vec![102, 103]);
}

#[test]
fn next_on_the_last_track_starts_over_under_repeat_all() {
    let mut playback = playing_with(repeating(Repeat::All), album(1, 2, 1));

    let effects = playback.update(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
}

#[test]
fn repeat_all_reshuffles_with_shuffle_on() {
    let playback = shuffled(repeating(Repeat::All));
    let mut playback = playing_with(playback, playlist(1, 8, Start::All));
    let first = play_order(&playback);
    for _ in 0..8 {
        and_play(&mut playback, Message::TrackFinished);
    }

    let second = play_order(&playback);
    assert_eq!(sorted(second.clone()), (101..=108).collect::<Vec<_>>());
    assert_ne!(second, first);
    assert_ne!(second, sorted(second.clone()));
}

#[test]
fn repeat_all_after_a_shuffle_play_starts_over_in_source_order() {
    let mut playback = playing_with(repeating(Repeat::All), playlist(1, 5, Start::Shuffled));
    for _ in 0..5 {
        and_play(&mut playback, Message::TrackFinished);
    }

    assert_eq!(play_order(&playback), vec![101, 102, 103, 104, 105]);
}

// Previous.

#[test]
fn previous_past_three_seconds_restarts_the_track() {
    let mut playback = playing(album(1, 3, 0));
    and_play(&mut playback, Message::Next);
    playback.update(Message::Position(3.5));

    assert_eq!(playback.update(Message::Previous), vec![Effect::Seek(0.0)]);
    assert_eq!(current(&playback), Some(102));
    assert_eq!(playback.position(), 0.0);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn previous_within_three_seconds_goes_back_a_step() {
    let mut playback = playing(album(1, 3, 0));
    and_play(&mut playback, Message::Next);
    playback.update(Message::Position(2.0));

    let effects = playback.update(Message::Previous);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(history(&playback), Vec::<u64>::new());
}

#[test]
fn previous_with_no_history_restarts_the_track() {
    let mut playback = playing(album(1, 3, 1));
    playback.update(Message::Position(1.0));

    assert_eq!(playback.update(Message::Previous), vec![Effect::Seek(0.0)]);
    assert_eq!(current(&playback), Some(102));
}

#[test]
fn previous_steps_back_through_a_shuffled_order() {
    let mut playback = playing_with(shuffled(new(1.0)), playlist(1, 6, Start::All));
    let order = play_order(&playback);
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);

    and_play(&mut playback, Message::Previous);
    and_play(&mut playback, Message::Previous);

    assert_eq!(play_order(&playback), order);
}

#[test]
fn previous_into_another_source_plays_it_under_that_source() {
    let mut playback = playing(album(1, 3, 0));
    and_play(&mut playback, Message::Start(album(2, 3, 0)));

    let effects = playback.update(Message::Previous);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
    assert_eq!(ids(playback.upcoming()), vec![201, 202, 203]);
    assert_eq!(history(&playback), Vec::<u64>::new());

    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(201));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(2)));
}

#[test]
fn history_keeps_the_last_500_tracks() {
    let mut playback = playing(playlist(1, 600, Start::All));
    for _ in 0..550 {
        playback.update(Message::Next);
    }

    let history = history(&playback);
    assert_eq!(history.len(), 500);
    assert_eq!(history.first(), Some(&151));
    assert_eq!(history.last(), Some(&650));
}

// Album gain.

#[test]
fn an_album_played_in_album_order_gets_album_gain() {
    let mut playback = new(1.0);

    assert!(album_gain(&playback.update(Message::Start(album(1, 3, 1)))));
}

#[test]
fn an_album_played_with_shuffle_on_gets_track_gain() {
    let mut playback = shuffled(new(1.0));
    let start = request(SourceRef::Album(1), 1, 3, Start::All);

    assert!(!album_gain(&playback.update(Message::Start(start))));
}

#[test]
fn a_shuffle_play_of_an_album_gets_track_gain() {
    let mut playback = new(1.0);
    let start = request(SourceRef::Album(1), 1, 3, Start::Shuffled);

    assert!(!album_gain(&playback.update(Message::Start(start))));
}

#[test]
fn turning_shuffle_on_mid_album_switches_to_track_gain() {
    let mut playback = shuffled(playing(album(1, 4, 0)));

    assert!(!album_gain(&playback.update(Message::Next)));
}

#[test]
fn other_sources_get_track_gain() {
    let mut playback = new(1.0);

    assert!(!album_gain(&playback.update(Message::Start(playlist(
        1,
        3,
        Start::All
    )))));
}
