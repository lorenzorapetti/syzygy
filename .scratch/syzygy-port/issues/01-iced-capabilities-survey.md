# iced capabilities survey

Type: research
Status: resolved

## Question

What does the current stable iced offer for syzygy's needs? Specifically: latest version and its API shape (application builder, `Task`, `Subscription`); running alongside tokio (executor feature, spawning blocking/async backend work); turning external channels (crossbeam/tokio) into a `Subscription`; loading and caching remote images (cover art); theming and custom styles; long or virtualized lists (playlists with thousands of tracks); overlays, drawers and context menus; sliders for seek and volume. Note any ecosystem crates (e.g. iced_aw) and their compatibility with that version.

## Answer

Target **iced 0.14.0** (latest stable, released 2025-12-07; master is 0.15.0-dev). Enable the features `tokio, image, svg, lazy, advanced`. The app uses the `iced::application(boot, update, view).subscription(..).theme(..)` builder, and `update` returns a `Task`.
- **tokio:** with the `tokio` feature, iced owns the runtime and enters it around update and tasks, so `tokio::spawn`/`spawn_blocking` work there, but `#[tokio::main]` is out. `Subscription::run`/`run_with` take `fn` pointers, so external channels are bridged with `iced::stream::channel` plus either the sender-handoff pattern or a global receiver slot.
- **Cover art:** `image::Handle::from_bytes`/`from_rgba` plus `image::allocate`, with `sensor` for lazy loading (see the `gallery` example). We need our own URL→Handle cache.
- **Lists:** there is no virtualized list upstream (only draw culling, `lazy`, and a non-virtual `table`). Long playlists need manual windowing via `scrollable::on_scroll(Viewport)` and fixed row heights.
- **Overlays and sliders:** `stack`/`opaque`/`mouse_area`/`pin`/`float`/`tooltip` and `Animation` cover modals, drawers and toasts. Core has no context menu, so use `iced_aw` 0.14.1 (`context_menu`, compatible with iced 0.14) or build one ourselves. `slider` has `on_change` + `on_release` for seeking, plus custom styles. Theming is `Theme::custom(name, Palette)` plus per-widget `.style` closures.

[findings](../research/iced-capabilities.md)
