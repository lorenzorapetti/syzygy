# 16: Cover images and icons

**What to build:** covers fade in on Home and in the sidebar once they're loaded, with a placeholder before, and scrolling stays smooth. The Lucide icons replace any stand-in glyphs. See user story 38 and the spec's "Visual system". The prototype on branch `prototype/visual-system` is the reference for widget and style code.

**Blocked by:** 15

**Status:** ready-for-agent

- [x] An App-level image LRU with a byte cap, URL → `Loading | Ready(Handle) | Failed`, keyed by cover id. It outlives the Shell
- [x] Bytes are fetched through the catalog (disk-cached), about 6 at a time, deduplicated. Pages request them with `Action::FetchImages`
- [x] Covers load from a `sensor().on_show` placeholder, are decoded off-thread and passed through `image::allocate`, then fade in with an `Animation`
- [x] `window::frames()` is subscribed only while something is animating
- [x] Lucide 0.563 SVGs are embedded with `include_str!`, with one cached `svg::Handle` per icon, tinted via `svg::Style::color`. Filled icons are swapped to `currentColor`
- [x] Styling uses `const` tokens from sone's `deriveTheme("#A855F7", "#130F1A")` output and one plain style fn per styled widget over `Theme::custom`

## Comments

Implemented. Notes for later tickets:

- **Cache.** `images::Images` lives on `App`, so it outlives the Shell. Its `update` is pure and returns `Effect::Fetch(url)` and `Effect::Allocate(url, handle)`; `App::run_images` runs them. A slot is `Loading | Allocating | Ready | Failed`. It's keyed by the sized URL, not the bare cover id: the same album at two sizes takes two entries. Up to 6 fetches run at once, a URL is asked for only once, and a failed cover isn't fetched again this run. The cap is 256 MB of decoded RGBA. Over it, the cover shown longest ago goes. Being drawn counts as shown (a `Cell` touched in `Images::cover`), so a cover that stays on screen isn't evicted. `Ready` holds the `Allocation`, so an evicted cover leaves the GPU.
- **Fetch.** `Catalog::image(url)` reads the disk cache (`CacheTier::Image`, tag `image`; stale counts as a hit) or calls the new `TidalClient::get_image`, an unauthenticated GET outside the rate gate. `images::fetch` decodes with the `image` crate on `spawn_blocking`.
- **URLs.** `Cover::url(size)` snaps to TIDAL's sizes as sone does: 160/320/640/1280 for images, 160/320/480/750 for artists, 550x400 for promos, and URLs as given. `page::cover(images, cover, size, wanted)` asks for twice the display size, so the drawer's large cover will get 640 or more.
- **Showing a cover.** `Images::cover` draws the image, fading in over 220 ms through an `Animation`, or a placeholder in a `sensor` keyed by the URL that anticipates 200px. Its `on_show` sends a Page message (`home::Message::CoverWanted`, or `page::Message::CoverWanted` for a Page with no messages of its own), which becomes `Action::FetchImages(urls)`. The Shell turns that into `app::Message::Images(Wanted)`. `window::frames()` is subscribed only while `Images::is_animating`. The Allocated message carries its own time, so `update` doesn't read the clock.
- **Sidebar.** It has no Library lists yet, so there are no sidebar covers. Ticket 19 should draw them through `page::cover` / `Images::cover`, and they'll fade in the same way.
- **Icons.** `icons::icon(Icon, size, color)` takes one `LazyLock` `svg::Handle` per icon. These icons are embedded with `include_str!` from `crates/syzygy/icons/`, copied from the prototype's lucide-static 0.563.0 set: chevron-left, chevron-right, house, library, search and x. They replace the ‹ › and × glyphs, and the sidebar and header have their icons. `icons::filled` swaps the root's `fill="none"` for `currentColor`, but nothing uses it until Play, Pause and Heart. To add an icon, put its SVG in `icons/` and add it to `Icon` and `SOURCES`.
- **Style.** `style.rs` holds the `deriveTheme("#A855F7", "#130F1A")` tokens (only the ones in use), `style::theme` over `Theme::custom`, and the shared style functions `placeholder` and `icon_button`. The Shell, Home, toasts, login and the fatal screen now use the tokens instead of `extended_palette()` and hardcoded colours.
- **Tested:** 3 `Cover::url` tests, plus 5 image-cache tests covering 6 at a time with dedupe, a finished fetch making room, failed covers not being re-fetched, eviction under the byte cap, and a cover that's still drawn not being evicted. That's 146 tests in the workspace. **Run:** with a stored Session, Home fetched 140 covers on the first run with no fetch, decode or allocate failures. The next run took 83 straight from the disk cache. I didn't look at the fade or scroll smoothness by eye.
