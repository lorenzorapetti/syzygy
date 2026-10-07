# 35: Library state and Favorites

**What to build:** the user can like and unlike tracks, albums, playlists and mixes, and follow and unfollow artists, from track rows, cards, Page headers, the player bar and the context menus. Edits show immediately and roll back with a toast if TIDAL refuses them. Hearts stay disabled until syzygy knows the item's Favorite state. This ticket introduces the pending-edit model the other Library editing tickets build on. See user stories 54–55 and the spec's "Library editing" and test Seam 3.

**Blocked by:** 19, 25

**Status:** ready-for-agent

- [ ] The Shell's `Library` state holds the Favorite id sets, the sidebar's root playlists and Folders, and one list of pending edits, each with an id. sone's overlay maps are not ported
- [ ] `library::apply(server_items, folder, &pending)` is the only merge. The sidebar, Library view-all, Favorites and the hearts all render through it
- [ ] An edit is pushed as pending and its `syzygy-catalog` mutation runs as a `Task`. On failure the edit is dropped and a toast is shown. On success the mutation invalidates its tags (`user:{id}`, `fav-*`), and the edit stays until a `Fresh` read of an affected tag that started after the success arrives
- [ ] Edits are serialized per target. A failure drops the edits queued behind it, with one toast
- [ ] Favorite id sets load when the Shell starts, `Cached` then `Fresh`. Hearts are disabled until their set has loaded
- [ ] Like/Unlike and Follow/Unfollow on track rows, cards, sidebar items, Page headers (the heart plus a "…" button opening the card's menu) and the player bar heart. Card and sidebar context menus get Play now, Play next, Add to queue and Like/Follow
- [ ] Seam 3 tests: optimistic apply, rollback on failure, an edit kept until a later `Fresh` read, per-target ordering and dropping queued edits
