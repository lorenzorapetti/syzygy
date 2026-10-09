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
//!
//! The Manual queue is separate: Queue entries the user added, each with
//! the Source tag "Playing from" shows while it plays. It plays before
//! what's left of the source, and Shuffle never reorders it.
//!
//! A long source starts with the tracks a Page or card had and reads the
//! rest in the background: a fill. Its pages join the play order as they
//! arrive.

use rand::RngExt;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use syzygy_catalog::{Track, TrackSort};

#[cfg(test)]
mod tests;

/// How many tracks History keeps.
const HISTORY: usize = 500;
/// Past this many seconds in, Previous restarts the track.
const RESTART_AFTER: f32 = 3.0;

pub struct Playback {
    listening: Listening,
    shuffle: bool,
    repeat: Repeat,
    /// Every shuffle draws from it: seeded from entropy at boot, fixed in
    /// tests.
    rng: SmallRng,
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
    /// Stamped on each source play, so History can tell a step back in the
    /// play order from a track that played under another one.
    next_play: u64,
    /// Stamped on each fill, so pages for one that stopped are dropped.
    next_fill: u64,
    /// Stamped on each Queue entry, so two copies of a track stay apart.
    next_entry: u64,
}

/// Where listening is: the source, the Manual queue, the current track and
/// History. Snapshotted whole for a rollback.
#[derive(Clone, Default)]
struct Listening {
    source: Option<SourcePlay>,
    /// Plays first to last, before what's left of the source.
    manual: VecDeque<Item>,
    current: Option<Item>,
    /// Oldest first.
    history: VecDeque<Item>,
}

/// The Playback source and where playback is in it: its tracks, the order
/// they play in and how far along that order it is. Not the Manual queue.
#[derive(Clone)]
struct SourcePlay {
    /// Which play of a source this is; a Repeat all round is a new one.
    play: u64,
    source: Arc<Source>,
    /// The source's tracks, in source order.
    tracks: Arc<Vec<Track>>,
    /// Indices into `tracks`, in the order they play.
    order: Vec<usize>,
    /// The steps before this have played or are playing; the rest are
    /// upcoming.
    next: usize,
    /// The order isn't the source's: a Shuffle play, or Shuffle on.
    shuffled: bool,
    /// The rest of the source, being read. Its id stays across Repeat all
    /// rounds.
    fill: Option<Fill>,
}

/// The source's unread rest, and the fill reading it.
#[derive(Clone)]
struct Fill {
    id: FillId,
    continuation: Continuation,
}

impl SourcePlay {
    /// The next of the source's tracks, read by its fill: on the end of the
    /// play order, or at random places in what's upcoming when the order is
    /// shuffled. Never ahead of what plays next, which may already be
    /// prepared, or put back there by Previous.
    fn append(&mut self, tracks: &[Track], rng: &mut SmallRng) {
        let first = self.tracks.len();
        Arc::make_mut(&mut self.tracks).extend_from_slice(tracks);
        for index in first..self.tracks.len() {
            let at = if self.shuffled {
                let len = self.order.len();
                rng.random_range((self.next + 1).min(len)..=len)
            } else {
                self.order.len()
            };
            self.order.insert(at, index);
        }
        if let Some(fill) = &mut self.fill {
            fill.continuation.offset += tracks.len();
        }
    }
}

/// A Queue entry: a track that plays, played or is playing, and where it
/// came from, its Source tag for a Manual queue entry.
#[derive(Clone)]
struct Item {
    id: EntryId,
    track: Track,
    from: Arc<Source>,
    /// Its step in the play it came from. None for a Manual queue entry.
    step: Option<Step>,
}

/// Each Queue entry's own, so the same track queued twice is two entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    play: u64,
    index: usize,
}

/// What a play replaced, put back if it fails.
struct Rollback {
    listening: Listening,
    status: Status,
    position: f32,
}

