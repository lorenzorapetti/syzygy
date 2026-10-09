//! What happens to the track playing, for the play reporter: derived after
//! each update by comparing playback with what was last reported, as the
//! desktop's media controls are.

use super::{EntryId, Item, Playback, SourceRef, Status, report};

/// The track last reported playing or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Reported {
    entry: EntryId,
    playing: bool,
}

impl From<&SourceRef> for report::Source {
    fn from(source: &SourceRef) -> Self {
        match source {
            SourceRef::Album(id) => report::Source::Album(*id),
            SourceRef::Playlist { uuid, .. } => report::Source::Playlist(uuid.clone()),
            SourceRef::Mix(id) | SourceRef::TrackRadio(id) => report::Source::Mix(id.clone()),
            SourceRef::Artist(id) => report::Source::Artist(*id),
            SourceRef::LovedTracks(_) => report::Source::LovedTracks,
            SourceRef::Search(_) | SourceRef::Track(_) => report::Source::Item,
        }
    }
}

impl Item {
    /// The play of this entry, under its Source tag for a queued one.
    fn report(&self) -> report::Play {
        report::Play {
            track_id: self.track.id,
            duration: self.track.duration,
            source: (&self.from.kind).into(),
            chosen_by_user: self.chosen,
        }
    }
}

impl Playback {
    /// What happened to the track playing since it was last reported.
    /// `ended` when the engine said the track playing reached its end, by
    /// itself or with no gap to the next: whatever plays after, even the
    /// same entry again, is a new play.
    pub(super) fn report(&mut self, ended: bool) -> Vec<report::Event> {
        let now = self.reporting_now();
        let mut was = self.reported.take();
        self.reported = now;
        let mut events = vec![];
        if ended && was.take().is_some() {
            events.push(report::Event::Finished);
        }
        match (was, now) {
            (Some(was), Some(now)) if was.entry == now.entry => {
                if was.playing != now.playing {
                    events.push(match now.playing {
                        true => report::Event::Resumed,
                        false => report::Event::Paused,
                    });
                }
            }
            (was, now) => {
                if was.is_some() {
                    events.push(report::Event::Stopped);
                }
                if let Some(now) = now {
                    let current = self.listening.current.as_ref().expect("reported");
                    events.push(report::Event::Started(current.report()));
                    if !now.playing {
                        events.push(report::Event::Paused);
                    }
                }
            }
        }
        events
    }

    /// The track playing or paused now. Not one loading: it hasn't started.
    fn reporting_now(&self) -> Option<Reported> {
        let playing = match self.status {
            Status::Playing => true,
            Status::Paused => false,
            Status::Stopped | Status::Loading(_) => return None,
        };
        let current = self.listening.current.as_ref()?;
        Some(Reported {
            entry: current.id,
            playing,
        })
    }
}
