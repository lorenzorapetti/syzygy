# 34: Play reporting and clean Quit

**What to build:** plays are reported to TIDAL so they show in "Recently Played", tracks from an Autoplay radio are reported as not chosen by the user, and unsent reports survive restarts. Quitting from the window or the desktop stops playback, flushes reports and saves everything. See user stories 118–121 and the spec's "Crate responsibilities" (`syzygy-report`) and "Bridge between engines and iced" (shutdown).

**Blocked by:** 27, 28

**Status:** ready-for-agent

- [x] `syzygy-report`: TIDAL play reporting with a persisted queue and its own flush loop
- [x] The reporter is reached only through `Report…` effects (non-blocking enqueues), and only when reporting is enabled in `Settings`
- [x] "Chosen by user" is false only for tracks from an Autoplay-started Track radio, whether the advance was gapless or not
- [x] iced's exit-on-close is off. Close requests and MPRIS Quit map to `Message::Quit`, which runs stop, report flush (~2 s timeout), settings save and snapshot save together, then `iced::exit()`
- [x] Seam 1 tests cover the "chosen by user" flag on reports

## Comments

- From ticket 32: Logout must also delete the persisted report queue. Hook it into `App::log_out` in `crates/syzygy/src/app.rs`, next to the `session.json` removal.

Implemented. Notes for later tickets:

- **Crate.** `syzygy-report` ports sone's `tidal_report`: event body, headers and SQS form (`event.rs`), the outbox (`outbox.rs`) and the play accounting (`play.rs`). `Reporter::new(tidal, http, store, path, enabled)` returns the handle and a worker future, which `boot` runs as a `Task`. Every call is a non-blocking channel send, and events are stamped with `Instant`/wall time when they're sent, so a busy worker doesn't miscount.
- **Outbox.** It lives at `<config>/syzygy/reports.json`, encrypted. Every report goes into the outbox, which is saved, before it's sent, and it leaves only when TIDAL accepts it (or a SenderFault drops it). That way a send cut off by Quit is retried next launch.
  - Unreachable sends (offline, timeout, no token) don't count as attempts; 5xx and auth failures do (10 max). Age cap is 14 days, size cap 500.
  - It sends at start, after each report and on Flush. After a failure, the next try waits for the 10-minute retry tick.
- **Effects.** Playback derives `Effect::Report(report::Event)` after each update (`playback/reporting.rs`): Started, Paused, Resumed, Finished, Stopped. Finished is emitted for `TrackFinished`/`TrackAdvanced`, so a Repeat-one replay (gapless or not) is a new play.
  - A loading track isn't playing. A play that fails and rolls back reports the old track Started again.
  - The runner sends these only while `Settings.report_plays` is on; the toggle also calls `set_enabled`.
- **Chosen by user.** `Play.chosen_by_user` comes from the entry's `chosen` (false only for Autoplay radio tracks) and is logged. The TIDAL payload is unchanged (user decision), because sone's verified `playback_session` shape has no field for it. Radio plays are attributed to the radio's MIX id.
- **Exception to "only via effects".** The Play/ArmNext runner tasks call `reporter.stream_resolved` with what TIDAL served (quality, audio mode, actual product id), as sone did. This is the runner's data, not playback's, and it only happens while reporting is on.
- **Quit.** In order:
  1. Take the position.
  2. Save the snapshot.
  3. Set `quitting`, so later snapshot saves (which would store position 0) are dropped and a second Quit is ignored.
  4. `playback::Message::Stop` reports the track Stopped.
  5. `reporter.flush()` with a 2 s timeout.
  6. Save settings.
  7. Then `iced::exit()`.
- **Logout and user switch.** `forget_account` calls `reporter.clear()`. Reports carry the account in their body, so another user's token can't send them.
- **TidalClient** gained `client_id()` and `refresh_access_token(stale)`.
- **Known, minor:** turning reporting on mid-track reports nothing for that track. The Stopped from a Logout's Reset is dropped, since by then the client is signed out.
- **Tested:**
  - 15 Seam 1 tests for Report effects, including "chosen by user" on gapless and non-gapless advances into a radio.
  - 19 crate tests: the event shape, the outbox and the play accounting.
