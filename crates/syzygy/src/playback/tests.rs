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
        available: true,
        volume: 1,
        date_added: None,
        track_radio: None,
    }
}

impl Playback {
    /// Send a message that mustn't need consent, for its effects. Asks to
    /// save the snapshot are left out: [`saves`] checks those.
    fn send(&mut self, message: Message) -> Vec<Effect> {
        match self.update(message) {
            Outcome::Effects(mut effects) => {
                effects.retain(|effect| *effect != Effect::SaveSnapshot);
                effects
            }
            Outcome::NeedsExplicitConsent(pending) => {
                panic!("expected effects, got a consent question for {pending:?}")
            }
        }
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
            autoplay: false,
            gapless: true,
            allow_explicit: true,
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
        continuation: None,
    }
}

/// An album of tracks 1..=n, started at track `start` (from 0).
fn album(id: u64, n: u64, start: usize) -> PlayRequest {
    request(SourceRef::Album(id), id, n, Start::Track(start))
}

fn playlist(id: u64, n: u64, start: Start) -> PlayRequest {
    request(playlist_ref(id), id, n, start)
}

fn playlist_ref(id: u64) -> SourceRef {
    SourceRef::Playlist {
        uuid: format!("playlist-{id}"),
        sort: None,
    }
}

/// The first `n` tracks of a playlist with more after them, to be read in
/// the background.
fn long_playlist(id: u64, n: u64, start: Start) -> PlayRequest {
    PlayRequest {
        continuation: Some(Continuation {
            source: playlist_ref(id),
            offset: n as usize,
        }),
        ..playlist(id, n, start)
    }
}

/// Tracks `from..=to` of playlist `id`'s, as a page of them.
fn page(id: u64, from: u64, to: u64) -> Vec<Track> {
    (from..=to).map(|i| track(id * 100 + i)).collect()
}

/// The fill among `effects`, which must have started one.
fn fill(effects: &[Effect]) -> (FillId, Continuation) {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::StartFill {
                fill_id,
                continuation,
            } => Some((*fill_id, continuation.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a StartFill, got {effects:?}"))
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

/// Whether the Play among `effects` asks for album gain.
fn album_gain(effects: &[Effect]) -> bool {
    match the_play(effects) {
        Effect::Play { album_gain, .. } => *album_gain,
        _ => unreachable!(),
    }
}

/// The one Play among `effects`, whatever else went out with it.
fn the_play(effects: &[Effect]) -> &Effect {
    let mut plays = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Play { .. }));
    match (plays.next(), plays.next()) {
        (Some(play), None) => play,
        _ => panic!("expected one Play, got {effects:?}"),
    }
}

fn shuffled(mut playback: Playback) -> Playback {
    playback.send(Message::ToggleShuffle);
    playback
}

/// Send `message` and let the play it starts succeed.
fn and_play(playback: &mut Playback, message: Message) -> Vec<Effect> {
    let effects = playback.send(message);
    if let Some(&Effect::Play { token, .. }) = effects
        .iter()
        .find(|effect| matches!(effect, Effect::Play { .. }))
    {
        playback.send(played(token));
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
    match the_play(effects) {
        Effect::Play { token, .. } => *token,
        _ => unreachable!(),
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

/// `effects` but the arms: what follows a track is armed again whenever it
/// changes.
fn unarmed(effects: Vec<Effect>) -> Vec<Effect> {
    effects
        .into_iter()
        .filter(|effect| !matches!(effect, Effect::ArmNext { .. }))
        .collect()
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

    let effects = playback.send(Message::Start(album(1, 5, 2)));

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

    let effects = playback.send(Message::TrackFinished);
    assert!(matches!(effects[..], [Effect::Play { track_id: 103, .. }]));
    assert_eq!(current(&playback), Some(103));
    assert_eq!(ids(playback.upcoming()), Vec::<u64>::new());
    playback.send(played(token(&effects)));

    let effects = playback.send(Message::TrackFinished);
    assert_eq!(effects, vec![]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(103), "the last track stays shown");
}

#[test]
fn the_current_track_changes_as_soon_as_it_is_chosen() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.send(Message::Start(album(2, 3, 1)));

    assert_eq!(current(&playback), Some(202));
    assert_eq!(playback.status(), Status::Loading(token(&effects)));
    assert_eq!(playback.position(), 0.0);
}

#[test]
fn a_play_result_for_an_older_choice_is_dropped() {
    let mut playback = new(1.0);
    let first = token(&playback.send(Message::Start(album(1, 3, 0))));
    let second = token(&playback.send(Message::Start(album(1, 3, 2))));

    assert_eq!(playback.send(played(first)), vec![]);
    assert_eq!(playback.status(), Status::Loading(second));
    assert_eq!(current(&playback), Some(103));

    playback.send(played(second));
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_failure_for_an_older_choice_rolls_nothing_back() {
    let mut playback = new(1.0);
    let first = token(&playback.send(Message::Start(album(1, 3, 0))));
    let second = token(&playback.send(Message::Start(album(2, 3, 0))));

    playback.send(resolve_failed(first));

    assert_eq!(current(&playback), Some(201));
    assert_eq!(playback.status(), Status::Loading(second));
}

#[test]
fn a_track_that_cant_be_resolved_rolls_back_to_what_still_plays() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(42.0));

    let effects = playback.send(Message::Start(album(2, 3, 1)));
    let effects = playback.send(resolve_failed(token(&effects)));

    assert_eq!(unarmed(effects), vec![]);
    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(playback.status(), Status::Playing);
    assert_eq!(playback.position(), 42.0);
}

#[test]
fn a_track_the_engine_refuses_rolls_back_stopped() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(42.0));

    let effects = playback.send(Message::Start(album(2, 3, 1)));
    playback.send(audio_failed(token(&effects)));

    // The engine let go of the old track to start the new one.
    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 42.0);
}

#[test]
fn rapid_choices_roll_back_to_what_played_before_them_all() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Start(album(2, 3, 0)));
    let last = token(&playback.send(Message::Start(album(3, 3, 0))));

    playback.send(resolve_failed(last));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_next_track_that_fails_rolls_back_to_the_finished_one_stopped() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.send(Message::TrackFinished);
    playback.send(resolve_failed(token(&effects)));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 0.0);
}

#[test]
fn only_track_finished_ends_a_track() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.send(Message::Position(500.0));

    assert_eq!(effects, vec![]);
    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.position(), 500.0);
}

#[test]
fn a_track_finishing_while_another_loads_is_ignored() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.send(Message::Start(album(2, 3, 0))));

    assert_eq!(playback.send(Message::TrackFinished), vec![]);
    assert_eq!(current(&playback), Some(201));
    assert_eq!(playback.status(), Status::Loading(loading));
}

#[test]
fn the_position_moves_only_while_playing() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(10.0));
    playback.send(Message::TogglePlay);

    playback.send(Message::Position(11.0));

    assert_eq!(playback.position(), 10.0);
}

#[test]
fn play_and_pause_toggle_the_engine() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(playback.send(Message::TogglePlay), vec![Effect::Pause]);
    assert_eq!(playback.status(), Status::Paused);
    assert_eq!(playback.send(Message::TogglePlay), vec![Effect::Resume]);
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn play_while_stopped_starts_the_current_track_where_it_stopped() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(42.0));
    playback.send(Message::EngineFailed("no sink".to_string()));
    assert_eq!(playback.status(), Status::Stopped);

    let effects = playback.send(Message::TogglePlay);

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
    playback.send(Message::TrackFinished);

    let effects = playback.send(Message::TogglePlay);

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
    assert_eq!(playback.send(Message::TogglePlay), vec![]);

    playback.send(Message::Start(album(1, 3, 0)));
    assert_eq!(playback.send(Message::TogglePlay), vec![]);
}

#[test]
fn a_seek_goes_to_the_engine_and_moves_the_position() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(playback.send(Message::Seek(30.0)), vec![Effect::Seek(30.0)]);
    assert_eq!(playback.position(), 30.0);
}

#[test]
fn a_seek_while_stopped_is_where_play_starts() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::EngineFailed("no sink".to_string()));

    assert_eq!(playback.send(Message::Seek(30.0)), vec![]);
    let effects = playback.send(Message::TogglePlay);

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
        playback.send(Message::SetVolume(0.3)),
        vec![Effect::SetVolume(0.3)]
    );
    assert_eq!(playback.volume(), 0.3);
}

