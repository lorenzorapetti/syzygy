# 27: Autoplay and gapless advance

**What to build:** when nothing is left and Repeat is off, Autoplay continues with the last track's Track radio, and "Playing from" shows it. Without a radio, playback stops cleanly. Tracks change with no silence whenever the next one is already known. See user stories 79–80, 85 and the spec's "Playback model" (Autoplay, gapless).

**Blocked by:** 25, 26

**Status:** ready-for-agent

- [ ] Autoplay is a `Settings` preference. When it starts a Track radio, the radio becomes the Playback source. No radio, or a failed fetch, stops playback with the status Stopped
- [ ] Tracks from an Autoplay-started Track radio are flagged "not chosen by user", ready for reporting (ticket 34)
- [ ] Gapless arms whatever comes next when state knows it: the Manual queue head, the next play-order entry, Repeat one, the first entry of the precomputed Repeat all rebuild, or the Track radio fetched when the last track starts. It never arms a track that will be skipped
- [ ] `TrackAdvanced` is reconciled by entry id and applies the Source tag switch
- [ ] Seam 1 tests cover Autoplay with and without a radio, gapless arming choices, `TrackAdvanced` reconciliation and the "chosen by user" flag
