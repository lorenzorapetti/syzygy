//! What plays, in what order, and whether it's playing (ADR 0002, ADR 0004).
//!
//! [`Playback::update`] is pure: it changes state and returns [`Effect`]s,
//! plain data that the App's runner turns into calls on the audio engine.
//! Never call an engine from here; the tests read the effects to see what
//! the logic asked for.
//!
//! The Playback source owns its tracks in source order. Playback keeps a
//! play order (indices into those tracks) and a cursor: the upcoming tracks
//! are the play order after the cursor.

use std::sync::Arc;
use syzygy_catalog::Track;

#[cfg(test)]
mod tests;

pub struct Playback {
    source_play: Option<SourcePlay>,
    status: Status,
    /// Seconds into the current track: from the engine while playing, the
    /// target while loading, where play starts again while stopped.
    position: f32,
    volume: f32,
    /// What unmuting goes back to. 0 when nothing was muted.
    pre_mute: f32,
    /// What to go back to if the play loading now fails.
    rollback: Option<Rollback>,
    next_token: u64,
}

/// The Playback source and where playback is in it: its tracks, the order
/// they play in and the cursor. Not the Manual queue.
#[derive(Clone)]
struct SourcePlay {
    #[expect(dead_code, reason = "\"Playing from\" (ticket 23) reads it")]
    source: SourceRef,
    /// The source's tracks, in source order.
    tracks: Arc<Vec<Track>>,
    /// Indices into `tracks`, in the order they play.
    order: Vec<usize>,
    /// Where the current track is in `order`.
    cursor: usize,
}

/// What a play replaced, put back if it fails.
struct Rollback {
    source_play: Option<SourcePlay>,
    status: Status,
    position: f32,
}

/// What playback is working through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    Album(u64),
}

/// How every play starts: the source, its tracks (all of them, or the first
/// page of them) and the one to start at. Playback goes from there to the
/// end of the source, never around to its start.
#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub source: SourceRef,
    pub first_page: Vec<Track>,
    /// Where in `first_page` to start.
    pub start: usize,
}

/// Stamped on each play and echoed by its result, so a result for an older
/// choice is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayToken(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    /// The current track is being fetched and started.
    Loading(PlayToken),
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub enum Message {
    Start(PlayRequest),
    /// Play or pause, as the player bar's button.
    TogglePlay,
    /// Go to this many seconds into the current track.
    Seek(f32),
    /// From 0 to 1.
    SetVolume(f32),
    ToggleMute,
    /// The engine's position, read while playing.
    Position(f32),
    /// How a play went.
    Played(PlayToken, Result<(), PlayError>),
    /// The playing track ended. The only way a track ends.
    TrackFinished,
    /// The engine stopped on an error of its own.
    EngineFailed,
}

/// Why a play failed.
#[derive(Debug, Clone)]
pub enum PlayError {
    /// TIDAL had no stream for it. Whatever played before still does.
    Resolve(Arc<syzygy_tidal::Error>),
    /// The engine couldn't play it, having let go of what played before.
    Audio(Arc<syzygy_audio::Error>),
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayError::Resolve(e) => write!(f, "no stream: {e}"),
            PlayError::Audio(e) => write!(f, "the engine refused it: {e}"),
        }
    }
}

/// What playback asks of the audio engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Fetch the track's stream and play it, from `from` seconds in.
    Play {
        token: PlayToken,
        track_id: u64,
        from: Option<f32>,
    },
    Pause,
    Resume,
    Seek(f32),
    SetVolume(f32),
}

