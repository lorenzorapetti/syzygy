// Copied from sone (https://github.com/lullabyX/sone) by lullabyX,
// src-tauri/src/rate_gate.rs at 0488f97. GPL-3.0-only.

//! Global cooldown after an upstream 429. Lock-free, and it never sleeps:
//! waiting is always the caller's job.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Used when a 429 arrives without a usable Retry-After. Long enough to break
/// a debounced prefetch loop, short enough not to feel bricked.
pub(crate) const DEFAULT_COOLDOWN_SECS: u64 = 5;
/// An unclamped server value could brick the client for hours.
const MAX_COOLDOWN_SECS: u64 = 120;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Default)]
pub(crate) struct RateGate {
    not_before_secs: AtomicU64,
}

impl RateGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seconds remaining at `now`, or None if clear. Single atomic load.
    pub fn cooling_down_at(&self, now: u64) -> Option<u64> {
        let not_before = self.not_before_secs.load(Ordering::Relaxed);
        (not_before > now).then(|| not_before - now)
    }

    pub fn cooling_down(&self) -> Option<u64> {
        self.cooling_down_at(now_secs())
    }

    /// Extend the cooldown. `fetch_max` so concurrent 429s can only lengthen
    /// it. Note this stores an ABSOLUTE deadline against the wall clock, so a
    /// backwards clock jump makes the cooldown outlast its intended duration;
    /// the 120s clamp bounds that.
    pub fn trip_at(&self, now: u64, secs: u64) {
        let secs = secs.clamp(1, MAX_COOLDOWN_SECS);
        self.not_before_secs
            .fetch_max(now + secs, Ordering::Relaxed);
    }

    pub fn trip(&self, secs: u64) {
        self.trip_at(now_secs(), secs);
    }
}

/// Retry-After delta-seconds form. The HTTP-date form is rejected rather than
/// mis-parsed; callers then fall back to DEFAULT_COOLDOWN_SECS, i.e. the
/// failure mode is a SHORTER cooldown than the server asked for.
pub(crate) fn parse_retry_after_value(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

pub(crate) fn retry_after_or_default(headers: &reqwest::header::HeaderMap) -> u64 {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
        .unwrap_or(DEFAULT_COOLDOWN_SECS)
}

#[cfg(test)]
mod tests {
    use super::{MAX_COOLDOWN_SECS, RateGate, parse_retry_after_value};

    #[test]
    fn clear_then_trips_then_expires() {
        let gate = RateGate::new();
        assert_eq!(gate.cooling_down_at(1_000), None);
        gate.trip_at(1_000, 5);
        assert_eq!(gate.cooling_down_at(1_002), Some(3));
        assert_eq!(gate.cooling_down_at(1_005), None);
        assert_eq!(gate.cooling_down_at(9_999), None);
    }

    #[test]
    fn trip_only_extends_never_shortens() {
        let gate = RateGate::new();
        gate.trip_at(1_000, 60);
        gate.trip_at(1_000, 1);
        assert_eq!(gate.cooling_down_at(1_000), Some(60));
    }

    #[test]
    fn cooldown_is_clamped() {
        let gate = RateGate::new();
        gate.trip_at(1_000, 99_999);
        assert_eq!(gate.cooling_down_at(1_000), Some(120));
    }

    #[test]
    fn huge_cooldown_is_reported_clamped() {
        // send() reports the gate's own view rather than the raw Retry-After,
        // so an hour-long header can never arm an hour-long frontend timer.
        let gate = RateGate::new();
        gate.trip(3_600);
        let reported = gate.cooling_down().expect("gate is cooling down");
        assert!(reported <= MAX_COOLDOWN_SECS, "reported {reported}s");
    }

    #[test]
    fn parses_delta_seconds_only() {
        assert_eq!(parse_retry_after_value("120"), Some(120));
        assert_eq!(parse_retry_after_value("  30 "), Some(30));
        assert_eq!(
            parse_retry_after_value("Wed, 21 Oct 2015 07:28:00 GMT"),
            None
        );
        assert_eq!(parse_retry_after_value("garbage"), None);
    }
}
