# Playback and queue model in update

Type: grilling
Status: resolved
Blocked by: 03, 05

## Question

How is sone's playback-actions behavior (queue, manual queue, original queue, shuffle, repeat, autoplay, history, gapless prefetch) modelled in syzygy's state and `update`, and how does it drive the audio engine through the bridge?

Rules to port are in [frontend behavior](../research/frontend-behavior.md). Decide deliberately whether to keep or fix sone's quirks: autoplay doesn't update "Playing from"; drag-reordering doesn't update the original queue; logout leaves queues and sources behind; pages loaded later are appended after the wrapped part of the queue; card play ignores shuffle mode; autoplay with no track mix doesn't clear `isPlaying`. Resolve the playback glossary terms into CONTEXT.md.

From 07: playback owns the queue-filling continuation. A Page hands over `PlayRequest { source, first_page, continuation }`, and playback runs the abortable task that appends the remaining pages, so it outlives the Page. Starting a new source cancels it.

## Answer

Terms (**Playback source**, **Queue entry**, **Manual queue**, **Source tag**, **Shuffle**, **Shuffle play**, **Repeat mode**, **Autoplay**, **Track radio**, **History**, **Album gain**, **Gapless advance**, **Explicit content**) are in `CONTEXT.md`.

- **Model** ([ADR 0004](../../../docs/adr/0004-playback-source-with-play-order.md)):
  - **Source and play order:** the Playback source owns its tracks in source order. Playback keeps a play order (indices into those tracks) and a cursor.
  - **Manual queue:** a separate list of Queue entries, each with a Source tag.
  - **What goes away:** `originalQueue` and the stashed context source.
  - **"Playing from":** shows the Source tag while a Manual queue entry plays, and the Playback source the rest of the time.
  - **Every play has a source.** A track played on its own (a search direct hit or top hit) gets a one-track source, and sone's "rebuild repeat-all from History" path goes away.
- **Starting playback:**
  - **No wrap-around:** clicking track k plays k…end.
  - **One entry point:** every start goes through `PlayRequest`. Card play now respects Shuffle.
  - **Shuffle play:** shuffles once and doesn't turn Shuffle on.
- **Shuffle:**
  - **On:** shuffles the unplayed tail of the play order.
  - **Off:** rebuilds the tail in source order, skipping what was already played or removed.
  - **Randomness:** a seedable `SmallRng` in playback state, seeded from entropy in `boot` and fixed in tests.
- **Repeat:** Repeat one replays the track on auto-advance only, while an explicit Next moves on. Repeat all rebuilds the play order from the source (shuffled again if Shuffle is on) and keeps History.
- **Autoplay:**
  - **Starting a radio:** the Track radio becomes the Playback source, which fixes sone's stale "Playing from".
  - **No mix, or a failed fetch:** playback stops. This fixes sone's quirk of leaving `isPlaying` true.
- **Previous:**
  - **More than 3 s in:** seek to 0.
  - **Otherwise, pop History:**
    - If the popped entry is the previous step in the play order, move the cursor back.
    - Otherwise, the current track goes to the front of the Manual queue, tagged with its source, and the popped entry plays under its own Source tag.
  - **Empty History:** seek to 0.
  - **What History entries record:** the entry and its Source tag.
- **Album gain:** derived. It applies when the current track comes from an Album source in album order (Shuffle off, not a Shuffle play). Otherwise track gain. It isn't persisted.
- **Explicit content:** the setting is in scope, stored in `Settings`.
  - **When the modal appears:**
    - Starting an explicit track, or a source that contains any explicit track, shows a modal: "[Allow explicit content] [Play without them]".
    - So does adding an explicit track to the Manual queue.
  - **Allow:** turns the setting on, then carries out the pending action. Cancel does nothing.
  - **Explicit entries reached during an advance:** skipped silently, without counting toward the skip cap.
  - **Turning it off:** wipes nothing. Explicit entries stay, are drawn dimmed and are filtered when reached, not when the queue is written.
  - **Who owns the modal:** `playback::update` returns `Outcome::NeedsExplicitConsent(pending)`. The Shell shows the modal and sends `Settings(AllowExplicit)` followed by the pending message.
