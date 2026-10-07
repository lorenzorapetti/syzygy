# 11: Workspace scaffold, store and app skeleton

**What to build:** syzygy exists as its own app. `cargo run` opens an empty syzygy window. Settings persist encrypted, and if neither the system keyring nor the fallback key file works, a fatal error screen shows and nothing is stored unencrypted. See the spec's "Workspace and identity", "Crate responsibilities" (`syzygy-store`), "Auth and Session" (keyring fallback) and "Errors, toasts and logging".

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [x] A virtual workspace with every crate under `crates/`, versions in `[workspace.dependencies]`, and the `sone-iced` package removed
- [x] `syzygy-store` holds sone's crypto, `DiskCache` (generic tiers and tags) and encrypted JSON file I/O with the `SYZY` magic, with sone's tests for these modules ported and passing
- [x] The binary owns the identity constants (app name, app ID `com.lorenzorapetti.syzygy`, config and cache dirs, keyring service `syzygy` / `master-key`), and no engine crate hardcodes them
- [x] iced 0.14.0 with features `tokio, image, svg, lazy, advanced`. iced owns the runtime (no `#[tokio::main]`)
- [x] `App { services, settings, …, phase }` with `Phase::Fatal`. A keyring failure falls back to a 0600 key file, and if both fail the fatal screen shows
- [x] `Settings` (trimmed: no session data, migrations, proxy or theme) loads at boot and is saved only by `update`, through a `Task` that encrypts and writes on `spawn_blocking`
- [x] Logging is ported from sone into the binary: a log file under syzygy's cache or state dir, with the level set by an env var
- [x] GPL-3.0 licence, crediting sone (lullabyX)

## Comments

Implemented. Notes for later tickets:

- **`Phase::Fatal` lives one level up.** Without a master key there's no `Store`, so there are no `Services` and no `App`. The iced state is `app::State { Fatal(syzygy_store::Error), Running(App) }`, and `App { services, settings, phase }` holds `Phase` (only `Shell` for now). Ticket 13 adds `Phase::Login` there. The spec's "`boot` enters `Phase::Fatal`" means `State::Fatal`.
- **Only `syzygy-store` and `syzygy` exist.** Each other crate arrives with its own ticket. `Paths` has no `cache_dir` field yet; the catalog ticket adds it (`<cache_dir>/syzygy`).
- **Tests.** sone's `crypto.rs` and `cache.rs` had no tests, so new ones cover `Store` and `DiskCache` through the public API. sone's logging tests only covered the logging toggle (out of scope) and were dropped. `report_plays_defaults_on` was ported.
- **Stricter than sone:** plaintext (non-`SYZY`) files and unknown format versions are refused; an existing but invalid key file is a fatal error, never overwritten; the key file is written atomically and backfilled when the key comes from the keyring; an unreadable `settings.json` is renamed to `settings.json.unreadable` before defaults are used.
- **`Settings`** has no `last_track_id` (ticket 04 listed it). Session restore (ticket 28) should keep the current track in `queue.json` instead. `max_quality` is still a `String`; ticket 12 can make it the TIDAL quality type.
- **Logging:** `RUST_LOG` sets the level; logs go to `<state_dir>/syzygy/logs`, falling back to `<cache_dir>/syzygy/logs`.
- Disk-cache stats were dropped (out of scope).