#[test]
fn unmuting_restores_the_volume_from_before() {
    let mut playback = new(0.8);

    assert_eq!(
        playback.send(Message::ToggleMute),
        vec![Effect::SetVolume(0.0)]
    );
    assert_eq!(playback.volume(), 0.0);
    assert_eq!(
        playback.send(Message::ToggleMute),
        vec![Effect::SetVolume(0.8)]
    );
}

#[test]
fn unmuting_with_no_volume_from_before_goes_to_half() {
    let mut playback = new(0.0);

    assert_eq!(
        playback.send(Message::ToggleMute),
        vec![Effect::SetVolume(0.5)]
    );
}

#[test]
fn a_track_that_ends_while_the_next_choice_loads_isnt_brought_back_playing() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.send(Message::Start(album(2, 3, 0))));

    playback.send(Message::TrackFinished);
    playback.send(resolve_failed(loading));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(playback.status(), Status::Stopped);
}

#[test]
fn a_track_that_breaks_while_the_next_choice_loads_isnt_brought_back_playing() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(42.0));
    let loading = token(&playback.send(Message::Start(album(2, 3, 0))));

    playback.send(Message::EngineFailed("no sink".to_string()));
    playback.send(resolve_failed(loading));

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
    playback.send(Message::CycleRepeat);

    let mut playback = playing_with(
        playback,
        request(SourceRef::Track(5), 5, 1, Start::Track(0)),
    );
    assert_eq!(playing_from(&playback), Some(SourceRef::Track(5)));

    let effects = playback.send(Message::TrackFinished);
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

    assert_eq!(playback.send(Message::Start(album(2, 3, 3))), vec![]);
    assert_eq!(current(&playback), Some(101));
}

// Shuffle.

#[test]
fn turning_shuffle_on_shuffles_only_what_is_left() {
    let mut playback = playing(playlist(1, 10, Start::All));
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);

    assert_eq!(unarmed(playback.send(Message::ToggleShuffle)), vec![]);

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

    playback.send(Message::ToggleShuffle);

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

    playback.send(Message::ToggleShuffle);

    assert_eq!(ids(playback.upcoming()), vec![108, 109, 110]);
}

#[test]
fn shuffle_with_nothing_playing_only_changes_the_mode() {
    let mut playback = new(1.0);

    assert_eq!(playback.send(Message::ToggleShuffle), vec![]);
    assert!(playback.shuffle());
    assert_eq!(current(&playback), None);
}

// Next.

#[test]
fn next_plays_what_comes_next_and_puts_the_current_track_in_history() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.send(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 102, .. }]));
    assert_eq!(current(&playback), Some(102));
    assert_eq!(ids(playback.upcoming()), vec![103]);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn next_on_the_last_track_stops() {
    let mut playback = playing(album(1, 2, 1));

    assert_eq!(playback.send(Message::Next), vec![Effect::Stop]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(playback.position(), 0.0);
    assert_eq!(current(&playback), Some(102), "the last track stays shown");
    assert_eq!(history(&playback), Vec::<u64>::new());
}

#[test]
fn next_with_nothing_playing_does_nothing() {
    assert_eq!(new(1.0).send(Message::Next), vec![]);
}

#[test]
fn rapid_nexts_each_move_on() {
    let mut playback = playing(album(1, 4, 0));

    playback.send(Message::Next);
    let last = token(&playback.send(Message::Next));

    assert_eq!(current(&playback), Some(103));
    assert_eq!(history(&playback), vec![101, 102]);
    assert_eq!(playback.status(), Status::Loading(last));
}

#[test]
fn a_next_that_fails_rolls_history_back_too() {
    let mut playback = playing(album(1, 3, 0));

    let effects = playback.send(Message::Next);
    playback.send(resolve_failed(token(&effects)));

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

    assert_eq!(playback.send(Message::CycleRepeat), vec![]);
    assert_eq!(playback.repeat(), Repeat::All);
    playback.send(Message::CycleRepeat);
    assert_eq!(playback.repeat(), Repeat::One);
    playback.send(Message::CycleRepeat);
    assert_eq!(playback.repeat(), Repeat::Off);
}

fn repeating(repeat: Repeat) -> Playback {
    let mut playback = new(1.0);
    while playback.repeat() != repeat {
        playback.send(Message::CycleRepeat);
    }
    playback
}

#[test]
fn repeat_one_replays_the_track_when_it_ends() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 3, 0));

    let effects = playback.send(Message::TrackFinished);

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

    let effects = playback.send(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 102, .. }]));
}

#[test]
fn next_on_the_last_track_stops_under_repeat_one() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 2, 1));

    assert_eq!(playback.send(Message::Next), vec![Effect::Stop]);
}

#[test]
fn repeat_all_starts_the_source_over_and_keeps_history() {
    let mut playback = playing_with(repeating(Repeat::All), album(1, 3, 1));
    and_play(&mut playback, Message::TrackFinished);

    let effects = playback.send(Message::TrackFinished);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(play_order(&playback), vec![101, 102, 103]);
    assert_eq!(history(&playback), vec![102, 103]);
}

#[test]
fn next_on_the_last_track_starts_over_under_repeat_all() {
    let mut playback = playing_with(repeating(Repeat::All), album(1, 2, 1));

    let effects = playback.send(Message::Next);

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
    playback.send(Message::Position(3.5));

    assert_eq!(playback.send(Message::Previous), vec![Effect::Seek(0.0)]);
    assert_eq!(current(&playback), Some(102));
    assert_eq!(playback.position(), 0.0);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn previous_within_three_seconds_goes_back_a_step() {
    let mut playback = playing(album(1, 3, 0));
    and_play(&mut playback, Message::Next);
    playback.send(Message::Position(2.0));

    let effects = playback.send(Message::Previous);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(current(&playback), Some(101));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    assert_eq!(history(&playback), Vec::<u64>::new());
}

#[test]
fn previous_with_no_history_restarts_the_track() {
    let mut playback = playing(album(1, 3, 1));
    playback.send(Message::Position(1.0));

    assert_eq!(playback.send(Message::Previous), vec![Effect::Seek(0.0)]);
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

    let effects = playback.send(Message::Previous);

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
        playback.send(Message::Next);
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

    assert!(album_gain(&playback.send(Message::Start(album(1, 3, 1)))));
}

#[test]
fn an_album_played_with_shuffle_on_gets_track_gain() {
    let mut playback = shuffled(new(1.0));
    let start = request(SourceRef::Album(1), 1, 3, Start::All);

    assert!(!album_gain(&playback.send(Message::Start(start))));
}

#[test]
fn a_shuffle_play_of_an_album_gets_track_gain() {
    let mut playback = new(1.0);
    let start = request(SourceRef::Album(1), 1, 3, Start::Shuffled);

    assert!(!album_gain(&playback.send(Message::Start(start))));
}

#[test]
fn turning_shuffle_on_mid_album_switches_to_track_gain() {
    let mut playback = shuffled(playing(album(1, 4, 0)));

    assert!(!album_gain(&playback.send(Message::Next)));
}

#[test]
fn other_sources_get_track_gain() {
    let mut playback = new(1.0);

    assert!(!album_gain(&playback.send(Message::Start(playlist(
        1,
        3,
        Start::All
    )))));
}

// Background fills.

#[test]
fn a_long_source_plays_at_once_and_reads_the_rest_from_where_its_first_page_ends() {
    let mut playback = new(1.0);

    let effects = playback.send(Message::Start(long_playlist(1, 3, Start::All)));

    assert!(matches!(
        the_play(&effects),
        Effect::Play { track_id: 101, .. }
    ));
    assert_eq!(
        fill(&effects).1,
        Continuation {
            source: playlist_ref(1),
            offset: 3,
        }
    );
}

#[test]
fn a_sorted_playlist_reads_the_rest_in_its_sort() {
    let sort = TrackSort {
        order: syzygy_catalog::TrackOrder::Title,
        direction: syzygy_catalog::Direction::Descending,
    };
    let kind = SourceRef::Playlist {
        uuid: "playlist-1".to_string(),
        sort: Some(sort),
    };
    let request = PlayRequest {
        source: source(kind.clone()),
        continuation: Some(Continuation {
            source: kind.clone(),
            offset: 3,
        }),
        ..playlist(1, 3, Start::All)
    };

    let effects = new(1.0).send(Message::Start(request));

    assert_eq!(fill(&effects).1.source, kind);
}

#[test]
fn a_source_with_nothing_more_to_read_starts_no_fill() {
    let effects = new(1.0).send(Message::Start(playlist(1, 3, Start::All)));

    assert_eq!(effects.len(), 1, "only the Play: {effects:?}");
}

#[test]
fn pages_that_arrive_go_on_the_end_of_the_play_order() {
    let mut playback = new(1.0);
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 3, Start::Track(1))),
    ));

    assert_eq!(
        playback.send(Message::PageArrived(fill_id, page(1, 4, 5))),
        vec![]
    );
    playback.send(Message::PageArrived(fill_id, page(1, 6, 6)));

    assert_eq!(play_order(&playback), vec![102, 103, 104, 105, 106]);
}

