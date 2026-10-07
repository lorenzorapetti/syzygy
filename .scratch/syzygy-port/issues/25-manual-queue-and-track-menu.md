# 25: Manual queue and the track context menu

**What to build:** right-clicking any track row gives "Play next", "Add to queue", "Go to Track radio", "Go to album" and "Go to artist". Queued tracks play before the rest of the Playback source, the same track can be queued twice, and "Playing from" shows where a Manual queue entry came from while it plays. See user stories 72–74 and the spec's "Playback model" and "Library editing" (context menus).

**Blocked by:** 23

**Status:** ready-for-agent

- [ ] The Manual queue is a separate list of Queue entries, each with its own id and a Source tag. Shuffle never reorders it
- [ ] "Playing from" shows the entry's Source tag while it plays, and the Playback source otherwise
- [ ] The Previous branch that isn't the previous play-order step pushes the current track to the front of the Manual queue with its tag, then plays the popped History entry under its own tag. History entries record their Source tag
- [ ] `iced_aw::context_menu` on track rows (every list, search results) with Play next, Add to queue, Go to Track radio, Go to album and Go to artist. The menu is built so later tickets can add Library items
- [ ] Seam 1 tests cover the Manual queue, Source tags and the Previous branch
