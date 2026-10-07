# 22: First sound: audio engine, pure playback update and player bar

**What to build:** clicking a track on an Album Page plays it bit-perfect, from that track to the end of the album, never wrapped around. The player bar shows what's playing, with play/pause, a seek bar that commits on release, volume and mute. This is the first slice through ADR 0002, ADR 0003 and ADR 0004, and the start of test Seam 1. See user stories 66, 88, 92, 94–95 and the spec's "Playback model" and "Bridge between engines and iced".

**Blocked by:** 17

**Status:** ready-for-agent

- [ ] `syzygy-audio`: sone's `AudioPlayer` (GStreamer and DirectAlsa) without proxy, `SignalPathTracker` without emits, `pipeline_probe` parsers (its 24 tests pass), `compute_norm_gain`, `PositionCell`, a typed `Event` enum. `GST_PLUGIN_PATH` selection lives in the binary
- [ ] The audio API is async (replies over `oneshot`), run as `Task::perform`. Engine events come through a boot-created `run_with` subscription mapped to `Message::Audio`
- [ ] `playback::update` is pure: it changes state and returns `Vec<Effect>`. A thin runner turns effects into `Task`s against `Services` (ADR 0002)
- [ ] Every start goes through `PlayRequest { source, first_page, continuation }`. The Playback source owns its tracks in source order, and playback keeps a play order and a cursor (ADR 0004)
- [ ] Each `Play` carries a `PlayToken` echoed by its result, and stale results are dropped. The current track changes optimistically and rolls back on failure
- [ ] Status is `Stopped | Loading(token) | Playing | Paused`. A 250 ms tick runs only while playing and reads `PositionCell`. Track end comes only from `TrackFinished`
- [ ] The 90px player bar (30/40/30): cover, title, artist, play/pause, a seek bar that commits on release, volume and mute (restores the pre-mute volume, 0.5 if none). Volume and mute are playback state
- [ ] Seam 1 tests cover starting a source with no wrap-around, and dropping stale play tokens