#[test]
fn a_page_for_a_fill_from_before_is_dropped() {
    let mut playback = new(1.0);
    let (old, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 2, Start::All)),
    ));
    and_play(
        &mut playback,
        Message::Start(long_playlist(2, 2, Start::All)),
    );

    playback.send(Message::PageArrived(old, page(1, 3, 4)));

    assert_eq!(play_order(&playback), vec![201, 202]);
}

#[test]
fn with_shuffle_on_pages_go_into_random_places_in_the_unplayed_tail() {
    let mut playback = shuffled(new(1.0));
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 10, Start::Track(0))),
    ));
    and_play(&mut playback, Message::Next);
    let playing_now = current(&playback);

    playback.send(Message::PageArrived(fill_id, page(1, 11, 20)));

    assert_eq!(history(&playback), vec![101]);
    assert_eq!(current(&playback), playing_now);
    assert_eq!(
        sorted(play_order(&playback)),
        (102..=120).collect::<Vec<_>>()
    );
    let upcoming = ids(playback.upcoming());
    assert_eq!(upcoming.len(), 18);
    assert_ne!(
        sorted(upcoming[8..].to_vec()),
        (111..=120).collect::<Vec<_>>(),
        "the new tracks aren't all at the end"
    );
}

#[test]
fn after_a_shuffle_play_pages_are_shuffled_in_too() {
    let mut playback = new(1.0);
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 10, Start::Shuffled)),
    ));

    playback.send(Message::PageArrived(fill_id, page(1, 11, 20)));

    let upcoming = ids(playback.upcoming());
    assert_ne!(
        sorted(upcoming[9..].to_vec()),
        (111..=120).collect::<Vec<_>>()
    );
}

#[test]
fn turning_shuffle_off_puts_tracks_that_arrived_back_in_source_order() {
    let mut playback = shuffled(new(1.0));
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 5, Start::Track(0))),
    ));
    playback.send(Message::PageArrived(fill_id, page(1, 6, 10)));

    playback.send(Message::ToggleShuffle);

    assert_eq!(ids(playback.upcoming()), (102..=110).collect::<Vec<_>>());
}

#[test]
fn a_new_source_cancels_the_fill() {
    let mut playback = playing(long_playlist(1, 3, Start::All));

    let effects = playback.send(Message::Start(album(2, 3, 0)));

    assert!(effects.contains(&Effect::CancelFill), "{effects:?}");
}

#[test]
fn a_new_long_source_cancels_the_old_fill_and_starts_its_own() {
    let mut playback = new(1.0);
    let (old, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 3, Start::All)),
    ));

    let effects = playback.send(Message::Start(long_playlist(2, 3, Start::All)));

    assert!(effects.contains(&Effect::CancelFill), "{effects:?}");
    let (new, continuation) = fill(&effects);
    assert_ne!(new, old);
    assert_eq!(continuation.source, playlist_ref(2));
}

#[test]
fn a_finished_fill_has_nothing_to_cancel() {
    let mut playback = new(1.0);
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 3, Start::All)),
    ));
    playback.send(Message::PageArrived(fill_id, page(1, 4, 5)));
    assert_eq!(playback.send(Message::FillEnded(fill_id)), vec![]);

    let effects = playback.send(Message::Start(album(2, 3, 0)));

    assert!(!effects.contains(&Effect::CancelFill), "{effects:?}");
}

#[test]
fn repeat_all_starts_over_with_what_has_loaded() {
    let mut playback = repeating(Repeat::All);
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 2, Start::All)),
    ));
    playback.send(Message::PageArrived(fill_id, page(1, 3, 3)));
    and_play(&mut playback, Message::TrackFinished);
    and_play(&mut playback, Message::TrackFinished);

    let effects = playback.send(Message::TrackFinished);

    assert!(matches!(
        the_play(&effects),
        Effect::Play { track_id: 101, .. }
    ));
    assert_eq!(play_order(&playback), vec![101, 102, 103]);
}

#[test]
fn pages_that_arrive_while_the_next_track_loads_survive_its_failure() {
    let mut playback = new(1.0);
    let (fill_id, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 2, Start::All)),
    ));
    let loading = token(&playback.send(Message::Next));

    playback.send(Message::PageArrived(fill_id, page(1, 3, 4)));
    playback.send(resolve_failed(loading));

    assert_eq!(play_order(&playback), vec![101, 102, 103, 104]);
}

#[test]
fn a_new_source_that_fails_to_play_brings_back_the_old_fill_from_where_it_got_to() {
    let mut playback = new(1.0);
    let (old, _) = fill(&and_play(
        &mut playback,
        Message::Start(long_playlist(1, 2, Start::All)),
    ));
    playback.send(Message::PageArrived(old, page(1, 3, 4)));
    let loading = token(&playback.send(Message::Start(long_playlist(2, 2, Start::All))));

    let effects = playback.send(resolve_failed(loading));

    let (refill, continuation) = fill(&effects);
    assert_ne!(refill, old);
    assert_eq!(
        continuation,
        Continuation {
            source: playlist_ref(1),
            offset: 4,
        }
    );
    playback.send(Message::PageArrived(refill, page(1, 5, 5)));
    assert_eq!(play_order(&playback), vec![101, 102, 103, 104, 105]);
}

#[test]
fn a_new_source_that_fails_to_play_cancels_its_own_fill() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.send(Message::Start(long_playlist(2, 2, Start::All))));

    let effects = playback.send(resolve_failed(loading));

    assert_eq!(unarmed(effects), vec![Effect::CancelFill]);
}

#[test]
fn shuffled_pages_never_go_ahead_of_what_plays_next() {
    for seed in 0..20 {
        let mut playback = shuffled(Playback::new(
            Preferences {
                volume: 1.0,
                shuffle: false,
                repeat: Repeat::Off,
                autoplay: false,
                gapless: true,
                allow_explicit: true,
            },
            SmallRng::seed_from_u64(seed),
        ));
        let (fill_id, _) = fill(&and_play(
            &mut playback,
            Message::Start(long_playlist(1, 3, Start::Track(0))),
        ));
        and_play(&mut playback, Message::Next);
        let stepped_back = current(&playback);
        and_play(&mut playback, Message::Previous);

        playback.send(Message::PageArrived(fill_id, page(1, 4, 10)));

        assert_eq!(
            playback.upcoming().next().map(|track| track.id),
            stepped_back,
            "seed {seed}"
        );
    }
}

// The Manual queue.

/// The Source tag of a track queued from album `id`.
fn tag(id: u64) -> Source {
    source(SourceRef::Album(id))
}

fn add(playback: &mut Playback, id: u64) -> Vec<Effect> {
    playback.send(Message::AddToQueue(track(id), tag(id / 100)))
}

fn play_next(playback: &mut Playback, id: u64) -> Vec<Effect> {
    playback.send(Message::PlayNext(track(id), tag(id / 100)))
}

/// The Manual queue, in order.
fn queued(playback: &Playback) -> Vec<u64> {
    playback.queued().map(|(_, track)| track.id).collect()
}

#[test]
fn queued_tracks_play_before_the_rest_of_the_source() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(unarmed(add(&mut playback, 901)), vec![]);
    add(&mut playback, 902);

    assert_eq!(queued(&playback), vec![901, 902]);
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(901));
    and_play(&mut playback, Message::TrackFinished);
    assert_eq!(current(&playback), Some(902));
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(102));
    assert_eq!(history(&playback), vec![101, 901, 902]);
}

#[test]
fn play_next_goes_ahead_of_what_was_queued_before() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);

    play_next(&mut playback, 902);

    assert_eq!(queued(&playback), vec![902, 901]);
}

#[test]
fn the_same_track_queued_twice_is_two_entries() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);
    add(&mut playback, 901);

    let entries: Vec<EntryId> = playback.queued().map(|(id, _)| id).collect();
    assert_eq!(entries.len(), 2);
    assert_ne!(entries[0], entries[1]);

    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(901));
    assert_eq!(history(&playback), vec![101, 901]);
    assert_eq!(queued(&playback), Vec::<u64>::new());
}

