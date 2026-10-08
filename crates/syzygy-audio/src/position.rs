//! Where the playing track is, read on the caller's thread without a
//! round-trip to the audio thread.

use arc_swap::ArcSwapOption;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// A cheap `Clone` handle. The audio thread swaps in what to read from as
/// each pipeline starts; readers never lock.
#[derive(Clone, Default)]
pub struct PositionCell {
    source: Arc<ArcSwapOption<PositionSource>>,
}

pub(crate) enum PositionSource {
    /// A `Normal` pipeline. Under `concat` its position is per track, so a
    /// gapless advance starts it again from 0.
    Pipeline(gst::Pipeline),
    /// `DirectAlsa`: the frames the writer has written, at its rate.
    Alsa {
        frames: Arc<AtomicU64>,
        rate: Arc<AtomicU32>,
    },
}

impl PositionCell {
    /// Seconds into the playing track; 0 when nothing is playing.
    pub fn get(&self) -> f32 {
        match self.source.load().as_deref() {
            Some(PositionSource::Pipeline(pipeline)) => pipeline
                .query_position::<gst::ClockTime>()
                .map(|pos| pos.nseconds() as f32 / 1_000_000_000.0)
                .unwrap_or(0.0),
            Some(PositionSource::Alsa { frames, rate }) => {
                let frames = frames.load(Ordering::Relaxed);
                let rate = rate.load(Ordering::Relaxed);
                if rate > 0 {
                    frames as f32 / rate as f32
                } else {
                    0.0
                }
            }
            None => 0.0,
        }
    }

    pub(crate) fn set(&self, source: PositionSource) {
        self.source.store(Some(Arc::new(source)));
    }

    pub(crate) fn clear(&self) {
        self.source.store(None);
    }
}
