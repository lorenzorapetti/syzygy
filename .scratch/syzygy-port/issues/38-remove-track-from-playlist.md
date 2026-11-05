# 38: Remove a track from an Own playlist

**What to build:** in their Own playlists, the user removes a track from the row's context menu, and the right track is removed however the list is sorted or filtered. See user story 61 and the spec's "Library editing" (track removal).

**Blocked by:** 35, 18

**Status:** ready-for-agent

- [x] "Remove from playlist" shows in the track row menu only on an Own playlist's Page
- [x] In the playlist's own order (filtered or not), removal uses the row's own-order index, and later rows' indices shift down by one
- [x] In a sorted view, removal first does a fresh (uncached) read of the playlist in its own order and finds the row matching (track id, dateAdded). No match or more than one is refused with a toast
- [x] Removals in one playlist are serialized (per-target ordering). The count goes down optimistically and rolls back on failure
- [x] Seam 3 tests: own-order indices shifting after a removal, and sorted-view resolution with no or several matches refused

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `remove_track(user_id, uuid, index)` (TIDAL's DELETE `/playlists/{uuid}/items/{index}` with the ETag) invalidates `playlist:{uuid}` and `user:{id}`. `playlist_order(uuid)` reads every item in own order, fresh and uncached: where a sorted removal finds its index.
- **Library.** `Message::RemoveTrack(Removal { playlist, track, at })`, where `At::Index(i)` is the row's own-order index as shown (with the removals still to land taken out) and `At::Sorted` means it isn't known. `Edit::Remove` targets the playlist, so removals in one playlist run one at a time. It shows the playlist with one track and its duration fewer until it settles. A sorted removal's run is `Effect::ReadOrder(id, uuid)`, and `Message::Order` resolves it by (track id, dateAdded) (`same_entry`). One match becomes `At::Index` and runs as `Mutation::RemoveTrack`. None or several refuse with a toast and drop what's queued behind it, like a failure. `Library::rows(uuid, tracks, sorted)` hides the rows whose removal hasn't landed: by index in the own order, else by entry (`locate`). A row's place among what's left is its own-order index. Once a removal lands, `Effect::Removed { uuid, track, index }` makes the Playlist Page drop the row from its loaded list (`Page::removed`), so pages read after follow on from TIDAL's, and `rows` stops hiding it.
- **Page.** On an Own playlist each row is `track_list::own`, whose menu (`menu::own_track`) ends with "Remove from playlist". Numbers are places among the rows left. Play queues those rows, and the continuation offset is the loaded count. `List::more` now goes back to idle when a page arrives for an offset that no longer fits (a row went while it loaded), so the end doesn't stay on "Loading…".
- **Known races, not handled.**
  - A next page that reaches TIDAL after it removed the track, but arrives before the removal's result, starts one row late, and a track goes missing from the loaded list.
  - A first-page read that started before a removal landed and arrives after it (a sort change or Retry mid-removal) shows the row again until the next read.
  - A playback continuation read after a removal landed starts one track late.
  - Fixing these needs read stamps on the Page's track reads.
- **Tested:** 11 Seam 3 tests:
  - own-order index and count
  - later rows moving up and the queued removal's index
  - landing and `Removed`
  - rollback with toast
  - Own only
  - sorted resolution among duplicates
  - none or several matches refused
  - waiting for the earlier removal before reading
  - a failed read
  - queued removals dropped on a refusal

  The workspace has 646 tests. **Run:** the app starts. I didn't remove a track on the live account.
