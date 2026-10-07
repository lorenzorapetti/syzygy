# iced capabilities survey (for syzygy)

Researched 2026-10-06 against the published crate sources on crates.io (iced 0.14.0, iced_widget 0.14.2, iced_futures 0.14.0, iced_runtime 0.14.0, iced_winit 0.14.0, iced_aw 0.14.1), the iced repo's `0.14` branch examples and `CHANGELOG.md`, and the crates.io API. Ticket: [01-iced-capabilities-survey](../issues/01-iced-capabilities-survey.md).

## TL;DR

- **Target `iced = "0.14"`** (0.14.0, released 2025-12-07). It is still the latest stable release; `master` is `0.15.0-dev` and the CHANGELOG's `[Unreleased]` section is empty. MSRV 1.88, edition 2024. Patch releases exist for subcrates: `iced_widget` 0.14.2 and `iced_winit` 0.14.1.
- **Features for syzygy:** `iced = { version = "0.14", features = ["tokio", "image", "svg", "lazy", "advanced"] }`. Add `sipper` if we want `Task::sip` for progress streams.
- The app is built with the `iced::application(boot, update, view)` builder, plus `.subscription(..)`, `.theme(..)` and `.title(..)`. `update` returns a `Task<Message>`.
- **tokio:** with the `tokio` feature, iced creates and owns a `tokio::runtime::Runtime` and **enters it** around `boot`, `update`, `subscription` and task polling. That means `tokio::spawn` and `spawn_blocking` work inside `update` and inside tasks.
- **External channels:** `Subscription::run` and `run_with` take **plain `fn` pointers, not capturing closures**. Two ways to bridge a channel: (a) a worker subscription that hands a `Sender` back to the app as a `Message`, which is the documented pattern, or (b) `run_with(id, ..)` pulling a receiver from a global slot.
- **Images:** the built-in `image` widget takes `image::Handle::from_bytes` or `from_rgba`. Fetch with reqwest, decode off-thread, and optionally `image::allocate` to avoid pop-in. The `gallery` example is a full model for lazily loading cover art with `sensor`. iced has no HTTP image cache, so we write our own (in memory plus on disk).
- **No virtualized list upstream.** 0.14 culls drawing in `column`/`row` but still lays out every child. For playlists with thousands of tracks, build manual windowing from `scrollable::on_scroll(Viewport)` with fixed row heights and spacers. `lazy` and `keyed_column` are only partial helps.
- **Overlays:** `stack` + `opaque` + `mouse_area` give modals, drawers and scrims, as the `modal` and `toast` examples show. There are also `float`, `pin`, `tooltip`, `pick_list`/`combo_box` (built-in overlay menus), and the `Animation` API for sliding a drawer. There is **no context menu in core**: use `iced_aw::ContextMenu` (feature `context_menu`) or roll our own with `mouse_area::on_right_press` + `on_move` + `stack`/`pin`.
- **Sliders:** `slider` takes `on_change` plus `on_release`, which is what a seek bar needs: preview while dragging, commit on release. Styling via `slider::Style { rail, handle }`.
- **Ecosystem:** `iced_aw` 0.14.x targets iced 0.14 (it depends on `iced_widget ^0.14.2`, `iced_core ^0.14.0` and `iced_fonts ^0.3`). There is no maintained virtual-list crate for upstream iced 0.14. The pop-os/libcosmic fork has a `list.rs` widget, but upstream does not.

---

## 1. Version and API shape

| crate | latest stable | date |
|---|---|---|
| iced | 0.14.0 | 2025-12-07 |
| iced_widget | 0.14.2 | |
| iced_winit | 0.14.1 | |
| iced_core / iced_futures / iced_runtime / iced_wgpu | 0.14.0 | |
| iced_aw | 0.14.1 | 2026-04-27 |
| iced_fonts | 0.3.0 | 2025-12-08 |

