# 14: Catalog crate and Shell with Back stack

**What to build:** the signed-in Shell, with the sidebar frame, the header with back and forward, and navigation between Pages. Page data comes through `syzygy-catalog` with stale-while-revalidate, so a revisited Page shows instantly and updates quietly. Demo with a minimal Home that lists its section titles. See user stories 25–28, 31, 34–37 and the spec's "Bridge between engines and iced" and "App state, messages and navigation".

**Blocked by:** 13

**Status:** ready-for-agent

- [ ] `syzygy-catalog` is the binary's only way to reach the Catalog. It owns the stale-while-revalidate policy over `DiskCache` (sone's tiers and TTLs), cache keys and tags. Reads return a stream of `Cached(v)` then `Fresh(v)`, run with `Task::run`. sone's home-cache encode/decode tests pass
- [ ] `Services` is a struct of cheap `Clone` handles built in `boot` and stored in `App`, with no globals
- [ ] The Shell layout: the 280px sidebar frame, the 64px header with back and forward, search and avatar placeholders, and content capped at a maximum width
- [ ] Each Page is a module with `State`, `Message`, `update -> Action` and `view`. Pages never touch `Services`. Each navigation stamps a new `PageId`, and messages for other ids are dropped. A Page's loads are aborted when it's dropped
- [ ] Back stack: entries are `Route` + scroll offset, capped at 50. Navigating somewhere new clears forward. Back and forward work from the header buttons, the mouse side buttons and Alt+←/→, and going back restores the scroll offset
- [ ] Page data is `Remote<T>`. A `Fresh` failure after `Cached` keeps the cached data and only logs. Not-found and failed-with-Retry render inline
- [ ] Toasts are Shell state (info and error, auto-dismiss)
- [ ] A minimal Home Page proves the path: it reads the home feed through the catalog and lists its section titles
