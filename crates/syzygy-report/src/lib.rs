//! Reports plays to TIDAL's Event Producer so they show in the user's
//! Recently Played, from sone. One `playback_session` event goes out per
//! play over 30 seconds. It's a private, undocumented endpoint, so this is
//! best-effort.
//!
//! The app tells the [`Reporter`] what happens to the track playing
//! ([`Event`]); sending never waits. Each event is stamped as it's sent,
//! so time is counted right however busy the reporter is. A worker owns
//! everything else: the play being listened to, and the outbox of reports
//! TIDAL hasn't taken yet, saved encrypted at the path the app gives, so
//! reports survive restarts. It sends at start, as each play is reported,
//! and every [`RETRY_EVERY`] while any wait.

mod event;
mod outbox;
mod play;

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use syzygy_store::Store;
use syzygy_tidal::TidalClient;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use event::SendOutcome;
use outbox::{Batch, Outbox};
use play::Listening;

/// How long reports wait, after a send fails, before the next try.
pub const RETRY_EVERY: Duration = Duration::from_secs(10 * 60);
/// How long one send may take.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// How many served streams are remembered for tracks that haven't started.
const META_KEPT: usize = 16;

/// What happened to the track playing.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A track started playing. What played before ended with
    /// [`Event::Finished`] or [`Event::Stopped`], or is stopped now.
    Started(Play),
    Paused,
    Resumed,
    /// The track played to its end, or advanced to the next with no gap.
    Finished,
    /// The track stopped before its end: skipped, replaced or stopped.
    Stopped,
}

/// A track that started playing.
#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    pub track_id: u64,
    /// In seconds.
    pub duration: u32,
    /// What it plays from.
    pub source: Source,
    /// False for a track the user didn't choose: one from a Track radio
    /// Autoplay started. Logged with the report; TIDAL's event has no field
    /// for it.
    pub chosen_by_user: bool,
}

/// What a play is attributed to in Recently Played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Album(u64),
    /// By its uuid.
    Playlist(String),
    /// A mix, or a track's mix (its Track radio), by the mix's id.
    Mix(String),
    Artist(u64),
    LovedTracks,
    /// A track played on its own or from search results, attributed to
    /// itself.
    Item,
}

/// What TIDAL actually served for a track. What's missing is reported as
/// lossless stereo, in full.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamMeta {
    /// The track served, when TIDAL served another.
    pub actual_product_id: Option<u64>,
    pub quality: Option<String>,
    pub audio_mode: Option<String>,
    pub presentation: Option<String>,
}

/// A cheap `Clone` handle to the reporter. Every call returns at once.
#[derive(Clone)]
pub struct Reporter {
    commands: UnboundedSender<Command>,
}

enum Command {
    Event {
        event: Event,
        at: Instant,
        wall_ms: i64,
    },
    Resolved {
        track_id: u64,
        meta: StreamMeta,
    },
    Enable(bool),
    Flush(oneshot::Sender<()>),
    Clear(oneshot::Sender<()>),
}

impl Reporter {
    /// The reporter, and its worker for the caller to run for as long as
    /// the app does. Unsent reports are read from, and saved to, `path`.
    /// While not `enabled`, events are dropped and nothing is sent.
    pub fn new(
        tidal: TidalClient,
        http: reqwest::Client,
        store: Store,
        path: PathBuf,
        enabled: bool,
    ) -> (Self, impl Future<Output = ()> + Send + 'static) {
        let (commands, receiver) = unbounded_channel();
        let worker = Worker {
            tidal,
            http,
            store,
            path,
            enabled,
            listening: None,
            meta: VecDeque::new(),
            outbox: Outbox::default(),
            waiting: false,
            flushes: Vec::new(),
        };
        (Self { commands }, worker.run(receiver))
    }

    pub fn send(&self, event: Event) {
        self.command(Command::Event {
            event,
            at: Instant::now(),
            wall_ms: now_ms(),
        });
    }

    /// What TIDAL served for `track_id`, for its report once it starts.
    pub fn stream_resolved(&self, track_id: u64, meta: StreamMeta) {
        self.command(Command::Resolved { track_id, meta });
    }

    /// Turned off, the play being listened to is forgotten: it would
    /// otherwise count the whole time off. Unsent reports stay, as they
    /// were made while it was on, and are sent once it's back on.
    pub fn set_enabled(&self, on: bool) {
        self.command(Command::Enable(on));
    }