Sources: crates.io API (`/api/v1/crates/iced`), [GitHub releases](https://github.com/iced-rs/iced/releases) (latest tag `0.14.0`), [CHANGELOG](https://github.com/iced-rs/iced/blob/master/CHANGELOG.md). The [master Cargo.toml](https://github.com/iced-rs/iced/blob/master/Cargo.toml) is `0.15.0-dev`.

### Default and relevant feature flags (iced 0.14.0 `Cargo.toml`)

- **Default:** `wgpu`, `tiny-skia` (software fallback), `crisp`, `web-colors`, `thread-pool`, `linux-theme-detection`, `x11`, `wayland`.
- **`tokio`:** switches the default executor to tokio. `iced_futures::backend::default` picks tokio over smol over thread-pool.
- **`image`:** the image widget plus the `image` crate with default codecs. `image-without-codecs` gives the widget without bundled codecs.
- **`svg`:** SVG widget, for icons.
- **`lazy`:** the `lazy` widget.
- **`advanced`:** custom widgets and overlays (`iced::advanced::*`).
- **`canvas`:** custom drawing.
- **`markdown`:** the markdown widget.
- **`sipper`:** `Task::sip` for progress-plus-result streams.
- **`debug` / `hot` / `time-travel`:** the comet devtools.
- **`tester`:** end-to-end testing.

### Application builder

From `iced/src/application.rs`:

```rust
pub fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)   // boot, update, view
        .title("syzygy")                                   // &'static str or Fn(&State)->String
        .subscription(App::subscription)                   // Fn(&State) -> Subscription<Message>
        .theme(App::theme)                                 // Theme value or Fn(&State) -> Option<Theme>
        .style(..)                                         // Fn(&State,&Theme) -> theme::Style (app bg/text)
        .font(include_bytes!("../fonts/…"))                // register fonts
        .default_font(Font::with_name("Inter"))
        .window_size((1280.0, 800.0))
        .executor::<MyExecutor>()                          // optional custom executor
        .run()
}
```

- `boot` is `Fn() -> C` where `C: IntoBoot`. It can return either `State` or `(State, Task<Message>)`, so startup work such as restoring a session can be a boot task.
- `update(&mut State, Message) -> impl Into<Task<Message>>`, and plain `()` works too.
- `view(&State) -> impl Into<Element<Message>>`.
- `iced::daemon(..)` is the multi-window or windowless variant. syzygy doesn't need it, since the miniplayer is out of scope.

### `Task` (iced_runtime 0.14.0 `task.rs`, re-exported as `iced::Task`)

- **Constructors:** `none`, `done(v)`, `perform(future, map)`, `run(stream, map)`, `sip(sipper, on_progress, on_output)` (needs the `sipper` feature), `future(f)`, `stream(s)` and `batch(iter)`.
- **Combinators:** `map`, `then`, `chain`, `collect`, `discard`, `and_then` and `map_err` (both on `Task<Result>`), and `abortable() -> (Task, Handle)`.
- **`Handle`:** has `abort` and `abort_on_drop`. This is useful for cancelling an in-flight search when the query changes.
- **Blocking work:** `iced_runtime::task::blocking(|sender| ..)` and `try_blocking` spawn a `std::thread` and stream what it sends. **They are not re-exported under `iced::task`**, which only exports `Handle, Task, Never, Sipper, Straw, sipper, stream`. With tokio it is simpler to call `Task::perform(async { tokio::task::spawn_blocking(..).await }, ..)`, which is what the `gallery` example does.
- **Widget operations:** a task can also run a widget operation, for example `scrollable::scroll_to` or `snap_to` (since 0.14 these can work on a single axis), focus, and `window::*` actions.

### `Subscription` (iced_futures 0.14.0 `subscription.rs`)

- **Constructors:** `none`, `run(fn() -> Stream)`, `run_with(data: impl Hash, fn(&D) -> Stream)` and `batch`.
- **Combinators:** `with(value)`, `map` and `filter_map`.
- **Identity:** a subscription's identity is the hash of its function pointer plus its data. While `subscription()` keeps returning it the stream stays alive, and the stream is dropped as soon as it stops being returned.
- **Built-in subscriptions:** `iced::time::every(Duration)` and `time::repeat`, which are useful for the playback-position tick. Also `iced::keyboard::listen()` (0.14 unified the keyboard subscriptions into one), `iced::event::listen*` and `window::*` events.

## 2. Running alongside tokio

- `iced_futures::backend::native::tokio::Executor = tokio::runtime::Runtime`, and `Executor::new()` calls `Runtime::new()`, which is multi-threaded. Source: `iced_futures/src/backend/native/tokio.rs`.
- `iced_winit/src/lib.rs` wraps `program::Instance::new` (boot), `program.update(message)`, `program.subscription()` and stream polling in `runtime.enter(..)`. So the iced-owned runtime is the ambient runtime inside `update`, and `tokio::spawn`, `Handle::current()` and reqwest all work there.
- `run()` blocks the main thread with winit's event loop, so **the app must not be inside `#[tokio::main]`**. Any backend runtime is either iced's own runtime, reached via `Handle::current()` captured during boot, or a custom `Executor` passed through `.executor::<E>()`. `Executor::new()` takes no arguments, so a custom executor that shares a runtime has to fetch it from a global (`OnceLock<Runtime>`).
- sone's `src-tauri` already uses `tokio` (full), `tokio-stream`, `tokio-util` and `crossbeam-channel`. These fit as long as backend calls are made from tasks or subscriptions, or from `update` via `tokio::spawn`.

## 3. External channels as a `Subscription`

`Subscription::run` and `run_with` take `fn` pointers, so a builder cannot capture a `Receiver`. There are three documented or idiomatic options.

**A. Worker hands a sender back to the app.** This is the pattern shown in the `Subscription::run` doc comment and the [`websocket` example](https://github.com/iced-rs/iced/tree/0.14/examples/websocket):

```rust
fn engine_events() -> impl Stream<Item = Message> {
    iced::stream::channel(100, async |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let _ = output.send(Message::EngineReady(tx)).await; // app stores tx, gives it to backend
        while let Some(ev) = rx.recv().await {
            let _ = output.send(Message::Engine(ev)).await;
        }
    })
}
// in subscription(): Subscription::run(engine_events)
```

**B. A receiver created outside iced.** Use this for the audio engine's crossbeam receiver, or for MPRIS's `tokio::mpsc::UnboundedReceiver`. Put the receiver in a global slot such as `static ENGINE_RX: Mutex<Option<Receiver<_>>>`, created at boot. Then call `Subscription::run(fn_that_takes_it)`. If you need several of them, use `run_with(id, ..)`. Inside:

- for tokio receivers: `tokio_stream::wrappers::UnboundedReceiverStream`
- for crossbeam receivers: `stream::channel` plus `spawn_blocking(move || while let Ok(ev) = rx.recv() { out.try_send(..) })`, or switch the engine to a tokio or futures channel.

**C. `iced::stream::channel` and `try_channel`.** These are thin helpers in `iced_futures/src/stream.rs` that turn an async closure that sends to `mpsc::Sender<T>` into a `Stream`. Options A and B both use them.

Practical note: the playback position can be a `time::every(250ms)` subscription that reads an atomic or shared state. That avoids pushing every tick through a channel, and the subscription can be turned off when nothing is playing.

## 4. Remote images (cover art)

- `iced::widget::image(handle)` builds on `image::Handle::from_path`, `from_bytes(impl Into<Bytes>)` and `from_rgba(w, h, pixels)` (`iced_core/src/image.rs`).
- **Widget options:** `.content_fit(ContentFit::Cover)`, `.border_radius(..)` (new in 0.14), `.opacity(..)`, `.scale(..)`, `.filter_method(..)` and `.rotation(..)`.
- **`iced::widget::image::allocate(handle) -> Task<Result<Allocation, Error>>`** (`iced_runtime/src/image.rs`) uploads an image explicitly. While you hold an `Allocation`, the image is guaranteed to draw in the very next frame, so there is no pop-in. iced 0.14 also added concurrent image decoding and uploading ([#3092](https://github.com/iced-rs/iced/pull/3092)).
- The **[`gallery` example](https://github.com/iced-rs/iced/tree/0.14/examples/gallery)** (features `tokio, sipper, image`, plus `reqwest 0.12`) is the reference for loading cover art lazily:
  - each card is wrapped in `sensor(card).on_show(|_| PoppedIn(id)).on_hide(PoppedOut(id))`
  - on `PoppedIn`, it downloads with reqwest, decodes with `tokio::task::spawn_blocking(|| image::load_from_memory(..).to_rgba8())`, builds a `Handle::from_rgba`, runs `.then(image::allocate)`, and stores the `Allocation`
  - on `PoppedOut`, it drops the allocation, which frees GPU memory
  - it uses `iced::Animation` for fade-in
- **Caching:** iced caches GPU textures per `Handle` id, so reusing a `Handle` clone is cheap. Creating a new `Handle` from the same bytes makes a new id. iced has **no HTTP or disk cache**, so syzygy needs a `HashMap<CoverKey, image::Handle>` (an LRU) plus an optional on-disk byte cache. TIDAL cover URLs are size-addressed (e.g. `/320x320.jpg`), so key the cache by URL.

## 5. Theming and custom styles

- `Theme` is an enum of built-ins plus `Theme::custom(name, Palette)` and `Theme::custom_with_fn(name, Palette, |p| palette::Extended)` (`iced_core/src/theme.rs`).
- `Palette { background, text, primary, success, warning, danger }`. `warning` is new in 0.14, and 0.14 reworked palette generation around Oklch ([#3028](https://github.com/iced-rs/iced/pull/3028)).
- Every built-in widget has a `Catalog` trait and takes `.style(|theme: &Theme, status| widget::Style { .. })` closures. Status is `Active`, `Hovered`, `Pressed`/`Dragged` or `Disabled`, depending on the widget. Built-in style functions such as `button::text`, `container::rounded_box` and `button::primary` can be used as presets.
- For one hardcoded dark theme matching sone, there are two options. The simplest is `Theme::custom("syzygy", Palette { .. })`, with per-widget style fns in a `theme` module reading `theme.extended_palette()`. The full-control option is our own `Theme` type implementing `theme::Base` and every widget `Catalog`; that is more work and is only worth it if the palette model is too limiting.
- `application(..).style(..)` sets the window background and text color.
- See the [`styling` example](https://github.com/iced-rs/iced/tree/0.14/examples/styling).

## 6. Long or virtualized lists

- **Upstream iced 0.14 has no virtualized list widget.** When an issue reported a lag in "iced's `List`", hecrj [replied](https://github.com/iced-rs/iced/issues/3429) "we don't have a `list` widget". That widget lives in the [pop-os/iced fork](https://github.com/pop-os/iced/tree/master/widget/src) (`list.rs`, used by libcosmic) and is not on upstream master.
- **What 0.14 gives:**
  - primitive culling in `column` and `row` ([#2611](https://github.com/iced-rs/iced/pull/2611)): offscreen children aren't drawn, but they are still built and laid out every `view`
  - `keyed_column`, which keeps widget state per key
  - `lazy(dep, view)`, which caches a subtree until `dep` (a `Hash`) changes, for example a playlist version
  - `scrollable(..).on_scroll(|Viewport| ..)`, where `Viewport` exposes `absolute_offset()`, `relative_offset()`, `bounds()` and `content_bounds()`, plus `.id(..)` and the `scroll_to`/`snap_to` operations
  - the `table` widget ([#3018](https://github.com/iced-rs/iced/pull/3018)): columns with header, width and alignment, plus separators, but **not virtualized**
  - `sensor` for show and hide callbacks
- **Recommended for track lists with thousands of rows: manual windowing.**
  1. Use fixed row heights.
  2. Store the offset from `on_scroll`.
  3. In `view`, build only rows `[first-overscan, last+overscan]`.
  4. Pad above and below with `space().height(n * ROW_H)`.

  This is simple and fast, and `lazy` can wrap the visible slice. A few thousand plain `row![text..]` rows may be acceptable without windowing, so that should be measured during the prototype. Cover thumbnails in rows should go through `sensor`.

## 7. Overlays, drawers and context menus

- **`stack![base, overlay]`:** layering, with `push_under` new in 0.14. Pair it with `opaque(..)` so clicks don't fall through, and with `mouse_area(..).on_press(Close)` for click-outside dismissal. The [`modal` example](https://github.com/iced-rs/iced/tree/0.14/examples/modal) shows exactly this, and the [`toast` example](https://github.com/iced-rs/iced/tree/0.14/examples/toast) shows a custom-overlay notification stack, which we can reuse for toasts.
- **The now-playing drawer** can be a `stack` layer aligned right or bottom, with its width or offset driven by `iced::Animation` (the app-level animation API added in 0.14, [#2757](https://github.com/iced-rs/iced/pull/2757)). Run a `window::frames()`-style redraw while it animates; gallery does this via its `Animate` message.
- **Other overlay widgets:**
  - `pin(content).x(..).y(..)` places content at absolute coordinates, which is useful for a context menu at the cursor
  - `float(content).scale/translate`
  - `tooltip(content, tip, Position::{Top, Bottom, Left, Right, FollowCursor})`, with `.delay(..)` new in 0.14
  - `pick_list` and `combo_box`, which open their own overlay menus and are fine for settings dropdowns
  - `hover(base, top)`, which shows `top` on hover and suits play-button-on-hover covers
- **`mouse_area`:** has `on_press`, `on_release`, `on_double_click`, `on_right_press`, `on_right_release`, `on_middle_*`, `on_scroll`, `on_enter`, `on_move(Point)`, `on_exit` and `interaction(..)`.
- **Context menus:** core has none. There are two options.
  - **`iced_aw::ContextMenu::new(underlay, || overlay_element)`** (feature `context_menu`) opens at the cursor on right-click. It supports `.style` and `.open(bool)`, and `iced_aw::DropDown` is also available.
  - **A DIY menu:** track the cursor with `on_move`, open on `on_right_press` by storing `(track_id, point)` in state, and render it with `stack![page, opaque(pin(menu).x(p.x).y(p.y))]`, with a full-screen `mouse_area` scrim to dismiss it. This needs no dependency and makes keyboard and nested-submenu behavior ours to define.
- `iced_aw` also has `menu` (menu bars with nested submenus) and `sidebar`.

## 8. Sliders (seek and volume)

- **`slider(range, value, on_change)`** has `.on_release(msg)`, `.step(..)`, `.shift_step(..)`, `.width`, `.height` and `.style(|theme, Status{Active, Hovered, Dragged}| slider::Style { rail: Rail { backgrounds, width, border }, handle: Handle { shape, background, .. } })`. `Style::with_circular_handle(r)` is also available. `vertical_slider` exists too. Source: `iced_widget/src/slider.rs`.
- **Seek pattern:**
  1. Keep `seek_preview: Option<f32>` in state.
  2. `on_change` sets the preview.
  3. `on_release` commits the seek to the engine.
  4. The view shows `seek_preview.unwrap_or(position)`.

  This keeps position ticks from fighting the drag.
- `progress_bar` (vertical support new in 0.14) can serve as a non-interactive buffer indicator.

## 9. Ecosystem crates and compatibility with 0.14

| crate | version | iced compat | notes |
|---|---|---|---|
| [iced_aw](https://crates.io/crates/iced_aw) | 0.14.1 (2026-04-27) | 0.14 (README table: iced 0.14 → iced_aw 0.13, 0.14) | Every widget is feature-gated, and the default is `full`, so set `default-features = false, features = ["context_menu", "menu", "drop_down"]`. Depends on `iced_widget ^0.14.2`, `iced_core ^0.14` and `iced_fonts ^0.3`. |
| [iced_fonts](https://crates.io/crates/iced_fonts) | 0.3.0 | 0.14 | Icon fonts (Bootstrap and others). An alternative is bundled SVG icons via the `svg` feature. |
| [sweeten](https://crates.io/crates/sweeten) | 0.14.0 | 0.14 | Drop-in "sweetened" versions of core widgets. Optional. |
| [iced_table](https://crates.io/crates/iced_table) | 0.14.0 | 0.14 | Superseded for us by the core `table`. Not virtualized. |
| virtual list | — | — | **None maintained for upstream 0.14.** The pop-os fork's `list` is not usable with upstream crates. |

## Known limitations and risks

- There is no virtual list, so large playlists need hand-rolled windowing, which is the main UI risk to prototype early.
- There is no context menu, menu bar or HTTP image cache in core. They come from iced_aw or our own code.
- `Subscription::run` and `run_with` take `fn` pointers, so channels must be bridged with globals or the sender-handoff pattern.
- `iced::application(..).run()` owns the main thread and the tokio runtime, so there is no `#[tokio::main]`.
- `task::blocking` is not re-exported by `iced`; use `tokio::task::spawn_blocking`.

## Sources

- iced 0.14.0 crate source: [docs.rs/iced/0.14.0](https://docs.rs/iced/0.14.0/iced/), [`application`](https://docs.rs/iced/0.14.0/iced/application/index.html), [`Task`](https://docs.rs/iced/0.14.0/iced/struct.Task.html), [`Subscription`](https://docs.rs/iced/0.14.0/iced/struct.Subscription.html), [`widget`](https://docs.rs/iced/0.14.0/iced/widget/index.html), [`widget::image::allocate`](https://docs.rs/iced/0.14.0/iced/widget/image/fn.allocate.html), [`stream::channel`](https://docs.rs/iced/0.14.0/iced/stream/fn.channel.html)
- iced_widget 0.14.2: [slider](https://docs.rs/iced_widget/0.14.2/iced_widget/slider/index.html), [scrollable](https://docs.rs/iced_widget/0.14.2/iced_widget/scrollable/index.html), [sensor](https://docs.rs/iced_widget/0.14.2/iced_widget/sensor/index.html), [mouse_area](https://docs.rs/iced_widget/0.14.2/iced_widget/struct.MouseArea.html), [table](https://docs.rs/iced_widget/0.14.2/iced_widget/table/index.html)
- iced_futures 0.14.0 tokio backend: [docs.rs](https://docs.rs/iced_futures/0.14.0/iced_futures/backend/native/tokio/index.html). iced_winit 0.14.0 `src/lib.rs` (`runtime.enter` calls).
- [iced CHANGELOG (0.14.0 section)](https://github.com/iced-rs/iced/blob/master/CHANGELOG.md), [releases](https://github.com/iced-rs/iced/releases)
- Examples (branch `0.14`): [gallery](https://github.com/iced-rs/iced/tree/0.14/examples/gallery), [modal](https://github.com/iced-rs/iced/tree/0.14/examples/modal), [toast](https://github.com/iced-rs/iced/tree/0.14/examples/toast), [websocket](https://github.com/iced-rs/iced/tree/0.14/examples/websocket), [styling](https://github.com/iced-rs/iced/tree/0.14/examples/styling), [slider](https://github.com/iced-rs/iced/tree/0.14/examples/slider), [lazy](https://github.com/iced-rs/iced/tree/0.14/examples/lazy), [table](https://github.com/iced-rs/iced/tree/0.14/examples/table), [scrollable](https://github.com/iced-rs/iced/tree/0.14/examples/scrollable)
- No virtual list upstream: [iced#3429](https://github.com/iced-rs/iced/issues/3429), [pop-os/iced widget dir](https://github.com/pop-os/iced/tree/master/widget/src)
- [iced_aw README / crates.io](https://crates.io/crates/iced_aw), [iced_aw context_menu docs](https://docs.rs/iced_aw/0.14.1/iced_aw/widget/context_menu/index.html)
- [The iced book: architecture](https://book.iced.rs/architecture.html)