#[test]
fn playing_from_shows_the_tag_while_a_queued_entry_plays_then_the_source_again() {
    let mut playback = playing(playlist(1, 3, Start::All));
    add(&mut playback, 901);

    and_play(&mut playback, Message::Next);
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(9)));

    and_play(&mut playback, Message::Next);
    assert_eq!(playing_from(&playback), Some(playlist_ref(1)));
}

#[test]
fn a_queued_entry_gets_track_gain_even_from_an_album() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);

    assert!(!album_gain(&playback.send(Message::Next)));
}

#[test]
fn next_on_the_last_track_plays_what_was_queued() {
    let mut playback = playing(album(1, 2, 1));
    add(&mut playback, 901);

    let effects = playback.send(Message::Next);

    assert!(matches!(effects[..], [Effect::Play { track_id: 901, .. }]));
}

#[test]
fn playback_stops_when_the_last_queued_entry_ends_with_nothing_left() {
    let mut playback = playing(album(1, 1, 0));
    add(&mut playback, 901);
    and_play(&mut playback, Message::TrackFinished);

    assert_eq!(and_play(&mut playback, Message::TrackFinished), vec![]);
    assert_eq!(current(&playback), Some(901));
}

#[test]
fn shuffle_never_reorders_the_manual_queue() {
    let mut playback = playing(playlist(1, 8, Start::All));
    for id in 901..=906 {
        add(&mut playback, id);
    }

    playback.send(Message::ToggleShuffle);

    assert_eq!(queued(&playback), (901..=906).collect::<Vec<_>>());
}

#[test]
fn queueing_with_nothing_playing_plays_it() {
    let mut playback = new(1.0);

    let effects = add(&mut playback, 901);

    assert!(matches!(effects[..], [Effect::Play { track_id: 901, .. }]));
    assert_eq!(current(&playback), Some(901));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(9)));
    assert_eq!(queued(&playback), Vec::<u64>::new());
}

#[test]
fn a_new_source_clears_the_manual_queue() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);

    and_play(&mut playback, Message::Start(album(2, 3, 0)));

    assert_eq!(queued(&playback), Vec::<u64>::new());
}

#[test]
fn a_queued_entry_that_fails_to_play_goes_back_in_the_queue() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);

    let effects = playback.send(Message::Next);
    playback.send(resolve_failed(token(&effects)));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(queued(&playback), vec![901]);
}

#[test]
fn previous_from_a_queued_entry_puts_it_back_at_the_front_of_the_queue() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);
    add(&mut playback, 902);
    and_play(&mut playback, Message::Next);

    let effects = playback.send(Message::Previous);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
    assert_eq!(queued(&playback), vec![901, 902]);
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
}

#[test]
fn previous_into_a_queued_entry_plays_it_under_its_tag() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Next);

    and_play(&mut playback, Message::Previous);

    assert_eq!(current(&playback), Some(901));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(9)));
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(102));
}

#[test]
fn previous_twice_across_sources_keeps_the_track_it_passes() {
    let mut playback = playing(album(1, 2, 0));
    and_play(&mut playback, Message::Next);
    and_play(&mut playback, Message::Start(album(2, 2, 0)));

    and_play(&mut playback, Message::Previous);
    and_play(&mut playback, Message::Previous);

    assert_eq!(current(&playback), Some(101));
    assert_eq!(queued(&playback), vec![102]);
    assert_eq!(ids(playback.upcoming()), vec![201, 202]);
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(102));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
    and_play(&mut playback, Message::Next);
    assert_eq!(current(&playback), Some(201));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(2)));
}

#[test]
fn a_track_queued_while_a_failing_play_loads_stays_queued() {
    let mut playback = playing(album(1, 3, 0));
    let loading = token(&playback.send(Message::Next));

    add(&mut playback, 901);
    playback.send(resolve_failed(loading));

    assert_eq!(current(&playback), Some(101));
    assert_eq!(queued(&playback), vec![901]);
}

#[test]
fn repeat_one_replays_the_track_and_leaves_the_queue_waiting() {
    let mut playback = playing_with(repeating(Repeat::One), album(1, 3, 0));
    add(&mut playback, 901);

    and_play(&mut playback, Message::TrackFinished);

    assert_eq!(current(&playback), Some(101));
    assert_eq!(queued(&playback), vec![901]);
}

#[test]
fn under_repeat_all_the_queue_plays_before_the_source_starts_over() {
    let mut playback = playing_with(repeating(Repeat::All), album(1, 3, 0));
    while playback.upcoming().next().is_some() {
        and_play(&mut playback, Message::Next);
    }
    add(&mut playback, 901);

    and_play(&mut playback, Message::TrackFinished);
    assert_eq!(current(&playback), Some(901));
    and_play(&mut playback, Message::TrackFinished);
    assert_eq!(current(&playback), Some(101));
}

// Failures and explicit content.

/// A track TIDAL says it won't stream.
fn unavailable(id: u64) -> Track {
    Track {
        available: false,
        ..track(id)
    }
}

fn explicit(id: u64) -> Track {
    Track {
        explicit: true,
        ..track(id)
    }
}

/// Album `id` of `tracks`, started as `start` says.
fn album_of(id: u64, tracks: Vec<Track>, start: Start) -> PlayRequest {
    PlayRequest {
        source: source(SourceRef::Album(id)),
        first_page: tracks,
        start,
        continuation: None,
    }
}

/// Playback with explicit tracks not allowed.
fn no_explicit() -> Playback {
    let mut playback = new(1.0);
    playback.send(Message::AllowExplicit(false));
    playback
}

/// TIDAL will never have a stream for it.
fn unplayable(token: PlayToken) -> Message {
    let error = syzygy_tidal::Error::Api {
        status: 404,
        body: String::new(),
    };
    Message::Played(token, Err(PlayError::Resolve(Arc::new(error))))
}

/// TIDAL is rate-limiting, and said for how long when `secs` is some.
fn rate_limited(token: PlayToken, secs: Option<u64>) -> Message {
    let body = match secs {
        Some(secs) => format!(r#"{{"status":429,"retryAfterSecs":{secs}}}"#),
        None => String::new(),
    };
    let error = syzygy_tidal::Error::Api { status: 429, body };
    Message::Played(token, Err(PlayError::Resolve(Arc::new(error))))
}

/// The notices among `effects`.
fn notices(effects: &[Effect]) -> Vec<Notice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notify(notice) => Some(notice.clone()),
            _ => None,
        })
        .collect()
}

/// The id of the track the one Play among `effects` asks for.
fn plays(effects: &[Effect]) -> u64 {
    match the_play(effects) {
        Effect::Play { track_id, .. } => *track_id,
        _ => unreachable!(),
    }
}

#[test]
fn moving_on_skips_an_unavailable_track_without_putting_it_in_history() {
    let tracks = vec![track(101), unavailable(102), track(103)];
    let mut playback = playing(album_of(1, tracks, Start::All));

    let effects = and_play(&mut playback, Message::TrackFinished);

    assert_eq!(plays(&effects), 103);
    assert_eq!(notices(&effects), vec![]);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn a_track_tidal_has_no_stream_for_is_skipped() {
    let mut playback = playing(album(1, 3, 0));
    let effects = playback.send(Message::TrackFinished);

    let effects = playback.send(unplayable(token(&effects)));

    assert_eq!(plays(&effects), 103);
    assert_eq!(current(&playback), Some(103));
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn three_tracks_in_a_row_that_cant_play_stop_playback_with_a_notice() {
    let tracks = vec![
        track(101),
        unavailable(102),
        track(103),
        track(104),
        track(105),
    ];
    let mut playback = playing(album_of(1, tracks, Start::All));
    let effects = playback.send(Message::TrackFinished);
    let effects = playback.send(unplayable(token(&effects)));

    let effects = playback.send(unplayable(token(&effects)));

    assert!(!effects.iter().any(|e| matches!(e, Effect::Play { .. })));
    assert_eq!(notices(&effects), vec![Notice::TooManyFailures]);
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(104));
    assert_eq!(ids(playback.upcoming()), vec![105]);
}

#[test]
fn playback_stops_on_the_third_unavailable_track_in_a_row_while_moving_on() {
    let tracks = vec![
        track(101),
        unavailable(102),
        unavailable(103),
        unavailable(104),
    ];
    let mut playback = playing(album_of(1, tracks, Start::All));

    let effects = playback.send(Message::Next);

    assert_eq!(
        effects,
        vec![Effect::Stop, Effect::Notify(Notice::TooManyFailures)]
    );
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(104));
}

