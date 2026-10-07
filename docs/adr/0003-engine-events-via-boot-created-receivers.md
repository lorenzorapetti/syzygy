# Engine events reach iced through receivers created in boot

The audio, MPRIS and TIDAL engines take an event sender when they are constructed. iced 0.14's `Subscription::run`/`run_with` take `fn` pointers, so they can't capture a receiver. The pattern iced documents (the websocket example) has the subscription create the channel and send its `Sender` to `update`. That would delay building each engine until that message arrived, and leave `Services` full of `Option`s. Instead, `boot` creates each channel and engine and keeps the receiver in an `EventSource<T>`: an `Arc<Mutex<Option<Receiver>>>` that hashes by a fixed id. `subscription()` returns `Subscription::run_with(source.clone(), ..)`, which takes the receiver once and streams it. Each engine has its own subscription, mapped into `Message`. Engine event channels are tokio `UnboundedSender`s.

## Consequences

The receiver can be taken only once. If `subscription()` ever stops returning an engine's subscription, its stream is dropped and that engine's events are lost until restart. Engine subscriptions are therefore always on and never conditional on app state.
