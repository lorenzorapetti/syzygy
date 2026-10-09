# 32: Logout, Session expiry and user switch

**What to build:** Logout in Settings signs the user out and leaves nothing of their account on the machine except preferences. Session expiry sends them back to log in with a banner, and their queue is kept if they sign back in as the same user. A different user signing in starts clean. See user stories 18–24, 113 and the spec's "Auth and Session".

**Blocked by:** 28, 31

**Status:** ready-for-agent

- [x] Logout in the Settings modal stops playback and deletes `session.json`, the disk cache, `queue.json` and the report queue, keeping preferences
- [x] Network failures and server errors are ordinary errors, never Session expiry
- [x] On `SessionExpired`: stop playback, clear tokens, keep `user_id`, `country_code`, the queue, cache and preferences, and show login with "Your session expired, sign in again"
- [x] A different `user_id` signing in clears the cache, the queue, and the Library recents and sorts in `Settings`. A new sign-in starts an empty Back stack at Home

## Comments

Implemented. Notes for later tickets:

- **Report queue (ticket 34).** There isn't one yet. When `syzygy-report` lands, Logout must delete its queue too: add it to `App::log_out` in `app.rs`, next to the `session.json` removal.
- **Recents (tickets 35/36/39).** `Settings::forget_account` clears the track sorts, the Loved tracks sort and the Library sorts. Whichever ticket adds recent playlists and Folders to `Settings` must clear them there too. Logout calls it as well, because sorts belong to the account rather than the machine. The search history is kept.
- **Session.** `Session::tokens` is now `Option`. Session expiry keeps `session.json` with no tokens, holding the `user_id`, `country_code` and Login method. A relaunch then opens login with the banner. The next sign-in compares users with `session::same_user`: only the same known `user_id` counts as the same user, and anyone unknown counts as someone else.
- **Who signed in.** The token response usually names the user. When it doesn't, the Shell opens without the queue and `App::unnamed_sign_in` holds the previous Session until the account refresh names them. Until then nothing is deleted, and neither the queue nor the Session is saved, so a failed refresh loses nothing.
- **Playback.** New `playback::Message::Reset`: it stops, cancels the fill and forgets the source, the Manual queue, the current track and History. Preferences stay, and it doesn't ask to save the snapshot. On expiry the position and snapshot are saved before the Reset, so the same user picks up where they were.
- **Order.** A different user's Shell opens only after the queue and cache are gone (`Message::AccountForgotten`), so no read sees the old cache. Logout lands on login with no banner. `TidalClient::sign_out` also resets the country.
- **Network failures.** Only `invalid_grant` from the token endpoint is Session expiry (`client/mod.rs`, already tested by `only_invalid_grant_is_session_expiry`). Nothing here changed that.
- **Known, minor.** Catalog reads already in flight when the cache is cleared could write an entry back. A same-user `restore_snapshot` reads `queue.json` without waiting on the save made at expiry, but the user's sign-in takes far longer than that save.
- **Tested:** 5 Seam 1 tests for `Reset`, 3 session tests (an expired Session round-trips, `same_user`) and 1 settings test, 300 in the binary. **Not run:** the app wasn't launched, so the Logout button and the banner were not seen.