#[test]
fn a_track_that_plays_starts_the_count_again() {
    let tracks = vec![
        track(101),
        unavailable(102),
        unavailable(103),
        track(104),
        unavailable(105),
        unavailable(106),
        track(107),
    ];
    let mut playback = playing(album_of(1, tracks, Start::All));

    and_play(&mut playback, Message::TrackFinished);
    assert_eq!(current(&playback), Some(104));
    let effects = and_play(&mut playback, Message::TrackFinished);

    assert_eq!(plays(&effects), 107);
}

#[test]
fn next_starts_the_count_again() {
    let tracks = vec![
        track(101),
        unavailable(102),
        unavailable(103),
        track(104),
        unavailable(105),
        track(106),
    ];
    let mut playback = playing(album_of(1, tracks, Start::All));
    playback.send(Message::TrackFinished);
    assert_eq!(current(&playback), Some(104));

    // Still loading 104, two failures counted.
    let effects = playback.send(Message::Next);

    assert_eq!(plays(&effects), 106);
}

#[test]
fn nothing_after_it_can_play_is_like_running_out() {
    let tracks = vec![track(101), track(102), unavailable(103)];
    let mut playback = playing(album_of(1, tracks, Start::Track(1)));

    assert_eq!(playback.send(Message::TrackFinished), vec![]);

    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(102));
    assert!(history(&playback).is_empty());
}

#[test]
fn a_new_source_none_of_which_can_play_rolls_back_to_what_plays() {
    let mut playback = playing(album(1, 3, 0));
    let effects = playback.send(Message::Start(album(2, 2, 0)));
    let effects = playback.send(unplayable(token(&effects)));

    let effects = playback.send(unplayable(token(&effects)));

    assert!(!effects.iter().any(|e| matches!(e, Effect::Play { .. })));
    assert_eq!(playback.status(), Status::Playing);
    assert_eq!(current(&playback), Some(101));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
}

#[test]
fn choosing_an_unavailable_track_says_so_and_plays_nothing() {
    let mut playback = playing(album(1, 3, 0));
    let tracks = vec![track(201), unavailable(202)];

    let effects = playback.send(Message::Start(album_of(2, tracks, Start::Track(1))));

    assert_eq!(effects, vec![Effect::Notify(Notice::Unavailable)]);
    assert_eq!(current(&playback), Some(101));
}

#[test]
fn playing_a_source_skips_its_unavailable_first_track() {
    let tracks = vec![unavailable(101), track(102)];

    let effects = new(1.0).send(Message::Start(album_of(1, tracks, Start::All)));

    assert_eq!(plays(&effects), 102);
}

#[test]
fn an_unavailable_queued_entry_is_skipped() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::AddToQueue(unavailable(901), tag(9)));

    let effects = and_play(&mut playback, Message::TrackFinished);

    assert_eq!(plays(&effects), 102);
    assert!(queued(&playback).is_empty());
}

#[test]
fn a_rate_limit_stops_on_the_track_and_waits_to_play_it() {
    let mut playback = playing(album(1, 3, 0));
    let next = token(&playback.send(Message::Next));

    let effects = playback.send(rate_limited(next, Some(7)));

    assert_eq!(
        effects,
        vec![
            Effect::Stop,
            Effect::ResumeAfter {
                token: next,
                delay: Duration::from_secs(8),
            },
            Effect::Notify(Notice::RateLimited(8)),
        ]
    );
    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(102));

    let effects = playback.send(Message::Resume(next));
    assert_eq!(plays(&effects), 102);
}

#[test]
fn a_rate_limit_that_doesnt_say_how_long_waits_six_seconds() {
    let mut playback = playing(album(1, 3, 0));
    let next = token(&playback.send(Message::TrackFinished));

    let effects = playback.send(rate_limited(next, None));

    // The finished track left the engine with nothing to stop.
    assert_eq!(
        effects,
        vec![
            Effect::ResumeAfter {
                token: next,
                delay: Duration::from_secs(6),
            },
            Effect::Notify(Notice::RateLimited(6)),
        ]
    );
}

#[test]
fn a_rate_limit_doesnt_resume_once_the_user_played_something() {
    let mut playback = playing(album(1, 3, 0));
    let next = token(&playback.send(Message::Next));
    playback.send(rate_limited(next, Some(7)));

    and_play(&mut playback, Message::Next);

    assert_eq!(playback.send(Message::Resume(next)), vec![]);
    assert_eq!(current(&playback), Some(103));
}

#[test]
fn a_rate_limit_resume_after_the_user_pressed_play_is_dropped() {
    let mut playback = playing(album(1, 3, 0));
    let next = token(&playback.send(Message::Next));
    playback.send(rate_limited(next, Some(7)));

    let again = token(&playback.send(Message::TogglePlay));
    playback.send(resolve_failed(again));

    assert_eq!(playback.send(Message::Resume(next)), vec![]);
}

#[test]
fn a_busy_device_is_reported_while_the_play_keeps_trying() {
    let mut playback = new(1.0);
    let first = token(&playback.send(Message::Start(album(1, 3, 0))));

    assert_eq!(
        playback.send(Message::DeviceBusy(first)),
        vec![Effect::Notify(Notice::DeviceBusy)]
    );
    let second = token(&playback.send(Message::Next));
    assert_eq!(playback.send(Message::DeviceBusy(first)), vec![]);
    assert_eq!(
        playback.send(Message::DeviceBusy(second)),
        vec![Effect::Notify(Notice::DeviceBusy)]
    );
}

#[test]
fn audio_errors_are_reported() {
    let mut playback = playing(album(1, 3, 0));
    let next = token(&playback.send(Message::Next));

    let effects = playback.send(audio_failed(next));
    assert_eq!(
        notices(&effects),
        vec![Notice::AudioError("no sink".to_string())]
    );

    let effects = playback.send(Message::EngineFailed("device gone".to_string()));
    assert_eq!(
        notices(&effects),
        vec![Notice::AudioError("device gone".to_string())]
    );
}

#[test]
fn starting_a_source_with_an_explicit_track_asks_first() {
    let mut playback = no_explicit();
    let tracks = vec![track(101), explicit(102)];

    let outcome = playback.update(Message::Start(album_of(1, tracks, Start::All)));

    assert!(matches!(outcome, Outcome::NeedsExplicitConsent(_)));
    assert_eq!(current(&playback), None);
}

#[test]
fn allowing_explicit_content_then_sending_what_waited_plays_it() {
    let mut playback = no_explicit();
    let tracks = vec![explicit(101), track(102)];
    let Outcome::NeedsExplicitConsent(pending) =
        playback.update(Message::Start(album_of(1, tracks, Start::All)))
    else {
        panic!("expected a consent question");
    };

    playback.send(Message::AllowExplicit(true));
    let effects = playback.send(pending.into());

    assert_eq!(plays(&effects), 101);
}

#[test]
fn playing_without_them_skips_the_explicit_tracks() {
    let mut playback = no_explicit();
    let tracks = vec![explicit(101), track(102), explicit(103), track(104)];
    let Outcome::NeedsExplicitConsent(pending) =
        playback.update(Message::Start(album_of(1, tracks, Start::All)))
    else {
        panic!("expected a consent question");
    };

    let effects = and_play(&mut playback, Message::WithoutExplicit(pending));
    assert_eq!(plays(&effects), 102);
    let effects = and_play(&mut playback, Message::TrackFinished);
    assert_eq!(plays(&effects), 104);
}

#[test]
fn explicit_tracks_before_the_chosen_one_dont_ask() {
    let mut playback = no_explicit();
    let tracks = vec![explicit(101), track(102), track(103)];

    let effects = playback.send(Message::Start(album_of(1, tracks, Start::Track(1))));

    assert_eq!(plays(&effects), 102);
}

#[test]
fn queueing_an_explicit_track_asks_first_and_without_them_queues_nothing() {
    let mut playback = playing_with(no_explicit(), album(1, 3, 0));

    let Outcome::NeedsExplicitConsent(pending) =
        playback.update(Message::AddToQueue(explicit(901), tag(9)))
    else {
        panic!("expected a consent question");
    };
    assert_eq!(playback.send(Message::WithoutExplicit(pending)), vec![]);

    assert!(queued(&playback).is_empty());
    assert!(matches!(
        playback.update(Message::PlayNext(explicit(901), tag(9))),
        Outcome::NeedsExplicitConsent(_)
    ));
}

