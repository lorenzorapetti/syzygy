//! One play of a track, as it's listened to: how long it has played, and
//! the event that reports it. Time is counted, not the position, so a seek
//! changes nothing. Every change takes the moment it happened,
//! stamped when the app sent it, so a busy reporter never miscounts.

use std::time::Instant;

use crate::event::{self, SessionEvent};
use crate::{Play, StreamMeta};

/// TIDAL's own rule: a play over 30 seconds counts as a stream.
const THRESHOLD_SECS: f64 = 30.0;

/// The play being listened to now.
pub struct Listening {
    session_id: String,
    play: Play,
    meta: StreamMeta,
    started_at_ms: i64,
    /// Seconds played before the last resume.
    accumulated_secs: f64,
    /// When it last started playing, while it plays.
    resumed_at: Option<Instant>,
}

impl Listening {
    pub fn new(play: Play, meta: StreamMeta, at: Instant, wall_ms: i64) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            play,
            meta,
            started_at_ms: wall_ms,
            accumulated_secs: 0.0,
            resumed_at: Some(at),
        }
    }

    pub fn play(&self) -> &Play {
        &self.play
    }

    /// Seconds played by `at`, paused time left out.
    pub fn elapsed(&self, at: Instant) -> f64 {
        self.accumulated_secs
            + self
                .resumed_at
                .map(|t| at.saturating_duration_since(t).as_secs_f64())
                .unwrap_or(0.0)
    }

    pub fn pause(&mut self, at: Instant) {
        if let Some(t) = self.resumed_at.take() {
            self.accumulated_secs += at.saturating_duration_since(t).as_secs_f64();
        }
    }

    pub fn resume(&mut self, at: Instant) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(at);
        }
    }

    /// The event that reports it, ended at `at`: `natural` when it played
    /// to its end. None when it played under the threshold.
    pub fn close(&self, natural: bool, at: Instant, wall_ms: i64) -> Option<SessionEvent> {
        let elapsed = self.elapsed(at);
        let track_id = self.play.track_id;
        if elapsed < THRESHOLD_SECS {
            log::debug!(
                "Not reporting track {track_id}: played {elapsed:.0}s, under the {THRESHOLD_SECS}s threshold"
            );
            return None;
        }
        let duration = self.play.duration as f64;
        let meta = &self.meta;
        Some(SessionEvent {
            session_id: self.session_id.clone(),
            requested_product_id: track_id,
            actual_product_id: meta.actual_product_id.unwrap_or(track_id).to_string(),
            quality: meta.quality.clone().unwrap_or_else(|| "LOSSLESS".into()),
            audio_mode: meta.audio_mode.clone().unwrap_or_else(|| "STEREO".into()),
            presentation: meta.presentation.clone().unwrap_or_else(|| "FULL".into()),
            source: event::resolve_source(&self.play.source, track_id),
            start_ts_ms: self.started_at_ms,
            end_ts_ms: wall_ms,
            end_asset_pos: if natural {
                duration
            } else {
                elapsed.min(duration)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Source;
    use std::time::Duration;

    fn listening(duration: u32, at: Instant) -> Listening {
        let play = Play {
            track_id: 1,
            duration,
            source: Source::Album(5),
            chosen_by_user: true,
        };
        Listening::new(play, StreamMeta::default(), at, 1_000)
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn threshold_is_a_flat_30_seconds() {
        // TIDAL's rule: a play over 30 seconds counts, whatever the length.
        let t0 = Instant::now();
        let cases = [(200, 30, true), (200, 45, true), (1000, 30, true)];
        for (duration, played, counts) in cases.into_iter().chain([(200, 29, false)]) {
            let play = listening(duration, t0);
            assert_eq!(
                play.close(false, t0 + secs(played), 0).is_some(),
                counts,
                "{played}s of {duration}s"
            );
        }
        // A 25 s track played to its end is still 25 s.
        assert!(listening(25, t0).close(true, t0 + secs(25), 0).is_none());
    }

    #[test]
    fn the_end_position_clamps_and_a_natural_end_is_the_whole_track() {
        let t0 = Instant::now();
        let play = listening(200, t0);
        let ev = play.close(false, t0 + secs(500), 0).unwrap();
        assert_eq!(ev.end_asset_pos, 200.0);
        let ev = play.close(true, t0 + secs(100), 0).unwrap();
        assert_eq!(ev.end_asset_pos, 200.0);
        let ev = play.close(false, t0 + secs(100), 0).unwrap();
        assert_eq!(ev.end_asset_pos, 100.0);
    }

    #[test]
    fn paused_time_doesnt_count() {
        let t0 = Instant::now();
        let mut play = listening(200, t0);
        play.pause(t0 + secs(20));
        play.resume(t0 + secs(120));
        assert_eq!(play.elapsed(t0 + secs(125)), 25.0);
        assert!(play.close(false, t0 + secs(125), 0).is_none());
        assert!(play.close(false, t0 + secs(130), 0).is_some());
    }

    #[test]
    fn a_second_pause_or_resume_changes_nothing() {
        let t0 = Instant::now();
        let mut play = listening(200, t0);
        play.resume(t0 + secs(5));
        play.pause(t0 + secs(10));
        play.pause(t0 + secs(50));
        assert_eq!(play.elapsed(t0 + secs(60)), 10.0);
    }

    #[test]
    fn the_event_carries_the_play_and_what_was_served() {
        let t0 = Instant::now();
        let mut play = listening(200, t0);
        play.meta = StreamMeta {
            actual_product_id: Some(2),
            quality: Some("HI_RES_LOSSLESS".into()),
            audio_mode: None,
            presentation: None,
        };
        let ev = play.close(true, t0 + secs(200), 201_000).unwrap();
        assert_eq!(ev.requested_product_id, 1);
        assert_eq!(ev.actual_product_id, "2");
        assert_eq!(ev.quality, "HI_RES_LOSSLESS");
        assert_eq!(ev.audio_mode, "STEREO");
        assert_eq!(ev.source, (event::SourceType::Album, "5".to_string()));
        assert_eq!((ev.start_ts_ms, ev.end_ts_ms), (1_000, 201_000));
    }
}
