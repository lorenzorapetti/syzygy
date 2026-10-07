# Visual system prototype

Type: prototype
Status: resolved
Blocked by: 01

## Question

What should syzygy look like in iced? Make a rough prototype: theme tokens taken from sone's default dark theme, skeletons for the sidebar, player bar and now-playing drawer, cover image loading, and a long track list. Then decide on the widget and style approach.

## Answer

Variant **A (faithful sone)** wins: 280px sidebar with library covers, a 64px header (back/forward, search, avatar) centred vertically, a playlist hero, 60px track rows with 40px covers (#, title + badges + artist, album, time), a 90px player bar split 30/40/30, and a now-playing drawer that slides up over everything above the player bar (45% cover, 55% Queue/Lyrics/Credits tabs, 80% black backdrop).

Widget and style approach:
- **Theme:** `Theme::custom(Palette)` covers only iced's defaults. Every styled widget uses a plain `fn(&Theme, Status) -> Style` that reads `const` tokens, which are sone's `deriveTheme("#A855F7", "#130F1A")` output, hardcoded. No custom `Theme` type or `Catalog` impls.
- **Track lists:** windowed, with a fixed row height, spacers and ±8 overscan driven by `scrollable::on_scroll(Viewport)`. This is required: building all 5000 rows costs 6.6ms of `view` per message.
- **Covers:** each placeholder is wrapped in `sensor().on_show`, the image is decoded off-thread, then `image::allocate`, then faded in with `Animation`, held in an LRU keyed by cover id. `window::frames()` is subscribed only while something animates.
- **Drawer:** `stack` + `opaque` + `responsive`, with a top spacer driven by `Animation`.
- **Fixed-height bars:** use `container(..).center_y(h)`, not `.height(h)`. A fixed-height container pins its content to the top, which left the header and player bar with no top padding.
- **Still to do in the real build:** cap the content width on wide windows, use sone's icon set, Lucide 0.563 (lucide-static SVGs embedded with `include_str!`, one cached `svg::Handle` per icon, tinted with `svg::Style::color`; sone's filled icons such as Play, Pause, Skip and a favourited Heart get `fill="none"` swapped for `currentColor`), give the drawer cover an explicit square box, and fetch 640px or larger covers for it.

Prototype: branch `prototype/visual-system` (`prototypes/visual-system/`, run with `cargo run --release --manifest-path prototypes/visual-system/Cargo.toml`).

## Comments

**2026-10-07 — prototype built, awaiting a variant pick.**

Throwaway crate: `prototypes/visual-system/` (own Cargo.toml, not part of syzygy). Run:

    cargo run --release --manifest-path prototypes/visual-system/Cargo.toml -- --variant A   # B, C; --drawer, --full

←/→ cycles variants (or the yellow bar), `D` toggles the drawer, `W` toggles windowed/full list, Space plays/pauses. The bar shows rows built, view time, cover cache stats and the scroll offset.

- **A, Faithful sone:** 280px sidebar with covers, hero, 60px rows with 40px covers, 90px player bar, drawer slides up over everything above the player bar (45/55, 80% backdrop).
- **B, Rail + docked panel:** 60px icon rail, dense 36px text rows, the now-playing panel docks on the right and pushes content, 72px bar with the scrubber along its top edge.
- **C, Top transport + player page:** player strip at the top, text-only sidebar tree, playlist grouped by album, now-playing replaces the content area.

Findings that hold whichever variant wins:
- **Style approach:** `Theme::custom(Palette)` covers only iced's defaults. Every widget we care about gets a plain `fn(&Theme, Status) -> Style` reading `const` tokens. The tokens are sone's `deriveTheme("#A855F7", "#130F1A")` output, hardcoded. That was enough for every widget here, so we don't need a custom `Theme` type implementing each `Catalog`.
- **Long lists:** windowing (fixed row height, spacers, ±8 overscan via `on_scroll(Viewport)`) builds about 40–70 rows and `view` takes about 0.07ms. Building all 5000 rows takes 6.6ms of `view` alone, before layout, on every message (including the 250ms position tick). Windowing is required. It also works over fixed-height album blocks (variant C).
- **Covers:** wrapping each placeholder in `sensor().on_show` loads the cover; it is then decoded off-thread, `image::allocate`d, and faded in with `Animation`, plus an LRU keyed by id. This works without flicker, and `window::frames()` is subscribed only while a fade or the drawer is animating.
- **Drawer:** `stack` + `opaque` + `responsive` with a top spacer driven by `Animation` gives sone's slide-up. iced pitfall: a `Fill` child inside a `Shrink` container collapses to 0 width.
- **Icons:** Unicode glyphs fall back to colour emoji (⏮ ⏭ render orange). We need a real icon set: SVGs via the `svg` feature, or an icon font.

**2026-10-07 — rechecked fullscreen (3050×1666 logical).**
- **Wide windows:** none of the variants caps the width of the track list. At 3050px, A's title and album columns are about 1100px apart, and in C the durations sit far from the titles. The winner needs a max content width (or capped column widths). sone's grid `minmax()` columns hide this on the web.
- **Large covers:** an `image` with `width(Fill)` but `Shrink` height keeps its source pixel height, so `ContentFit::Contain` has nothing to scale into. The hero cover needs an explicit square box, e.g. `width(Fill).max_width(560).height(560)`, with the image at `Fill`×`Fill`. Request 640px or 1280px covers for the drawer; the 160px synthetic ones look pixelated when scaled up.
- `--shot <path>` makes the prototype screenshot its own window (via `window::screenshot`) and exit, so overlapping windows can't spoil a capture.
