# 35: Library state and Favorites

**What to build:** the user can like and unlike tracks, albums, playlists and mixes, and follow and unfollow artists, from track rows, cards, Page headers, the player bar and the context menus. Edits show immediately and roll back with a toast if TIDAL refuses them. Hearts stay disabled until syzygy knows the item's Favorite state. This ticket introduces the pending-edit model the other Library editing tickets build on. See user stories 54–55 and the spec's "Library editing" and test Seam 3.

**Blocked by:** 19, 25

**Status:** ready-for-agent

- [x] The Shell's `Library` state holds the Favorite id sets, the sidebar's root playlists and Folders, and one list of pending edits, each with an id. sone's overlay maps are not ported
- [x] `library::apply(server_items, folder, &pending)` is the only merge. The sidebar, Library view-all, Favorites and the hearts all render through it
- [x] An edit is pushed as pending and its `syzygy-catalog` mutation runs as a `Task`. On failure the edit is dropped and a toast is shown. On success the mutation invalidates its tags (`user:{id}`, `fav-*`), and the edit stays until a `Fresh` read of an affected tag that started after the success arrives
- [x] Edits are serialized per target. A failure drops the edits queued behind it, with one toast
- [x] Favorite id sets load when the Shell starts, `Cached` then `Fresh`. Hearts are disabled until their set has loaded
- [x] Like/Unlike and Follow/Unfollow on track rows, cards, sidebar items, Page headers (the heart plus a "…" button opening the card's menu) and the player bar heart. Card and sidebar context menus get Play now, Play next, Add to queue and Like/Follow
- [x] Seam 3 tests: optimistic apply, rollback on failure, an edit kept until a later `Fresh` read, per-target ordering and dropping queued edits

## Comments

Implemented. Notes for later tickets:

- **Catalog.** New `favorites` module: `FavoriteId` (with `tag()`), `FavoriteIds` (`contains`, `set`), `ids_tags`, `loved_tags`. `Catalog::favorite_ids(user)` is one cached read of `favorites/ids` plus the v2 mix ids, paged until a short page (key `fav-ids:{user}`, every `fav-*` tag and `user:{id}`). `Catalog::set_favorite(user, id, on)` invalidates only that kind's `fav-*` tag, as sone does; it doesn't invalidate `user:{id}`, which would throw away every Library read for one like. `Shelf::tags()` and `Shelf::lists(&FavoriteId)` are public.
- **`library` (binary).** The Shell's `Library` holds the id sets, the sidebar's root playlists and Folders (`Library::root`, a `Shelved`), and `Vec<Pending>`. `update` is pure and returns `Effect`s: `Mutate`, `ReadFavorites`, `Refresh(tags)`, `Toast`. Reads are stamped through `start_read()`. The Shell's `noting` sends `Message::Fresh(stamp, tags)` after the `Fresh` answer of a Library shelf, the sidebar or the Loved tracks, and that settles the landed edits it shows. `apply(server, Listing, &pending)` (`Listing::Loved` or `Listing::Shelf(&shelf)`) is the one merge. The last edit of a target counts. Unliked items are hidden. Liked ones not yet listed go first, at the top level only. `Edit` and `Mutation` have one variant each for now: tickets 36–39 add theirs, with `Edit::target`/`tags` growing to playlists and Folders.
- **Where this differs from the letter of the spec.** When an edit lands, the id sets are patched with it and the lists showing its tags are read again (the sidebar via `Sidebar::refresh`, the Page via `Page::refresh`, both through `List::reread`, so the new first page replaces even a paged list). A `Fresh` id-set read settles nothing. Otherwise it could drop an edit that a list still on its old read needs, and the list would show the old state until its own read lands. When the sidebar and a Page both list the same tag, whichever read lands first still settles the edit for both.
- **Hearts and menus.** `page::menu` replaces `track_menu`. It has track and card menus, `heart`, and `more` (the "…" button: a small `LeftClick` widget that turns a left click into the context menu's right click). Track rows (every list, the drawer, search) get a heart column and Like/Unlike in their menu. Cards, Library tiles and sidebar rows get Play now / Play next / Add to queue / Like or Follow. Own playlists get no like. Album, Artist, Mix and Playlist headers get the heart and "…". The player bar gets the heart. Card queueing reads the card (`play_card::read`) and queues what that brings: a long playlist's first page. Playback's `PlayNext`/`AddToQueue` now take `Vec<Track>`, and `WithoutExplicit` queues the rest. New icons: ellipsis, user-check, user-plus.
- **Not done.** A failed first read of the id sets isn't retried: the hearts stay disabled until the next sign-in. Cards show no heart of their own, only the menu's.
- **Tested:** 19 Seam 3 tests in `library/tests.rs`, 4 playback tests for queueing several tracks, 3 catalog tests. 592 in the workspace. **Run:** the app starts and reads the Favorite ids from TIDAL with the stored Session. I didn't click through the UI, and I didn't send a like to the live account.