    /// Report the play being listened to, and try once to send what's
    /// unsent. Unsent reports are already on disk. Done when the try
    /// settles; the caller bounds how long it waits.
    pub fn flush(&self) -> impl Future<Output = ()> + Send + 'static {
        let (done, wait) = oneshot::channel();
        self.command(Command::Flush(done));
        async move {
            let _ = wait.await;
        }
    }

    /// Forget everything: the play being listened to and every unsent
    /// report, on disk too. For Logout.
    pub fn clear(&self) -> impl Future<Output = ()> + Send + 'static {
        let (done, wait) = oneshot::channel();
        self.command(Command::Clear(done));
        async move {
            let _ = wait.await;
        }
    }

    fn command(&self, command: Command) {
        if self.commands.send(command).is_err() {
            log::warn!("The play reporter has stopped");
        }
    }
}

type Sending = Pin<Box<dyn Future<Output = (Batch, SendOutcome)> + Send>>;

struct Worker {
    tidal: TidalClient,
    http: reqwest::Client,
    store: Store,
    path: PathBuf,
    enabled: bool,
    listening: Option<Listening>,
    /// Served streams of tracks that haven't started, newest last.
    meta: VecDeque<(u64, StreamMeta)>,
    outbox: Outbox,
    /// A send failed: nothing more goes until the next retry or flush.
    waiting: bool,
    /// Flushes waiting for sending to settle.
    flushes: Vec<oneshot::Sender<()>>,
}

impl Worker {
    async fn run(mut self, mut commands: UnboundedReceiver<Command>) {
        self.load().await;
        let mut sending: Option<Sending> = None;
        // The first tick is at once: what an earlier run left goes now.
        let mut retry = tokio::time::interval(RETRY_EVERY);
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command).await,
                    None => break,
                },
                (batch, outcome) = async { sending.as_mut().expect("guarded").await },
                    if sending.is_some() =>
                {
                    sending = None;
                    self.settle(batch, outcome).await;
                }
                _ = retry.tick() => self.waiting = false,
            }
            if sending.is_none() {
                sending = self.next_send().await;
            }
            if sending.is_none() {
                for flush in self.flushes.drain(..) {
                    let _ = flush.send(());
                }
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Event { event, at, wall_ms } if self.enabled => {
                self.event(event, at, wall_ms).await
            }
            Command::Event { .. } => {}
            Command::Resolved { track_id, meta } => {
                self.meta.retain(|(id, _)| *id != track_id);
                self.meta.push_back((track_id, meta));
                if self.meta.len() > META_KEPT {
                    self.meta.pop_front();
                }
            }
            Command::Enable(on) => {
                self.enabled = on;
                if on {
                    self.waiting = false;
                } else {
                    self.listening = None;
                    self.meta.clear();
                }
            }
            Command::Flush(done) => {
                self.close(false, Instant::now(), now_ms()).await;
                self.waiting = false;
                self.flushes.push(done);
            }
            Command::Clear(done) => {
                self.listening = None;
                self.meta.clear();
                self.outbox = Outbox::default();
                self.save().await;
                let _ = done.send(());
            }
        }
    }

    async fn event(&mut self, event: Event, at: Instant, wall_ms: i64) {
        match event {
            Event::Started(play) => {
                self.close(false, at, wall_ms).await;
                let at_meta = self.meta.iter().position(|(id, _)| *id == play.track_id);
                let meta = at_meta
                    .and_then(|i| self.meta.remove(i))
                    .map(|(_, meta)| meta)
                    .unwrap_or_default();
                log::debug!(
                    "Listening to track {} ({:?}, chosen by the user: {})",
                    play.track_id,
                    play.source,
                    play.chosen_by_user
                );
                self.listening = Some(Listening::new(play, meta, at, wall_ms));
            }
            Event::Paused => {
                if let Some(listening) = &mut self.listening {
                    listening.pause(at);
                }
            }
            Event::Resumed => {
                if let Some(listening) = &mut self.listening {
                    listening.resume(at);
                }
            }
            Event::Finished => self.close(true, at, wall_ms).await,
            Event::Stopped => self.close(false, at, wall_ms).await,
        }
    }

    /// The play being listened to ends: into the outbox, if it counts.
    async fn close(&mut self, natural: bool, at: Instant, wall_ms: i64) {
        let Some(listening) = self.listening.take() else {
            return;
        };
        let Some(event) = listening.close(natural, at, wall_ms) else {
            return;
        };
        let Some(tokens) = self.tidal.tokens() else {
            log::warn!(
                "Signed out: can't report track {}",
                event.requested_product_id
            );
            return;
        };
        let play = listening.play();
        log::debug!(
            "Reporting track {} ({:?}, chosen by the user: {})",
            play.track_id,
            play.source,
            play.chosen_by_user
        );
        let claims = event::parse_claims(&tokens.access_token);
        self.outbox
            .push(event::build_body(&event, &claims), now_secs());
        self.waiting = false;
        self.save().await;
    }

    /// The next batch to send, if there's one to send now.
    async fn next_send(&mut self) -> Option<Sending> {
        if !self.enabled || self.waiting || self.outbox.is_empty() {
            return None;
        }
        if self.outbox.expire(now_secs()) {
            self.save().await;
        }
        if self.outbox.is_empty() {
            return None;
        }
        let batch = self.outbox.batch(event::MAX_BATCH);
        Some(Box::pin(post(self.tidal.clone(), self.http.clone(), batch)))
    }

    async fn settle(&mut self, batch: Batch, outcome: SendOutcome) {
        match outcome {
            SendOutcome::Accepted => {
                log::debug!("TIDAL took {} play report(s)", batch.len());
                self.outbox.remove(&batch);
            }
            SendOutcome::SenderFault => {
                log::warn!(
                    "TIDAL refused {} play report(s), dropping them",
                    batch.len()
                );
                self.outbox.remove(&batch);
            }
            SendOutcome::Retryable | SendOutcome::AuthFailed => {
                self.outbox.failed(&batch);
                self.waiting = true;
            }
            SendOutcome::Unreachable => self.waiting = true,
        }
        self.save().await;
    }

    async fn load(&mut self) {
        let (store, path) = (self.store.clone(), self.path.clone());
        let read = tokio::task::spawn_blocking(move || store.read_json::<Outbox>(&path)).await;
        match read {
            Ok(Ok(Some(outbox))) => {
                if !outbox.is_empty() {
                    log::info!("{} play report(s) left unsent", outbox.len());
                }
                self.outbox = outbox;
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => log::warn!("Could not read the unsent play reports: {e}"),
            Err(e) => log::warn!("Could not read the unsent play reports: {e}"),
        }
    }

    /// Write the outbox, or delete the file when it's empty: after a
    /// clear, a send that settles late leaves nothing behind.
    async fn save(&self) {
        let (store, path, outbox) = (self.store.clone(), self.path.clone(), self.outbox.clone());
        let written = tokio::task::spawn_blocking(move || match outbox.is_empty() {
            true => store.remove(&path),
            false => store.write_json(&path, &outbox),
        })
        .await;
        match written {
            Ok(Ok(())) => {}
            Ok(Err(e)) => log::warn!("Could not save the unsent play reports: {e}"),
            Err(e) => log::warn!("Could not save the unsent play reports: {e}"),
        }
    }
}

