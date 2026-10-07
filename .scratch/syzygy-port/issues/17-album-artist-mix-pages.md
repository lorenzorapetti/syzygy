# 17: Album, Artist and Mix Pages, with windowed track lists

**What to build:** the user can open an album, an artist (with "all top tracks" and the "view all" Pages) and a mix from any card. The header draws straight away from the card they clicked, and track lists with thousands of rows scroll smoothly. See user stories 29, 39, 45–46, 48.

**Blocked by:** 16

**Status:** ready-for-agent

- [x] A windowed track list: fixed 60px rows with 40px covers, spacers, ±8 overscan, driven by `scrollable::on_scroll(Viewport)`. It requests images only for rows near the viewport
- [x] Album, Artist, Artist tracks, Artist view-all (with its section tab in the route) and Mix Pages, read through the catalog
- [x] Album, Playlist, Artist and Mix routes carry an optional `Preview`, and the Page hero draws from it while the rest loads
- [x] A Page that doesn't exist shows not-found. A failed load shows an inline error with Retry

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `Catalog::album`, `artist`, `artist_tracks` / `more_artist_tracks`, `artist_view_all` / `more_artist_view_all` and `mix` are new reads. First pages go through the cache (`album-page:{id}`, `artist-page:{id}`, `artist-top-tracks-all:{id}`, `artist-view-all:{id}:{path}`, `mix-page:{id}`, all Dynamic, tagged `album`/`album:{id}`, `artist`/`artist:{id}`, `mix-page`/`mix:{id}`). Later pages are uncached, 50 at a time, and `Paged::has_more` is "the page was full", as in sone. Typed results: `Album`, `Artist` (sections of `Content::Tracks` or `Content::Cards`, with their view-all path), `Mix`, and a display `Track` (title with version, artist and album refs, explicit, volume). `Artist::from(Value)` ports sone's v2/v1 artist page parsing. `AlbumPageSection` now also deserializes `sectionType`, so a cached album page reads back with its sections.
- **Windowed track lists.** `page::track_list`: 60px rows, a 36px column header, spacers above and below, ±8 overscan. A Page passes where its list starts (`top`), worked out from fixed heights above it (`hero::HEIGHT` is 232). The Shell passes each Page a `Viewport` (offset and height from `on_scroll`), with a 4000px height until the scrollable reports one, because a scrollable whose content fits never reports. Covers are asked for only by rows that are built. Album volumes are heading rows of the same height. Ticket 18 should reuse it for playlists.
- **Routes.** `ArtistTracks { id }` and `ArtistViewAll { id, section }`, where `section` is the view-all path. The view-all Page's tabs are the artist's card sections that have a path. Switching tabs goes through `Action::Replace`. Album, Artist and Mix draw the hero from the `Preview` while they load.
- **Shared Page pieces.** `page::Link` (`CoverWanted`, `Open`) is what covers, cards and links emit; `page::cards` holds the card, the card row with arrows (moved out of Home) and the grid; `page::hero`; `page::paged::List` for lists read a page at a time, showing an inline error with Retry when a later page fails. `Load::Artist` carries the message its read becomes, since three Pages read the artist. `Action::Batch` runs several actions.
- **Not done.** Play, Shuffle and hearts wait for playback (22) and Library state (35). A tab's scroll offset isn't kept per tab: switching resets it to the top, as on Home. TIDAL answers an unknown mix id with a 500, not a 404, so that shows the error with Retry rather than not-found.
- **Tested:** artist page v1/v2 parsing, credits and empty modules skipped, bio markup stripped, top-track and view-all pages, the album page and its cache round trip, and the window's row range. That's 158 tests in the workspace. **Run:** every new read was checked against TIDAL with a stored Session, through a throwaway test: an album with three sections (and its cached reread), a missing album as not-found, an artist page, two pages of top tracks, three view-all sections, and a mix. The app starts and Home loads. I didn't click through the new Pages or watch the scrolling.
