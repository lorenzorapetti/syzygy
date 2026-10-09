//! Reports not yet accepted by TIDAL, from sone's queue. Every report goes
//! through it and stays until TIDAL takes it, so one being sent when the
//! app quits is still there next launch. Holds only the frozen
//! MessageBody: the Headers attribute carries the access token and is built
//! at send time.

use serde::{Deserialize, Serialize};

/// The most reports kept; the oldest go first.
const MAX_ENTRIES: usize = 500;
/// Failed sends before a report is dropped.
const MAX_ATTEMPTS: u32 = 10;
/// How old a report can get before it's dropped.
const MAX_AGE_SECS: u64 = 14 * 86400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub body: String,
    attempts: u32,
    /// Seconds since the Unix epoch.
    queued_at: u64,
}

/// Which entries a send took. Bodies are unique (each has its own uuid), so
/// they find their entries again on settling, whatever came and went since.
pub type Batch = Vec<String>;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outbox {
    entries: Vec<Entry>,
}

impl Outbox {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn push(&mut self, body: String, now: u64) {
        self.entries.push(Entry {
            body,
            attempts: 0,
            queued_at: now,
        });
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    /// Drop what's too old or failed too often. True when anything went.
    pub fn expire(&mut self, now: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| {
            e.attempts < MAX_ATTEMPTS && now.saturating_sub(e.queued_at) <= MAX_AGE_SECS
        });
        self.entries.len() != before
    }

    /// The oldest `max` reports, left in place until the send settles.
    pub fn batch(&self, max: usize) -> Batch {
        self.entries
            .iter()
            .take(max)
            .map(|e| e.body.clone())
            .collect()
    }

    /// TIDAL took the batch, or refused it for good: it goes.
    pub fn remove(&mut self, batch: &Batch) {
        for body in batch {
            if let Some(at) = self.entries.iter().position(|e| &e.body == body) {
                self.entries.remove(at);
            }
        }
    }

    /// The batch couldn't be sent: it stays, one attempt nearer its limit.
    pub fn failed(&mut self, batch: &Batch) {
        for e in &mut self.entries {
            if batch.contains(&e.body) {
                e.attempts += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outbox(n: usize, now: u64) -> Outbox {
        let mut outbox = Outbox::default();
        for i in 0..n {
            outbox.push(format!("body {i}"), now);
        }
        outbox
    }

    #[test]
    fn a_batch_is_the_oldest_and_stays_until_settled() {
        let mut outbox = outbox(12, 0);
        let batch = outbox.batch(10);
        assert_eq!(batch.first().map(String::as_str), Some("body 0"));
        assert_eq!(batch.len(), 10);
        assert_eq!(outbox.len(), 12);

        outbox.push("late".into(), 0);
        outbox.remove(&batch);
        assert_eq!(outbox.batch(10), vec!["body 10", "body 11", "late"]);
    }

    #[test]
    fn a_failed_send_counts_until_the_report_is_dropped() {
        let mut outbox = outbox(2, 0);
        let first = outbox.batch(1);
        for _ in 0..MAX_ATTEMPTS - 1 {
            outbox.failed(&first);
        }
        assert!(!outbox.expire(0));
        outbox.failed(&first);
        assert!(outbox.expire(0));
        assert_eq!(outbox.batch(10), vec!["body 1"]);
    }

    #[test]
    fn old_reports_are_dropped() {
        let mut outbox = outbox(1, 0);
        outbox.push("new".into(), MAX_AGE_SECS);
        assert!(!outbox.expire(MAX_AGE_SECS));
        assert!(outbox.expire(MAX_AGE_SECS + 1));
        assert_eq!(outbox.batch(10), vec!["new"]);
    }

    #[test]
    fn past_the_cap_the_oldest_go() {
        let outbox = outbox(MAX_ENTRIES + 3, 0);
        assert_eq!(outbox.len(), MAX_ENTRIES);
        assert_eq!(outbox.batch(1), vec!["body 3"]);
    }

    #[test]
    fn it_survives_a_round_trip() {
        let mut outbox = outbox(2, 7);
        outbox.failed(&outbox.batch(1));
        let json = serde_json::to_string(&outbox).unwrap();
        assert_eq!(serde_json::from_str::<Outbox>(&json).unwrap(), outbox);
    }
}
