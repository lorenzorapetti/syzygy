# 23: Playback source rules: Next, Previous, History, Shuffle and Repeat

**What to build:** every play remembers where it came from, and "Playing from" says so. Next, Previous, Shuffle, Shuffle play and Repeat all behave as the glossary defines them, from every surface that can start playback. See user stories 67–69, 71, 75–78, 81–82, 84, 93 and the spec's "Playback model".

**Blocked by:** 22

**Status:** ready-for-agent

- [x] Every Playback source kind (album, playlist, mix, artist, Loved tracks, search results, single track, Track radio) can be started from its Pages and cards, and a single track played on its own still gets a source
- [x] "Playing from" shows the Playback source, and clicking it or the cover navigates there
- [x] Card play respects Shuffle. Shuffle play shuffles once without turning Shuffle on
- [x] Shuffle on shuffles the unplayed tail. Shuffle off rebuilds the tail in source order, minus tracks already played or removed. Randomness is a seedable `SmallRng` in playback state, seeded from entropy at boot and fixed in tests
- [x] Repeat one replays only on auto-advance, and Next still moves on. Repeat all rebuilds the play order (reshuffled if Shuffle is on) and keeps History
- [x] Previous: past 3 s it seeks to 0. Otherwise it pops History (moving the cursor back if that's the previous step in the play order, otherwise playing the popped entry). With empty History it seeks to 0. History is capped at 500
- [x] Album gain applies only to an Album source played in album order, without Shuffle and not as a Shuffle play
- [x] Shuffle and Repeat mode are saved in `Settings`
- [x] Seam 1 tests cover each rule above

## Comments

Implemented. Notes for later tickets:

- **`playback`.** `Source { kind: SourceRef, name }`. `SourceRef` covers every kind: `Album`, `Playlist(uuid)`, `Mix`, `TrackRadio(mix id)`, `Artist` (top tracks), `LovedTracks`, `Search(query)`, `Track`. `PlayRequest { source, first_page, start: Start }`, where `Start` is `Track(k)` (k to the end, and with Shuffle on the rest after k is shuffled; it never wraps), `All` (a Page's or card's Play, respecting Shuffle) or `Shuffled` (Shuffle play). There's still no `continuation`; ticket 24 adds it. `Playback::new(Preferences { volume, shuffle, repeat }, SmallRng)`, with `rand::make_rng()` at boot. New messages: `Next`, `Previous`, `ToggleShuffle`, `CycleRepeat` (Off → All → One). New effect: `Stop` (Next past the end without Repeat all). The runner drops the loading play task, then stops. `Play` carries `album_gain`. The runner uses TIDAL's album gain and peak when they're present and falls back to the track's.
- **State shape.** `Listening { source: Option<SourcePlay>, current: Option<Item>, history: VecDeque<Item> }` is snapshotted whole for rollback, so a failed Next or Previous puts History back too. In `SourcePlay`, `order[..next]` has played or is playing, `order[next..]` is upcoming. Each play, and each Repeat all round, gets a `play` stamp. An `Item` records its `Step { play, index }`, so Previous can tell "the step before in this play" from a track from elsewhere. `shuffled` marks an order that isn't source order (a Shuffle play, or Shuffle on). Album gain = an Album source, the current item from this play, and `!shuffled`. Turning Shuffle off, or Repeat all without Shuffle, puts the order back in source order, so album gain comes back.
- **Previous, for ticket 25.** If the current track is the latest step of the play, the cursor moves back (`next = step.index`), so it's upcoming again. The popped History entry becomes current under its own source. That covers both "step back" and "back into another source", and for the latter it stands in for pushing to the Manual queue. **Gap:** if the current track was itself popped from History (Previous twice across sources), it's dropped. Ticket 25 should push it to the front of the Manual queue instead.
- **Settings.** `shuffle` and `repeat` are saved after `ToggleShuffle` and `CycleRepeat` (`App::save_modes`).
- **Catalog.** `Mix::track_radio` (`mixType == "TRACK_MIX"`, from `pages/mix`; the legacy fallback doesn't know). `Catalog::track(id)` (uncached) for track cards. `Error::NotATrack`.
- **UI.**
  - **Pages.** Every track list now plays: Album, Playlist, Mix, Loved tracks, the Artist Page's top tracks, Artist tracks and Search (rows play the results as `Search(query)`; a top hit or suggestion plays as `Track`). Each list Page sends `Message::Play(Start)` and `TogglePlay`.
  - **Page buttons.** `page::play_buttons` gives Play (accent) and Shuffle (Shuffle play). While the Page's source is what plays, Play becomes Pause or Resume (sone's `SourcePlayButton`). On Playlist and Loved tracks they share the filter's row.
  - **Track rows.** Rows outside the Album Page are only lit when current (`track_list::playable_track`). The accent marks are still Album-only.
  - **Cards.** On hover, cards and Home shortcuts show an accent play disc (`Link::PlayCard`). The Shell reads the target (`shell/play_card.rs`, the first Catalog value; a playlist's and Loved tracks' saved sort) and starts it with `Start::All`. A newer play aborts that read, and a failed read shows a toast. A track card plays wherever it's clicked.
  - **Player bar.** Shuffle, Previous, play/pause, Next and Repeat (Repeat one uses Lucide's `repeat-1`; modes are the accent with a dot). "Playing from <name>" sits under the artists, and it and the cover lead to `page::source_route` (a single track leads to its album).
- **Tested:** 61 Seam 1 tests (33 new): each source kind's "Playing from", Play, Shuffle and Shuffle play, Shuffle on and off (including never bringing back tracks before the one chosen), Next, rapid Nexts, rollback of History, Repeat one versus Next, the Repeat all rebuild (reshuffled, and in source order after a Shuffle play), every Previous branch, the History cap and the album gain cases. 345 in the workspace. **Not run** in the window for this ticket.
