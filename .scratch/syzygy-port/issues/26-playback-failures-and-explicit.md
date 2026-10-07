# 26: Playback failures and Explicit content

**What to build:** a bad track doesn't stop playback, but a broken source doesn't spin forever. A rate-limit pauses and resumes on its own, a busy device is retried with a message, and the user decides whether Explicit content may play. See user stories 83, 86–87, 89–91 and the spec's "Playback model" (concurrency, failures, explicit content).

**Blocked by:** 23

**Status:** ready-for-agent

- [ ] The skip drain is a state machine that skips unplayable or unavailable tracks and stops after 3 in a row, with a toast. No locks or re-entry guards
- [ ] A 429 puts the entry back at the head, pauses, and emits `ResumeAfter(token, delay)`. It resumes unless the user did something in between
- [ ] A busy device is retried inside the runner's `Play` task, surfacing `AudioBusyRetrying` as a "device busy" toast. Audio errors are toasted, cut to 80 characters
- [ ] The allow-explicit setting is saved in `Settings`. With it off, starting, queueing or playing a source containing an explicit track returns `Outcome::NeedsExplicitConsent(pending)`. The Shell shows the modal: "Allow explicit content" sends `Settings(AllowExplicit)` then the pending message, and "Play without them" proceeds with them skipped
- [ ] Explicit entries reached any other way are skipped silently, shown dimmed, and don't count toward the skip cap. Turning the setting off removes nothing
- [ ] Seam 1 tests cover the skip drain and its cap, 429 with `ResumeAfter`, and the explicit outcomes and silent skips
