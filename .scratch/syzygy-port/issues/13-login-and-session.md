# 13: Login and Session

**What to build:** a signed-out user can sign in with Browser login or Device-code login and land in the signed-in Shell, a placeholder for now. A signed-in user who relaunches goes straight there with no wait for the network. See user stories 2–3, 6–17 and the spec's "Auth and Session".

**Blocked by:** 12

**Status:** ready-for-agent

- [x] `Phase::Login` and `Phase::Shell`. A stored Session opens straight into the Shell, and account info (user id, country) refreshes in the background
- [x] Browser login is the main action: it opens TIDAL's sign-in page in the system browser (`open` crate), offers "Copy link" as a fallback, and takes the pasted URL or code from a field or a "Paste from clipboard" button
- [x] A pasted `error=` shows TIDAL's error description and "Start over", which generates a new verifier. A failed code exchange shows inline and leaves the field ready for another paste
- [x] A secondary "Use a code instead (no lossless)" link starts Device-code login: the code in large type with Copy, "Open link.tidal.com" and an iced `qr_code`
- [x] Device-code polling runs at the server's interval as an abortable `Task`. Cancel aborts it, and expiry offers "Get a new code"
- [x] The Session (tokens, `AuthMethod`, `user_id`, `country_code`) is saved in its own encrypted `session.json`, separate from `Settings`, and restored at launch
- [x] `syzygy-tidal` events arrive through a `run_with` subscription created at boot, mapped to `Message::Tidal` (ADR 0003). `TokensRefreshed` saves the Session

## Comments

Implemented. Notes for later tickets:

- **Login is pure.** `login::State::update(Message) -> Vec<Effect>` (ADR 0002 style); `App::run_login_effect` performs the effects. The device-code poll `Task` is `abortable`, and its `abort_on_drop` handle lives in the code screen's state, so leaving that screen (Cancel, or the whole Login phase going away) aborts it. "Use a code" is disabled while a code exchange is in flight, because a successful exchange signs the client in.
- **Session** is `session::Session { tokens, login_method, user_id, country_code }` in `<config>/syzygy/session.json`. `AuthMethod` is `LoginMethod`. An unreadable file means signing in again; unlike settings, it isn't kept aside.
- **Saves are ordered.** `persist::write_json` / `persist::remove` take an order number when `update` calls them, and an older save that finishes late is skipped. `Settings::save` and `Session::save` return the future eagerly (not `async fn`) so the number is taken in `update`.
- **`get_session_info`** now returns `SessionInfo { user_id, country_code: Option<_> }`, so the client's default country is never saved as the user's.
- **Events.** `events::EventSource<T>` per ADR 0003. The TIDAL subscription is on whenever the app is `Running`. The audio and MPRIS engines can reuse it.
- **Session expiry stub (issue 32).** `SessionExpired` drops the Session, deletes `session.json` and shows login, with no banner. Issue 32 should keep `user_id` and `country_code` (making `tokens` optional, with a serde default, keeps old files readable).
- **Device-code polling** retries network errors, 429s and 5xx responses. `slow_down` can't be told apart from `authorization_pending` in `syzygy-tidal`, so the interval never backs off.
- **Not tested:** the iced glue (boot opening the Shell, `TokensRefreshed` saving, the subscription, the poll loop). It was tested at the agreed seams: the login state machine, Session load/save and save ordering. A real sign-in against TIDAL was not run.
- **Styling** uses iced's built-in button styles until the visual system lands.
