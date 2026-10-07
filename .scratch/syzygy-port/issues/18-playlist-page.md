# 18: Playlist Page

**What to build:** the user can open any playlist, sort it (remembered per playlist), filter it by title, artist or album, and see the playlist's recommendations. syzygy knows whether it's one of their Own playlists. See user story 47 and the spec's "Library editing" (track removal, sorting).

**Blocked by:** 17

**Status:** ready-for-agent

- [ ] The Playlist Page loads tracks page by page through the catalog, in a windowed list
- [ ] Sorting (title, artist, album, date added, duration, ascending or descending) is applied by the server, as in sone, and the choice is saved per playlist in `Settings`
- [ ] The filter runs on the loaded rows. Each row keeps its index in the playlist's own order, whether it's filtered or not
- [ ] Own playlists are detected (creator is the signed-in user), on every way of reaching the Page, including routes without a `Preview`
- [ ] Recommendations show below the tracks once all have loaded
- [ ] Date added shows for Own playlists
