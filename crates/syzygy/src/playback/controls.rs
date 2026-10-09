//! The desktop's media controls (MPRIS): what they show of playback, and
//! what they ask of it.

use super::{Effect, Message, Playback, Repeat, Status, mpris};

/// The cover size the desktop gets, as sone sent.
const COVER_SIZE: u32 = 320;

impl From<Repeat> for mpris::Repeat {
    fn from(repeat: Repeat) -> Self {
        match repeat {
            Repeat::Off => mpris::Repeat::Off,
            Repeat::All => mpris::Repeat::All,
            Repeat::One => mpris::Repeat::One,
        }
    }
}

impl Playback {
    /// What the desktop's media controls were last told.
    pub fn mpris_view(&self) -> &mpris::View {
        &self.mpris
    }

    /// What the desktop's media controls should show now.
    pub(super) fn mpris_now(&self) -> mpris::View {
        let track = self.current().map(|track| mpris::Track {
            id: track.id,
            title: track.title.clone(),
            artists: track.artists.iter().map(|a| a.name.clone()).collect(),
            album: track.album.as_ref().map(|album| album.title.clone()),
            art_url: track
                .album
                .as_ref()
                .and_then(|album| album.cover.as_ref())
                .map(|cover| cover.url(COVER_SIZE)),
            length: track.duration,
        });
        mpris::View {
            track,
            status: match self.status {
                Status::Stopped => mpris::Status::Stopped,
                // The track shows as playing while it starts, as the player
                // bar does.
                Status::Loading(_) | Status::Playing => mpris::Status::Playing,
                Status::Paused => mpris::Status::Paused,
            },
            shuffle: self.shuffle,
            repeat: self.repeat.into(),
            volume: self.volume,
        }
    }

    /// Tell the desktop what changed since it was last told, if anything.
    pub(super) fn announce(&mut self) -> Option<Effect> {
        let now = self.mpris_now();
        let diff = self.mpris.diff(&now)?;
        self.mpris = now;
        Some(Effect::Mpris(diff))
    }

    /// What the in-app buttons would send to do what the desktop asks.
    /// Nothing for what isn't playback's (Raise, Quit), or what wouldn't
    /// change anything. A seek moves from the last position read, so the
    /// caller reads the engine's first.
    pub fn answer_mpris(&self, event: mpris::Event) -> Vec<Message> {
        match event {
            mpris::Event::Play => match self.status {
                Status::Paused | Status::Stopped => vec![Message::TogglePlay],
                Status::Playing | Status::Loading(_) => vec![],
            },
            mpris::Event::Pause => match self.status {
                Status::Playing => vec![Message::TogglePlay],
                _ => vec![],
            },
            mpris::Event::PlayPause => vec![Message::TogglePlay],
            mpris::Event::Stop => vec![Message::Stop],
            mpris::Event::Next => vec![Message::Next],
            mpris::Event::Previous => vec![Message::Previous],
            mpris::Event::Seek(offset) => {
                let Some(track) = self.current() else {
                    return vec![];
                };
                let to = self.position + offset;
                // Past the end is the next track, as MPRIS has it.
                if to > track.duration as f32 {
                    vec![Message::Next]
                } else {
                    vec![Message::Seek(to.max(0.0))]
                }
            }
            mpris::Event::SetPosition { track_id, position } => match self.current() {
                // MPRIS ignores a position past the track or for another.
                Some(track)
                    if track.id == track_id
                        && (0.0..=track.duration as f32).contains(&position) =>
                {
                    vec![Message::Seek(position)]
                }
                _ => vec![],
            },
            mpris::Event::SetVolume(volume) => vec![Message::SetVolume(volume)],
            mpris::Event::SetShuffle(on) if on != self.shuffle => vec![Message::ToggleShuffle],
            mpris::Event::SetShuffle(_) => vec![],
            mpris::Event::SetRepeat(to) => {
                // Repeat only cycles, off, all, one: as many steps as get
                // there.
                let step = |repeat| match repeat {
                    mpris::Repeat::Off => 0,
                    mpris::Repeat::All => 1,
                    mpris::Repeat::One => 2,
                };
                let steps = (3 + step(to) - step(self.repeat.into())) % 3;
                vec![Message::CycleRepeat; steps]
            }
            mpris::Event::Raise | mpris::Event::Quit => vec![],
        }
    }
}
