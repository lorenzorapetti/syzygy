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
//!
//! Moving on skips what can't play: unavailable tracks, counted so that a
//! broken source stops after [`MAX_FAILURES`] in a row, and explicit ones
//! while they aren't allowed, silently. Choosing explicit tracks while
//! they aren't allowed asks first: [`Outcome::NeedsExplicitConsent`].
//!
//! With Autoplay on and Repeat off, the last track's Track radio is fetched
//! as it starts, and becomes the Playback source when nothing is left.
//!
//! Whatever comes next, when playback already knows it and it won't be
//! skipped, is armed in the engine so it follows with no gap. The engine
//! says when it took over ([`Message::TrackAdvanced`]), and playback moves
//! on to it without playing anything.
//!
//! Whatever changes where listening is asks for it to be saved
//! ([`Effect::SaveSnapshot`]), and the next launch restores it stopped.
//!
//! The drawer's Queue tab picks, moves and removes [`Entry`]s, and clears
//! what's upcoming.

use rand::RngExt;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use syzygy_catalog::{Track, TrackSort};

mod snapshot;
#[cfg(test)]
mod tests;

/// How many tracks History keeps.
const HISTORY: usize = 500;
/// Past this many seconds in, Previous restarts the track.
const RESTART_AFTER: f32 = 3.0;
/// After this many tracks in a row can't play, playback stops.
const MAX_FAILURES: u8 = 3;
/// How long to wait out a rate limit that didn't say.
const RATE_LIMIT_SECS: u64 = 5;
/// How long the volume takes to slide to where bit-perfect output puts it,
/// as sone's 12 steps over 300 ms.
pub const RAMP: Duration = Duration::from_millis(300);

pub struct Playback {
    listening: Listening,
    shuffle: bool,
    repeat: Repeat,
    autoplay: bool,
    /// Whether what comes next is armed to follow with no gap.
    gapless: bool,
    /// Every shuffle draws from it: seeded from entropy at boot, fixed in
    /// tests.
    rng: SmallRng,
    status: Status,
    /// Whether explicit tracks may play.
    allow_explicit: bool,
    /// Tracks in a row that couldn't play since one last did, or since the
    /// user chose something.
    failures: u8,
    /// The play a rate limit stopped, to try again once the wait is over
    /// unless the user did something first.
    rate_limited: Option<PlayToken>,
    /// The Track radio Autoplay fetched, or is fetching, for the last track.
    radio: Option<RadioFetch>,
    /// What the engine has prepared to follow the current track.
    armed: Option<Armed>,
    /// Seconds into the current track: from the engine while playing, the
    /// target while loading, where play starts again while stopped.
    position: f32,
    volume: f32,
    /// What unmuting goes back to. 0 when nothing was muted.
    pre_mute: f32,
    /// Whether tracks are levelled by their ReplayGain.
    normalization: bool,
    output: Output,
    /// What bit-perfect output put aside, to have back when it's off.
    before_bit_perfect: Option<Levels>,
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
    /// Stamped on each radio fetch, so one for a track that's no longer
    /// last is dropped.
    next_radio: u64,
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
    /// The steps before this have played, are playing or were skipped by
    /// a pick; the rest are upcoming.
    next: usize,
    /// The order isn't the source's: a Shuffle play, or Shuffle on.
    shuffled: bool,
    /// The rest of the source, being read. Its id stays across Repeat all
    /// rounds.
    fill: Option<Fill>,
    /// The order of the next Repeat all round, drawn once this one runs
    /// out so its first track can be armed.
    round: Option<Vec<usize>>,
    /// A Track radio Autoplay started.
    autoplay: bool,
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
        self.round = None;
    }

    /// The play order has no steps left.
    fn ran_out(&self) -> bool {
        self.next == self.order.len()
    }

    /// The tracks of the steps left, in order.
    fn rest(&self) -> impl Iterator<Item = &Track> {
        self.order[self.next..]
            .iter()
            .map(|&index| &self.tracks[index])
    }

    /// Where `slot` is in the play order, if it's still upcoming in it.
    fn find(&self, slot: Slot) -> Option<usize> {
        if slot.play != self.play {
            return None;
        }
        let at = self.order[self.next..]
            .iter()
            .position(|&index| index == slot.track)?;
        Some(self.next + at)
    }
}

impl Listening {
    /// The track of the drawer's row, while it's still listed.
    fn track(&self, entry: Entry) -> Option<&Track> {
        fn item(items: &VecDeque<Item>, id: EntryId) -> Option<&Track> {
            let item = items.iter().find(|item| item.id == id)?;
            Some(&item.track)
        }
        match entry {
            Entry::Played(id) => item(&self.history, id),
            Entry::Queued(id) => item(&self.manual, id),
            Entry::Upcoming(slot) => {
                let play = self.source.as_ref()?;
                play.find(slot).map(|at| &play.tracks[play.order[at]])
            }
        }
    }

    /// Take `entry` out of what's upcoming. History stays as it is.
    fn remove(&mut self, entry: Entry) {
        match entry {
            Entry::Played(_) => {}
            Entry::Queued(id) => self.manual.retain(|queued| queued.id != id),
            Entry::Upcoming(slot) => {
                if let Some(play) = &mut self.source
                    && let Some(at) = play.find(slot)
                {
                    play.order.remove(at);
                }
            }
        }
    }

