# Auth flow in iced

Type: grilling
Status: resolved
Blocked by: 02

## Question

How does login work in syzygy? Device-code screen UX; how the PKCE system-browser redirect is caught without `tauri-plugin-oauth`; how keyring and crypto persistence work under syzygy's own identity; token refresh, and what the app does at startup with a saved session.

## Answer

Terms (**Session**, **Login method**, **Browser login**, **Device-code login**, **Session expiry**) are in `CONTEXT.md`.

- **Login screen:** the main action is a "Sign in with browser" button. Below it is a smaller "Use a code instead (no lossless)" link. syzygy uses only the embedded client ID pairs, with no custom credentials. `AuthMethod` records which pair the tokens belong to. sone's legacy-login toast is dropped.
- **Browser login:** the `redirect_uri` is fixed to `https://tidal.com/android/login/auth` and tied to the client ID, so nothing can listen for the redirect. The `open` crate opens the authorize URL, with a "Copy link" fallback. The user can type into a paste field or press "Paste from clipboard" (`iced::clipboard::read`). The input can be a full redirect URL or just the code: trim it, take `code` from the query string if present, and otherwise use the whole input. If the URL has `error=`, show `error_description` and "Start over". If the code exchange fails, show the error inline and let the user paste again. "Start over" generates a new verifier.
- **Device-code login:** the screen shows the code in large type with a Copy button, an "Open link.tidal.com" button (`verification_uri_complete`) and an iced `qr_code`. It polls at the server's `interval` as an abortable `Task`, and Cancel aborts it. When `expires_in` passes, it shows "Get a new code".
- **Storage:** the keyring entry is `syzygy` / `master-key`, with a fallback key file at `config_dir/syzygy.key` (0600). Encrypted files start with the `SYZY` magic. If neither the keyring nor the file works, `boot` shows a fatal error screen. Data is never stored unencrypted and persistence is never silently dropped.
- **Session file:** the Session lives in its own encrypted `session.json`: tokens, `AuthMethod`, `user_id` and `country_code`. `Settings` holds only preferences. `update` writes both files.
- **Startup:** if `session.json` has tokens, go straight to the main screen with no network call. A background `get_session_info` refreshes `user_id` and `country_code`.
- **Refresh:** tokens refresh only after a 401, single-flight (ticket 05), with no proactive refresh. `TokensRefreshed` rewrites `session.json`.
- **Session expiry:** only the token endpoint refusing the refresh token (400 or 401, `invalid_grant`) counts. Network errors and 5xx responses are ordinary errors. On expiry, stop playback and clear the tokens. Keep `user_id`, `country_code`, the queue, the cache and preferences. Show the login screen with a "Your session expired, sign in again" banner. If a different `user_id` signs in next, clear the cache and queue.
- **Logout:** stop playback and delete `session.json`, the disk cache, the saved queue and the report queue. Keep audio and quality preferences.
