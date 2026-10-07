# Backend dependency and Tauri coupling inventory

Answers [issue 02](../issues/02-backend-dependency-inventory.md). Primary source: `sone/src-tauri` at upstream `0488f97`. All paths below are relative to `sone/src-tauri/src/` unless stated. Line numbers point at the `fn` line, or at the `#[tauri::command]` attribute directly above it.

## TL;DR

- **The engine is mostly Tauri-free already.** `tidal_api.rs` (7.3k lines), `cache.rs`, `crypto.rs`, `rate_gate.rs`, `proxy.rs`, `proxy_http.rs`, `pipeline_probe.rs`, `embedded_config.rs`, `logging.rs` import nothing from Tauri. They only reach back into the crate root for `SoneError`, `ProxySettings`/`ProxyType` and `now_secs`.
- **Four in-scope modules are coupled to Tauri, and only at the edges.** `audio.rs`, `signal_path.rs` and `mpris.rs` hold an `AppHandle` purely to `emit` events. `mpris.rs` also uses `try_state`, `app.exit`, and `tray::restore_window`. `error.rs` has one `From<tauri::Error>`. Replace the handle with a channel or callback sink and these become Tauri-free.
- **All Tauri coupling lives in `lib.rs` and `commands/*`.** These are thin `#[tauri::command]` wrappers over `State<AppState>`. Some take an `AppHandle` only to re-fetch `AppState` inside a spawned task. syzygy rewrites this layer as iced `Task`s and does not port it.
- **Proxy cannot be cut cleanly.** `TidalClient::new` needs a `proxy_http::ProxiedHttp`, `AudioPlayer::new` needs a `ProxySettings`, and `audio.rs` builds GStreamer sources from `proxy::Route`. You can bring it along and pass disabled settings, which plan to `Direct`. Or you can refactor it out, and lose about 130 proxy tests.
- **Out-of-scope services are wired into in-scope commands.** Discord, scrobble, tidal_report, MCP and idle-inhibit all hang off `AppState` and are called from `logout`, `stop_playback`, `pause`, `resume`, `seek`, `resolve_play_uri`, the MPRIS-update commands, and the `track-finished`/`track-advanced` listeners. Every one of those call sites has to be deleted.

## 1. How `AppState` is built (`lib.rs`)

`AppState` struct: `lib.rs:225-271`. Construction is `AppState::new(app_handle)` at `lib.rs:315-536`, registered with `app.manage(...)` in `.setup` at `lib.rs:668`.

Order in `new`:

1. Config dir `dirs::config_dir()/sone` (`317-319`), with `settings.json` and `cache/` (`321-323`). `config_dir_for_env()` (`280`) duplicates the same path for `main.rs`.
2. `Crypto::new(&config_dir)` (`326`). It panics on failure. The key comes from the OS keyring (service `"sone"`, entry `"master-key"`) with a fallback file at `config_dir/sone.key` (`crypto.rs:20-24`). Settings and cache are AES-GCM encrypted with a `SONE` magic header (`crypto.rs:9-12`).
3. `DiskCache::new(&cache_dir, crypto)` (`334`).
4. Load `Settings` (struct at `lib.rs:123-189`, decrypt + JSON at `337-341`), then run three one-shot migrations: titlebar (`347-362`), incomplete proxy (`369-383`), plaintext→encrypted (`386-402`).
5. Copy the audio prefs into atomics (`404-417`).
6. Proxy: `proxy::write_sidecar` (`433`), `audio::probe_host_caps()` (`434`), `ProxiedHttp::from_settings` (`446`).
7. `ScrobbleManager::new(app_handle, …)` (`448`), `TidalReporter::new(app_handle, …)` (`456`), `DiscordHandle::new()` plus Connect (`469-475`). All out of scope.
8. `SignalPathTracker::new(app_handle)` (`477`), then `AudioPlayer::new(app_handle, signal_path, proxy_settings)` (`481`), then `PipelineProbe::new(signal_path, audio_player)` (`486`).
9. `TidalClient::new(proxied_http)` with `set_token_persist`, a closure that re-encrypts `settings.json` whenever tokens refresh (`491-498`, `persist_auth_tokens` at `295-312`).
10. `MprisHandle::new(app_handle)` (`522`), `IdleInhibitor::new()`, `mcp::new_state()`, `overlay::new_state()` (`526-533`).

What it holds:

