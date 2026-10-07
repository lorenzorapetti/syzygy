# App state, messages and navigation

Type: grilling
Status: resolved
Blocked by: 01, 03

## Question

What is the shape of syzygy's top-level iced state and `Message` type? How are screens modelled (enum of pages, per-screen sub-state and messages), how does navigation and the back stack work, and where does async page data loading and caching live?

Inputs: the per-screen command table and the candidate glossary in [frontend behavior](../research/frontend-behavior.md). Seed CONTEXT.md with the terms that get resolved here.

## Answer

Terms (**Page**, **Back stack**) are in `CONTEXT.md`.

- **Top-level state:** `App { services, settings, playback, images, phase }` with `enum Phase { Fatal(Error), Login(login::State), Shell(Shell) }`. Playback, settings and the image cache live as long as the process, so Session expiry can keep the queue. `Shell` holds the Back stack, the current `Page`, the overlays (sidebar, header search and suggestions, drawer, maximized player, settings modal) and the global `Library` state. Leaving the Shell (expiry or logout) drops all of it.
- **Pages:** each Page is a module with `State`, `Message`, `update(&mut self, msg) -> Action` and `view`, collected in `enum Page { Home(home::State), … }`. Pages never touch playback or `Services`. They return `Action`s (`Navigate(Route)`, `Play(PlayRequest)`, `FetchImages(urls)`, `Run(Task)`, `None`), and the shell turns these into messages. This fits ADR 0002.
- **The 15 Pages:** Home, Search, Album, Artist, Artist tracks, Artist view-all, Playlist, Mix, Favorites (Loved tracks), Library view-all (playlists with folders, albums, artists, mixes), Explore, ExplorePage (`get_page_section(apiPath)`), Feed, Profile and ProfilePlaylists.
  - Profile is read-only; sone's profile edit modal is not ported.
  - Feed calls `mark_feed_seen` when it opens, and the sidebar shows an unseen badge from one `get_feed` check after login.
  - ProfilePlaylists is routed by `user_id` and fetches its list, instead of carrying the list in the route as sone does.
- **`Message`:**
  ```rust
  enum Message {
      Login(login::Message),
      Page(PageId, page::Message),
      Navigate(Route),
      Back, Forward,
      Shell(shell::Message),
      Library(library::Message),
      Playback(playback::Message), // 08
      Images(images::Message),
      Audio(audio::Event), Mpris(mpris::Event), Tidal(tidal::Event),
      WindowFocused, CloseRequested, Quit,
  }
  ```
  `Navigate` comes from cards, the sidebar, the player bar ("Playing from", the cover) and the drawer header.
- **Back stack:**
  - **Controls:** back and forward. The triggers are the header buttons, the mouse side buttons (`mouse::Button::Back`/`Forward`) and Alt+←/→. Navigating somewhere new clears the forward stack.
  - **Entries:** each one is a `Route` plus a scroll offset, capped at 50 with the oldest dropped. Going back rebuilds the Page from its `Route`. The catalog's cached copy arrives first, and the offset is reapplied with `scrollable::scroll_to`, clamped to the content that has loaded.
  - **Routes:** they are plain data. The Album, Playlist, Artist and Mix routes carry `preview: Option<Preview>` (title, cover, artist name) from the card that was clicked, so the header draws straight away.
  - **Tabs:** tabs inside a Page (Home feed, Search, Library type, Artist view-all section) are part of the route. Switching tabs replaces the current entry, and scroll offsets are stored per (`PageId`, tab).
  - **Search:** typing never navigates. Suggestions (debounced 300 ms) appear in a dropdown owned by the shell header. Submitting a query or picking a suggestion pushes `Search { query, tab }`. The 10-entry search history is persisted in `Settings`.
  - **Overlays:** any navigation closes the drawer and the maximized player. Settings is a modal and never enters the Back stack.
  - **Deletions:** deleting a playlist or folder removes its entries from both the back and forward stacks.
  - **Start and reset:** the app always opens at Home. After a new sign-in the Back stack starts empty, at Home.
- **Page data loading:**
  - **`PageId`:** each navigation stamps a new `PageId(u64)`. The shell drops any `Message::Page(id, _)` whose id isn't the current page's. The same id also keys the saved scroll offset.
  - **Cancellation:** a Page's state holds the `abort_on_drop` handle of its load tasks, so replacing the Page cancels work still in flight.
  - **`Remote<T>`:** each Page holds its data as `enum Remote<T> { Loading, Loaded(T), NotFound, Failed(Arc<Error>) }`. A 404 shows a not-found message, and a failure shows an inline error with Retry. When `Fresh` fails after `Cached` has been shown, the cached data stays on screen and the error is only logged.
  - **Caching:** reads use `Cached` then `Fresh` from `syzygy-catalog` (ticket 05). The binary keeps no second in-memory cache of Catalog data. Revisiting a Page refreshes it through `Fresh`.
  - **Focus refresh:** Home also refetches on `window::Event::Focused`, at most every 5 minutes.
- **Queue-filling loads belong to playback:** sone's `fetchRemaining` keeps appending pages to the queue after the user navigates away. So the Page hands over a `PlayRequest { source, first_page, continuation }`, and playback owns the abortable continuation task. Starting a new source cancels it. How appends interact with shuffle and wrapping is decided in 08.
- **Drawer:** the queue, suggested, lyrics and credits tabs are drawer state, not Pages. Each data tab holds a `Remote<_>` for the current track id only. It loads while the tab is visible, and when the track changes the old data is dropped and the open tab reloads.
- **Images:** an `Images` cache at the `App` level (the player bar needs it outside the Shell) maps URL → `Loading | Ready(image::Handle) | Failed`. It is an LRU with a byte cap.
  - Pages request covers with `Action::FetchImages(urls)` when their data arrives.
  - The cache skips duplicates and fetches the bytes through the catalog, which caches them on disk, about 6 at a time.
  - `view` draws a placeholder until an image is ready.
  - Once long lists are windowed, a list requests only the rows near the viewport.
- **No ADR.** None of these decisions is both hard to reverse and surprising.
