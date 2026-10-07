# Backend dependency and Tauri coupling inventory

Type: research
Status: resolved

## Question

For each prototype feature (auth, browse, library editing, playback, now playing with lyrics, settings, MPRIS), which `sone/src-tauri/src` modules and functions does it need, transitively? List every Tauri coupling point in those modules (`AppHandle`, `State`, `Manager`, `Emitter::emit` with event names and payloads, `#[tauri::command]`, tauri plugins), how `AppState` is built in `lib.rs`, and which modules are pure (no Tauri). Note which tests exist alongside them.

## Answer

Most of the engine is already Tauri-free: `tidal_api`, `cache`, `crypto`, `rate_gate`, `proxy`, `proxy_http`, `pipeline_probe`, `embedded_config` and `logging`. Only `audio`, `signal_path` and `mpris` hold an `AppHandle`. All three use it to `emit`. `mpris` also uses `try_state`, `app.exit` and `tray::restore_window`. Swapping the handle for an event channel is enough to free them.
All other Tauri coupling sits in `lib.rs` (`AppState::new`, plugins, listeners) and in `commands/*`. Those are thin `State<AppState>` wrappers. Their `AppHandle` params exist only to re-fetch state in stale-while-revalidate cache refreshes and in `restore_session_services`. syzygy rewrites that layer and does not port it.
Proxy can't be cut cleanly: `TidalClient::new(ProxiedHttp)` and `AudioPlayer::new(ProxySettings)` both depend on it. Discord, scrobble, tidal_report, MCP and idle-inhibit calls are scattered through `logout`, `stop_playback`, `pause`/`resume`/`seek`, `resolve_play_uri` and the MPRIS-update commands. Each of those call sites has to be removed.
PKCE through the system browser finishes when the user pastes the redirect URL back. There is no callback server. Tests cover the TIDAL JSON parsing, `quality_tiers`, `pipeline_probe` and proxy. Playback, `cache`, `crypto`, `mpris` and auth have none.

[findings](../research/backend-inventory.md)
