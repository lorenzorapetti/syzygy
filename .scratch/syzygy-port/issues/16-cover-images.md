# 16: Cover images and icons

**What to build:** covers fade in on Home and in the sidebar once they're loaded, with a placeholder before, and scrolling stays smooth. The Lucide icons replace any stand-in glyphs. See user story 38 and the spec's "Visual system". The prototype on branch `prototype/visual-system` is the reference for widget and style code.

**Blocked by:** 15

**Status:** ready-for-agent

- [ ] An App-level image LRU with a byte cap, URL → `Loading | Ready(Handle) | Failed`, keyed by cover id. It outlives the Shell
- [ ] Bytes are fetched through the catalog (disk-cached), about 6 at a time, deduplicated. Pages request them with `Action::FetchImages`
- [ ] Covers load from a `sensor().on_show` placeholder, are decoded off-thread and passed through `image::allocate`, then fade in with an `Animation`
- [ ] `window::frames()` is subscribed only while something is animating
- [ ] Lucide 0.563 SVGs are embedded with `include_str!`, with one cached `svg::Handle` per icon, tinted via `svg::Style::color`. Filled icons are swapped to `currentColor`
- [ ] Styling uses `const` tokens from sone's `deriveTheme("#A855F7", "#130F1A")` output and one plain style fn per styled widget over `Theme::custom`