| Field | In scope? | Notes |
|---|---|---|
| `audio_player: Arc<AudioPlayer>` | yes | |
| `pipeline_probe: Arc<PipelineProbe>` | indirectly | Only `refresh_signal_path` uses it. The signal-path panel is out of scope. |
| `tidal_client: tokio::Mutex<TidalClient>` | yes | Every API call locks it. |
| `proxied_http: ProxiedHttp` | entangled | The shared reqwest cell. Images use it too (`get_image_bytes`). |
| `host_caps: std::Mutex<HostCaps>` | entangled | Proxy advisory checks. |
| `settings_path`, `cache_dir`, `disk_cache`, `crypto` | yes | |
| `minimize_to_tray`, `decorations` (AtomicBool) | no | tray/window |
| `volume_normalization`, `exclusive_mode`, `bit_perfect`, `gapless` (AtomicBool), `max_quality`, `exclusive_device` (std Mutex) | yes | settings |
| `cached_audio_devices` | yes | Pre-warmed on a thread in setup (`lib.rs:710-718`). |
| `last_replay_gain`, `last_peak_amplitude` (AtomicU64 f64 bits) | yes | Lets the normalization toggle take effect live. |
| `mpris: MprisHandle` (linux) | yes | |
| `scrobble_manager`, `tidal_reporter`, `discord`, `idle_inhibitor`, `mcp_state`, `mcp_handle`, `overlay_state`, `overlay_handle` | no | See §5. |
| `signal_path: Arc<SignalPathTracker>` | entangled | Required by `AudioPlayer`. |

Methods: `host_caps()` (`540`), `refresh_host_caps()` (`550`), `load_settings()` (`570`), `save_settings()` (`577`), `read_state_file`/`write_state_file` (`586`/`612`). The last two are encrypted files under `cache/`, which is where `queue.json` lives.

Other Tauri wiring in `run()` (`lib.rs:624-1178`):

- **Plugins.** `tauri_plugin_opener` (`641`), `tauri_plugin_window_state` (`643-649`), `tauri_plugin_single_instance` (`652-660`), `tauri_plugin_deep_link` (`662-666`) and `tauri_plugin_global_shortcut` (`893-926`, which emits `tray:*`). `tauri-plugin-oauth` is in `Cargo.toml` but nothing in `src/` references it.
- **Setup side effects.**
  - In scope: apply the saved exclusive, bit-perfect and gapless modes to the audio thread (`691-707`).
  - Out of scope: start MCP/overlay (`675-688`), init the scrobble providers and drain queues (`721-791`), the window icon and WebKit settings (`842-883`), the theme watcher (`886`) and the tray (`890`).
- **Backend `app.listen` listeners.**
  - `"track-finished"` (`796-803`) calls `scrobble_manager.try_scrobble_finished()` and `tidal_reporter.try_finish()`.
  - `"track-advanced"` (`810-839`) parses `replayGain`/`peakAmplitude` into `last_replay_gain`/`last_peak_amplitude`, then makes the same scrobble/report calls. **For syzygy, keep the rg/peak store and drop the rest.**
- **Window events** (`930-969`): close-to-tray, `miniplayer-closed`, and `pkce-login` window destroyed → `auth::on_pkce_window_closed`. All out of scope.
- **`invoke_handler`** (`970-1164`) registers about 150 commands.
- **`RunEvent::Exit`** (`1167-1177`): Discord disconnect, idle uninhibit, scrobble and tidal_report flush. None of it is in scope.

`main.rs` pre-run, while still single-threaded:

- WebKit DMA-BUF env (webview, so not needed).
- Proxy env scrub driven by the sidecar (`main.rs:44-107`; proxy).
- **`GST_PLUGIN_PATH` selection for bundles** (`main.rs:110-131`, `gst_plugin_path_choice` with 9 tests). Keep this for AppImage-style packaging.

## 2. Per-feature module map (transitive)

Shared base for every feature: `tidal_api::TidalClient` → `proxy_http::ProxiedHttp` → `proxy` (plan/HostCaps) + `rate_gate` + `error::SoneError`. Persistence: `crypto`, `cache::DiskCache`, `lib.rs` `Settings` + `AppState::{load,save}_settings`. `TidalClient` needs a tokio runtime (reqwest 0.11 async).

### Auth: device code + PKCE in the system browser, with a persisted session

