//! syzygy's MPRIS server, from sone: the desktop's media controls show
//! what [`View`] says, and what the user does there comes back as
//! [`Event`]s on the sender [`Mpris::start`] was given. Nothing here acts
//! on playback itself; the app turns each event into the message its own
//! button sends.
//!
//! The server runs on a thread of its own, so a slow or missing session
//! bus never holds up the app. Without one, [`Mpris`] calls do nothing.

use mpris_server::zbus::fdo;
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Signal, Time, TrackId, Volume,
};
use std::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Who the app is on the bus. The binary owns these; this crate names no
/// app.
#[derive(Debug, Clone)]
pub struct Identity {
    /// The bus name is `org.mpris.MediaPlayer2.<bus_name>`.
    pub bus_name: String,
    /// What the desktop calls the player.
    pub identity: String,
    /// The desktop file's name, without `.desktop`, for its icon and window.
    pub desktop_entry: String,
}

/// What the user did in the desktop's media controls.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    /// Move this many seconds, back when negative.
    Seek(f32),
    /// Go to `position` seconds into the track with this id. A client that
    /// still thinks another track plays sends someone else's id.
    SetPosition {
        track_id: u64,
        position: f32,
    },
    /// From 0 to 1, or past it as the client sends it.
    SetVolume(f32),
    SetShuffle(bool),
    SetRepeat(Repeat),
    /// Bring the window to the front.
    Raise,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// MPRIS's loop status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    /// The Playback source again: MPRIS's `Playlist`.
    All,
    /// The track again: MPRIS's `Track`.
    One,
}

/// The track the desktop shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// The cover's URL.
    pub art_url: Option<String>,
    /// In seconds.
    pub length: u32,
}

/// Everything the desktop shows but the position, which it reads when it
/// asks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub track: Option<Track>,
    pub status: Status,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// From 0 to 1.
    pub volume: f32,
}

/// What changed from one [`View`] to the next: `None` for what didn't.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diff {
    pub track: Option<Option<Track>>,
    pub status: Option<Status>,
    pub shuffle: Option<bool>,
    pub repeat: Option<Repeat>,
    pub volume: Option<f32>,
}

impl View {
    /// What changes from `self` to `next`, or `None` when nothing does.
    pub fn diff(&self, next: &View) -> Option<Diff> {
        fn changed<T: PartialEq + Clone>(from: &T, to: &T) -> Option<T> {
            (from != to).then(|| to.clone())
        }
        let diff = Diff {
            track: changed(&self.track, &next.track),
            status: changed(&self.status, &next.status),
            shuffle: changed(&self.shuffle, &next.shuffle),
            repeat: changed(&self.repeat, &next.repeat),
            volume: changed(&self.volume, &next.volume),
        };
        (diff != Diff::default()).then_some(diff)
    }

    fn apply(&mut self, diff: &Diff) {
        if let Some(track) = &diff.track {
            self.track = track.clone();
        }
        if let Some(status) = diff.status {
            self.status = status;
        }
        if let Some(shuffle) = diff.shuffle {
            self.shuffle = shuffle;
        }
        if let Some(repeat) = diff.repeat {
            self.repeat = repeat;
        }
        if let Some(volume) = diff.volume {
            self.volume = volume;
        }
    }
}

enum Command {
    Update(Diff),
    Seeked(f32),
}

/// A cheap `Clone` handle to the server.
#[derive(Clone)]
pub struct Mpris {
    commands: UnboundedSender<Command>,
}

impl Mpris {
    /// Start the server on a thread of its own, showing `view` until told
    /// otherwise. It reads the position from `position`, in seconds, each
    /// time a client asks.
    pub fn start(
        identity: Identity,
        view: View,
        position: impl Fn() -> f32 + Send + Sync + 'static,
        events: UnboundedSender<Event>,
    ) -> Self {
        let (commands, receiver) = unbounded_channel();
        let player = Player {
            identity: identity.identity,
            desktop_entry: identity.desktop_entry,
            view: Mutex::new(view),
            position: Box::new(position),
            events,
        };
        let spawned = std::thread::Builder::new()
            .name("mpris".to_string())
            .spawn(move || serve(identity.bus_name, player, receiver));
        if let Err(e) = spawned {
            log::error!("Could not start the MPRIS thread: {e}");
        }
        Self { commands }
    }

