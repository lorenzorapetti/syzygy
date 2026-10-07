# 37: Add to playlist

**What to build:** from a track row, a card or the player bar, the user picks a playlist from a popover with their recent playlists first, a title filter and "Create new". Adding a single duplicate says "Track already in this playlist". Adding a selection adds what's new and says how many were already there. See user stories 56, 59–60 and the spec's "Library editing".

**Blocked by:** 36

**Status:** ready-for-agent

- [ ] The popover, anchored to the menu item, runs a catalog read of every playlist (`DATE_UPDATED DESC`, paged in parallel, tagged `user:{id}`), rendered through `library::apply`. No list is kept in Library state
- [ ] Up to 8 recent playlists first, stored in `Settings`. A title filter. "Create new" opens the create dialog with the tracks
- [ ] A single track uses `onDupes=FAIL`, and a 409 or "dupe" shows "Track already in this playlist"
- [ ] A selection uses `onDupes=SKIP`, then re-reads the playlist and toasts "Added N (M already in playlist)" when counts differ. The optimistic count is not trusted. The second refresh after 3 s (for the generated cover) is kept
- [ ] Create-with-tracks is two pending edits. If the add fails, the playlist is kept and a toast says the tracks weren't added
- [ ] Add to playlist ▸ in the track row, card (not artists) and player bar menus
- [ ] Seam 3 test: a selection add with skipped duplicates