| Step | Command (`commands/auth.rs`) | Engine calls |
|---|---|---|
| Restore session at startup | `load_saved_auth` :118 | `AppState::load_settings`, `resolve_credentials` :32 (picks embedded key pair by `AuthMethod`), `TidalClient::get_session_info` (`tidal_api.rs:1719`) for country code |
| Embedded creds | `get_default_credentials` :159, `has_pkce_defaults` :173 | `embedded_config::{stream_key_a..d, has_stream_keys, has_pkce_keys}` (`embedded_config.rs:49-70`). Generated by `sone/scripts/gen_credentials.py`, obfuscated byte arrays. |
| Device code start | `start_device_auth` :271 | `set_credentials`, `TidalClient::start_device_auth` (`tidal_api.rs:1546`) → `DeviceAuthResponse` (user_code, verification_uri(_complete), expires_in, interval) |
| Device code poll | `poll_device_auth` :283 | `poll_device_token` (`:1591`, `Ok(None)` = still pending), `get_session_info`, saves `auth_tokens` + `AuthMethod::LoginCode`, then `restore_session_services` |
| PKCE via browser (embedded creds) | `start_pkce_browser_login` :680 → `complete_pkce_browser_login` :694 → `finish_embedded_pkce` :537 | `build_pkce_params` :334 (verifier, S256 challenge, `client_unique_key`, authorize URL), `exchange_pkce_code` (`tidal_api.rs:1639`), saves `AuthMethod::Pkce` |
| PKCE via browser (user creds) | `start_pkce_auth` :360 → `complete_pkce_auth` :369 | same |
| Refresh | `refresh_tidal_auth` :326; also automatic after a 401 inside `TidalClient` | `refresh_token` (`tidal_api.rs:1359`) → `token_persist` hook |
| Logout | `logout` :413 | see §5. Also clears tokens and country, and settings `auth_tokens`/`last_track_id`/`scrobble`. `disk_cache.clear()` |
| Misc | `get_session_user_id` :502, `get_user_profile` :705, `consume_legacy_auth_notice` :486 | |

How the PKCE browser flow finishes: `redirect_uri` is fixed to `https://tidal.com/android/login/auth` (`auth.rs:103`, used in `build_pkce_params`). No local callback server or deep link catches it. The frontend opens `authorizeUrl` with `@tauri-apps/plugin-opener`'s `openUrl` (`sone/src/components/Login.tsx:335,417`). The user then **pastes the redirect URL back**, and the frontend extracts `code` from the query string (`Login.tsx:427-446`). Device code also uses `openUrl` (`Login.tsx:252,785`). syzygy needs a pure-Rust opener (xdg-open or a crate) and a paste field.

### Browse: home, search, album, artist, playlist, favorites, library

- `commands/pages.rs`: `get_home_page` :222, `refresh_home_page` :310, `get_home_page_more` :334, `get_page_section` :372, `get_album_detail` :21, `get_album_page` :74, `get_album_tracks` :155, `get_artist_detail` :467, `get_artist_page` :703, `get_artist_top_tracks(_all)` :507/:775, `get_artist_albums` :585, `get_artist_bio` :630, `get_artist_view_all` :858, `get_mix_items` :436.
- `commands/search.rs`: `search_tidal` :8, `get_suggestions` :19.
- `commands/metadata.rs`: `get_playlist_details` :24, `get_track` :34, `get_track_credits` :54.
- `commands/library.rs` reads: `get_user_playlists` :12, `get_all_playlists` :97, `get_playlist_tracks(_page)` :201/:279, `get_favorite_{playlists,albums,tracks,artists,mixes}` :395/:482/:689/:1203/:1099, the `*_ids` lookups (:784, :902, :963, :1013, :1059, :1092), `get_playlist_folders` :1295, `get_all_flattened_playlists` :1426, `get_playlist_recommendations` :1511.
- Images: `commands/utility.rs::get_image_bytes` :48 goes through the `DiskCache` Image tier and then `proxied_http.client()`. It returns `tauri::ipc::Response`.
- Engine: the matching `TidalClient` methods (`tidal_api.rs:1742-5499`). Home is `get_home_page` :4332 / `fetch_v2_home_feed` :4158. Album page is `parse_album_page` :5309.

How caching works: the commands, not the client, own a stale-while-revalidate wrapper. It calls `disk_cache.get(key, tier)`. `Fresh` returns at once. `Stale` returns at once and also, guarded by `mark_in_flight` and `should_retry_refresh(key, 300)`, starts a `tokio::spawn` that re-locks `tidal_client` and `put`s with tags. `Miss` fetches and then `put`s (`library.rs:25-94` is the template). Tiers and TTLs are UserContent 15 min, Dynamic 4 h, StaticMeta 7 d, Image 30 d (`cache.rs:18-40`). **This is the only reason those commands take an `AppHandle`.** The spawned task calls `handle.state::<AppState>()`. With an `Arc<AppState>` the coupling disappears.

