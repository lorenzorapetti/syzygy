# 29: Now-playing drawer: Queue, Suggested and Credits

**What to build:** the user opens a drawer over the content with a large cover and tabs. In the Queue tab they see everything that played and will play, and can pick, reorder, remove and clear entries. Suggested shows the current track's Track radio, and Credits shows who made it. See user stories 31, 96–99, 102–103 and the spec's "Playback model" (drawer operations) and "Visual system".

**Blocked by:** 25

**Status:** ready-for-agent

- [ ] The drawer slides up over everything above the player bar: 45% cover in an explicit square box (a 640px or larger image), 55% tabs, an 80% black backdrop. The maximized player is included
- [ ] Navigating anywhere closes the drawer and the maximized player
- [ ] The Queue tab lists recent History, the current track, the Manual queue ("Next in queue") and the rest of the source ("Next up from <source>")
- [ ] Picking a source entry jumps the cursor, and skipped entries don't enter History. Picking a Manual entry plucks it and plays it. A History row plays under its own tag without changing what's upcoming
- [ ] Drag reorders only within its section. Remove works on single entries. Clear empties the Manual queue and the rest of the play order and cancels the fill, keeping the source for Repeat all
- [ ] Suggested and Credits are each a `Remote<_>` for the current track only, loaded while visible and reloaded when the track changes
- [ ] Seam 1 tests cover the drawer operations