    /// Show what changed.
    pub fn update(&self, diff: Diff) {
        let _ = self.commands.send(Command::Update(diff));
    }

    /// The track jumped to `position` seconds, not where it was heading.
    pub fn seeked(&self, position: f32) {
        let _ = self.commands.send(Command::Seeked(position));
    }
}

fn serve(bus_name: String, player: Player, mut commands: UnboundedReceiver<Command>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            log::error!("Could not build the MPRIS runtime: {e}");
            return;
        }
    };
    runtime.block_on(async move {
        let server = match Server::new(&bus_name, player).await {
            Ok(server) => server,
            Err(e) => {
                log::error!("Could not start the MPRIS server: {e}");
                return;
            }
        };
        log::info!("MPRIS server up as org.mpris.MediaPlayer2.{bus_name}");
        while let Some(command) = commands.recv().await {
            let sent = match command {
                Command::Update(diff) => {
                    server.imp().apply(&diff);
                    server.properties_changed(properties(diff)).await
                }
                Command::Seeked(position) => {
                    let position = time(position);
                    server.emit(Signal::Seeked { position }).await
                }
            };
            if let Err(e) = sent {
                log::warn!("Could not tell MPRIS clients: {e}");
            }
        }
    });
}

/// The properties a diff changes.
fn properties(diff: Diff) -> Vec<Property> {
    let mut properties = vec![];
    if let Some(track) = diff.track {
        properties.push(Property::Metadata(metadata(track.as_ref())));
    }
    if let Some(status) = diff.status {
        properties.push(Property::PlaybackStatus(playback_status(status)));
    }
    if let Some(shuffle) = diff.shuffle {
        properties.push(Property::Shuffle(shuffle));
    }
    if let Some(repeat) = diff.repeat {
        properties.push(Property::LoopStatus(loop_status(repeat)));
    }
    if let Some(volume) = diff.volume {
        properties.push(Property::Volume(volume as Volume));
    }
    properties
}

fn time(secs: f32) -> Time {
    Time::from_micros((f64::from(secs) * 1_000_000.0) as i64)
}

fn secs(time: Time) -> f32 {
    time.as_micros() as f32 / 1_000_000.0
}

const TRACK_PATH: &str = "/org/mpris/MediaPlayer2/Track/";

fn track_id(id: u64) -> Option<TrackId> {
    TrackId::try_from(format!("{TRACK_PATH}{id}")).ok()
}

fn metadata(track: Option<&Track>) -> Metadata {
    let mut metadata = Metadata::new();
    let Some(track) = track else {
        metadata.set_trackid(Some(TrackId::NO_TRACK));
        return metadata;
    };
    metadata.set_trackid(track_id(track.id));
    metadata.set_title(Some(&track.title));
    metadata.set_artist(Some(&track.artists));
    metadata.set_album(track.album.as_ref());
    metadata.set_art_url(track.art_url.as_ref());
    metadata.set_length(Some(Time::from_secs(i64::from(track.length))));
    metadata
}

fn playback_status(status: Status) -> PlaybackStatus {
    match status {
        Status::Stopped => PlaybackStatus::Stopped,
        Status::Playing => PlaybackStatus::Playing,
        Status::Paused => PlaybackStatus::Paused,
    }
}

fn loop_status(repeat: Repeat) -> LoopStatus {
    match repeat {
        Repeat::Off => LoopStatus::None,
        Repeat::All => LoopStatus::Playlist,
        Repeat::One => LoopStatus::Track,
    }
}

