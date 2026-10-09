//! Where listening is, as `queue.json` keeps it between launches: the
//! Playback source with its play order and unread rest, the Manual queue,
//! History, the current track and the position. Shuffle, Repeat mode and the
//! other preferences live in `Settings`.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use syzygy_catalog::Track;

use super::{Continuation, Source};
use super::{Effect, Fill, Item, Listening, Playback, SourcePlay, Status, Step};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub(super) source: Option<SavedSource>,
    pub(super) manual: Vec<SavedEntry>,
    pub(super) current: Option<SavedEntry>,
    /// Oldest first.
    pub(super) history: Vec<SavedEntry>,
    /// Seconds into the current track.
    pub(super) position: f32,
}

/// The Playback source and where playback was in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedSource {
    play: u64,
    source: Source,
    /// In source order.
    tracks: Vec<Track>,
    /// Indices into `tracks`, in the order they play.
    pub(super) order: Vec<usize>,
    next: usize,
    shuffled: bool,
    /// Where the fill had got to, when it hadn't read the whole source.
    unread: Option<Continuation>,
    autoplay: bool,
}

/// A Queue entry. Entry ids aren't kept: each launch stamps its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedEntry {
    track: Track,
    from: Source,
    /// Its step, for one that came from a source. Steps keep their play,
    /// so Previous and album gain still tell a step of the source's play
    /// from one of an older play.
    step: Option<Step>,
    chosen: bool,
}

impl Playback {
    /// Where listening is now, to save.
    pub fn to_snapshot(&self) -> Snapshot {
        let listening = &self.listening;
        let save = |entry: &Item| SavedEntry {
            track: entry.track.clone(),
            from: (*entry.from).clone(),
            step: entry.step,
            chosen: entry.chosen,
        };
        Snapshot {
            source: listening.source.as_ref().map(|play| SavedSource {
                play: play.play,
                source: (*play.source).clone(),
                tracks: play.tracks.to_vec(),
                order: play.order.clone(),
                next: play.next,
                shuffled: play.shuffled,
                unread: play.fill.as_ref().map(|fill| fill.continuation.clone()),
                autoplay: play.autoplay,
            }),
            manual: listening.manual.iter().map(save).collect(),
            current: listening.current.as_ref().map(save),
            history: listening.history.iter().map(save).collect(),
            position: self.position,
        }
    }

    /// Pick up where a saved snapshot left off, stopped: play goes on from
    /// the saved position. A fill that hadn't finished starts again. A
    /// source whose play order doesn't fit its tracks is dropped.
    pub fn restore(&mut self, snapshot: Snapshot) -> Vec<Effect> {
        let source = snapshot.source.filter(|saved| {
            let fits = saved.next <= saved.order.len()
                && saved.order.iter().all(|&index| index < saved.tracks.len());
            if !fits {
                log::warn!("The saved play order doesn't fit its source, leaving it out");
            }
            fits
        });
        // New plays are stamped past every saved one.
        let plays = source.iter().map(|saved| saved.play);
        let steps = snapshot
            .manual
            .iter()
            .chain(&snapshot.current)
            .chain(&snapshot.history)
            .filter_map(|entry| entry.step.map(|step| step.play));
        self.next_play = plays.chain(steps).max().unwrap_or(0);

        let mut effects = Vec::new();
        let source = source.map(|saved| {
            let fill = saved.unread.map(|continuation| {
                let id = self.stamp_fill();
                effects.push(Effect::StartFill {
                    fill_id: id,
                    continuation: continuation.clone(),
                });
                Fill { id, continuation }
            });
            SourcePlay {
                play: saved.play,
                source: Arc::new(saved.source),
                tracks: Arc::new(saved.tracks),
                order: saved.order,
                next: saved.next,
                shuffled: saved.shuffled,
                fill,
                round: None,
                autoplay: saved.autoplay,
            }
        });
        let mut restore = |saved: SavedEntry| Item {
            id: self.stamp_entry(),
            track: saved.track,
            from: Arc::new(saved.from),
            step: saved.step,
            chosen: saved.chosen,
        };
        self.listening = Listening {
            source,
            manual: snapshot.manual.into_iter().map(&mut restore).collect(),
            current: snapshot.current.map(&mut restore),
            history: snapshot.history.into_iter().map(&mut restore).collect(),
        };
        self.status = Status::Stopped;
        self.position = match &self.listening.current {
            Some(_) if snapshot.position.is_finite() => snapshot.position.max(0.0),
            _ => 0.0,
        };
        effects
    }
}