### Library editing: favorites, playlists, folders

`commands/library.rs`: `create_playlist` :574, `update_playlist` :596, `add_track_to_playlist` :618, `add_tracks_to_playlist` :1179, `remove_track_from_playlist` :640, `delete_playlist` :663, `add/remove_favorite_{track,album,playlist,artist,mix}` (:809/:827, :927/:945, :973/:993, :1023/:1041, :1069/:1079), `create/rename/delete_playlist_folder` :1434/:1457/:1476, `move_playlist_to_folder` :1490. Each mutation calls the `TidalClient` method and then `disk_cache.invalidate_tag(...)`. The tags are `user:{id}`, `playlist:{id}`, `folders` and `fav-{tracks,albums,playlists,artists,mixes}` (`library.rs:589-1506`). None of these take an `AppHandle` or emit anything.

### Playback, including bit-perfect, queue, shuffle and repeat

- `commands/playback.rs`
  - `resolve_play_uri` :47: quality cascade `quality_tiers` :32. The ceiling is `max_quality`, and Hi-Res needs a `client_secret`. It calls `TidalClient::get_stream_url` (`tidal_api.rs:3645`). DASH becomes a `data:application/dash+xml;base64,…` URI, and BTS uses the direct URL. It selects album/track RG, and `compute_norm_gain` :9 applies.
  - `play_tidal_track` :196: `explain_block` :178 (proxy advisory), stores rg/peak, then `set_normalization_gain` + `play_url` via `spawn_blocking`, and saves `last_track_id`.
  - `set_next_track` :242 (gapless pre-arm, takes `qid`), `clear_next_track` :265, `get_stream_info` :274, `pause_track` :307, `resume_track` :316, `stop_track` :338 → `stop_playback` :328, `set_volume` :344 (also MPRIS SetVolume, and persists `volume`), `get_playback_position` :365, `seek_track` :370, `is_track_finished` :389, `save/load_playback_queue` :394/:403 (opaque JSON in encrypted `cache/queue.json`).
- `audio.rs` holds `AudioPlayer` (`:1741`): a command thread over `mpsc` driving GStreamer. Normal backend is `autoaudiosink`. DirectAlsa is exclusive mode, an `appsink` feeding an ALSA writer thread with bit-depth promotion, rate probing, and a gapless `concat` branch.
  - Public API: `new` :1751, `play_url` :3477, `pause`/`resume`/`stop` :3484-3490, `set_volume` :3493, `set_normalization_gain` :3496, `seek` :3499, `get_position` :3505, `is_finished` :3508, `set_exclusive_mode` :3511, `set_bit_perfect` :3518, `set_proxy_settings` :3521, `set_gapless` :3524, `set_next_track` :3528, `clear_next_track` :3549, `list_devices` :3552.
  - Free functions: `probe_host_caps` :17, `list_alsa_devices` :3989, `gapless_supported` :4055.
- `audio.rs` depends on `signal_path::SignalPathTracker` (it calls `sp.record_*`/`set_output` throughout), on `pipeline_probe::PadCaps`, and on `proxy::{HostCaps, Capability, Route, BlockReason, ProxyPlan, launch_bypass_was_set}` (`audio.rs:17-250`).
- Queue, shuffle, repeat, autoplay and history are **TypeScript**, so there is no backend code. The backend sees only `play_tidal_track`/`set_next_track` plus `track-finished`/`track-advanced`.

### Now playing and lyrics

- `commands/metadata.rs::get_track_lyrics` :43 calls `TidalClient::get_track_lyrics` (`tidal_api.rs:3872`, GET `/tracks/{id}/lyrics?countryCode=`). It has no cache. It returns `TidalLyrics` (`tidal_api.rs:505`): `lyrics` (plain), `subtitles` (LRC-timed), `is_right_to_left` and provider ids.
- `get_track_credits` :54 caches in StaticMeta. Its `get_track` :34 is uncached.
- The position comes from `get_playback_position` (the frontend polls it). There is no position event.

### Settings: quality, output, bit-perfect, logout

`commands/utility.rs`:

- Quality: `get/set_max_quality` :250/:255. Allowed values are `HI_RES_LOSSLESS|LOSSLESS|HIGH`.
- Output: `get/set_exclusive_mode` :163/:168 (disabling it also disables bit-perfect), `get/set_exclusive_device` :267/:272, `list_audio_devices` :287 (cached).
- Bit-perfect: `get/set_bit_perfect` :195/:200 (enabling it forces exclusive).
- Also: `get/set_gapless` :227/:237, `get_gapless_supported` :232, `get/set_volume_normalization` :130/:135 (recomputes gain from `last_replay_gain`/`last_peak_amplitude` and updates `signal_path`), `get_cache_stats`/`clear_disk_cache` :83/:90, `get/set_enable_logging` :693/:707 (plaintext `logging.toggle` sidecar read by `logging::read_logging_preference`).

Each setter writes the atomic or mutex, calls `audio_player.set_*`, then does `load_settings` → mutate → `save_settings`. Logout is `auth::logout`.

### MPRIS

`mpris.rs`: `MprisHandle::new(app_handle)` :49 spawns a thread with a current-thread tokio runtime and a `LocalSet`. That thread builds an `mpris_server::Player` (bus name `io.github.lullabyX.sone`, or `sone` under snap; identity `"SONE"`, `:65-106`), wires callbacks, runs a 1 s position tick, and drains the `MprisCommand` channel (`:8-42`, the handler at `:227-317`).

Inbound (app → MPRIS) comes from `commands/playback.rs` `update_mpris_metadata` :435, `update_mpris_playback_status` :474, `update_mpris_shuffle` :494, `update_mpris_fullscreen` :507 and `update_mpris_loop_status` :520, which the frontend calls. It also comes from `set_volume`, `seek_track` and `stop_playback`. `MprisMetadata` DTO is at `playback.rs:410-432`.

## 3. Every Tauri coupling point in the in-scope modules

### Engine modules

| Module | Coupling | Detail |
|---|---|---|
| `audio.rs` | `use tauri::Emitter` :9; `AppHandle` param of the ALSA writer spawn :1065 and `AudioPlayer::new` :1752; cloned into the bus and rebuild threads | emits only, listed below |
| `signal_path.rs` | `use tauri::Emitter` :14; field `app_handle` :70; `new(app_handle)` :74 | `emit("signal-path-changed", SignalPath)` :90, called after every mutator (:113-303) |
| `mpris.rs` | `use tauri::{Emitter, Manager}` :4; `new(AppHandle)` :49 | emits listed below; `crate::tray::restore_window(&app)` on Raise (:167); `app.exit(0)` on Quit (:172); `app_handle.try_state::<AppState>()` → `audio_player.get_position()` in the position tick (:213-217) |
| `error.rs` | `impl From<tauri::Error> for SoneError` :108-112 | `SoneError` is `Serialize` with `#[serde(tag="kind", content="message")]` (:5-7), the IPC error shape |
| `lib.rs` | everything in §1 | |

### Events emitted (name → payload)

| Event | Emitter | Payload | Frontend consumer |
|---|---|---|---|
| `track-finished` | `audio.rs:1495` (ALSA writer EOS), `:2459` (Normal bus terminal EOS) | `()` | `AppInitializer.tsx`; also the `lib.rs:796` listener |
| `track-advanced` | `audio.rs:3414` (gapless promotion) | `{trackId: u64, qid: String, replayGain: f64\|null, peakAmplitude: f64\|null}` | `AppInitializer.tsx`; also the `lib.rs:810` listener |
| `audio-error` | `audio.rs:1355,1391,1406,1416,1438,1453,1487,1506,1530,1545,1558,1579,1594,1634,2138,2540,3152` | `{kind}` or `{kind, message}`. `kind` is one of `device_disconnected`, `device_changed`, `device_busy`, `format_change_failed`, `playback_error`, or a `write_bytes` error string | `AppInitializer.tsx` |
| `audio-resampled` | `audio.rs:1465` | `{from: u32, to: u32}` (Hz) | `AppInitializer.tsx` |
| `audio-bit-depth-changed` | `audio.rs:1161` | `{from: String, to: String}` (GStreamer formats, e.g. `S24LE`→`S32LE`) | `AppInitializer.tsx` |
| `signal-path-changed` | `signal_path.rs:90` | `SignalPath` (camelCase, `signal_path.rs:17-66`) | signal-path panel (out of scope) |
| `tray:toggle-play` / `tray:next-track` / `tray:prev-track` | `mpris.rs:111/131/136` (PlayPause/Next/Previous); also `tray.rs`, global shortcuts `lib.rs:901-907` | `()` | `AppInitializer.tsx` |
| `mpris:play` / `mpris:pause` / `mpris:stop` | `mpris.rs:116/121/126` | `()` | ✓ |
| `mpris:seek` | `mpris.rs:142` | `f64` relative offset, seconds | ✓ |
| `mpris:set-position` | `mpris.rs:192` | `f64` absolute seconds | ✓ |
| `mpris:set-volume` | `mpris.rs:147` | `f64` | ✓ |
| `mpris:set-shuffle` | `mpris.rs:152` | `bool` | ✓ |
| `mpris:set-loop-status` | `mpris.rs:162` | `u8` (0 none, 1 playlist, 2 track) | ✓ |
| `mpris:set-fullscreen` | `mpris.rs:177` | `bool` | ✓ |
| `mpris:open-uri` | `mpris.rs:186` | `String` | ✓ |
| `pkce-login-success` / `-error` / `-cancelled` | `commands/auth.rs:638/641,649/533` | `AuthTokens` / `String` / `()` | the webview PKCE window (out of scope) |
| `scrobble-auth-error` | `scrobble/mod.rs:401,489,549` | provider name `String` | out of scope |
| `miniplayer-closed` | `lib.rs:941,946` (`emit_to("main")`) | `()` | out of scope |