    /// Move `entry` to place `to` of its section: the Manual queue, or
    /// what's left of the source. Past the end is the end.
    fn shift(&mut self, entry: Entry, to: usize) {
        match entry {
            Entry::Played(_) => {}
            Entry::Queued(id) => {
                let manual = &mut self.manual;
                if let Some(at) = manual.iter().position(|queued| queued.id == id) {
                    let queued = manual.remove(at).expect("found above");
                    manual.insert(to.min(manual.len()), queued);
                }
            }
            Entry::Upcoming(slot) => {
                if let Some(play) = &mut self.source
                    && let Some(at) = play.find(slot)
                {
                    let index = play.order.remove(at);
                    let to = (play.next + to).min(play.order.len());
                    play.order.insert(to, index);
                }
            }
        }
    }

    /// Nothing upcoming: no Manual queue, the play order ends with the
    /// current track and the fill stops. The source stays, for Repeat all.
    fn clear(&mut self) {
        self.manual.clear();
        if let Some(play) = &mut self.source {
            play.order.truncate(play.next);
            play.fill = None;
            play.round = None;
        }
    }
}

/// Every index of `len` tracks: in random order with `shuffle` on, in
/// source order without.
fn round_order(len: usize, shuffle: bool, rng: &mut SmallRng) -> Vec<usize> {
    let mut order: Vec<_> = (0..len).collect();
    if shuffle {
        order.shuffle(rng);
    }
    order
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
    /// False only for a track from a Track radio Autoplay started.
    chosen: bool,
}

/// Each Queue entry's own, so the same track queued twice is two entries.
/// The engine echoes it for an armed entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryId(pub u64);

/// A row of the drawer's Queue tab: a track in History, in the Manual
/// queue, or in what's left of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Played(EntryId),
    Queued(EntryId),
    Upcoming(Slot),
}

/// An upcoming track's place in a play of the source: it stays put while
/// the play order around it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    play: u64,
    /// Its index in the source's tracks, which the play order holds once.
    track: usize,
}

/// A Track radio fetch, for the entry `after`, the last to play.
struct RadioFetch {
    id: RadioId,
    after: EntryId,
    state: RadioState,
    /// Nothing was left when the last track ended: the radio plays as
    /// soon as it arrives.
    waiting: bool,
}

/// Where a Track radio fetch got to.
enum RadioState {
    Fetching,
    /// What it has that hasn't played.
    Ready(Radio),
    /// There was none, the fetch failed, or it already started.
    Spent,
}

/// Stamped on each Track radio fetch and echoed by its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioId(u64);

/// A Track radio, as Autoplay plays it.
#[derive(Debug, Clone)]
pub struct Radio {
    pub source: Source,
    pub tracks: Vec<Track>,
}

/// What moving on would make current, and a gapless advance moves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Upcoming {
    /// The current track again, under Repeat one.
    Again,
    /// The head of the Manual queue.
    Queued(EntryId),
    /// The next step of the play order.
    Step(Step),
    /// The first step of a new Repeat all round of the play.
    NewRound(u64),
    /// The first track of the Track radio from this fetch.
    Radio(RadioId),
}

/// What the engine was asked to prepare, as `entry`.
struct Armed {
    upcoming: Upcoming,
    entry: EntryId,
    track_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub kind: SourceRef,
    pub name: String,
}

/// What playback is working through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub autoplay: bool,
    pub gapless: bool,
    pub allow_explicit: bool,
    pub normalization: bool,
    pub output: Output,
    pub before_bit_perfect: Option<Levels>,
}

/// How sound leaves syzygy: through the system mixer, or straight to an
/// ALSA device (exclusive mode), untouched (bit-perfect). Bit-perfect is
/// always exclusive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub exclusive: bool,
    /// The ALSA device exclusive output goes to, such as `hw:1,0`.
    pub device: Option<String>,
    pub bit_perfect: bool,
}

/// The volume and normalization from before bit-perfect output, which
/// fixes them at full and off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Levels {
    pub volume: f32,
    pub normalization: bool,
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
    /// The play is waiting for a busy audio device, trying again.
    DeviceBusy(PlayToken),
    /// A rate limit's wait, after this play hit it, is over.
    Resume(PlayToken),
    /// The playing track ended with nothing armed. The only way a track
    /// ends without a gapless advance.
    TrackFinished,
    /// The engine moved on, with no gap, to the entry armed as this.
    TrackAdvanced(EntryId),
    /// The engine stopped on an error of its own, in its words.
    EngineFailed(String),
    /// A fill read these tracks, the next ones in the source.
    PageArrived(FillId, Vec<Track>),
    /// A fill read the last of its source, or gave up.
    FillEnded(FillId),
    /// Whether explicit tracks may play.
    AllowExplicit(bool),
    /// Whether the last track's Track radio follows when nothing is left.
    Autoplay(bool),
    /// Whether what comes next is armed to follow with no gap.
    Gapless(bool),
    /// Whether tracks are levelled by their ReplayGain. Not while
    /// bit-perfect.
    Normalization(bool),
    /// Exclusive output on or off. On with no device chosen, it goes to
    /// `first_device`, the first one listed. Off, it takes bit-perfect off
    /// with it.
    Exclusive {
        on: bool,
        first_device: Option<String>,
    },
    /// Bit-perfect output on or off. On, it turns exclusive output on (to
    /// `first_device` if none is chosen), fixes the volume at full and
    /// turns normalization off; off, it brings them back as they were.
    BitPerfect {
        on: bool,
        first_device: Option<String>,
    },
    /// Exclusive output goes to this ALSA device.
    OutputDevice(String),
    /// The Track radio fetch came back: None when there's no radio or the
    /// fetch failed.
    RadioArrived(RadioId, Option<Radio>),
    /// What waited for consent, to go ahead with explicit tracks skipped.
    WithoutExplicit(Pending),
    /// Play the drawer's row now. A track from the source jumps there,
    /// passing over the ones before it; a queued one leaves the queue; one
    /// from History plays again, leaving what's upcoming as it is.
    Pick(Entry),
    /// Take the drawer's row out of what's upcoming.
    Remove(Entry),
    /// Move the drawer's row to this place in its section.
    Move(Entry, usize),
    /// Empty the Manual queue and what's left of the source.
    Clear,
}

