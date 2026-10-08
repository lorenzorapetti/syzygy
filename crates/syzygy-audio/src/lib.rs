//! syzygy's audio engine, from sone: [`AudioPlayer`] plays a URI through
//! GStreamer, or straight to an ALSA device for exclusive and bit-perfect
//! output. Its calls are async; what happens on its own (a track ending, a
//! gapless switch, a device error) comes as [`Event`]s on the sender it was
//! built with.

pub mod pipeline_probe;
mod player;
mod position;
pub mod signal_path;

pub use player::{AudioDevice, AudioPlayer, gapless_supported, list_alsa_devices};
pub use position::PositionCell;
pub use signal_path::SignalPath;

/// What the engine reports without being asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The playing track ended, with nothing armed to follow it. This is
    /// the only signal that a track has ended.
    TrackFinished,
    /// The track armed with `set_next_track` took over without a gap.
    TrackAdvanced {
        track_id: u64,
        /// The id it was armed with.
        entry: u64,
        replay_gain: f64,
        peak_amplitude: f64,
    },
    /// Playback broke, or the output device did.
    Failed {
        kind: ErrorKind,
        message: Option<String>,
    },
    /// Exclusive output is resampling: the DAC can't take the track's rate.
    Resampled { from: u32, to: u32 },
    /// Bit-perfect output widened the samples to a format the DAC takes.
    BitDepthChanged { from: String, to: String },
}

impl Event {
    fn failed(kind: ErrorKind, message: Option<String>) -> Self {
        Event::Failed { kind, message }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Another program has the device.
    DeviceBusy,
    DeviceDisconnected,
    /// The device came back in a different format than asked for.
    DeviceChanged,
    FormatChangeFailed,
    WriteError,
    /// GStreamer couldn't fetch or decode the track.
    PlaybackError,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the audio device is busy")]
    DeviceBusy,
    #[error("{0}")]
    Engine(String),
    /// The audio thread is gone: GStreamer couldn't start, or it panicked.
    #[error("the audio engine isn't running")]
    Gone,
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        if message == "device_busy" {
            Error::DeviceBusy
        } else {
            Error::Engine(message)
        }
    }
}

/// The volume multiplier that levels a track by its ReplayGain, with 4 dB
/// of pre-amp, never past its peak, at 80%. Unity without a gain.
pub fn compute_norm_gain(replay_gain: Option<f64>, peak_amplitude: Option<f64>) -> f64 {
    match replay_gain {
        Some(rg) => {
            let pre_amp = 4.0;
            let linear = 10.0_f64.powf((rg + pre_amp) / 20.0);
            let peak = peak_amplitude.filter(|&p| p > 0.0).unwrap_or(1.0);
            let sf = linear.min(1.0 / peak);
            0.8 * sf
        }
        None => 1.0,
    }
}
