//! The sidebar's dot on Feed: whether there's something in it the user
//! hasn't seen. It comes from one check of the Feed after login, and goes
//! once the Feed has been opened.

#[derive(Debug, Default)]
pub struct Unseen {
    count: u32,
    /// The check has been asked for.
    checked: bool,
    /// The user has opened the Feed, so all of it is seen.
    opened: bool,
}

impl Unseen {
    /// The user to check the Feed for: the first time they're known.
    pub fn check(&mut self, user_id: Option<u64>) -> Option<u64> {
        if self.checked {
            return None;
        }
        let user_id = user_id?;
        self.checked = true;
        Some(user_id)
    }

    /// What the check found. A check that comes back after the Feed was
    /// opened may have read it before, so it's ignored.
    pub fn arrived(&mut self, count: u32) {
        if !self.opened {
            self.count = count;
        }
    }

    /// The user opened the Feed.
    pub fn opened(&mut self) {
        self.opened = true;
        self.count = 0;
    }

    pub fn any(&self) -> bool {
        self.count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_is_checked_once_for_the_first_known_user() {
        let mut unseen = Unseen::default();
        assert_eq!(unseen.check(None), None);
        assert_eq!(unseen.check(Some(7)), Some(7));
        assert_eq!(unseen.check(Some(7)), None);
        assert!(!unseen.any());
    }

    #[test]
    fn the_dot_shows_what_the_check_found_until_the_feed_is_opened() {
        let mut unseen = Unseen::default();
        unseen.check(Some(7));
        unseen.arrived(3);
        assert!(unseen.any());
        unseen.opened();
        assert!(!unseen.any());
    }

    #[test]
    fn nothing_unseen_shows_no_dot() {
        let mut unseen = Unseen::default();
        unseen.check(Some(7));
        unseen.arrived(0);
        assert!(!unseen.any());
    }

    #[test]
    fn a_check_that_comes_back_after_the_feed_was_opened_is_ignored() {
        let mut unseen = Unseen::default();
        unseen.check(Some(7));
        unseen.opened();
        unseen.arrived(3);
        assert!(!unseen.any());
    }
}
