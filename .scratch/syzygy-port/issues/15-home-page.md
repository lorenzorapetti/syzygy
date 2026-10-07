# 15: Home Page

**What to build:** Home works the way TIDAL suggests music: feed tabs, sections of cards, more loaded as the user scrolls, and a refresh when the user comes back to the window after a while. See user stories 30, 40–41.

**Blocked by:** 14

**Status:** ready-for-agent

- [x] Home shows its feed tabs and sections with cards (covers are placeholders until ticket 16)
- [x] Tabs are part of the route: switching one replaces the Back stack entry instead of adding a step, and Back restores the tab
- [x] More sections load as the user scrolls
- [x] Home refreshes when the window regains focus, at most every 5 minutes
- [x] Clicking a card navigates to its route with a `Preview` (title, cover, artist) for the Page to draw straight away

## Comments

Implemented. Notes for later tickets:

- **Typed feed.** `syzygy_catalog::home_feed` turns TIDAL's loose section items into `HomeFeed { tabs, sections, cursor }`, with `Section { title, layout: Shortcuts | Row, cards }` and `Card { title, subtitle, cover, target }`, using sone's `itemHelpers` rules (type hint, mixed rows typed by their first item, magazine and featured promos, "My Tracks" as Favorites). The shortcut grid starts with Loved Tracks and holds 8. The cache still stores the raw `HomePageResponse`, so sone's encode/decode tests are unchanged. `Cover` is `Image | Artist | Promo | Url` by TIDAL id, so ticket 16 picks the size. "By You" for the user's own playlists isn't done: the catalog doesn't know the user.
- **Reads.** `Catalog::refresh_home_feed` forces a fetch through the new `swr::refresh`: it yields one `Fresh`, or nothing when the encoder refuses the value. `Catalog::more_home_feed(tab, cursor)` isn't cached and drops shortcut and page-link sections, as sone does.
- **Tabs.** `Route::Home { tab }` holds the slug. Switching a tab stays inside the Page and returns `Action::Replace(route, load)`. The Shell rewrites the current Back stack entry's route, resets its offset and scrolls to the top. Forward isn't cleared. Feed reads carry their tab, and reads for a tab the user left are dropped. Home keeps the last non-empty tab list, so the tabs stay on screen while a tab loads or fails. Going back to Home rebuilds the Page from its route, so a tab's loaded "more" sections aren't kept the way sone's per-tab cache keeps them.
- **More sections.** An iced `sensor` after the last section is keyed by the cursor and anticipates 200px, so it fires again for each page while the end stays in view. A failed cursor isn't retried. A result applies only if its cursor is still the feed's cursor. Once more sections are paged in, new first pages and focus refreshes are ignored, as in sone.
- **Focus refresh.** `Message::WindowFocused` comes from `window::Event::Focused` in the Shell's event listener, then `Shell::focused`, then `Page::focused`. Home refreshes at most every 5 minutes after it opened or last refreshed, and not while more sections are loading or after they've been added.
- **Routes with `Preview`.** `Album`, `Artist`, `Playlist` and `Mix` routes carry `Option<Preview { title, cover, artist }>`, and only album cards set `artist`. `Route::Favorites` also exists. Until tickets 17–19 they open `page::unbuilt`, which draws the Preview. Track and video cards do nothing until playback (ticket 22).
- **`Action`** gains `Navigate(Route)` and `Replace(Route, Load)`. `Load` gains `RefreshHomeFeed` and `MoreHomeFeed`.
- **Tested:** 7 home-feed mapping tests and 3 `swr::refresh` tests (138 in the workspace). **Run:** the signed-in Shell, with a stored Session, shows the tabs, the shortcut grid and the card rows. Clicking (tab switches, cards, Back) and loading more on scroll weren't tried by hand.
