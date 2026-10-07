# 17: Album, Artist and Mix Pages, with windowed track lists

**What to build:** the user can open an album, an artist (with "all top tracks" and the "view all" Pages) and a mix from any card. The header draws straight away from the card they clicked, and track lists with thousands of rows scroll smoothly. See user stories 29, 39, 45–46, 48.

**Blocked by:** 16

**Status:** ready-for-agent

- [ ] A windowed track list: fixed 60px rows with 40px covers, spacers, ±8 overscan, driven by `scrollable::on_scroll(Viewport)`. It requests images only for rows near the viewport
- [ ] Album, Artist, Artist tracks, Artist view-all (with its section tab in the route) and Mix Pages, read through the catalog
- [ ] Album, Playlist, Artist and Mix routes carry an optional `Preview`, and the Page hero draws from it while the rest loads
- [ ] A Page that doesn't exist shows not-found. A failed load shows an inline error with Retry