impl Playback {
    pub fn new(volume: f32) -> Self {
        Self {
            source_play: None,
            status: Status::Stopped,
            position: 0.0,
            volume: volume.clamp(0.0, 1.0),
            pre_mute: 0.0,
            rollback: None,
            next_token: 0,
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Start(request) => self.start(request),
            Message::TogglePlay => match self.status {
                Status::Playing => {
                    self.status = Status::Paused;
                    vec![Effect::Pause]
                }
                Status::Paused => {
                    self.status = Status::Playing;
                    vec![Effect::Resume]
                }
                Status::Stopped if self.source_play.is_some() => {
                    self.snapshot();
                    let from = (self.position > 0.0).then_some(self.position);
                    self.play(from)
                }
                Status::Stopped | Status::Loading(_) => vec![],
            },
            Message::Seek(position) => match self.status {
                Status::Playing | Status::Paused => {
                    self.position = position;
                    vec![Effect::Seek(position)]
                }
                Status::Stopped if self.source_play.is_some() => {
                    self.position = position;
                    vec![]
                }
                Status::Stopped | Status::Loading(_) => vec![],
            },
            Message::SetVolume(volume) => {
                self.volume = volume.clamp(0.0, 1.0);
                vec![Effect::SetVolume(self.volume)]
            }
            Message::ToggleMute => {
                if self.volume > 0.0 {
                    self.pre_mute = self.volume;
                    self.volume = 0.0;
                } else {
                    self.volume = if self.pre_mute > 0.0 {
                        self.pre_mute
                    } else {
                        0.5
                    };
                }
                vec![Effect::SetVolume(self.volume)]
            }
            Message::Position(position) => {
                if self.status == Status::Playing {
                    self.position = position;
                }
                vec![]
            }
            Message::Played(token, result) => {
                if self.status != Status::Loading(token) {
                    return vec![];
                }
                let rollback = self.rollback.take();
                match (result, rollback) {
                    (Ok(()), _) => self.status = Status::Playing,
                    (Err(e), Some(rollback)) => {
                        log::warn!("Could not play the track: {e}");
                        self.source_play = rollback.source_play;
                        self.position = rollback.position;
                        self.status = match e {
                            PlayError::Resolve(_) => rollback.status,
                            PlayError::Audio(_) => Status::Stopped,
                        };
                    }
                    (Err(e), None) => {
                        log::warn!("Could not play the track: {e}");
                        self.status = Status::Stopped;
                    }
                }
                vec![]
            }
            Message::TrackFinished => {
                if let Status::Loading(_) = self.status {
                    self.ended_while_loading(0.0);
                    return vec![];
                }
                if self.status != Status::Playing {
                    return vec![];
                }
                // The finished track is what a failed next one rolls back to.
                self.status = Status::Stopped;
                self.position = 0.0;
                let next = self
                    .source_play
                    .as_ref()
                    .is_some_and(|play| play.cursor + 1 < play.order.len());
                if !next {
                    return vec![];
                }
                self.snapshot();
                if let Some(play) = &mut self.source_play {
                    play.cursor += 1;
                }
                self.play(None)
            }
            Message::EngineFailed => {
                match self.status {
                    Status::Playing | Status::Paused => self.status = Status::Stopped,
                    Status::Loading(_) => {
                        let position = self.rollback.as_ref().map_or(0.0, |r| r.position);
                        self.ended_while_loading(position);
                    }
                    Status::Stopped => {}
                }
                vec![]
            }
        }
    }

    /// The track playing, or that would play.
    pub fn current(&self) -> Option<&Track> {
        let play = self.source_play.as_ref()?;
        play.tracks.get(play.order[play.cursor])
    }

    /// What plays after the current track, in order.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the SourcePlay tab (ticket 29) lists them")
    )]
    pub fn upcoming(&self) -> impl Iterator<Item = &Track> {
        self.source_play.iter().flat_map(|play| {
            play.order[play.cursor + 1..]
                .iter()
                .map(|&index| &play.tracks[index])
        })
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    fn start(&mut self, request: PlayRequest) -> Vec<Effect> {
        if request.start >= request.first_page.len() {
            return vec![];
        }
        self.snapshot();
        self.source_play = Some(SourcePlay {
            source: request.source,
            order: (0..request.first_page.len()).collect(),
            tracks: Arc::new(request.first_page),
            cursor: request.start,
        });
        self.position = 0.0;
        self.play(None)
    }

    /// Remember what's playing, to go back to if the next play fails. While
    /// a play is loading, what's playing is what was there before it.
    fn snapshot(&mut self) {
        if !matches!(self.status, Status::Loading(_)) {
            self.rollback = Some(Rollback {
                source_play: self.source_play.clone(),
                status: self.status,
                position: self.position,
            });
        }
    }

    /// What played before the loading choice stopped on its own, at
    /// `position`: if the choice fails, it comes back stopped there.
    fn ended_while_loading(&mut self, position: f32) {
        if let Some(rollback) = &mut self.rollback
            && matches!(rollback.status, Status::Playing | Status::Paused)
        {
            rollback.status = Status::Stopped;
            rollback.position = position;
        }
    }

    /// Fetch and play the current track.
    fn play(&mut self, from: Option<f32>) -> Vec<Effect> {
        let Some(track_id) = self.current().map(|track| track.id) else {
            return vec![];
        };
        let token = PlayToken(self.next_token);
        self.next_token += 1;
        self.status = Status::Loading(token);
        self.position = from.unwrap_or(0.0);
        vec![Effect::Play {
            token,
            track_id,
            from,
        }]
    }
}
