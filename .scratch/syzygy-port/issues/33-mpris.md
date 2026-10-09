# 33: MPRIS

**What to build:** syzygy shows up in the desktop's media controls with the current track, cover, status, position, Shuffle, loop status and volume. Media keys and desktop controls act exactly like the in-app buttons, "Raise" focuses the window and "Quit" quits. See user stories 115–117 and the spec's "Crate responsibilities" and "Playback model" (MPRIS).

**Blocked by:** 23

**Status:** ready-for-agent

- [x] `syzygy-mpris`: sone's MPRIS server with bus name `org.mpris.MediaPlayer2.syzygy` and identity `Syzygy` passed in by the binary, position from a closure reading `PositionCell`, and a typed `Event` enum through a boot-created `run_with` subscription
- [x] After each `update` that touches playback, an `MprisView` is derived and compared with the last one sent. `Effect::Mpris(diff)` goes out only on a change
- [x] Play, pause, stop, next, previous, seek, set position, volume, Shuffle and loop go through the same playback messages as the UI
- [x] `Raise` maps to `window::gain_focus`. `Quit` maps to `Message::Quit`
- [x] Seam 1 tests cover MPRIS diffs going out only on change

## Comments

Ticket 41 adds the desktop file `com.lorenzorapetti.syzygy.desktop`. Set the MPRIS `DesktopEntry` property to `com.lorenzorapetti.syzygy`, passed in by the binary like the bus name and `Identity`. If 41 isn't done yet, add the `DESKTOP_ENTRY` constant to `identity.rs` here.

Implemented. Notes for later tickets:

- **Crate.** `syzygy-mpris` runs `mpris_server::Server` (0.10, the `Send` server rather than sone's `Player`) on its own thread with a current-thread runtime. `Position` calls the closure on every read, so nothing ticks. `Mpris::start` takes `Identity` (bus name `syzygy`, `Syzygy`, `DESKTOP_ENTRY` = `com.lorenzorapetti.syzygy`, all in `identity.rs`), the first `View`, the closure and the event sender. With no session bus it logs, the thread ends, and the handle's calls do nothing.
- **Diffs.** `Playback` keeps the last `mpris::View` it sent. `update` and `restore` end with `announce()`, which emits `Effect::Mpris(diff)` only when the view changes (`playback/controls.rs`). `Loading` shows as Playing, the same as the player bar.
- **Seeked.** This isn't part of the view. The runner sends it on every `Effect::Seek`.
- **Commands.** `Playback::answer_mpris(event)` turns desktop events into the UI's messages:
  - Play and Pause toggle only when that changes something.
  - Seek reads the engine's position first. A seek past the end is Next.
  - SetPosition applies only to the current track and only within its length.
  - Shuffle toggles when the new value differs.
  - Loop status sends as many `CycleRepeat`s as it takes to reach it.
  - Volume from MPRIS is saved straight away, since there's no slider release to wait for.
- **New `playback::Message::Stop`** (the UI has no Stop). It lets go of the track and keeps it current at position 0, so Play starts it over, as MPRIS specifies.
- **Raise** is `window::latest().and_then(window::gain_focus)`. **Quit** is `Message::Quit`, which still doesn't stop playback or flush reports. That's ticket 34's.
- **Not done:** OpenUri (deep links are out of scope), Fullscreen, and the TrackList interface.
- **Known, minor:**
  - While stopped or paused before the first play, `Position` reads the engine (0), not the restored position.
  - Pause from the desktop while a track is loading does nothing, the same as the in-app button.
  - Each MPRIS volume step saves settings.
- **Tested:**
  - 17 Seam 1 tests: diffs only on change, Stop, and the event mapping.
  - 3 crate tests.
  - 318 tests in the binary.
  - Launching the app put `org.mpris.MediaPlayer2.syzygy` on the session bus with the identity, the desktop entry and the restored track's metadata and cover.
- **Not run:** media keys and desktop buttons weren't pressed against a playing track.