/// The Playback source: what it is, and its name for "Playing from".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceRef,
    pub name: String,
}

/// What playback is working through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    Album(u64),
    /// In the order it was sorted in, or else its own.
    Playlist {
        uuid: String,
        sort: Option<TrackSort>,
    },
    Mix(String),
    /// A track's mix, by the mix's id.
    TrackRadio(String),
    /// An artist's top tracks.
    Artist(u64),
    /// In the order they were sorted in, or else last added first.
    LovedTracks(Option<TrackSort>),
    /// The tracks a search found, by its query.
    Search(String),
    /// One track played on its own.
    Track(u64),
}

/// How every play starts: the source, its tracks (all of them, or the first
/// page of them) and where to start. Playback goes from there to the end
/// of the source, never around to its start.
#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub source: Source,
    pub first_page: Vec<Track>,
    pub start: Start,
    /// Where the rest of the source is read from, when `first_page` isn't
    /// all of it.
    pub continuation: Option<Continuation>,
}

/// Where the unread rest of a source starts: plain data, so a fill can be
/// started again from it. A sorted source's sort is part of `source`, so
/// the rest comes in the same order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    pub source: SourceRef,
    /// How many of its tracks have been read.
    pub offset: usize,
}

/// Stamped on each fill and echoed by its pages, so pages for a source that
/// no longer plays are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// From the track at this place in `first_page` to the end, the rest
    /// shuffled when Shuffle is on.
    Track(usize),
    /// The whole source, as Shuffle says: a Page's or a card's Play.
    All,
    /// The whole source in random order, once, leaving Shuffle as it is.
    Shuffled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    /// Start the source over when it runs out.
    All,
    /// Replay the track when it ends. Next still moves on.
    One,
}

/// What playback starts with, from `Settings`.
pub struct Preferences {
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
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
    /// Skip to what comes next.
    Next,
    /// Restart the track, or go back through History.
    Previous,
    /// Put the track at the front of the Manual queue, under its Source tag.
    PlayNext(Track, Source),
    /// Put the track at the end of the Manual queue, under its Source tag.
    AddToQueue(Track, Source),
    ToggleShuffle,
    /// Off, all, one, off.
    CycleRepeat,
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
    /// A fill read these tracks, the next ones in the source.
    PageArrived(FillId, Vec<Track>),
    /// A fill read the last of its source, or gave up.
    FillEnded(FillId),
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
    /// Fetch the track's stream and play it, from `from` seconds in, with
    /// the album's gain rather than the track's when normalizing.
    Play {
        token: PlayToken,
        track_id: u64,
        from: Option<f32>,
        album_gain: bool,
    },
    Pause,
    Resume,
    /// Let go of the track, and of any play still loading.
    Stop,
    Seek(f32),
    SetVolume(f32),
    /// Read the rest of the source from `continuation` on, a page at a
    /// time, until it runs out. Replaces any fill running.
    StartFill {
        fill_id: FillId,
        continuation: Continuation,
    },
    /// Stop reading the source.
    CancelFill,
}

