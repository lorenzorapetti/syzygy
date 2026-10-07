# 23: Playback source rules: Next, Previous, History, Shuffle and Repeat

**What to build:** every play remembers where it came from, and "Playing from" says so. Next, Previous, Shuffle, Shuffle play and Repeat all behave as the glossary defines them, from every surface that can start playback. See user stories 67–69, 71, 75–78, 81–82, 84, 93 and the spec's "Playback model".

**Blocked by:** 22

**Status:** ready-for-agent

- [ ] Every Playback source kind (album, playlist, mix, artist, Loved tracks, search results, single track, Track radio) can be started from its Pages and cards, and a single track played on its own still gets a source
- [ ] "Playing from" shows the Playback source, and clicking it or the cover navigates there
- [ ] Card play respects Shuffle. Shuffle play shuffles once without turning Shuffle on
- [ ] Shuffle on shuffles the unplayed tail. Shuffle off rebuilds the tail in source order, minus tracks already played or removed. Randomness is a seedable `SmallRng` in playback state, seeded from entropy at boot and fixed in tests
- [ ] Repeat one replays only on auto-advance, and Next still moves on. Repeat all rebuilds the play order (reshuffled if Shuffle is on) and keeps History
- [ ] Previous: past 3 s it seeks to 0. Otherwise it pops History (moving the cursor back if that's the previous step in the play order, otherwise playing the popped entry). With empty History it seeks to 0. History is capped at 500
- [ ] Album gain applies only to an Album source played in album order, without Shuffle and not as a Shuffle play
- [ ] Shuffle and Repeat mode are saved in `Settings`
- [ ] Seam 1 tests cover each rule above