/// What [`Playback::update`] did.
#[must_use]
#[derive(Debug)]
pub enum Outcome {
    /// It changed what it had to and asks for these.
    Effects(Vec<Effect>),
    /// The message chose explicit tracks while they aren't allowed, so
    /// nothing changed. Allowing them and sending the message again plays
    /// them; [`Message::WithoutExplicit`] goes ahead without them.
    NeedsExplicitConsent(Pending),
}

/// A message waiting for consent to explicit tracks.
#[derive(Debug, Clone)]
pub struct Pending(Box<Message>);

impl From<Pending> for Message {
    fn from(pending: Pending) -> Self {
        *pending.0
    }
}

/// Whether `message` chooses explicit tracks to play: a start whose tracks
/// from the chosen one on include one, or queueing or picking one.
fn chooses_explicit(message: &Message, listening: &Listening) -> bool {
    match message {
        Message::Start(request) => {
            let from = match request.start {
                Start::Track(index) => index,
                Start::All | Start::Shuffled => 0,
            };
            request
                .first_page
                .get(from..)
                .is_some_and(|tracks| tracks.iter().any(|track| track.explicit))
        }
        Message::PlayNext(track, _) | Message::AddToQueue(track, _) => track.explicit,
        Message::Pick(entry) => listening.track(*entry).is_some_and(|track| track.explicit),
        _ => false,
    }
}

/// Whether `message` may change what a snapshot saves. Position ticks
/// don't: the position is saved with whatever else changes, and on Quit.
/// Preferences are `Settings`'.
fn changes_listening(message: &Message) -> bool {
    match message {
        Message::Start(_)
        | Message::TogglePlay
        | Message::Next
        | Message::Previous
        | Message::PlayNext(..)
        | Message::AddToQueue(..)
        | Message::ToggleShuffle
        | Message::Seek(_)
        | Message::Played(..)
        | Message::Resume(_)
        | Message::TrackFinished
        | Message::TrackAdvanced(_)
        | Message::EngineFailed(_)
        | Message::PageArrived(..)
        | Message::FillEnded(_)
        | Message::RadioArrived(..)
        | Message::WithoutExplicit(_)
        | Message::Pick(_)
        | Message::Remove(_)
        | Message::Move(..)
        | Message::Clear => true,
        Message::Position(_)
        | Message::DeviceBusy(_)
        | Message::SetVolume(_)
        | Message::ToggleMute
        | Message::CycleRepeat
        | Message::AllowExplicit(_)
        | Message::Autoplay(_)
        | Message::Gapless(_)
        | Message::Normalization(_)
        | Message::Exclusive { .. }
        | Message::BitPerfect { .. }
        | Message::OutputDevice(_) => false,
    }
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
    /// the album's gain rather than the track's when normalizing. Whatever
    /// was armed is dropped at once, so the old track can't advance to it.
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
    /// Slide the volume from `from` to `to` over `over`, rather than jump.
    RampVolume {
        from: f32,
        to: f32,
        over: Duration,
    },
    /// Level tracks by their ReplayGain, or stop.
    SetNormalization(bool),
    /// Send sound out this way from the next track on.
    SetOutput(Output),
    /// Let the engine advance to what's armed with no gap, or not.
    SetGapless(bool),
    /// Read the rest of the source from `continuation` on, a page at a
    /// time, until it runs out. Replaces any fill running.
    StartFill {
        fill_id: FillId,
        continuation: Continuation,
    },
    /// Stop reading the source.
    CancelFill,
    /// Prepare the track to follow the current one with no gap, as
    /// `entry`, which [`Message::TrackAdvanced`] echoes. What was armed is
    /// dropped at once, before the new track is fetched.
    ArmNext {
        entry: EntryId,
        track_id: u64,
        album_gain: bool,
    },
    /// Drop what was armed.
    ClearNext,
    /// Fetch `track`'s Track radio, for [`Message::RadioArrived`].
    FetchTrackRadio {
        radio: RadioId,
        track: Track,
    },
    /// Send [`Message::Resume`] with `token` once `delay` has passed.
    ResumeAfter {
        token: PlayToken,
        delay: Duration,
    },
    /// Tell the user.
    Notify(Notice),
    /// Where listening is changed: save [`Playback::to_snapshot`] soon.
    SaveSnapshot,
}

/// What playback tells the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// [`MAX_FAILURES`] tracks in a row couldn't play, so playback stopped.
    TooManyFailures,
    /// The chosen track can't play.
    Unavailable,
    /// TIDAL is rate-limiting: playback resumes in this many seconds.
    RateLimited(u64),
    /// Another program has the audio device; the play keeps trying.
    DeviceBusy,
    /// The audio engine failed, in its words.
    AudioError(String),
}

