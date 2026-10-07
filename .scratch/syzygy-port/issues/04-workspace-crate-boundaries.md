# Workspace crate boundaries

Type: grilling
Status: resolved
Blocked by: 02

## Question

Which crates make up the syzygy workspace (one per backend subsystem plus the iced binary), which sone module goes into which crate, and how are shared types (`error.rs`, models from `tidal_api.rs`, settings) and `AppState` split between them?

Also decide: proxy is out of the prototype's scope, but `TidalClient::new` needs `ProxiedHttp`, `AudioPlayer::new` needs `ProxySettings`, and audio routes GStreamer sources through `proxy::Route` (about 130 tests). Keep proxy code in place with settings disabled, or strip it out? Also, how are calls into out-of-scope services (discord, scrobble, mcp, idle-inhibit) removed from the playback and auth paths? tidal_report is in scope and stays wired in. See [backend inventory](../research/backend-inventory.md).

## Answer

A virtual workspace with every crate under `crates/`. Versions live in `[workspace.dependencies]`, and every dependency is bumped to current stable as it is ported (reqwest 0.11→0.12+ included). The package is no longer called `sone-iced`.

| Crate | Contains | Depends on |
|---|---|---|
| `syzygy-tidal` | `tidal_api` split into `client`/`auth`/`models` modules, `rate_gate`, `embedded_config`; `auth` helpers moved from `commands/auth.rs` (`resolve_credentials`, `build_pkce_params`, redirect URI, parsing `code` from a pasted redirect URL); `stream` module: `resolve_stream(track_id, max_quality) -> PlayableStream` from `resolve_play_uri` + `quality_tiers` (cascade tests come along) | none |
| `syzygy-store` | `crypto`, `DiskCache` (generic tiers and tags), encrypted JSON file I/O, `now_secs`; file magic `SYZY` | none |
| `syzygy-audio` | `audio` minus `AudioProxy`, `SignalPathTracker` without its emit, `pipeline_probe` parsers and types (24 tests; no `PipelineProbe` struct), `compute_norm_gain`; typed `Event` enum (`TrackFinished`, `TrackAdvanced`, `Error{kind: enum}`, `Resampled`, `BitDepthChanged`) sent through a sender passed to the constructor | none |
| `syzygy-mpris` | `mpris`; the position comes from a closure, and Raise and Quit become events; typed `Event` enum | none |
| `syzygy-catalog` | The binary's only way to reach the Catalog: stale-while-revalidate policy, cache keys, tags for each mutation, browse reads, search and suggestions, lyrics, credits, Library reads and edits, image bytes; the home-cache encode/decode tests | tidal, store |
| `syzygy-report` | `tidal_report`; holds a shared TIDAL client handle (in whatever form 05 picks); its queue is persisted through store | tidal, store |
| `syzygy` (bin) | The iced app; trimmed `Settings` (auth, audio prefs, volume, `last_track_id`, `max_quality`, `report_plays`; no migrations); identity constants passed into engine crates; logging; `GST_PLUGIN_PATH` selection; URL→`image::Handle` cache | all |

- **No `core` crate.** Each crate has its own error type. `Serialize` is dropped, and so are the Scrobble, MCP and ProxyBlocked variants.
- **Proxy is stripped** ([ADR 0001](../../../docs/adr/0001-strip-proxy-support.md)). `TidalClient` takes a plain `reqwest::Client`. `explain_block`, the `source_guards` tests and the proxy tests are dropped.
- **`AppState` is not ported.** syzygy's state follows the iced architecture. The discord, scrobble, mcp and idle-inhibit call sites live only in `commands/*` and the `lib.rs` listeners, so they disappear when that layer goes. Engine code needs no surgery.
- **Engine crates hardcode no app identity.** The keyring service, dirs and MPRIS bus name and identity are passed in by the binary.
- **Left to other tickets:**
  - 05: how state is owned, the channel→`Subscription` bridge, and how background catalog refreshes reach `update`.
  - 06: auth UX.
  - 08: where `update` calls the report hooks.
  - Map: the actual identity values.
