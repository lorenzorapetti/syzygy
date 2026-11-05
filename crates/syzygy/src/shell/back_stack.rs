//! The Back stack: where back and forward go.

use crate::page::Route;

/// The most entries kept behind the current Page; the oldest go first.
const CAP: usize = 50;

/// A Page the user can return to, and how far down it was scrolled.
#[derive(Debug, Clone)]
pub struct Entry {
    pub route: Route,
    pub offset: f32,
}

#[derive(Debug, Default)]
pub struct BackStack {
    back: Vec<Entry>,
    forward: Vec<Entry>,
}

impl BackStack {
    /// Leave `current` for somewhere new. Forward is cleared.
    pub fn push(&mut self, current: Entry) {
        self.forward.clear();
        self.back.push(current);
        if self.back.len() > CAP {
            self.back.remove(0);
        }
    }

    /// The entry to go back to, leaving `current` as the next forward.
    pub fn back(&mut self, current: Entry) -> Option<Entry> {
        let entry = self.back.pop()?;
        self.forward.push(current);
        Some(entry)
    }

    /// The entry to go forward to, leaving `current` as the next back.
    pub fn forward(&mut self, current: Entry) -> Option<Entry> {
        let entry = self.forward.pop()?;
        self.back.push(current);
        Some(entry)
    }

    /// Take the entries for the routes `gone` says are gone out of both
    /// stacks.
    pub fn remove(&mut self, gone: impl Fn(&Route) -> bool) {
        self.back.retain(|entry| !gone(&entry.route));
        self.forward.retain(|entry| !gone(&entry.route));
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}