#[test]
fn explicit_tracks_reached_by_moving_on_are_skipped_silently_and_not_counted() {
    let tracks = vec![
        track(101),
        unavailable(102),
        unavailable(103),
        explicit(104),
        explicit(105),
        track(106),
    ];
    let mut playback = playing(album_of(1, tracks, Start::All));
    playback.send(Message::AllowExplicit(false));

    let effects = and_play(&mut playback, Message::TrackFinished);

    assert_eq!(plays(&effects), 106);
    assert_eq!(notices(&effects), vec![]);
}

#[test]
fn turning_explicit_content_off_removes_nothing() {
    let tracks = vec![explicit(101), explicit(102), track(103)];
    let mut playback = playing(album_of(1, tracks, Start::All));

    playback.send(Message::AllowExplicit(false));

    assert_eq!(play_order(&playback), vec![101, 102, 103]);
    assert_eq!(playback.status(), Status::Playing);
}

#[test]
fn a_run_of_skips_that_runs_out_doesnt_count_toward_the_next_one() {
    let tracks = vec![track(101), track(102), unavailable(103), unavailable(104)];
    let mut playback = playing(album_of(1, tracks, Start::Track(1)));
    playback.send(Message::TrackFinished);
    add(&mut playback, 901);

    let effects = playback.send(Message::TogglePlay);
    let effects = playback.send(unplayable(token(&effects)));

    assert!(notices(&effects).is_empty());
    assert_eq!(plays(&effects), 901);
}

#[test]
fn a_next_that_stopped_playback_cancels_a_rate_limit_resume() {
    let mut playback = playing(album(1, 2, 0));
    let next = token(&playback.send(Message::Next));
    playback.send(rate_limited(next, Some(7)));

    playback.send(Message::Next);

    assert_eq!(playback.send(Message::Resume(next)), vec![]);
}

#[test]
fn repeat_one_moves_on_from_an_explicit_track_once_they_arent_allowed() {
    let tracks = vec![explicit(101), track(102)];
    let mut playback = playing_with(repeating(Repeat::One), album_of(1, tracks, Start::All));
    playback.send(Message::AllowExplicit(false));

    let effects = playback.send(Message::TrackFinished);

    assert_eq!(plays(&effects), 102);
}

// Autoplay.

/// Send `message` and let the play it starts succeed: what the success
/// asked for.
fn once_playing(playback: &mut Playback, message: Message) -> Vec<Effect> {
    let effects = playback.send(message);
    playback.send(played(token(&effects)))
}

/// Playback with Autoplay on.
fn autoplaying() -> Playback {
    let mut playback = new(1.0);
    playback.send(Message::Autoplay(true));
    playback
}

/// Track radio `id` of `tracks`.
fn radio_of(id: u64, tracks: Vec<Track>) -> Radio {
    Radio {
        source: Source {
            kind: radio_ref(id),
            name: format!("Radio {id}"),
        },
        tracks,
    }
}

fn radio_ref(id: u64) -> SourceRef {
    SourceRef::TrackRadio(format!("radio-{id}"))
}

/// Track radio `id` of `n` tracks numbered from `id * 100 + 1`.
fn radio(id: u64, n: u64) -> Radio {
    radio_of(id, (1..=n).map(|i| track(id * 100 + i)).collect())
}

/// The Track radio fetch among `effects`, and the track it's for.
fn radio_fetch(effects: &[Effect]) -> Option<(RadioId, u64)> {
    effects.iter().find_map(|effect| match effect {
        Effect::FetchTrackRadio { radio, track } => Some((*radio, track.id)),
        _ => None,
    })
}

/// Autoplay on, playing the last of album 1's 2 tracks, with its radio
/// being fetched.
fn on_the_last_track() -> (Playback, RadioId) {
    let mut playback = autoplaying();
    let effects = once_playing(&mut playback, Message::Start(album(1, 2, 1)));
    let (id, _) = radio_fetch(&effects).expect("a radio fetch");
    (playback, id)
}

#[test]
fn autoplay_fetches_the_last_tracks_radio_once_it_plays() {
    let mut playback = playing_with(autoplaying(), album(1, 2, 0));

    let effects = playback.send(Message::TrackFinished);
    assert_eq!(radio_fetch(&effects), None, "not while it loads");

    let effects = playback.send(played(token(&effects)));
    assert_eq!(radio_fetch(&effects).map(|(_, id)| id), Some(102));
}

#[test]
fn autoplay_fetches_no_radio_while_there_is_more_to_play() {
    let mut playback = autoplaying();

    let effects = once_playing(&mut playback, Message::Start(album(1, 2, 0)));

    assert_eq!(radio_fetch(&effects), None);
}

#[test]
fn autoplay_continues_with_the_radio_when_the_source_runs_out() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    let effects = playback.send(Message::TrackFinished);

    assert_eq!(plays(&effects), 501);
    assert_eq!(playing_from(&playback), Some(radio_ref(5)));
    assert_eq!(ids(playback.upcoming()), vec![502, 503]);
    assert_eq!(history(&playback), vec![102]);
}

#[test]
fn a_radio_that_arrives_after_the_track_ended_plays_then() {
    let (mut playback, id) = on_the_last_track();
    assert_eq!(playback.send(Message::TrackFinished), vec![]);
    assert_eq!(playback.status(), Status::Stopped);

    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    assert_eq!(plays(&effects), 501);
    assert_eq!(playing_from(&playback), Some(radio_ref(5)));
}

#[test]
fn next_on_the_last_track_goes_on_to_the_radio() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    let effects = playback.send(Message::Next);

    assert_eq!(plays(&effects), 501);
}

#[test]
fn next_on_the_last_track_stops_until_the_radio_arrives() {
    let (mut playback, id) = on_the_last_track();

    assert_eq!(playback.send(Message::Next), vec![Effect::Stop]);
    assert_eq!(playback.status(), Status::Stopped);

    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));
    assert_eq!(plays(&effects), 501);
}

#[test]
fn a_track_that_ends_before_its_radio_was_asked_for_asks_and_waits() {
    let mut playback = autoplaying();
    playback.send(Message::Start(album(1, 2, 1)));

    let effects = playback.send(Message::Next);
    let (id, track) = radio_fetch(&effects).expect("a radio fetch");
    assert_eq!(track, 102);
    assert_eq!(playback.status(), Status::Stopped);

    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));
    assert_eq!(plays(&effects), 501);
}

#[test]
fn without_a_radio_autoplay_stops_cleanly() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::RadioArrived(id, None));

    assert_eq!(playback.send(Message::TrackFinished), vec![]);

    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(102));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
}

#[test]
fn a_radio_fetch_that_fails_after_the_track_ended_leaves_playback_stopped() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::TrackFinished);

    assert_eq!(playback.send(Message::RadioArrived(id, None)), vec![]);

    assert_eq!(playback.status(), Status::Stopped);
    assert_eq!(current(&playback), Some(102));
}

#[test]
fn the_radio_leaves_out_what_already_played() {
    let mut playback = playing_with(autoplaying(), album(1, 2, 0));
    let effects = once_playing(&mut playback, Message::TrackFinished);
    let (id, _) = radio_fetch(&effects).expect("a radio fetch");
    let tracks = vec![track(102), track(101), track(501)];
    playback.send(Message::RadioArrived(id, Some(radio_of(5, tracks))));

    let effects = playback.send(Message::TrackFinished);

    assert_eq!(plays(&effects), 501);
    assert_eq!(ids(playback.upcoming()), Vec::<u64>::new());
}

#[test]
fn a_radio_of_nothing_new_stops_cleanly() {
    let (mut playback, id) = on_the_last_track();
    let tracks = vec![track(102)];
    playback.send(Message::RadioArrived(id, Some(radio_of(5, tracks))));

    assert_eq!(playback.send(Message::TrackFinished), vec![]);
    assert_eq!(playback.status(), Status::Stopped);
}

#[test]
fn autoplay_off_fetches_no_radio_and_stops() {
    let mut playback = new(1.0);

    let effects = once_playing(&mut playback, Message::Start(album(1, 2, 1)));
    assert_eq!(radio_fetch(&effects), None);

    assert_eq!(playback.send(Message::TrackFinished), vec![]);
    assert_eq!(playback.status(), Status::Stopped);
}

#[test]
fn autoplay_leaves_the_end_to_repeat_while_it_is_on() {
    for repeat in [Repeat::All, Repeat::One] {
        let mut playback = repeating(repeat);
        playback.send(Message::Autoplay(true));

        let effects = once_playing(&mut playback, Message::Start(album(1, 2, 1)));

        assert_eq!(radio_fetch(&effects), None, "under {repeat:?}");
    }
}

