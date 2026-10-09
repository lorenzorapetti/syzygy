# 26: Playback failures and Explicit content

**What to build:** a bad track doesn't stop playback, but a broken source doesn't spin forever. A rate-limit pauses and resumes on its own, a busy device is retried with a message, and the user decides whether Explicit content may play. See user stories 83, 86–87, 89–91 and the spec's "Playback model" (concurrency, failures, explicit content).

**Blocked by:** 23

**Status:** ready-for-agent

- [x] The skip drain is a state machine that skips unplayable or unavailable tracks and stops after 3 in a row, with a toast. No locks or re-entry guards
- [x] A 429 puts the entry back at the head, pauses, and emits `ResumeAfter(token, delay)`. It resumes unless the user did something in between
- [x] A busy device is retried inside the runner's `Play` task, surfacing `AudioBusyRetrying` as a "device busy" toast. Audio errors are toasted, cut to 80 characters
- [x] The allow-explicit setting is saved in `Settings`. With it off, starting, queueing or playing a source containing an explicit track returns `Outcome::NeedsExplicitConsent(pending)`. The Shell shows the modal: "Allow explicit content" sends `Settings(AllowExplicit)` then the pending message, and "Play without them" proceeds with them skipped
- [x] Explicit entries reached any other way are skipped silently, shown dimmed, and don't count toward the skip cap. Turning the setting off removes nothing
- [x] Seam 1 tests cover the skip drain and its cap, 429 with `ResumeAfter`, and the explicit outcomes and silent skips

## Comments

Implemented. Notes for later tickets:

- **`playback::update` returns `Outcome`**: `Effects(Vec<Effect>)` or `NeedsExplicitConsent(Pending)`. `Pending` turns back into its `Message` (`Message::from`). Seam 1 tests use a test-only `Playback::send` that unwraps the effects.
- **Skipping.** `Track::available` (catalog) is false when TIDAL says `streamReady: false` or `allowStreaming: false`. `streamStartDate` isn't read; such a track 404s and is skipped that way. Moving on (TrackFinished, Next, Start, queueing into an idle player) goes through `play_or_skip`: explicit tracks are skipped silently while they aren't allowed, unavailable ones count. A play that fails with a terminal resolve error (404/410/451, terminal 401) counts too, and playback moves past it. On the 3rd in a row playback stops on that track with `Notice::TooManyFailures`. The count restarts on a track that plays, on Start/Next/Previous, and when a run of skips runs out. If nothing after can play, the state goes back to how it was (for an async failure, the usual rollback). That happens silently, with no toast.
- **Choosing an unavailable track** (`Start::Track` on one) toasts "Track unavailable" and plays nothing, as sone does. Not in the ticket.
- **429.** The entry stays current, `Stopped` (nothing is loaded in the engine, so not `Paused`). `Effect::Stop` goes out if the old track was still playing, along with `ResumeAfter { token, delay }` (Retry-After + 1 s, or 6 s) and `Notice::RateLimited`. The runner sleeps, then sends `Message::Resume(token)`. Start, TogglePlay, Next, Previous or Seek in between cancels it. The old track's rollback is dropped: after a 429, Next doesn't bring it back.
- **Device busy.** The runner's `Play` task is a stream (`iced::stream::channel`). It tries `play_url` again up to 10 times, 500 ms apart, and sends `Message::DeviceBusy(token)` once, which becomes `Notice::DeviceBusy` while that play is still loading. Audio errors (`Played` with `PlayError::Audio`, and `EngineFailed(String)`) become `Notice::AudioError`. The Shell cuts them to 80 characters.
- **Explicit consent.** `Message::AllowExplicit(bool)` changes the setting, and the App saves it to `Settings::allow_explicit` (`save_modes`). Ticket 31's toggle should send it. The Shell's modal (`shell/consent.rs`): Allow sends `AllowExplicit(true)`, then the pending message. "Play without them" sends `WithoutExplicit(pending)`, which starts the source with them skipped, or does nothing for a queue message. Escape or a click outside dismisses it. A `Start` asks only about its first page from the chosen track on; explicit tracks that arrive later through a fill are skipped silently. Repeat one moves on from an explicit track once they're disallowed. Previous, TogglePlay and Resume still play the current or History track even if it's explicit.
- **Dimmed rows.** `track_list::track`/`marked` take `allow_explicit` and dim unavailable or blocked rows. Every list Page's `view` takes it, from `Playback::allow_explicit()`.
- **Tested:** 27 new Seam 1 tests, 1 catalog parsing test, 1 Shell test. 409 in the workspace. **Not run** in the window for this ticket.