impl Playback {
    pub fn new(preferences: Preferences, rng: SmallRng) -> Self {
        Self {
            listening: Listening::default(),
            shuffle: preferences.shuffle,
            repeat: preferences.repeat,
            rng,
            status: Status::Stopped,
            position: 0.0,
            volume: preferences.volume.clamp(0.0, 1.0),
            pre_mute: 0.0,
            rollback: None,
            next_token: 0,
            next_play: 0,
            next_fill: 0,
            next_entry: 0,
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
                Status::Stopped if self.listening.current.is_some() => {
                    self.snapshot();
                    let from = (self.position > 0.0).then_some(self.position);
                    self.play(from)
                }
                Status::Stopped | Status::Loading(_) => vec![],
            },
            Message::Next => self.next(),
            Message::Previous => self.previous(),
            Message::PlayNext(track, tag) => self.enqueue(track, tag, true),
            Message::AddToQueue(track, tag) => self.enqueue(track, tag, false),
            Message::ToggleShuffle => {
                self.shuffle = !self.shuffle;
                if let Some(play) = &mut self.listening.source {
                    let tail = &mut play.order[play.next..];
                    if self.shuffle {
                        tail.shuffle(&mut self.rng);
                    } else {
                        // The tail holds what hasn't played and wasn't
                        // removed, so sorting it is source order minus those.
                        tail.sort_unstable();
                    }
                    play.shuffled = self.shuffle;
                }
                vec![]
            }
            Message::CycleRepeat => {
                self.repeat = match self.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
                vec![]
            }
            Message::Seek(position) => match self.status {
                Status::Playing | Status::Paused => {
                    self.position = position;
                    vec![Effect::Seek(position)]
                }
                Status::Stopped if self.listening.current.is_some() => {
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
                    (Ok(()), _) => {
                        self.status = Status::Playing;
                        vec![]
                    }
                    (Err(e), Some(rollback)) => {
                        log::warn!("Could not play the track: {e}");
                        let filling = self.fill_id();
                        self.listening = rollback.listening;
                        self.position = rollback.position;
                        self.status = match e {
                            PlayError::Resolve(_) => rollback.status,
                            PlayError::Audio(_) => Status::Stopped,
                        };
                        self.refill(filling)
                    }
                    (Err(e), None) => {
                        log::warn!("Could not play the track: {e}");
                        self.status = Status::Stopped;
                        vec![]
                    }
                }
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
                if self.repeat == Repeat::One {
                    self.snapshot();
                    return self.play(None);
                }
                self.snapshot();
                if self.advance() {
                    self.play(None)
                } else {
                    self.rollback = None;
                    vec![]
                }
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
            Message::PageArrived(id, tracks) => {
                let (plays, rng) = self.filling(id);
                for play in plays {
                    play.append(&tracks, rng);
                }
                vec![]
            }
            Message::FillEnded(id) => {
                for play in self.filling(id).0 {
                    play.fill = None;
                }
                vec![]
            }
        }
    }

    /// The track playing, or that would play.
    pub fn current(&self) -> Option<&Track> {
        self.listening.current.as_ref().map(|entry| &entry.track)
    }

    /// Where the current track comes from.
    pub fn playing_from(&self) -> Option<&Source> {
        self.listening
            .current
            .as_ref()
            .map(|entry| entry.from.as_ref())
    }