impl Playback {
    pub fn new(preferences: Preferences, rng: SmallRng) -> Self {
        Self {
            listening: Listening::default(),
            shuffle: preferences.shuffle,
            repeat: preferences.repeat,
            autoplay: preferences.autoplay,
            gapless: preferences.gapless,
            rng,
            status: Status::Stopped,
            allow_explicit: preferences.allow_explicit,
            failures: 0,
            rate_limited: None,
            radio: None,
            armed: None,
            position: 0.0,
            volume: preferences.volume.clamp(0.0, 1.0),
            pre_mute: 0.0,
            normalization: preferences.normalization,
            output: preferences.output,
            before_bit_perfect: preferences.before_bit_perfect,
            rollback: None,
            next_token: 0,
            next_play: 0,
            next_fill: 0,
            next_entry: 0,
            next_radio: 0,
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        if !self.allow_explicit && chooses_explicit(&message, &self.listening) {
            return Outcome::NeedsExplicitConsent(Pending(Box::new(message)));
        }
        let saves = changes_listening(&message);
        let mut effects = self.apply(message);
        effects.extend(self.prepare());
        if saves {
            effects.push(Effect::SaveSnapshot);
        }
        Outcome::Effects(effects)
    }

    fn apply(&mut self, message: Message) -> Vec<Effect> {
        if matches!(
            message,
            Message::Start(_)
                | Message::TogglePlay
                | Message::Next
                | Message::Previous
                | Message::Seek(_)
                | Message::Pick(_)
        ) {
            self.rate_limited = None;
            if let Some(fetch) = &mut self.radio {
                fetch.waiting = false;
            }
        }
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
                    play.round = None;
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
            // Bit-perfect output fixes the volume at full.
            Message::SetVolume(_) | Message::ToggleMute | Message::Normalization(_)
                if self.output.bit_perfect =>
            {
                vec![]
            }
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
                match result {
                    Ok(()) => {
                        self.rollback = None;
                        self.failures = 0;
                        self.status = Status::Playing;
                        vec![]
                    }
                    Err(e) => {
                        log::warn!("Could not play the track: {e}");
                        self.play_failed(token, e)
                    }
                }
            }
            Message::DeviceBusy(token) => {
                if self.status != Status::Loading(token) {
                    return vec![];
                }
                vec![Effect::Notify(Notice::DeviceBusy)]
            }
            Message::Resume(token) => {
                if self.rate_limited.take() != Some(token) {
                    return vec![];
                }
                self.snapshot();
                let from = (self.position > 0.0).then_some(self.position);
                self.play(from)
            }
            Message::TrackFinished => {
                if let Status::Loading(_) = self.status {
                    self.ended_while_loading(0.0);
                    return vec![];
                }
                if self.status != Status::Playing {
                    return vec![];
                }
                self.finished()
            }
            Message::TrackAdvanced(entry) => match self.status {
                // The choice loading now replaces what the engine moved to.
                Status::Loading(_) => {
                    self.ended_while_loading(0.0);
                    vec![]
                }
                Status::Stopped => {
                    log::warn!("The engine moved on to entry {entry:?} while stopped");
                    vec![Effect::Stop]
                }
                Status::Playing | Status::Paused => {
                    let armed = self.armed.take().filter(|armed| {
                        armed.entry == entry
                            && self.what_follows().is_some_and(|(upcoming, track)| {
                                upcoming == armed.upcoming && track.id == armed.track_id
                            })
                    });
                    if let Some(armed) = armed {
                        return self.advanced(armed);
                    }
                    // It took what was armed before the state moved on:
                    // play what comes next now, over it.
                    log::warn!("The engine moved on to entry {entry:?}, no longer next");
                    self.status = Status::Playing;
                    let mut effects = self.finished();
                    if !effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::Play { .. }))
                    {
                        effects.insert(0, Effect::Stop);
                    }
                    effects
                }
            },
            Message::EngineFailed(error) => {
                match self.status {
                    Status::Playing | Status::Paused => self.status = Status::Stopped,
                    Status::Loading(_) => {
                        let position = self.rollback.as_ref().map_or(0.0, |r| r.position);
                        self.ended_while_loading(position);
                    }
                    Status::Stopped => {}
                }
                vec![Effect::Notify(Notice::AudioError(error))]
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
            Message::AllowExplicit(allow) => {
                self.allow_explicit = allow;
                vec![]
            }
            Message::Autoplay(on) => {
                self.autoplay = on;
                vec![]
            }
            Message::Gapless(on) => {
                self.gapless = on;
                vec![Effect::SetGapless(on)]
            }
            Message::Normalization(on) => {
                self.normalization = on;
                vec![Effect::SetNormalization(on)]
            }
            Message::Exclusive { on, first_device } => {
                let mut effects = vec![];
                if on {
                    if self.output.device.is_none() {
                        self.output.device = first_device;
                    }
                } else if self.output.bit_perfect {
                    effects = self.bit_perfect_off();
                }
                self.output.exclusive = on;
                effects.push(Effect::SetOutput(self.output.clone()));
                effects
            }
            Message::OutputDevice(device) => {
                self.output.device = Some(device);
                vec![Effect::SetOutput(self.output.clone())]
            }
            Message::BitPerfect { on, first_device } => {
                let mut effects = if on && !self.output.bit_perfect {
                    self.bit_perfect_on(first_device)
                } else if !on && self.output.bit_perfect {
                    self.bit_perfect_off()
                } else {
                    vec![]
                };
                effects.push(Effect::SetOutput(self.output.clone()));
                effects
            }
            Message::RadioArrived(id, radio) => self.radio_arrived(id, radio),
            Message::WithoutExplicit(pending) => match *pending.0 {
                // Moving on skips them.
                message @ Message::Start(_) => self.apply(message),
                // Queued or picked, it would only be skipped.
                _ => vec![],
            },
            Message::Pick(entry) => self.pick(entry),
            Message::Remove(entry) => {
                self.edit(|listening| listening.remove(entry));
                vec![]
            }
            // Its place is where the drawer shows it, so what a play
            // loading now would roll back to, which lists that play's
            // track as upcoming, keeps its own order.
            Message::Move(entry, to) => {
                self.listening.shift(entry, to);
                vec![]
            }
            // Only what the drawer shows: a failed play brings back what
            // it would have played after.
            Message::Clear => {
                let filling = self.fill_id();
                self.listening.clear();
                match filling {
                    Some(_) => vec![Effect::CancelFill],
                    None => vec![],
                }
            }
        }
    }

    /// Whether what comes next is armed. The engine never advances with
    /// no gap to exclusive output, so nothing is armed for it.
    fn arms(&self) -> bool {
        self.gapless && !self.output.exclusive
    }

    /// Put the volume and normalization aside, and fix them at full and
    /// off. The output is the caller's to send.
    fn bit_perfect_on(&mut self, first_device: Option<String>) -> Vec<Effect> {
        self.before_bit_perfect = Some(Levels {
            volume: self.volume,
            normalization: self.normalization,
        });
        self.output.bit_perfect = true;
        self.output.exclusive = true;
        if self.output.device.is_none() {
            self.output.device = first_device;
        }
        self.slide_to(Levels {
            volume: 1.0,
            normalization: false,
        })
    }

    /// Bring back the volume and normalization from before bit-perfect.
    /// The output is the caller's to send.
    fn bit_perfect_off(&mut self) -> Vec<Effect> {
        self.output.bit_perfect = false;
        match self.before_bit_perfect.take() {
            Some(levels) => self.slide_to(levels),
            None => vec![],
        }
    }

    /// Slide the volume to `levels`' and switch normalization to match.
    fn slide_to(&mut self, levels: Levels) -> Vec<Effect> {
        let mut effects = vec![];
        if self.volume != levels.volume {
            effects.push(Effect::RampVolume {
                from: self.volume,
                to: levels.volume,
                over: RAMP,
            });
            self.volume = levels.volume;
        }
        if self.normalization != levels.normalization {
            self.normalization = levels.normalization;
            effects.push(Effect::SetNormalization(levels.normalization));
        }
        effects
    }

    /// Change what's upcoming, in what a play loading now would roll back
    /// to as well, so the change outlasts it failing.
    fn edit(&mut self, change: impl Fn(&mut Listening)) {
        change(&mut self.listening);
        if let Some(rollback) = &mut self.rollback {
            change(&mut rollback.listening);
        }
    }

    /// Play the drawer's row now, the current track going into History.
    /// Nothing for a row that's no longer listed.
    fn pick(&mut self, entry: Entry) -> Vec<Effect> {
        let Some(track) = self.listening.track(entry) else {
            return vec![];
        };
        if !track.available {
            return vec![Effect::Notify(Notice::Unavailable)];
        }
        self.failures = 0;
        self.snapshot();
        match entry {
            Entry::Played(id) => {
                let history = &self.listening.history;
                let played = history.iter().find(|item| item.id == id).cloned();
                // A play of its own: the cursor stays where it is.
                let again = Item {
                    id: self.stamp_entry(),
                    step: None,
                    ..played.expect("listed above")
                };
                self.retire_current();
                self.listening.current = Some(again);
            }
            Entry::Queued(id) => {
                let manual = &mut self.listening.manual;
                let at = manual.iter().position(|queued| queued.id == id);
                let queued = manual.remove(at.expect("listed above"));
                self.retire_current();
                self.listening.current = queued;
            }
            Entry::Upcoming(slot) => {
                if let Some(play) = &mut self.listening.source {
                    play.next = play.find(slot).expect("listed above");
                }
                self.step();
            }
        }
        self.play(None)
    }

    /// The play loading now failed. A track that can't play is skipped,
    /// and a rate limit waits and tries it again. Otherwise playback goes
    /// back to what it was before the choice: still playing if TIDAL had
    /// no stream, stopped if the engine had already let go of it.
    fn play_failed(&mut self, token: PlayToken, error: PlayError) -> Vec<Effect> {
        let Some(rollback) = self.rollback.take() else {
            self.status = Status::Stopped;
            return vec![];
        };
        let engine_busy = matches!(rollback.status, Status::Playing | Status::Paused);
        match &error {
            PlayError::Resolve(e) if e.is_terminal_unplayable() => {
                if let Some(effects) = self.count_failure() {
                    return effects;
                }
                // The track that failed never played: it leaves no History.
                self.rollback = Some(rollback);
                self.listening.current = None;
                if self.advance()
                    && let Some(effects) = self.play_or_skip()
                {
                    return effects;
                }
                self.failures = 0;
                let rollback = self.rollback.take().expect("put back above");
                let status = rollback.status;
                self.roll_back(rollback, status)
            }
            PlayError::Resolve(e) if e.is_rate_limited() => {
                // The track stays current, to play once the wait is over.
                let secs = e.retry_after_secs().unwrap_or(RATE_LIMIT_SECS) + 1;
                self.status = Status::Stopped;
                self.rate_limited = Some(token);
                let mut effects = if engine_busy {
                    vec![Effect::Stop]
                } else {
                    vec![]
                };
                effects.push(Effect::ResumeAfter {
                    token,
                    delay: Duration::from_secs(secs),
                });
                effects.push(Effect::Notify(Notice::RateLimited(secs)));
                effects
            }
            PlayError::Resolve(_) => {
                let status = rollback.status;
                self.roll_back(rollback, status)
            }
            PlayError::Audio(e) => {
                let mut effects = self.roll_back(rollback, Status::Stopped);
                effects.push(Effect::Notify(Notice::AudioError(e.to_string())));
                effects
            }
        }
    }

    /// Go back to what played before the choice, in `status`.
    fn roll_back(&mut self, rollback: Rollback, status: Status) -> Vec<Effect> {
        let filling = self.fill_id();
        self.listening = rollback.listening;
        self.position = rollback.position;
        self.status = status;
        self.refill(filling)
    }

    /// The track playing, or that would play.
    pub fn current(&self) -> Option<&Track> {
        self.listening.current.as_ref().map(|entry| &entry.track)
    }

    /// False only while a track from a Track radio Autoplay started plays.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "play reporting (ticket 34) sends it")
    )]
    pub fn chosen_by_user(&self) -> bool {
        self.listening
            .current
            .as_ref()
            .is_none_or(|entry| entry.chosen)
    }

    /// Where the current track comes from.
    pub fn playing_from(&self) -> Option<&Source> {
        self.listening
            .current
            .as_ref()
            .map(|entry| entry.from.as_ref())
    }

    /// The Manual queue, in the order it plays.
    pub fn queued(&self) -> impl Iterator<Item = (EntryId, &Track)> {
        self.listening
            .manual
            .iter()
            .map(|entry| (entry.id, &entry.track))
    }

    /// What's left of the source after the Manual queue, in order.
    pub fn upcoming(&self) -> impl Iterator<Item = (Slot, &Track)> {
        self.listening.source.iter().flat_map(|play| {
            play.order[play.next..].iter().map(|&index| {
                let slot = Slot {
                    play: play.play,
                    track: index,
                };
                (slot, &play.tracks[index])
            })
        })
    }

    /// The tracks already played, oldest first.
    pub fn history(&self) -> impl Iterator<Item = (EntryId, &Track)> {
        self.listening
            .history
            .iter()
            .map(|entry| (entry.id, &entry.track))
    }

    /// The Playback source, which stays while a queued entry plays.
    pub fn source(&self) -> Option<&Source> {
        let play = self.listening.source.as_ref()?;
        Some(&play.source)
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

    pub fn allow_explicit(&self) -> bool {
        self.allow_explicit
    }

    pub fn autoplay(&self) -> bool {
        self.autoplay
    }

    pub fn gapless(&self) -> bool {
        self.gapless
    }

    pub fn normalization(&self) -> bool {
        self.normalization
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    /// What bit-perfect output put aside.
    pub fn before_bit_perfect(&self) -> Option<Levels> {
        self.before_bit_perfect
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
        if let Start::Track(index) = request.start
            && !request.first_page[index].available
        {
            return vec![Effect::Notify(Notice::Unavailable)];
        }
        match request.start {
            // The chosen track plays first whatever Shuffle says.
            Start::Track(_) if shuffled => order[1..].shuffle(&mut self.rng),
            _ if shuffled => order.shuffle(&mut self.rng),
            _ => {}
        }
        self.failures = 0;
        let before = self.listening.clone();
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
            round: None,
            autoplay: false,
        });
        let Some(mut effects) = self.move_on(before) else {
            self.nothing_moved();
            return vec![];
        };
        effects.extend(self.refill(filling));
        effects
    }

    /// Next, chosen by the user: on even under Repeat one. Past the end it
    /// starts over under Repeat all, and otherwise stops.
    fn next(&mut self) -> Vec<Effect> {
        if self.listening.current.is_none() {
            return vec![];
        }
        self.failures = 0;
        let before = self.listening.clone();
        self.snapshot();
        if let Some(effects) = self.move_on(before) {
            return effects;
        }
        self.rollback = None;
        self.position = 0.0;
        let mut effects = match self.status {
            Status::Stopped => vec![],
            _ => {
                self.status = Status::Stopped;
                vec![Effect::Stop]
            }
        };
        effects.extend(self.await_radio());
        effects
    }

    /// The track into the Manual queue: at the front for Play next, else
    /// at the end. With nothing playing, it plays.
    fn enqueue(&mut self, track: Track, tag: Source, next: bool) -> Vec<Effect> {
        let entry = Item {
            id: self.stamp_entry(),
            track,
            from: Arc::new(tag),
            step: None,
            chosen: true,
        };
        if self.listening.current.is_none() {
            let before = self.listening.clone();
            self.snapshot();
            self.listening.current = Some(entry);
            return self.play_or_skip().unwrap_or_else(|| {
                self.listening = before;
                self.nothing_moved();
                vec![]
            });
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
        self.failures = 0;
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
    /// the head of the Manual queue, the next step of the play order, under
    /// Repeat all the first step of a new round, or with Autoplay the first
    /// track of the Track radio. False, with nothing changed, when nothing
    /// comes next.
    fn advance(&mut self) -> bool {
        if let Some(entry) = self.listening.manual.pop_front() {
            self.retire_current();
            self.listening.current = Some(entry);
            return true;
        }
        let source = self.listening.source.as_ref();
        if source.is_none_or(SourcePlay::ran_out) {
            if self.repeat == Repeat::All && source.is_some_and(|play| !play.tracks.is_empty()) {
                self.start_over();
            } else if !self.start_radio() {
                return false;
            }
        }
        self.step()
    }

    /// Put the current track in History and make the next step of the
    /// play order current. False, with nothing changed, without a source.
    fn step(&mut self) -> bool {
        if self.listening.source.is_none() {
            return false;
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
            chosen: !play.autoplay,
        });
        true
    }

    /// The track playing ended: replay it under Repeat one, or move on.
    /// With nothing to move on to, it stays current, stopped.
    fn finished(&mut self) -> Vec<Effect> {
        // The finished track is what a failed next one rolls back to.
        self.status = Status::Stopped;
        self.position = 0.0;
        let blocked = self.current().is_some_and(|track| self.blocked(track));
        if self.repeat == Repeat::One && !blocked {
            self.snapshot();
            return self.play(None);
        }
        let before = self.listening.clone();
        self.snapshot();
        self.move_on(before).unwrap_or_else(|| {
            self.rollback = None;
            self.await_radio()
        })
    }

    /// The engine moved on to what was armed: it's current and playing.
    fn advanced(&mut self, armed: Armed) -> Vec<Effect> {
        if armed.upcoming != Upcoming::Again {
            self.advance();
            if let Some(entry) = &mut self.listening.current {
                entry.id = armed.entry;
            }
        }
        self.status = Status::Playing;
        self.position = 0.0;
        self.rollback = None;
        self.failures = 0;
        vec![]
    }

    /// Whether Autoplay may follow the end of the source: it's on, Repeat
    /// is off, and no fill is still reading the source.
    fn autoplays(&self) -> bool {
        self.autoplay
            && self.repeat == Repeat::Off
            && self
                .listening
                .source
                .as_ref()
                .is_none_or(|play| play.fill.is_none())
    }

    /// Autoplay: the Track radio fetched for the last track to play becomes
    /// the Playback source. False when there's none.
    fn start_radio(&mut self) -> bool {
        if !self.autoplays() {
            return false;
        }
        let listening = &self.listening;
        let Some(last) = listening.current.as_ref().or(listening.history.back()) else {
            return false;
        };
        let last = last.id;
        let Some(fetch) = self.radio.as_mut().filter(|fetch| fetch.after == last) else {
            return false;
        };
        // Started once: a rollback past it doesn't bring it back, so moving
        // on again stops.
        let radio = match std::mem::replace(&mut fetch.state, RadioState::Spent) {
            RadioState::Ready(radio) => radio,
            state => {
                fetch.state = state;
                return false;
            }
        };
        let play = self.stamp_play();
        self.listening.source = Some(SourcePlay {
            play,
            source: Arc::new(radio.source),
            order: (0..radio.tracks.len()).collect(),
            tracks: Arc::new(radio.tracks),
            next: 0,
            shuffled: false,
            fill: None,
            round: None,
            autoplay: true,
        });
        true
    }

    /// Nothing is left to play. With Autoplay, wait for the current track's
    /// Track radio, asking for it if nothing has.
    fn await_radio(&mut self) -> Vec<Effect> {
        if !self.autoplays() {
            return vec![];
        }
        let Some(current) = &self.listening.current else {
            return vec![];
        };
        if let Some(fetch) = self
            .radio
            .as_mut()
            .filter(|fetch| fetch.after == current.id)
        {
            // A radio that's ready but didn't start had nothing to play.
            fetch.waiting = matches!(fetch.state, RadioState::Fetching);
            return vec![];
        }
        let (after, track) = (current.id, current.track.clone());
        vec![self.fetch_radio(after, track, true)]
    }

    /// Fetch `track`'s Track radio, for the entry `after`, replacing any
    /// other fetch.
    fn fetch_radio(&mut self, after: EntryId, track: Track, waiting: bool) -> Effect {
        self.next_radio += 1;
        let id = RadioId(self.next_radio);
        self.radio = Some(RadioFetch {
            id,
            after,
            state: RadioState::Fetching,
            waiting,
        });
        Effect::FetchTrackRadio { radio: id, track }
    }

    /// Keep what the radio has that hasn't played, and play it if playback
    /// was waiting for it.
    fn radio_arrived(&mut self, id: RadioId, radio: Option<Radio>) -> Vec<Effect> {
        let listening = &self.listening;
        let played: HashSet<u64> = listening
            .current
            .iter()
            .chain(&listening.history)
            .map(|entry| entry.track.id)
            .collect();
        let Some(fetch) = self.radio.as_mut().filter(|fetch| fetch.id == id) else {
            return vec![];
        };
        fetch.state = match radio {
            Some(mut radio) => {
                radio.tracks.retain(|track| !played.contains(&track.id));
                if radio.tracks.is_empty() {
                    RadioState::Spent
                } else {
                    RadioState::Ready(radio)
                }
            }
            None => RadioState::Spent,
        };
        if !std::mem::take(&mut fetch.waiting) {
            return vec![];
        }
        let before = self.listening.clone();
        self.snapshot();
        self.move_on(before).unwrap_or_else(|| {
            self.rollback = None;
            vec![]
        })
    }

    /// What moving on would make current, unless it would be skipped: what
    /// can be armed.
    fn what_follows(&self) -> Option<(Upcoming, &Track)> {
        let listening = &self.listening;
        let current = listening.current.as_ref()?;
        let source = listening.source.as_ref();
        let (upcoming, track) = if self.repeat == Repeat::One && !self.blocked(&current.track) {
            (Upcoming::Again, &current.track)
        } else if let Some(entry) = listening.manual.front() {
            (Upcoming::Queued(entry.id), &entry.track)
        } else if let Some(play) = source.filter(|play| !play.ran_out()) {
            let step = Step {
                play: play.play,
                index: play.next,
            };
            (Upcoming::Step(step), &play.tracks[play.order[play.next]])
        } else if self.repeat == Repeat::All
            && let Some(play) = source
            && !play.tracks.is_empty()
        {
            let round = play.round.as_ref()?;
            (Upcoming::NewRound(play.play), &play.tracks[round[0]])
        } else if self.autoplays()
            && let Some(fetch) = &self.radio
            && fetch.after == current.id
            && let RadioState::Ready(radio) = &fetch.state
        {
            (Upcoming::Radio(fetch.id), &radio.tracks[0])
        } else {
            return None;
        };
        (track.available && !self.blocked(track)).then_some((upcoming, track))
    }

    /// Ahead of what comes next: fetch the Track radio once the last track
    /// plays, and arm what follows the current track, or clear what no
    /// longer does. The engine holds an armed track only while it plays or
    /// is paused: it lets go of it when it stops, ends a track or starts
    /// another.
    fn prepare(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if !matches!(self.status, Status::Playing | Status::Paused) {
            self.armed = None;
            return effects;
        }
        let listening = &self.listening;
        // Nothing left that will play: the rest would all be skipped.
        let last = listening
            .manual
            .iter()
            .map(|entry| &entry.track)
            .chain(listening.source.iter().flat_map(SourcePlay::rest))
            .all(|track| !track.available || self.blocked(track));
        if self.autoplays()
            && last
            && let Some(current) = &listening.current
            && self
                .radio
                .as_ref()
                .is_none_or(|fetch| fetch.after != current.id)
        {
            let (after, track) = (current.id, current.track.clone());
            effects.push(self.fetch_radio(after, track, false));
        }
        if self.repeat == Repeat::All
            && let Some(play) = &mut self.listening.source
            && play.ran_out()
            && play.round.is_none()
        {
            play.round = Some(round_order(play.tracks.len(), self.shuffle, &mut self.rng));
        }
        let next = self
            .what_follows()
            .filter(|_| self.arms())
            .map(|(upcoming, track)| (upcoming, track.id));
        match (next, &self.armed) {
            (Some((upcoming, track_id)), Some(armed))
                if armed.upcoming == upcoming && armed.track_id == track_id => {}
            (Some((upcoming, track_id)), _) => {
                let entry = match upcoming {
                    Upcoming::Queued(entry) => entry,
                    _ => self.stamp_entry(),
                };
                let album_gain = self.album_gain_of(upcoming);
                self.armed = Some(Armed {
                    upcoming,
                    entry,
                    track_id,
                });
                effects.push(Effect::ArmNext {
                    entry,
                    track_id,
                    album_gain,
                });
            }
            (None, Some(_)) => {
                self.armed = None;
                effects.push(Effect::ClearNext);
            }
            (None, None) => {}
        }
        effects
    }

    /// Whether `upcoming` gets album gain once it plays, as
    /// [`Self::album_gain`] will say then.
    fn album_gain_of(&self, upcoming: Upcoming) -> bool {
        let album = |shuffled: bool| {
            self.listening
                .source
                .as_ref()
                .is_some_and(|play| matches!(play.source.kind, SourceRef::Album(_)) && !shuffled)
        };
        let source = self.listening.source.as_ref();
        match upcoming {
            Upcoming::Again => self.album_gain(),
            Upcoming::Queued(_) | Upcoming::Radio(_) => false,
            Upcoming::Step(_) => album(source.is_some_and(|play| play.shuffled)),
            Upcoming::NewRound(_) => album(self.shuffle),
        }
    }

    /// An explicit track while they aren't allowed.
    fn blocked(&self, track: &Track) -> bool {
        track.explicit && !self.allow_explicit
    }

    /// Move on, and play what comes next that can. When nothing can, the
    /// state goes back to `before` and it's None.
    fn move_on(&mut self, before: Listening) -> Option<Vec<Effect>> {
        if self.advance()
            && let Some(effects) = self.play_or_skip()
        {
            return Some(effects);
        }
        self.failures = 0;
        self.listening = before;
        None
    }

    /// Play the current track, which playback moved on to, or skip past it
    /// while it can't play: silently if it's explicit and they aren't
    /// allowed, counting toward [`MAX_FAILURES`] if it's unavailable. None,
    /// with the state part changed, when nothing after it can play.
    fn play_or_skip(&mut self) -> Option<Vec<Effect>> {
        loop {
            let track = &self.listening.current.as_ref()?.track;
            let blocked = self.blocked(track);
            if !blocked && track.available {
                return Some(self.play(None));
            }
            if !blocked && let Some(effects) = self.count_failure() {
                return Some(effects);
            }
            // Skipped, it never played: it leaves no History.
            self.listening.current = None;
            if !self.advance() {
                self.failures = 0;
                return None;
            }
        }
    }

    /// The current track couldn't play: one more in a row. At
    /// [`MAX_FAILURES`] playback stops on it, and the user is told.
    fn count_failure(&mut self) -> Option<Vec<Effect>> {
        self.failures += 1;
        if self.failures < MAX_FAILURES {
            return None;
        }
        self.failures = 0;
        self.rollback = None;
        self.position = 0.0;
        let mut effects = match self.status {
            Status::Stopped => vec![],
            _ => vec![Effect::Stop],
        };
        self.status = Status::Stopped;
        effects.push(Effect::Notify(Notice::TooManyFailures));
        Some(effects)
    }

    /// Nothing could play, so the choice changed nothing: no rollback, unless
    /// an earlier choice is still loading.
    fn nothing_moved(&mut self) {
        if !matches!(self.status, Status::Loading(_)) {
            self.rollback = None;
        }
    }

    /// Repeat all: a new round of the whole source, reshuffled if Shuffle is
    /// on and in source order if not.
    fn start_over(&mut self) {
        let round = self.stamp_play();
        let Some(play) = &mut self.listening.source else {
            return;
        };
        play.play = round;
        play.order = match play.round.take() {
            Some(order) if order.len() == play.tracks.len() => order,
            _ => round_order(play.tracks.len(), self.shuffle, &mut self.rng),
        };
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