#[test]
fn a_radio_for_a_track_no_longer_playing_is_dropped() {
    let (mut playback, id) = on_the_last_track();
    and_play(&mut playback, Message::Start(album(2, 3, 0)));

    assert_eq!(
        playback.send(Message::RadioArrived(id, Some(radio(5, 3)))),
        vec![]
    );

    assert_eq!(playing_from(&playback), Some(SourceRef::Album(2)));
    assert_eq!(ids(playback.upcoming()), vec![202, 203]);
}

#[test]
fn choosing_something_while_the_radio_loads_keeps_it_from_playing() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::TrackFinished);
    let replay = playback.send(Message::TogglePlay);
    assert_eq!(plays(&replay), 102);

    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    assert!(plays_none(&effects));
    assert_eq!(current(&playback), Some(102));
}

#[test]
fn radio_tracks_are_not_chosen_by_the_user() {
    let (mut playback, id) = on_the_last_track();
    assert!(playback.chosen_by_user());
    playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    and_play(&mut playback, Message::TrackFinished);
    assert!(!playback.chosen_by_user());

    and_play(&mut playback, Message::TrackFinished);
    assert!(!playback.chosen_by_user());
}

#[test]
fn queued_entries_during_a_radio_are_chosen_by_the_user() {
    let (mut playback, id) = on_the_last_track();
    playback.send(Message::RadioArrived(id, Some(radio(5, 3))));
    and_play(&mut playback, Message::TrackFinished);
    add(&mut playback, 901);

    and_play(&mut playback, Message::TrackFinished);
    assert!(playback.chosen_by_user());

    and_play(&mut playback, Message::Start(album(2, 3, 0)));
    assert!(playback.chosen_by_user());
}

// Gapless advance.

/// The arm among `effects`: the track and whether it gets album gain.
fn armed(effects: &[Effect]) -> Option<(u64, bool)> {
    effects.iter().find_map(|effect| match effect {
        Effect::ArmNext {
            track_id,
            album_gain,
            ..
        } => Some((*track_id, *album_gain)),
        _ => None,
    })
}

/// The entry the arm among `effects` is for.
fn armed_entry(effects: &[Effect]) -> EntryId {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ArmNext { entry, .. } => Some(*entry),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected an ArmNext, got {effects:?}"))
}

#[test]
fn a_track_that_plays_arms_the_next_in_the_play_order() {
    let mut playback = new(1.0);

    let effects = playback.send(Message::Start(album(1, 3, 0)));
    assert_eq!(armed(&effects), None, "not while it loads");

    let effects = playback.send(played(token(&effects)));
    assert_eq!(armed(&effects), Some((102, true)));
}

#[test]
fn the_manual_queue_head_is_armed_ahead_of_the_source() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(armed(&add(&mut playback, 901)), Some((901, false)));
    assert_eq!(add(&mut playback, 902), vec![], "the head is the same");
}

#[test]
fn repeat_one_arms_the_track_again() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::CycleRepeat);

    let effects = playback.send(Message::CycleRepeat);

    assert_eq!(armed(&effects), Some((101, true)));
}

#[test]
fn repeat_all_arms_the_first_track_of_the_next_round() {
    let playback = shuffled(repeating(Repeat::All));
    let mut playback = playing_with(playback, playlist(1, 8, Start::All));
    for _ in 0..6 {
        and_play(&mut playback, Message::TrackFinished);
    }
    let effects = once_playing(&mut playback, Message::TrackFinished);
    let (first, _) = armed(&effects).expect("an arm");

    playback.send(Message::TrackAdvanced(armed_entry(&effects)));

    assert_eq!(current(&playback), Some(first));
    assert_eq!(
        sorted(play_order(&playback)),
        (101..=108).collect::<Vec<_>>()
    );
}

#[test]
fn autoplay_arms_the_radio_once_it_arrives() {
    let (mut playback, id) = on_the_last_track();

    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    assert_eq!(armed(&effects), Some((501, false)));
}

#[test]
fn a_track_that_will_be_skipped_is_never_armed() {
    let tracks = vec![track(101), unavailable(102), track(103)];
    let mut playback = new(1.0);

    let effects = once_playing(
        &mut playback,
        Message::Start(album_of(1, tracks, Start::All)),
    );

    assert_eq!(armed(&effects), None);
}

#[test]
fn disallowing_explicit_tracks_clears_an_armed_explicit_one() {
    let tracks = vec![track(101), explicit(102)];
    let mut playback = playing(album_of(1, tracks, Start::All));

    assert_eq!(
        playback.send(Message::AllowExplicit(false)),
        vec![Effect::ClearNext]
    );
}

#[test]
fn nothing_is_armed_with_nothing_after() {
    let mut playback = new(1.0);

    let effects = once_playing(&mut playback, Message::Start(album(1, 2, 1)));

    assert_eq!(armed(&effects), None);
}

#[test]
fn turning_shuffle_on_arms_the_new_next_track() {
    let mut playback = playing(playlist(1, 8, Start::All));

    let effects = playback.send(Message::ToggleShuffle);

    let next = ids(playback.upcoming())[0];
    assert_ne!(next, 102);
    assert_eq!(armed(&effects), Some((next, false)));
}

#[test]
fn a_gapless_advance_moves_on_without_playing_anything() {
    let mut playback = new(1.0);
    let effects = once_playing(&mut playback, Message::Start(album(1, 3, 0)));

    let effects = playback.send(Message::TrackAdvanced(armed_entry(&effects)));

    assert!(plays_none(&effects));
    assert_eq!(armed(&effects), Some((103, true)), "and arms the one after");
    assert_eq!(current(&playback), Some(102));
    assert_eq!(history(&playback), vec![101]);
    assert_eq!(playback.status(), Status::Playing);
    assert_eq!(playback.position(), 0.0);
}

/// No Play among `effects`.
fn plays_none(effects: &[Effect]) -> bool {
    !effects
        .iter()
        .any(|effect| matches!(effect, Effect::Play { .. }))
}

#[test]
fn a_gapless_advance_into_a_queued_entry_shows_its_tag_then_the_source_again() {
    let mut playback = playing(album(1, 3, 0));
    let effects = add(&mut playback, 901);

    let effects = playback.send(Message::TrackAdvanced(armed_entry(&effects)));
    assert_eq!(current(&playback), Some(901));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(9)));
    assert_eq!(queued(&playback), Vec::<u64>::new());

    playback.send(Message::TrackAdvanced(armed_entry(&effects)));
    assert_eq!(current(&playback), Some(102));
    assert_eq!(playing_from(&playback), Some(SourceRef::Album(1)));
}

#[test]
fn a_gapless_advance_into_the_radio_makes_it_the_source() {
    let (mut playback, id) = on_the_last_track();
    let effects = playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    let effects = playback.send(Message::TrackAdvanced(armed_entry(&effects)));

    assert!(plays_none(&effects));
    assert_eq!(current(&playback), Some(501));
    assert_eq!(playing_from(&playback), Some(radio_ref(5)));
    assert_eq!(history(&playback), vec![102]);
    assert!(!playback.chosen_by_user());
    assert_eq!(armed(&effects), Some((502, false)));
}

#[test]
fn a_gapless_replay_under_repeat_one_leaves_history_alone() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::CycleRepeat);
    let effects = playback.send(Message::CycleRepeat);

    let effects = playback.send(Message::TrackAdvanced(armed_entry(&effects)));

    assert!(plays_none(&effects));
    assert_eq!(current(&playback), Some(101));
    assert_eq!(history(&playback), Vec::<u64>::new());
    assert_eq!(ids(playback.upcoming()), vec![102, 103]);
}

#[test]
fn an_advance_to_an_entry_no_longer_next_plays_what_is() {
    let mut playback = new(1.0);
    let effects = once_playing(&mut playback, Message::Start(album(1, 3, 0)));
    let stale = armed_entry(&effects);
    play_next(&mut playback, 901);

    let effects = playback.send(Message::TrackAdvanced(stale));

    assert_eq!(plays(&effects), 901);
    assert_eq!(history(&playback), vec![101]);
}

#[test]
fn an_advance_with_nothing_left_to_play_stops_the_engine() {
    let tracks = vec![track(101), explicit(102)];
    let mut playback = new(1.0);
    let effects = once_playing(
        &mut playback,
        Message::Start(album_of(1, tracks, Start::All)),
    );
    let stale = armed_entry(&effects);
    playback.send(Message::AllowExplicit(false));

    let effects = playback.send(Message::TrackAdvanced(stale));

    assert_eq!(effects, vec![Effect::Stop]);
    assert_eq!(playback.status(), Status::Stopped);
}

