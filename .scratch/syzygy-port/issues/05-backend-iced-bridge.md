# Backend-to-iced bridge

Type: grilling
Status: resolved
Blocked by: 01, 02

## Question

What replaces Tauri's command/event bridge? How do Tauri commands become calls the iced app makes (async fns run as `Task`s, or a service handle), and how do backend events (`emit` from audio, MPRIS input and output) reach `update` (channels into a `Subscription`)? Who owns long-lived backend state such as the audio engine, the API client and the cache?

## Answer

- **Ownership:** `Services` is a struct of cheap `Clone` handles (each one an `Arc` inside): the player, `PositionCell`, MPRIS handle, catalog, reporter, TIDAL client and store. It is built in `boot`, stored as a field of the iced `App` state, and cloned into `Task` futures. There are no globals.
- **`TidalClient` is `Clone` and its methods take `&self`.** Tokens live in an internal `RwLock`, and a single-flight refresh means concurrent 401s trigger only one refresh. This replaces sone's `Mutex<TidalClient>`, which ran every request one at a time.
- **The `syzygy-audio` API is async.** Replies come over a `tokio::oneshot` instead of the blocking `rx.recv()` in `send_cmd`, and calls run as `Task::perform(player.x(..), Message::..)`. Nothing that blocks runs on the UI thread.
- **Engine events** ([ADR 0003](../../../docs/adr/0003-engine-events-via-boot-created-receivers.md)): `boot` creates each tokio unbounded channel and its engine, and keeps the receiver in an `EventSource<T>`. `subscription()` returns `Subscription::run_with(source.clone(), ..)`, with one subscription per engine: `Message::Audio(audio::Event)`, `Message::Mpris(mpris::Event)` and `Message::Tidal(tidal::Event)`. These subscriptions are always on.
- **Playback logic is pure** ([ADR 0002](../../../docs/adr/0002-playback-logic-returns-effects.md)): it changes state and returns `Vec<Effect>`. A thin `run(effects, &Services) -> Task<Message>` performs them, and tests assert on state plus effects. Which effects exist is up to 08.
- **Catalog reads return a `Stream`**: `Cached(v)` and then `Fresh(v)`, or a single value. The app runs it with `Task::run`, so the same message arrives once or twice, and screens that are no longer showing ignore it or abort the task. Library mutations are plain `async fn`s that invalidate tags.
- **Tokens and settings:** `syzygy-tidal` emits `TokensRefreshed(tokens)` and `SessionExpired`. `update` is the only writer of `Settings`. It saves through a `Task` (encrypt and write on `spawn_blocking`). `SessionExpired` sends the user back to login (UX in 06).
- **Playback position:** `syzygy-audio` exposes `PositionCell`, which holds the current source (`Pipeline(gst::Pipeline)` or `Alsa{frames, rate}` atomics). The audio thread swaps the source in when the backend changes. `get()` doesn't block and runs today's `GetPosition` calculation on the caller's thread, which replaces the round-trip. `time::every(250ms)` runs only while a track is playing, and the MPRIS position closure reads the same cell. Track end comes only from `audio::Event::TrackFinished`, and `is_finished` goes away.
- **Reporter:** `Reporter::new(client.clone(), store, …)` is built in `boot` and starts its own flush loop with `tokio::spawn`. `update` reaches it only through `Report…` effects, which are non-blocking enqueues.
- **Errors in messages** are `Result<T, Arc<CrateError>>`, so they stay typed and `Clone`. How errors are shown is a separate open item on the map.
- **Shutdown:** iced's automatic exit on close is off. The window close request and `mpris::Event::Quit` both map to `Message::Quit`, which runs `Task::batch` (stop, flush the report queue with a ~2 s timeout, save settings) `.chain(iced::exit())`. MPRIS `Raise` maps to `window::gain_focus`.
