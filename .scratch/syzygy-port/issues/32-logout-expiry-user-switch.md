# 32: Logout, Session expiry and user switch

**What to build:** Logout in Settings signs the user out and leaves nothing of their account on the machine except preferences. Session expiry sends them back to log in with a banner, and their queue is kept if they sign back in as the same user. A different user signing in starts clean. See user stories 18–24, 113 and the spec's "Auth and Session".

**Blocked by:** 28, 31

**Status:** ready-for-agent

- [ ] Logout in the Settings modal stops playback and deletes `session.json`, the disk cache, `queue.json` and the report queue, keeping preferences
- [ ] Network failures and server errors are ordinary errors, never Session expiry
- [ ] On `SessionExpired`: stop playback, clear tokens, keep `user_id`, `country_code`, the queue, cache and preferences, and show login with "Your session expired, sign in again"
- [ ] A different `user_id` signing in clears the cache, the queue, and the Library recents and sorts in `Settings`. A new sign-in starts an empty Back stack at Home
