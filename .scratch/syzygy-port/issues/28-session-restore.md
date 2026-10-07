# 28: Playback session restore

**What to build:** on launch the user finds their queue, Manual queue, History, current track and position exactly as they left them, paused. A background fill that hadn't finished starts again. See user stories 104–106 and the spec's "Playback model" (persistence).

**Blocked by:** 24, 25

**Status:** ready-for-agent

- [ ] `queue.json` (in `syzygy-store`, encrypted, with no fallback store) holds the source (tracks, play order, cursor, unfinished fill), the Manual queue, History, the current track and the position
- [ ] It is saved through `Effect::SaveSnapshot`, debounced about 2 s, and again on Quit
- [ ] Restore comes back paused at the saved position, and an unfinished fill restarts. Play while Stopped with a current track plays from the stored position
- [ ] Shuffle, Repeat mode, Autoplay, volume and allow-explicit come from `Settings`
- [ ] A Seam 1 test round-trips save and restore
