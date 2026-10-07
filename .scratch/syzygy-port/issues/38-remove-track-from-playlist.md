# 38: Remove a track from an Own playlist

**What to build:** in their Own playlists, the user removes a track from the row's context menu, and the right track is removed however the list is sorted or filtered. See user story 61 and the spec's "Library editing" (track removal).

**Blocked by:** 35, 18

**Status:** ready-for-agent

- [ ] "Remove from playlist" shows in the track row menu only on an Own playlist's Page
- [ ] In the playlist's own order (filtered or not), removal uses the row's own-order index, and later rows' indices shift down by one
- [ ] In a sorted view, removal first does a fresh (uncached) read of the playlist in its own order and finds the row matching (track id, dateAdded). No match or more than one is refused with a toast
- [ ] Removals in one playlist are serialized (per-target ordering). The count goes down optimistically and rolls back on failure
- [ ] Seam 3 tests: own-order indices shifting after a removal, and sorted-view resolution with no or several matches refused
