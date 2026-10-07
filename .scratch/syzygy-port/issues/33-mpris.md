# 33: MPRIS

**What to build:** syzygy shows up in the desktop's media controls with the current track, cover, status, position, Shuffle, loop status and volume. Media keys and desktop controls act exactly like the in-app buttons, "Raise" focuses the window and "Quit" quits. See user stories 115–117 and the spec's "Crate responsibilities" and "Playback model" (MPRIS).

**Blocked by:** 23

**Status:** ready-for-agent

- [ ] `syzygy-mpris`: sone's MPRIS server with bus name `org.mpris.MediaPlayer2.syzygy` and identity `Syzygy` passed in by the binary, position from a closure reading `PositionCell`, and a typed `Event` enum through a boot-created `run_with` subscription
- [ ] After each `update` that touches playback, an `MprisView` is derived and compared with the last one sent. `Effect::Mpris(diff)` goes out only on a change
- [ ] Play, pause, stop, next, previous, seek, set position, volume, Shuffle and loop go through the same playback messages as the UI
- [ ] `Raise` maps to `window::gain_focus`. `Quit` maps to `Message::Quit`
- [ ] Seam 1 tests cover MPRIS diffs going out only on change
