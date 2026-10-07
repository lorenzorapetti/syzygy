# 13: Login and Session

**What to build:** a signed-out user can sign in with Browser login or Device-code login and land in the signed-in Shell, a placeholder for now. A signed-in user who relaunches goes straight there with no wait for the network. See user stories 2–3, 6–17 and the spec's "Auth and Session".

**Blocked by:** 12

**Status:** ready-for-agent

- [ ] `Phase::Login` and `Phase::Shell`. A stored Session opens straight into the Shell, and account info (user id, country) refreshes in the background
- [ ] Browser login is the main action: it opens TIDAL's sign-in page in the system browser (`open` crate), offers "Copy link" as a fallback, and takes the pasted URL or code from a field or a "Paste from clipboard" button
- [ ] A pasted `error=` shows TIDAL's error description and "Start over", which generates a new verifier. A failed code exchange shows inline and leaves the field ready for another paste
- [ ] A secondary "Use a code instead (no lossless)" link starts Device-code login: the code in large type with Copy, "Open link.tidal.com" and an iced `qr_code`
- [ ] Device-code polling runs at the server's interval as an abortable `Task`. Cancel aborts it, and expiry offers "Get a new code"
- [ ] The Session (tokens, `AuthMethod`, `user_id`, `country_code`) is saved in its own encrypted `session.json`, separate from `Settings`, and restored at launch
- [ ] `syzygy-tidal` events arrive through a `run_with` subscription created at boot, mapped to `Message::Tidal` (ADR 0003). `TokensRefreshed` saves the Session
