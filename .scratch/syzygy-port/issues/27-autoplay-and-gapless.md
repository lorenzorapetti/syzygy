# 27: Autoplay and gapless advance

**What to build:** when nothing is left and Repeat is off, Autoplay continues with the last track's Track radio, and "Playing from" shows it. Without a radio, playback stops cleanly. Tracks change with no silence whenever the next one is already known. See user stories 79–80, 85 and the spec's "Playback model" (Autoplay, gapless).

**Blocked by:** 25, 26

**Status:** ready-for-agent

- [x] Autoplay is a `Settings` preference. When it starts a Track radio, the radio becomes the Playback source. No radio, or a failed fetch, stops playback with the status Stopped
- [x] Tracks from an Autoplay-started Track radio are flagged "not chosen by user", ready for reporting (ticket 34)
- [x] Gapless arms whatever comes next when state knows it: the Manual queue head, the next play-order entry, Repeat one, the first entry of the precomputed Repeat all rebuild, or the Track radio fetched when the last track starts. It never arms a track that will be skipped
- [x] `TrackAdvanced` is reconciled by entry id and applies the Source tag switch
- [x] Seam 1 tests cover Autoplay with and without a radio, gapless arming choices, `TrackAdvanced` reconciliation and the "chosen by user" flag

## Comments

Implemented. Notes for later tickets:

- **After every message**, `Playback::update` runs a step (`prepare`) that works out what comes next. While the track plays or is paused, it fetches the Track radio (`Effect::FetchTrackRadio { radio, track }`) once nothing playable is left. It arms what follows (`Effect::ArmNext { entry, track_id, album_gain }`) or clears it (`Effect::ClearNext`). An arm goes out only when the target changes. Leaving Playing/Paused drops the arm silently, because the engine drops it too: a play, a stop or a track ending all do that. The runner's `Play` and `ArmNext` send `clear_next_track` straight away, before resolving the stream, so the old track can't advance to a stale arm.
- **What gets armed** follows the order `advance` uses: Repeat one (the current track again), then the Manual queue head, then the next play-order step, then the first step of the next Repeat all round, then the radio's first track. The next round's order is drawn once the order runs out (`SourcePlay::round`) and used by `start_over`. Pages arriving or Shuffle changing redraw it. Nothing is armed when that track would be skipped. Sone does the same.
- **`TrackAdvanced(EntryId)`.** If the entry matches what is armed and that is still what follows, playback advances without a `Play` (History, Source tag switch, radio as source). Otherwise it plays what really follows over it, or sends `Stop` if nothing does. While another choice is loading, the choice wins (handled like `TrackFinished` while loading).
- **Autoplay.** `Message::Autoplay(bool)` and `Message::Gapless(bool)` are preferences saved by `save_modes`. **Ticket 31's toggles should send them.** Autoplay only applies with Repeat off and no fill still reading. The radio drops tracks already in History or current, and plays in its own order (never shuffled). If the track ends, or Next is pressed, while the radio is still fetching, playback is Stopped and waits (`waiting`). The radio plays when it arrives unless the user pressed play, Next, Previous, Seek or started something first. No radio, a failed fetch or a radio with nothing new left stops playback. `radio::read` takes the mix id from `track.track_radio`, or else reads the track, then takes the first mix read, cached or fresh.
- **"Chosen by user".** `Playback::chosen_by_user()` is false only while a track from an Autoplay-started radio is current. The flag is kept per entry, so it doesn't matter whether the advance was gapless. That's ready for ticket 34.
- **Engine.** `configure_engine` sends the saved gapless setting at boot. Exclusive mode and bit-perfect turn arming off inside the engine.
- **Tested:** 40 new Seam 1 tests. 4 existing tests now ignore `ArmNext` (`unarmed`), because what follows is armed again after a rollback or a queue change. 445 in the workspace. **Not run** in the window for this ticket, so a real gapless switch hasn't been heard.