/// The D-Bus side: properties from the last view, methods as events.
struct Player {
    identity: String,
    desktop_entry: String,
    view: Mutex<View>,
    position: Box<dyn Fn() -> f32 + Send + Sync>,
    events: UnboundedSender<Event>,
}

impl Player {
    fn view(&self) -> std::sync::MutexGuard<'_, View> {
        self.view.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn apply(&self, diff: &Diff) {
        self.view().apply(diff);
    }

    fn send(&self, event: Event) -> fdo::Result<()> {
        // The app is quitting: nothing left to act on it.
        let _ = self.events.send(event);
        Ok(())
    }
}

/// Setters take what a client sends and pass it on; the property changes
/// once the app's view does.
impl RootInterface for Player {
    async fn raise(&self) -> fdo::Result<()> {
        self.send(Event::Raise)
    }

    async fn quit(&self) -> fdo::Result<()> {
        self.send(Event::Quit)
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok(self.identity.clone())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok(self.desktop_entry.clone())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl PlayerInterface for Player {
    async fn next(&self) -> fdo::Result<()> {
        self.send(Event::Next)
    }

    async fn previous(&self) -> fdo::Result<()> {
        self.send(Event::Previous)
    }

    async fn pause(&self) -> fdo::Result<()> {
        self.send(Event::Pause)
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        self.send(Event::PlayPause)
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.send(Event::Stop)
    }

    async fn play(&self) -> fdo::Result<()> {
        self.send(Event::Play)
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        self.send(Event::Seek(secs(offset)))
    }

    async fn set_position(&self, track_id: TrackId, position: Time) -> fdo::Result<()> {
        let id = track_id
            .as_str()
            .strip_prefix(TRACK_PATH)
            .and_then(|id| id.parse().ok());
        match id {
            Some(track_id) => self.send(Event::SetPosition {
                track_id,
                position: secs(position),
            }),
            // Not a track of ours: the spec says to ignore it.
            None => Ok(()),
        }
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported(
            "Opening URIs isn't supported".into(),
        ))
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(playback_status(self.view().status))
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(loop_status(self.view().repeat))
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> mpris_server::zbus::Result<()> {
        let repeat = match loop_status {
            LoopStatus::None => Repeat::Off,
            LoopStatus::Playlist => Repeat::All,
            LoopStatus::Track => Repeat::One,
        };
        let _ = self.events.send(Event::SetRepeat(repeat));
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    // Always 1×: the minimum and maximum say so.
    async fn set_rate(&self, _rate: PlaybackRate) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.view().shuffle)
    }

    async fn set_shuffle(&self, shuffle: bool) -> mpris_server::zbus::Result<()> {
        let _ = self.events.send(Event::SetShuffle(shuffle));
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(metadata(self.view().track.as_ref()))
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.view().volume as Volume)
    }

    async fn set_volume(&self, volume: Volume) -> mpris_server::zbus::Result<()> {
        let _ = self.events.send(Event::SetVolume(volume as f32));
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        Ok(time((self.position)()))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_view_has_no_diff() {
        let view = View {
            volume: 0.5,
            ..View::default()
        };
        assert_eq!(view.diff(&view.clone()), None);
    }

    #[test]
    fn a_diff_holds_only_what_changed_and_applies_back() {
        let from = View::default();
        let to = View {
            status: Status::Playing,
            volume: 0.3,
            ..View::default()
        };
        let diff = from.diff(&to).expect("changed");
        assert_eq!(
            diff,
            Diff {
                status: Some(Status::Playing),
                volume: Some(0.3),
                ..Diff::default()
            }
        );
        let mut applied = from;
        applied.apply(&diff);
        assert_eq!(applied, to);
    }

    #[test]
    fn track_ids_round_trip_through_their_object_path() {
        let id = track_id(1234).expect("a valid path");
        assert_eq!(id.as_str(), "/org/mpris/MediaPlayer2/Track/1234");
    }
}