#[test]
fn an_advance_while_another_choice_loads_is_left_to_it() {
    let mut playback = new(1.0);
    let effects = once_playing(&mut playback, Message::Start(album(1, 3, 0)));
    playback.send(Message::Start(album(2, 3, 0)));

    assert_eq!(
        playback.send(Message::TrackAdvanced(armed_entry(&effects))),
        vec![]
    );
    assert_eq!(current(&playback), Some(201));
}

#[test]
fn with_gapless_off_nothing_is_armed() {
    let mut playback = new(1.0);
    playback.send(Message::Gapless(false));

    let effects = once_playing(&mut playback, Message::Start(album(1, 3, 0)));

    assert_eq!(armed(&effects), None);
}

#[test]
fn turning_gapless_off_clears_what_was_armed() {
    let mut playback = playing(album(1, 3, 0));

    assert_eq!(
        playback.send(Message::Gapless(false)),
        vec![Effect::ClearNext]
    );
}

#[test]
fn autoplay_fetches_the_radio_early_when_the_rest_would_all_be_skipped() {
    let tracks = vec![track(101), unavailable(102), explicit(103)];
    let mut playback = autoplaying();
    let effects = playback.send(Message::Start(album_of(1, tracks, Start::All)));
    playback.send(Message::AllowExplicit(false));

    let effects = playback.send(played(token(&effects)));
    let (id, track) = radio_fetch(&effects).expect("a radio fetch");
    assert_eq!(track, 101);
    playback.send(Message::RadioArrived(id, Some(radio(5, 3))));

    let effects = playback.send(Message::TrackFinished);

    assert_eq!(plays(&effects), 501);
    assert_eq!(history(&playback), vec![101]);
}

// Session restore.

/// What `playback` saves, written out and read back into a new Playback, as
/// at the next launch, and what restoring it asked for.
fn relaunched(playback: &Playback) -> (Playback, Vec<Effect>) {
    let json = serde_json::to_vec(&playback.to_snapshot()).unwrap();
    let mut restored = new(1.0);
    let effects = restored.restore(serde_json::from_slice(&json).unwrap());
    (restored, effects)
}

/// Whether `message` asks for the snapshot to be saved.
fn saves(playback: &mut Playback, message: Message) -> bool {
    match playback.update(message) {
        Outcome::Effects(effects) => effects.contains(&Effect::SaveSnapshot),
        Outcome::NeedsExplicitConsent(_) => false,
    }
}

#[test]
fn a_restore_comes_back_paused_where_listening_left_off() {
    let mut playback = playing(long_playlist(1, 5, Start::All));
    and_play(&mut playback, Message::Next);
    add(&mut playback, 901);
    play_next(&mut playback, 902);
    playback.send(Message::Position(42.0));

    let (restored, _) = relaunched(&playback);

    assert_eq!(restored.status(), Status::Stopped);
    assert_eq!(current(&restored), Some(102));
    assert_eq!(restored.position(), 42.0);
    assert_eq!(ids(restored.upcoming()), vec![103, 104, 105]);
    assert_eq!(queued(&restored), vec![902, 901]);
    assert_eq!(history(&restored), vec![101]);
    assert_eq!(playing_from(&restored), Some(playlist_ref(1)));
}

#[test]
fn play_after_a_restore_starts_where_listening_left_off() {
    let mut playback = playing(album(1, 3, 1));
    playback.send(Message::Position(42.0));
    let (mut restored, _) = relaunched(&playback);

    let effects = restored.send(Message::TogglePlay);

    assert_eq!(
        effects,
        vec![Effect::Play {
            token: token(&effects),
            track_id: 102,
            from: Some(42.0),
            album_gain: true,
        }]
    );
}

#[test]
fn a_fill_that_hadnt_finished_starts_again_from_where_it_got_to() {
    let mut playback = new(1.0);
    let effects = and_play(
        &mut playback,
        Message::Start(long_playlist(1, 3, Start::All)),
    );
    let (fill_id, _) = fill(&effects);
    playback.send(Message::PageArrived(fill_id, page(1, 4, 5)));

    let (mut restored, effects) = relaunched(&playback);

    let (fill_id, continuation) = fill(&effects);
    assert_eq!(
        continuation,
        Continuation {
            source: playlist_ref(1),
            offset: 5,
        }
    );
    restored.send(Message::PageArrived(fill_id, page(1, 6, 6)));
    assert_eq!(ids(restored.upcoming()), vec![102, 103, 104, 105, 106]);
}

#[test]
fn a_finished_fill_isnt_started_again() {
    let mut playback = new(1.0);
    let effects = and_play(
        &mut playback,
        Message::Start(long_playlist(1, 3, Start::All)),
    );
    playback.send(Message::FillEnded(fill(&effects).0));

    let (_, effects) = relaunched(&playback);

    assert_eq!(effects, vec![]);
}

#[test]
fn a_shuffled_order_comes_back_as_it_was() {
    let request = request(SourceRef::Album(1), 1, 8, Start::All);
    let mut playback = playing_with(shuffled(new(1.0)), request);
    and_play(&mut playback, Message::Next);

    let (mut restored, _) = relaunched(&playback);

    assert_eq!(play_order(&restored), play_order(&playback));
    // Still shuffled, so the album plays with track gain.
    let effects = restored.send(Message::TogglePlay);
    assert!(!album_gain(&effects));
}

#[test]
fn previous_after_a_restore_steps_back_through_the_play_order() {
    let mut playback = playing(album(1, 3, 0));
    and_play(&mut playback, Message::Next);
    let (mut restored, _) = relaunched(&playback);

    let effects = restored.send(Message::Previous);

    assert!(matches!(effects[..], [Effect::Play { track_id: 101, .. }]));
    assert_eq!(ids(restored.upcoming()), vec![102, 103]);
    assert_eq!(queued(&restored), Vec::<u64>::new());
}

#[test]
fn a_queued_entry_comes_back_under_its_tag() {
    let mut playback = playing(album(1, 3, 0));
    add(&mut playback, 901);
    and_play(&mut playback, Message::Next);

    let (restored, _) = relaunched(&playback);

    assert_eq!(playing_from(&restored), Some(SourceRef::Album(9)));
    assert_eq!(ids(restored.upcoming()), vec![102, 103]);
}

#[test]
fn nothing_played_restores_nothing() {
    let (restored, effects) = relaunched(&new(1.0));

    assert_eq!(effects, vec![]);
    assert_eq!(current(&restored), None);
    assert_eq!(playing_from(&restored), None);
}

#[test]
fn a_snapshot_whose_play_order_doesnt_fit_its_tracks_restores_no_source() {
    let mut playback = playing(album(1, 3, 0));
    playback.send(Message::Position(10.0));
    let mut snapshot = playback.to_snapshot();
    snapshot.source.as_mut().unwrap().order.push(7);

    let mut restored = new(1.0);
    restored.restore(snapshot);

    assert_eq!(playing_from(&restored), Some(SourceRef::Album(1)));
    assert_eq!(ids(restored.upcoming()), Vec::<u64>::new());
    assert_eq!(current(&restored), Some(101), "the current track stays");
}

#[test]
fn what_changes_listening_asks_for_a_save() {
    let mut playback = new(1.0);
    assert!(saves(&mut playback, Message::Start(album(1, 3, 0))));
    let Status::Loading(loading) = playback.status() else {
        unreachable!("the start is loading")
    };
    assert!(saves(&mut playback, played(loading)));
    assert!(saves(&mut playback, Message::Next));
    assert!(saves(
        &mut playback,
        Message::AddToQueue(track(901), tag(9))
    ));
    assert!(saves(&mut playback, Message::Seek(20.0)));
    assert!(saves(&mut playback, Message::TogglePlay));
}

#[test]
fn ticks_and_preferences_ask_for_no_save() {
    let mut playback = playing(album(1, 3, 0));

    assert!(!saves(&mut playback, Message::Position(5.0)));
    assert!(!saves(&mut playback, Message::SetVolume(0.5)));
    assert!(!saves(&mut playback, Message::ToggleMute));
    assert!(!saves(&mut playback, Message::CycleRepeat));
    assert!(!saves(&mut playback, Message::AllowExplicit(false)));
    assert!(!saves(&mut playback, Message::Autoplay(true)));
    assert!(!saves(&mut playback, Message::Gapless(false)));
}
