# 18: Playlist Page

**What to build:** the user can open any playlist, sort it (remembered per playlist), filter it by title, artist or album, and see the playlist's recommendations. syzygy knows whether it's one of their Own playlists. See user story 47 and the spec's "Library editing" (track removal, sorting).

**Blocked by:** 17

**Status:** ready-for-agent

- [x] The Playlist Page loads tracks page by page through the catalog, in a windowed list
- [x] Sorting (title, artist, album, date added, duration, ascending or descending) is applied by the server, as in sone, and the choice is saved per playlist in `Settings`
- [x] The filter runs on the loaded rows. Each row keeps its index in the playlist's own order, whether it's filtered or not
- [x] Own playlists are detected (creator is the signed-in user), on every way of reaching the Page, including routes without a `Preview`
- [x] Recommendations show below the tracks once all have loaded
- [x] Date added shows for Own playlists

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `Catalog::playlist` (details, cached as the typed `Playlist` under `playlist-details:{uuid}`), `playlist_tracks(uuid, sort)` (first page cached under `playlist-page:{uuid}:{NAME:ASC|…|own}`), and uncached `more_playlist_tracks` and `playlist_recommendations`. All are tagged `playlist` / `playlist:{uuid}`. `Paged::has_more` is "offset + items < total", not "the page was full". `Track` now carries `date_added`. `Playlist::is_own(user_id)` is the one Own rule (creator id equals the signed-in user). `PAGE_SIZE` is now public.
- **Sorting.** `TrackSort { order: TrackOrder, direction: Direction }` is serde and lives in the catalog crate. `TrackSort::clicked` is sone's header logic: the same column flips, another column sorts ascending. Clicking "#" goes back to the own order. sone had no way back, so this part is new. `Settings::track_sorts` (uuid → sort) holds the choice, written through `Action::SaveTrackSort` → `app::Message::TrackSort`. Clearing it when a different user signs in is left for ticket 32. Track and More messages carry the sort they were read with, and a stale one is dropped.
- **Context.** `Page::open` now takes a `page::Context { user_id, settings }`, which the App builds and the Shell passes along. It's read once, when the Page opens: if `SessionInfo` brings a user id later, an open Page doesn't see it until it's reopened.
- **Filter and indices.** The filter matches title, artists or album, ignoring case, on the loaded rows. While a filter is on, every remaining page loads. `shown` holds each row's position in the loaded list, and that's the number drawn. In the own order the position is the playlist index ticket 38 needs. Under a sort it's the sorted position, and per the spec, removal there resolves by (track id, dateAdded).
- **Track list.** `track_list::view` takes its header element. `header(columns)` is plain, and `sortable_header(columns, sort, on_sort)` puts chevrons by the sorted column and "TITLE · ARTIST" in the title column. `Columns` gained `date_added`. Dates use sone's wording ("This week", "Last week", "Last month", else "Mar 5, 2024"), computed in UTC without a date crate.
- **Recommendations.** These are read from the start, 50 at a time, and show ten at a time once every track has loaded. Refresh shows the next ten, then reads the next 50, then starts over when TIDAL has none, as in sone. Adding a recommendation to the playlist waits for Library editing.
- **Hero.** An Own playlist reads "You", because syzygy doesn't know the user's name yet. If the details fail, the error and Retry show even when the tracks loaded. New icons: chevron-up, chevron-down, refresh-cw. New styles: `style::pill_button` and `style::filter_input`.
- **Tested:** playlist details parsing and Own detection, has_more, date added, sort clicks and TIDAL params, the per-playlist sort in Settings, the filter, the recommendations paging, and the date wording. That's 178 tests in the workspace. **Run:** checked against TIDAL with the stored Session, through a throwaway test: an Own playlist (36 tracks, own=true, dateAdded present), Title/Date added/Duration DESC sorted by TIDAL, recommendations at 0 (50) and 50 (49), someone else's playlist (own=false) paged 50 + 50 to its 100 tracks, a cached reread, and a missing uuid as not-found. The app starts and Home loads. I didn't click into a Playlist Page or watch it by eye.