- **Drawer operations:**
  - **Picking a source entry:** jumps the cursor to it, and the skipped entries don't go to History.
  - **Picking a Manual queue entry:** plucks it out and plays it.
  - **A History row:** plays that entry under its Source tag without changing what's upcoming.
  - **Drag:** reorders only within its section.
  - **Clear:** empties the Manual queue and the rest of the play order, and cancels the fill. The source stays, so Repeat all starts it over.
- **Continuation** (from 07):
  - **Plain data:** `PlayRequest.continuation` is plain data (`SourceRef` plus a cursor or offset) and is saved in the snapshot. An unfinished fill starts again after a restore.
  - **Running it:** playback emits `Effect::StartFill { fill_id, continuation }`, and the `App` holds the abort handle. Pages come back as `PageArrived(fill_id, tracks)`, and stale ids are dropped. A new source emits `CancelFill`.
  - **Where pages go:** at the end of the play order, or (with Shuffle on) at random positions in the unplayed tail.
  - **Shuffle play on a paginated source:** starts at once from the first page.
  - **Repeat all:** uses whatever has loaded so far.
- **Concurrency:**
  - **Play tokens:** each `Effect::Play` carries a `PlayToken`, and its result echoes it. Stale results are dropped.
  - **Skip drain:** a state machine. It stops after 3 unplayable or unavailable tracks in a row and shows a toast.
  - **No re-entry guard:** no lock and no 250 ms guard.
  - **Optimistic current track:** it changes straight away and rolls back on failure.
- **Failures:**
  - **Device busy:** retried inside the runner's `Play` task. `update` sees `AudioBusyRetrying` (for the toast) and then the final result.
  - **429:** the entry goes back to the head, playback pauses, and `Effect::ResumeAfter(token, delay)` resumes it unless the user acted in between.
- **Status and position:**
  - **Status:** `Stopped | Loading(token) | Playing | Paused`. This replaces `isPlaying` and `userPaused`.
  - **Position:** kept in state from the 250 ms tick, which reads `PositionCell`. While Loading it shows the target position.
  - **Play while Stopped with a current track:** plays it from the stored position.
- **Gapless:**
  - **What gets armed:** whatever comes next, when state already knows it:
    - Manual queue heads, whatever their source.
    - The next entry in the play order.
    - Repeat one (the same track).
    - The first entry of the repeat-all rebuild (its order is computed ahead of time).
    - The Track radio, fetched when the last track starts.
  - **Never armed:** a track that will be skipped (explicit or unavailable).
  - **`TrackAdvanced`:** reconciled by entry id, and it applies the Source tag switch.
- **MPRIS:** after each `update` that touches playback, derive an `MprisView` and compare it with the last one sent. `Effect::Mpris(diff)` goes out only on a change. MPRIS commands go through the same playback messages as the UI.
- **Reporting:** "chosen by user" is false only for tracks from a Track radio that Autoplay started, whether the advance was gapless or not.
- **Volume, mute and bit-perfect:**
  - **Ownership:** they're part of playback state.
  - **The ramp:** a single `Effect::RampVolume { to, over }`.
  - **Linked modes:** turning exclusive mode or bit-perfect on or off also changes the other, and that rule stays in `update`.
  - **Still open:** which controls the Settings modal shows stays with the map's open settings item.
- **Persistence:**
  - **Preferences** (shuffle, repeat, autoplay, volume, allow-explicit) live in `Settings`, survive logout, and playback reads them from there.
  - **`queue.json`** (in `syzygy-store`, with no fallback store) holds the source (tracks, play order, cursor, unfinished fill), the Manual queue, History (capped at 500), the current track and the position. It's saved through `Effect::SaveSnapshot`, debounced ~2 s and again on Quit.
  - **Restore:** comes back paused at the saved position.
  - **Logout:** wipes it (as decided in 06).
- **Effects:** `Play`, `Pause`, `Resume`, `Stop`, `Seek`, `ArmNext`/`ClearNext`, `SetVolume`/`RampVolume`, `StartFill`/`CancelFill`, `FetchTrackRadio`, `ResumeAfter`, `Mpris(diff)`, `Report…`, `SaveSnapshot`.