/// Send `batch` with the current tokens, refreshing them once if they're
/// refused.
async fn post(tidal: TidalClient, http: reqwest::Client, batch: Batch) -> (Batch, SendOutcome) {
    let Some(tokens) = tidal.tokens() else {
        return (batch, SendOutcome::Unreachable);
    };
    let Some(client_id) = tidal.client_id() else {
        return (batch, SendOutcome::Unreachable);
    };
    let token = tokens.access_token;
    let outcome = match try_post(&http, &batch, &token, &client_id).await {
        SendOutcome::AuthFailed => match tidal.refresh_access_token(&token).await {
            Ok(token) => try_post(&http, &batch, &token, &client_id).await,
            Err(e) => {
                log::debug!("Could not refresh the token to report plays: {e}");
                SendOutcome::Unreachable
            }
        },
        outcome => outcome,
    };
    (batch, outcome)
}

async fn try_post(
    http: &reqwest::Client,
    bodies: &[String],
    access_token: &str,
    client_id: &str,
) -> SendOutcome {
    let now = now_ms();
    let events: Vec<_> = bodies
        .iter()
        .map(|body| {
            let headers = event::build_headers(client_id, access_token, now);
            (body.clone(), headers)
        })
        .collect();
    let send = http
        .post(event::EC_URL)
        .bearer_auth(access_token)
        .form(&event::sqs_form(&events))
        .send();
    match tokio::time::timeout(SEND_TIMEOUT, send).await {
        Ok(Ok(response)) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            log::debug!("POST {} -> {status}", event::EC_URL);
            event::classify(status, &body)
        }
        Ok(Err(e)) => {
            log::debug!("Could not send play reports: {e}");
            SendOutcome::Unreachable
        }
        Err(_) => {
            log::debug!("Sending play reports timed out");
            SendOutcome::Unreachable
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn now_secs() -> u64 {
    syzygy_store::now_secs()
}
