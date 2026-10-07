# 12: `syzygy-tidal` crate

**What to build:** the TIDAL client, ported from sone into its own crate with no Tauri and no proxy, ready for login and the Catalog to use. Verifiable through its tests. See the spec's "Crate responsibilities", "Bridge between engines and iced" and "Auth and Session", and ADR 0001.

**Blocked by:** 11

**Status:** ready-for-agent

- [ ] `TidalClient` is split into client, auth and models modules, with the rate gate and embedded config, and takes a plain `reqwest::Client` (proxy stripped, ADR 0001). reqwest is 0.12 or later
- [ ] `TidalClient` is `Clone` with `&self` methods. Tokens sit behind an internal `RwLock`, and refresh is single-flight, so concurrent 401s refresh once
- [ ] Tokens refresh only after a 401. Only `invalid_grant` from the token endpoint is Session expiry, emitted as `SessionExpired`. A successful refresh emits `TokensRefreshed`. Both go out on a channel created by the caller (ADR 0003)
- [ ] Auth helpers: embedded credential pairs and an `AuthMethod`, PKCE params, the fixed redirect URI, and a parser for pasted input (trim; `code` from the query string if present, else the whole input; `error=` yields `error_description`)
- [ ] `resolve_stream(track_id, max_quality) -> PlayableStream` with the quality cascade
- [ ] The crate has its own error type, without `Serialize` or the Scrobble, MCP or ProxyBlocked variants
- [ ] sone's TIDAL JSON parsing and quality-cascade tests pass, plus new tests for the pasted-input parser (full URL, bare code, `error=`)
