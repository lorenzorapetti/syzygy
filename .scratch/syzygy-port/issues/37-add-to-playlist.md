# 37: Add to playlist

**What to build:** from a track row, a card or the player bar, the user picks a playlist from a popover with their recent playlists first, a title filter and "Create new". Adding a single duplicate says "Track already in this playlist". Adding a selection adds what's new and says how many were already there. See user stories 56, 59–60 and the spec's "Library editing".

**Blocked by:** 36

**Status:** ready-for-agent

- [x] The popover, anchored to the menu item, runs a catalog read of every playlist (`DATE_UPDATED DESC`, paged in parallel, tagged `user:{id}`), rendered through `library::apply`. No list is kept in Library state
- [x] Up to 8 recent playlists first, stored in `Settings`. A title filter. "Create new" opens the create dialog with the tracks
- [x] A single track uses `onDupes=FAIL`, and a 409 or "dupe" shows "Track already in this playlist"
- [x] A selection uses `onDupes=SKIP`, then re-reads the playlist and toasts "Added N (M already in playlist)" when counts differ. The optimistic count is not trusted. The second refresh after 3 s (for the generated cover) is kept
- [x] Create-with-tracks is two pending edits. If the add fails, the playlist is kept and a toast says the tracks weren't added
- [x] Add to playlist ▸ in the track row, card (not artists) and player bar menus
- [x] Seam 3 test: a selection add with skipped duplicates

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `own_playlists(user_id)` reads `/users/{id}/playlists` (`DATE_UPDATED DESC`, 500 a page, the rest at once from the first page's real size), keeps `is_own` ones, cached under `own-playlists:{id}` tagged `user:{id}`. `add_track` (`onDupes=FAIL`) and `add_tracks` (`onDupes=SKIP`) both invalidate `playlist:{uuid}` and `user:{id}`. `add_tracks` reads the playlist's count before and after (fresh details) and returns `Added { asked, new, tracks }`. `forget_playlist` is the 3 s cover re-read's invalidation. `Error::is_duplicate` is a 409 or "dupe" in the error. `playlist::from_tidal` is the one `TidalPlaylist` → `Playlist` conversion.
- **Library.** `Edit::Add(Adding)` targets the playlist, shows it with the tracks counted at once, and gets TIDAL's count when it lands. One track goes as `Mutation::AddTrack`, more as `Mutation::AddTracks` (result `Message::Added`). `Message::CreatePlaylist(fields, tracks)` and `Ask::NewPlaylist(tracks)` now carry tracks: once `Created` comes back, the add is a second pending edit (`new_playlist`), and its failure toasts "Created "X", but couldn't add the tracks". New effects: `Inform` (an info toast), `Recent(uuid)` (when an add is pushed, so a refused add still counts as recent), `Cover(uuid)` (the Shell waits 3 s, forgets the cache, and sends `Shell::Message::Reread(library::playlist_tags(uuid))`). `Listing::Own` is the picker's list: it ignores likes and shows creates, edits, deletes and adds.
- **Toasts.** A single track: `Added "<title>" to playlist` (ticket 40's wording) or "Track already in this playlist". A selection: "Added N (M already in playlist)" when some were skipped, else `Added N tracks to "<playlist>"`. The last one wasn't asked for.
- **Picker.** `Message::Pick { tracks, at }` → `Effect::Pick` → the Shell's `picker: Option<Picker>` (`shell/picker.rs`), drawn between the maximized player and the modals. `at` is the item's or button's rectangle on screen, from `menu::anchor` (a wrapper that publishes its own bounds on press). It opens to the right, or to the left at the window's edge. Recents (8, `Settings::recent_playlists`, cleared by `forget_account`) come first unless filtered out, and placeholders aren't offered. `Tracks::Card` reads all the card's tracks while it's open (`play_card::tracks`: albums, playlists and Loved tracks paged to the end, mixes, a track). Rows and "Create new" wait for them. Esc, a click outside or navigating closes it.
- **Entry points.** Track menus (`menu::track`), card menus except artists' (and the Page header "…" through `menu::card`), Own playlist menus, and a list-music button by the player bar's heart.
- **Not checked.** Whether `/users/{id}/playlists` lists playlists inside Folders. Whether TIDAL caps how many ids one SKIP POST takes (Loved tracks can be thousands). Whether TIDAL's count lags right after a POST, which would make N too low.
- **Tested:** 14 Seam 3 tests (FAIL and SKIP mutations, the dupe message and rollback, "Added 1 (2 already in playlist)" with TIDAL's count over the guess, refresh and cover effects, Own-only, per-playlist ordering, create-with-tracks and its failure, `Listing::Own`, the picker's open), 3 picker tests (recents, filter, placement), a settings test for recents, 2 catalog tests. 635 in the workspace. **Run:** the app starts. I didn't open the picker or add tracks on the live account.