### Commands layer (all `#[tauri::command]`, all `State<'_, AppState>`)

- `commands/auth.rs`: `use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder}` :8.
  - These take an `AppHandle` only to call `restore_session_services` :469: `import_session` :208, `poll_device_auth` :286, `complete_pkce_auth` :372 and `complete_pkce_browser_login` :696. `restore_session_services` uses `app.state::<AppState>()`, Discord Connect and `mcp::ensure_mcp_started(app)`, all out of scope.
  - `finish_embedded_pkce` :537 uses `app.state`.
  - `start_pkce_login_window` :579 uses `WebviewWindowBuilder` and `on_navigation`. Out of scope.
- `commands/library.rs` :1 and `commands/pages.rs` :3: `use tauri::{Manager, State}`. The `AppHandle` params, used only for SWR refresh `handle.state::<AppState>()`, are on `get_user_playlists`, `get_all_playlists`, `get_playlist_tracks`, `get_playlist_tracks_page`, `get_favorite_playlists`, `get_favorite_albums`, `get_favorite_tracks`, `get_favorite_mixes`, `get_favorite_artists`, `get_playlist_folders`, `get_album_page`, `get_home_page`, `get_page_section`, `get_artist_top_tracks`, `get_artist_bio`, `get_artist_page`, `get_artist_top_tracks_all` and `get_artist_view_all`. **None of them emit.**
- `commands/search.rs`, `metadata.rs`, `playback.rs`: `State` only.
- `commands/utility.rs` :2: `use tauri::{Manager, State}`.
  - `get_image_bytes` returns `tauri::ipc::Response` :51.
  - `set_decorations` takes `tauri::Window` :103.
  - `update_tray_tooltip` uses `app.try_state::<TrayHandle>()` :38-40.
  - `inhibit_idle` takes `tauri::WebviewWindow` :615.
  - The last three are out of scope.

### Plugins the frontend uses that matter for scope

- `@tauri-apps/plugin-opener` `openUrl`, for the auth URLs. syzygy needs a replacement.
- window-state, single-instance, deep-link and global-shortcut are all out of scope.

## 4. Tauri-free modules (pure)

All of these are free of Tauri. They still reference crate-root items that must move with them into a workspace crate (`SoneError` in `error.rs`, `ProxySettings`/`ProxyType`/`now_secs` in `lib.rs`).

| Module | Lines | Crate-root/sibling deps | External deps |
|---|---|---|---|
| `tidal_api.rs` | 7266 | `SoneError`, `proxy_http`, `rate_gate`, `proxy` (types) | reqwest, serde_json |
| `cache.rs` | 658 | `crypto::Crypto`, `now_secs` | tokio::sync, sha2 |
| `crypto.rs` | 157 | `SoneError` | aes-gcm, keyring, zeroize |
| `rate_gate.rs` | 112 | `now_secs` | — |
| `proxy.rs` | 2230 | `ProxySettings`, `ProxyType` | — |
| `proxy_http.rs` | 1156 | `proxy`, `ProxySettings` | reqwest |
| `pipeline_probe.rs` | 836 | `signal_path::SignalPathTracker`, `audio::AudioPlayer` (struct `PipelineProbe` only; the parsers are pure) | `pactl`/`/proc/asound` via std::process |
| `embedded_config.rs` | 73 | — | — |
| `logging.rs` | 139 | — | flexi_logger |
| `error.rs` | 112 | `tidal_api::is_terminal_sub_status` | thiserror; **one** `From<tauri::Error>` to delete |
| `http_util.rs` | 43 | — | Only MCP/overlay use it. Out of scope. |
| `discord.rs`, `discord_ipc.rs` | — | — | Out of scope. |
| `scrobble/{lastfm,librefm,listenbrainz,musicbrainz,queue}.rs`, `tidal_report/{event,queue}.rs`, `mcp/{events,sanitizer,state_mirror}.rs`, `overlay/state.rs` | — | — | Out of scope. |