    /// The Manual queue, in the order it plays.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the drawer's Queue tab (ticket 29) lists them")
    )]
    pub fn queued(&self) -> impl Iterator<Item = (EntryId, &Track)> {
        self.listening
            .manual
            .iter()
            .map(|entry| (entry.id, &entry.track))
    }

    /// What's left of the source after the Manual queue, in order.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the drawer's Queue tab (ticket 29) lists them")
    )]
    pub fn upcoming(&self) -> impl Iterator<Item = &Track> {
        self.listening.source.iter().flat_map(|play| {
            play.order[play.next..]
                .iter()
                .map(|&index| &play.tracks[index])
        })
    }

    /// The tracks already played, oldest first.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the drawer's Queue tab (ticket 29) lists them")
    )]
    pub fn history(&self) -> impl Iterator<Item = &Track> {
        self.listening.history.iter().map(|entry| &entry.track)
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

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    fn start(&mut self, request: PlayRequest) -> Vec<Effect> {
        let len = request.first_page.len();
        let (mut order, shuffled) = match request.start {
            Start::Track(index) if index < len => ((index..len).collect::<Vec<_>>(), self.shuffle),
            Start::All | Start::Shuffled if len > 0 => (
                (0..len).collect(),
                self.shuffle || request.start == Start::Shuffled,
            ),
            _ => return vec![],
        };
        match request.start {
            // The chosen track plays first whatever Shuffle says.
            Start::Track(_) if shuffled => order[1..].shuffle(&mut self.rng),
            _ if shuffled => order.shuffle(&mut self.rng),
            _ => {}
        }
        self.snapshot();
        self.retire_current();
        // A new source starts afresh, as in sone.
        self.listening.manual.clear();
        let filling = self.fill_id();
        let fill = request.continuation.map(|continuation| Fill {
            id: self.stamp_fill(),
            continuation,
        });
        self.listening.source = Some(SourcePlay {
            play: self.stamp_play(),
            source: Arc::new(request.source),
            tracks: Arc::new(request.first_page),
            order,
            next: 0,
            shuffled,
            fill,
        });
        self.advance();
        let mut effects = self.play(None);
        effects.extend(self.refill(filling));
        effects
    }

    /// Next, chosen by the user: on even under Repeat one. Past the end it
    /// starts over under Repeat all, and otherwise stops.
    fn next(&mut self) -> Vec<Effect> {
        if self.listening.current.is_none() {
            return vec![];
        }
        self.snapshot();
        if self.advance() {
            return self.play(None);
        }
        self.rollback = None;
        self.position = 0.0;
        match self.status {
            Status::Stopped => vec![],
            _ => {
                self.status = Status::Stopped;
                vec![Effect::Stop]
            }
        }
    }

    /// The track into the Manual queue: at the front for Play next, else
    /// at the end. With nothing playing, it plays.
    fn enqueue(&mut self, track: Track, tag: Source, next: bool) -> Vec<Effect> {
        let entry = Item {
            id: self.stamp_entry(),
            track,
            from: Arc::new(tag),
            step: None,
        };
        if self.listening.current.is_none() {
            self.snapshot();
            self.listening.current = Some(entry);
            return self.play(None);
        }
        // A play loading now may fail: what it rolls back to keeps the
        // entry too.
        let rollback = self.rollback.as_mut().map(|r| &mut r.listening.manual);
        for manual in std::iter::once(&mut self.listening.manual).chain(rollback) {
            if next {
                manual.push_front(entry.clone());
            } else {
                manual.push_back(entry.clone());
            }
        }
        vec![]
    }

    /// Past 3 s in, or with nothing in History, restart the track.
    /// Otherwise play the last track in History under its own tag. The
    /// current track goes back to being upcoming: a step back in the play
    /// order when it's the source's latest step, or else to the front of
    /// the Manual queue.
    fn previous(&mut self) -> Vec<Effect> {
        if self.listening.current.is_none() {
            return vec![];
        }
        if self.position > RESTART_AFTER || self.listening.history.is_empty() {
            return self.restart();
        }
        self.snapshot();
        let listening = &mut self.listening;
        let entry = listening.history.pop_back().expect("History isn't empty");
        let current = listening.current.replace(entry);
        match (&mut listening.source, current) {
            (Some(play), Some(current))
                if current
                    .step
                    .is_some_and(|step| step.play == play.play && step.index + 1 == play.next) =>
            {
                play.next -= 1;
            }
            (_, Some(current)) => listening.manual.push_front(current),
            (_, None) => {}
        }
        self.play(None)
    }

    fn restart(&mut self) -> Vec<Effect> {
        self.position = 0.0;
        match self.status {
            Status::Playing | Status::Paused => vec![Effect::Seek(0.0)],
            Status::Stopped | Status::Loading(_) => vec![],
        }
    }

    /// Put the current track in History and make what comes next current:
    /// the head of the Manual queue, the next step of the play order, or
    /// under Repeat all the first step of a new round. False, with nothing
    /// changed, when nothing comes next.
    fn advance(&mut self) -> bool {
        if let Some(entry) = self.listening.manual.pop_front() {
            self.retire_current();
            self.listening.current = Some(entry);
            return true;
        }
        let Some(play) = &self.listening.source else {
            return false;
        };
        if play.next == play.order.len() {
            if self.repeat != Repeat::All || play.tracks.is_empty() {
                return false;
            }
            self.start_over();
        }
        self.retire_current();
        let id = self.stamp_entry();
        let Some(play) = &mut self.listening.source else {
            return false;
        };
        let index = play.next;
        play.next += 1;
        self.listening.current = Some(Item {
            id,
            track: play.tracks[play.order[index]].clone(),
            from: play.source.clone(),
            step: Some(Step {
                play: play.play,
                index,
            }),
        });
        true
    }

    /// Repeat all: a new round of the whole source, reshuffled if Shuffle is
    /// on and in source order if not.
    fn start_over(&mut self) {
        let round = self.stamp_play();
        let Some(play) = &mut self.listening.source else {
            return;
        };
        play.play = round;
        play.order = (0..play.tracks.len()).collect();
        if self.shuffle {
            play.order.shuffle(&mut self.rng);
        }
        play.shuffled = self.shuffle;
        play.next = 0;
    }

    /// The current track has played: into History, which keeps the last
    /// [`HISTORY`].
    fn retire_current(&mut self) {
        if let Some(entry) = self.listening.current.take() {
            self.listening.history.push_back(entry);
            if self.listening.history.len() > HISTORY {
                self.listening.history.pop_front();
            }
        }
    }

    /// The source plays fill `id` reads for: the one playing, and the one a
    /// failed play would roll back to, which pages must reach too.
    fn filling(&mut self, id: FillId) -> (impl Iterator<Item = &mut SourcePlay>, &mut SmallRng) {
        let rollback = self.rollback.as_mut().map(|r| &mut r.listening);
        let plays = std::iter::once(&mut self.listening)
            .chain(rollback)
            .filter_map(|listening| listening.source.as_mut())
            .filter(move |play| play.fill.as_ref().is_some_and(|fill| fill.id == id));
        (plays, &mut self.rng)
    }

    /// The fill running for the source, if it's still reading.
    fn fill_id(&self) -> Option<FillId> {
        let play = self.listening.source.as_ref()?;
        play.fill.as_ref().map(|fill| fill.id)
    }

    /// The source changed from one whose fill was `filling`: stop that
    /// fill, and read the rest of this one from where it got to.
    fn refill(&mut self, filling: Option<FillId>) -> Vec<Effect> {
        let mut effects = Vec::new();
        if filling.is_some() && filling != self.fill_id() {
            effects.push(Effect::CancelFill);
        }
        let id = self.stamp_fill();
        if let Some(play) = &mut self.listening.source
            && let Some(fill) = &mut play.fill
            && filling != Some(fill.id)
        {
            // Pages for the fill it had before were dropped with it.
            fill.id = id;
            effects.push(Effect::StartFill {
                fill_id: id,
                continuation: fill.continuation.clone(),
            });
        }
        effects
    }

    fn stamp_fill(&mut self) -> FillId {
        self.next_fill += 1;
        FillId(self.next_fill)
    }

    fn stamp_entry(&mut self) -> EntryId {
        self.next_entry += 1;
        EntryId(self.next_entry)
    }

    fn stamp_play(&mut self) -> u64 {
        self.next_play += 1;
        self.next_play
    }

    /// Album gain only for an album's own track in album order: not after
    /// a Shuffle play or with Shuffle on.
    fn album_gain(&self) -> bool {
        let (Some(play), Some(entry)) = (&self.listening.source, &self.listening.current) else {
            return false;
        };
        matches!(entry.from.kind, SourceRef::Album(_))
            && entry.step.is_some_and(|step| step.play == play.play)
            && !play.shuffled
    }

    /// Remember what's playing, to go back to if the next play fails. While
    /// a play is loading, what's playing is what was there before it.
    fn snapshot(&mut self) {
        if !matches!(self.status, Status::Loading(_)) {
            self.rollback = Some(Rollback {
                listening: self.listening.clone(),
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
            album_gain: self.album_gain(),
        }]
    }
}
