# Playback logic returns effects instead of calling the engines

syzygy ports sone's TypeScript queue, shuffle, repeat, autoplay and history logic into the iced `update`, and wants it tested at that level. But the engines behind it are a real GStreamer player, a D-Bus MPRIS server and a TIDAL reporter. So the playback logic is pure: it changes state and returns a list of plain-data `Effect`s (`Play`, `Pause`, `Seek`, `ArmNext`, `MprisUpdate`, `Report…`). A thin runner turns those effects into `Task`s against the `Services` handles. Tests assert on the state and on the effects, with no fakes and no traits in the engine crates.

## Considered Options

- **Traits over the services, with fakes in tests.** Rejected: every engine crate would grow an interface whose only purpose is tests, and tests would assert on recorded calls rather than on data.
- **`Services` as an `Option`, absent in tests.** Rejected: tests could then check only state, never what the logic asked the engines to do, and "what plays next" is exactly what matters.

## Consequences

Don't "simplify" by calling `services.player` from `update`. Calling the engines straight from `update` skips the effect list, and the tests can no longer see what the logic did.
