# 14: Catalog crate and Shell with Back stack

**What to build:** the signed-in Shell, with the sidebar frame, the header with back and forward, and navigation between Pages. Page data comes through `syzygy-catalog` with stale-while-revalidate, so a revisited Page shows instantly and updates quietly. Demo with a minimal Home that lists its section titles. See user stories 25–28, 31, 34–37 and the spec's "Bridge between engines and iced" and "App state, messages and navigation".

**Blocked by:** 13

**Status:** ready-for-agent

- [x] `syzygy-catalog` is the binary's only way to reach the Catalog. It owns the stale-while-revalidate policy over `DiskCache` (sone's tiers and TTLs), cache keys and tags. Reads return a stream of `Cached(v)` then `Fresh(v)`, run with `Task::run`. sone's home-cache encode/decode tests pass
- [x] `Services` is a struct of cheap `Clone` handles built in `boot` and stored in `App`, with no globals
- [x] The Shell layout: the 280px sidebar frame, the 64px header with back and forward, search and avatar placeholders, and content capped at a maximum width
- [x] Each Page is a module with `State`, `Message`, `update -> Action` and `view`. Pages never touch `Services`. Each navigation stamps a new `PageId`, and messages for other ids are dropped. A Page's loads are aborted when it's dropped
- [x] Back stack: entries are `Route` + scroll offset, capped at 50. Navigating somewhere new clears forward. Back and forward work from the header buttons, the mouse side buttons and Alt+←/→, and going back restores the scroll offset
- [x] Page data is `Remote<T>`. A `Fresh` failure after `Cached` keeps the cached data and only logs. Not-found and failed-with-Retry render inline
- [x] Toasts are Shell state (info and error, auto-dismiss)
- [x] A minimal Home Page proves the path: it reads the home feed through the catalog and lists its section titles

## Comments

Implemented. Notes for later tickets:

- **Catalog reads.** `Catalog::home_feed(slug)` returns a `BoxStream` of `Read<T>`: `Cached(T)` and/or `Fresh(Result<T, Arc<Error>>)`. A hit inside the TTL is only `Cached`, a miss only `Fresh`, and a stale hit both. `swr::read` takes an `Entry { key, tier, tags, encode, decode }` and a fetch closure; new reads add a method on `Catalog` that builds one. A failed refresh is remembered with `mark_refresh_attempt`, and a stale entry isn't refreshed again for 5 minutes. Nothing is marked before the fetch, so an aborted read doesn't hold off the next one. sone's `mark_in_flight` single-flight wasn't kept.
- **A refused refresh keeps the cached copy.** When the encoder refuses a value (an empty home feed), a refresh after `Cached` yields nothing, so Home isn't blanked. On a miss, the refused value is still shown.
- **Refreshes die with their Page.** A stale refresh runs inside the Page's abortable load, so leaving early leaves the entry stale until the next read. sone refreshed on a detached `tokio::spawn` instead.
- **`Action::Load(Load)` instead of `Run(Task)`.** A Page can't build a catalog `Task` without `Services`, so it returns a `Load` (data). The Shell runs it (`shell::read` maps each `Read` into the Page's message) and keeps its `abort_on_drop` handle next to the Page in `Current`. `Action::Navigate` returns with the first Page that has cards.
- **Scroll restore.** One `operation::scroll_to(offset)` when a Page opens. iced keeps the absolute offset and clamps it at layout, so it lands once the data is tall enough. The offset comes from `on_scroll` and is stored per Back stack entry.
- **Navigating to the current route** scrolls to the top and adds no entry.
- **Toasts.** `Shell::toast(Kind, text)` shows up to 4, each for 3 s (as in sone), with a close button. Nothing calls it yet.
- **Route** has only `Home`, so back and forward have nowhere to go until more Pages land. The search box and avatar are placeholders, and the arrows are text until the Lucide icons land.
- **Not tested** (per the spec): the Shell, navigation and Pages. Tested: sone's five home-cache encode/decode tests, and the read policy over a real `DiskCache` (miss, hit, failure, refused encode and decode, tag invalidation). The stale path isn't covered, because entries can't be aged. The app launches to login. The signed-in Shell wasn't run, because no Session is stored on this machine.
