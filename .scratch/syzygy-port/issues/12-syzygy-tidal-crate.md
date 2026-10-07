# 12: `syzygy-tidal` crate

**What to build:** the TIDAL client, ported from sone into its own crate with no Tauri and no proxy, ready for login and the Catalog to use. Verifiable through its tests. See the spec's "Crate responsibilities", "Bridge between engines and iced" and "Auth and Session", and ADR 0001.

**Blocked by:** 11

**Status:** ready-for-agent

- [x] `TidalClient` is split into client, auth and models modules, with the rate gate and embedded config, and takes a plain `reqwest::Client` (proxy stripped, ADR 0001). reqwest is 0.12 or later
- [x] `TidalClient` is `Clone` with `&self` methods. Tokens sit behind an internal `RwLock`, and refresh is single-flight, so concurrent 401s refresh once
- [x] Tokens refresh only after a 401. Only `invalid_grant` from the token endpoint is Session expiry, emitted as `SessionExpired`. A successful refresh emits `TokensRefreshed`. Both go out on a channel created by the caller (ADR 0003)
- [x] Auth helpers: embedded credential pairs and an `AuthMethod`, PKCE params, the fixed redirect URI, and a parser for pasted input (trim; `code` from the query string if present, else the whole input; `error=` yields `error_description`)
- [x] `resolve_stream(track_id, max_quality) -> PlayableStream` with the quality cascade
- [x] The crate has its own error type, without `Serialize` or the Scrobble, MCP or ProxyBlocked variants
- [x] sone's TIDAL JSON parsing and quality-cascade tests pass, plus new tests for the pasted-input parser (full URL, bare code, `error=`)

## Comments

Implemented. Notes for later tickets:

- **API.** `TidalClient::new(reqwest::Client, UnboundedSender<Event>)`. `Event` is `TokensRefreshed(AuthTokens)` or `SessionExpired`. Session plumbing: `restore_session(tokens, LoginMethod, country)`, `sign_out`, `tokens`, `login_method`, `country_code`. Sign-in: `PkceParams::generate()`, `parse_pasted_input`, `exchange_pkce_code(code, &params)`, `start_device_auth`, `poll_device_token`. Both sign-ins install the tokens on the client; call `get_session_info` afterwards for `user_id` and country.
- **`LoginMethod`**, not `AuthMethod`, to match CONTEXT.md. It serializes as `browser` / `device_code`.
- **Refresh.** Every authenticated request, including mutations, refreshes once on a 401 and retries (sone only did this for GETs). A 401 with a playbackinfo sub-status never refreshes. If a sign-out or sign-in lands mid-refresh, the new state wins. A transient refresh failure (network, 5xx) isn't shared: each waiting request tries its own refresh.
- **Not tested:** the refresh path and single-flight. They would need a local HTTP server and configurable base URLs; only `invalid_grant` classification is unit-tested.
- **`Quality`** is the max-quality type, with lenient deserialization (unknown → `HiResLossless`). `Settings.max_quality` now uses it.
- **`PlayableStream { track_id, uri, is_dash, info: StreamInfo }`**. The audio crate computes the normalization gain from `info` (`compute_norm_gain` moves there).
- **Dropped:** proxy, video stream and favorite-video endpoints, profile editing, and their tests. `get_video` is gone too; the video models stay because search parsing uses them.
- **reqwest 0.13** with `json`, `form`, `query`.