`audio.rs`, `signal_path.rs` and `mpris.rs` are "Tauri-free but for an emit sink". `audio.rs` uses the handle only for `.emit`. `signal_path.rs` likewise. `mpris.rs` needs three more replacements: Raise, Quit and the position source (`try_state`).

Rebranding hard-codes that syzygy must change:

- `dirs::config_dir()/"sone"` (`lib.rs:281,318`)
- keyring service `"sone"` (`crypto.rs:22`)
- the `SONE` magic in encrypted files (`crypto.rs:9`). Optional: keeping it is harmless.
- MPRIS bus name, desktop entry and identity (`mpris.rs:65-106`)

## 5. Out-of-scope modules entangled with in-scope ones

| Out-of-scope module | Where it is wired into in-scope code | Tauri coupling inside it |
|---|---|---|
| **proxy / proxy_http** | `TidalClient::new(ProxiedHttp)` (`tidal_api.rs:1265`), `client_at` maps `BlockReason` to `SoneError::ProxyBlocked` (`:1353`). `AudioPlayer::new(…, ProxySettings)` (`audio.rs:1751`), `AudioProxy` routing GStreamer sources (`audio.rs:17-250`), `probe_host_caps() -> proxy::HostCaps`. `play_tidal_track`/`set_next_track` call `explain_block` (`playback.rs:178,214,256`). `get_image_bytes` uses `proxied_http.client()`. `AppState.host_caps`. `main.rs` env scrub. `tests/source_guards.rs` asserts several proxy invariants against sone's file layout. | none |
| **signal_path** (the panel is out of scope, the tracker is not) | `AudioPlayer::new` requires `Arc<SignalPathTracker>`, and audio calls its recorders. `set_volume_normalization` calls `set_normalization_enabled`. | emit `signal-path-changed` |
| **pipeline_probe** | `audio.rs` fields `decoded_caps_cell`/`output_caps_cell: PadCaps` (:1746-1747). `SignalPath.dac`/`os_mixer` types. | none |
| **scrobble** | `AppState.scrobble_manager` (`lib.rs:262,448`). Called by `pause_track`, `resume_track`, `stop_playback`, `seek_track` (`playback.rs:312,321,334,385`), `logout` (`auth.rs:419`), the `lib.rs` track-finished/advanced listeners and the startup provider init. | `ScrobbleManager::new(AppHandle)`, emit `scrobble-auth-error` |
| **tidal_report** (TIDAL "Recently Played" reporting; the map neither includes nor excludes it) | `resolve_play_uri` → `note_stream_resolved` (`playback.rs:96`). pause/resume/stop/seek (`playback.rs:311,320,333,384`). `logout` → `clear` (`auth.rs:420`). Listeners. `notify_track_started` (`commands/scrobble.rs:69`) starts its session. | `TidalReporter::new(AppHandle)`. It reaches `AppState.tidal_client` via `app_handle.state()` (`tidal_report/mod.rs:402,411,448`). |
| **discord** | `stop_playback` Stop (`playback.rs:332`), `seek_track` Seeked (`:381`), `update_mpris_metadata` SetMetadata (`playback.rs:461`), `update_mpris_playback_status` SetPlaying (`:486`), `logout` Disconnect (`auth.rs:430`), `restore_session_services` Connect (`auth.rs:475`), `AppState::new` (`lib.rs:469-475`) | none |
| **mcp** | `logout` shuts down `mcp_handle` (`auth.rs:433`). `restore_session_services` → `ensure_mcp_started(app)` (`auth.rs:477`). Startup spawn (`lib.rs:677`). | heavy (`app.state`, tools emit `mcp:*`) |
| **overlay** | `AppState` fields and startup spawn only | `ensure_overlay_started(&AppHandle)` |
| **idle_inhibit** | `logout` uninhibit (`auth.rs:438`), `RunEvent::Exit` | `tauri::WebviewWindow` (Wayland) |
| **tray** | `mpris.rs:167` Raise → `tray::restore_window(&AppHandle)` (`tray.rs:46`) | heavy |
| **theme_config**, **updates**, **profile**, **feed**, **webview_proxy_auth** | Not referenced by in-scope code, only registered in `invoke_handler`. | — |

## 6. Existing tests

Counts are of `#[test]`/`#[tokio::test]`. There are no tests for views or the frontend here. Integration tests live in `sone/src-tauri/tests/`.

| Module | Tests | What they cover |
|---|---|---|
| `tidal_api.rs` | 53 | `home_tab_tests` 9, `profile_tests` 9, `profile_upload_tests` 5, `rate_limit_error_tests` 1, `sub_status_tests` 4, `direct_hit_tests` 15, `feed_tests` 8, `proxy_routing_tests` 2. Mostly JSON parsing, with no network. Profile and feed are out of scope. |
| `commands/playback.rs` | 10 | `quality_tiers` cascade (`:533-637`) and the `explain_block` proxy messages (`:639+`) |
| `commands/pages.rs` | 5 | Home-page cache encode/decode (`:1099-1151`). Empty homes are never cached. |
| `commands/utility.rs` | 12 | All proxy save/test/probe (`:721+`) |
| `audio.rs` | 21 | All in `proxy_source_tests` (`:4060+`): proxy routes onto GStreamer sources, curl promotion, rebuild on route change. **No tests for playback, the ALSA writer, gapless or bit-perfect.** |
| `pipeline_probe.rs` | 24 | Parsing `hw_params`, pactl and ALSA card names |
| `rate_gate.rs` | 5 | Cooldown semantics, Retry-After parsing |
| `proxy.rs` | 76 + 1 proptest block | |
| `proxy_http.rs` | 26 | |
| `main.rs` | 15 | 6 for the proxy scrub, 9 for `gst_plugin_path_choice` |
| `logging.rs` | 6 | |
| `lib.rs` | 1 | `report_plays_defaults_on` |
| `cache.rs`, `crypto.rs`, `error.rs`, `mpris.rs`, `signal_path.rs`, `embedded_config.rs`, `commands/{auth,library,search,metadata}.rs` | **0** | |
| `tests/source_guards.rs` | 12 | Substring guards over `src/`: one reqwest builder site, a single `dispatch`, env mutation only in `main.rs`, audio proxy hooks. These are tied to sone's file names and only port if the proxy design comes along. |
| `tests/proxy_route_strings.rs`, `tests/proxy_host_abort.rs` | 3 + 1 | Proxy (the second needs python) |
| Out of scope | | `discord_ipc` 17, `theme_config` 14, `tidal_report/event` 13, `scrobble/musicbrainz` 9, `updates` 8, `overlay/server` 6, `webview_proxy_auth` 5, `idle_inhibit` 4, `tidal_report/mod` 3, `scrobble/listenbrainz` 3, `scrobble/lastfm` 2, `mcp/sanitizer` 2 |

## 7. Implications for the crate-boundary ticket (observations, not decisions)

1. An "events out" seam replaces `AppHandle` in `audio`, `signal_path` and `mpris`. The events are `TrackFinished`, `TrackAdvanced{track_id, qid, rg, peak}`, `AudioError{kind, message}`, `Resampled`, `BitDepthChanged` and `SignalPath`, plus the MPRIS control requests. An iced `Subscription` over a channel fits all three.
2. A "state in" seam for the MPRIS position tick replaces `try_state::<AppState>()`. Pass it an `Arc<AudioPlayer>` or a position closure.
3. `Settings`, `ProxySettings`, `AuthMethod`, `now_secs` and `persist_auth_tokens` currently live in `lib.rs` next to Tauri. They need a home in a Tauri-free crate. `Settings` carries many out-of-scope fields: scrobble, discord, mcp, overlay, proxy, tray, decorations.
4. Proxy has two options:
   - **Keep it.** Port `proxy`/`proxy_http` verbatim with default (disabled) settings, so the existing tests stay valid. This costs about 3.4k lines.
   - **Strip it.** Replace `ProxiedHttp` with a plain `reqwest::Client` and remove `AudioProxy` from `audio.rs`. This touches `audio.rs:17-250` and its route plumbing, plus `tidal_api.rs:1253-1356`, and drops the only tests `audio.rs` has.
5. Session lifecycle side effects (`restore_session_services`, `logout`, `stop_playback`) need trimming to just tidal_client, audio, MPRIS, settings and cache.
6. tidal_report's scope status is open. It is not on the out-of-scope list, but it is not part of the prototype scope either.
