# Frontend behavior inventory (sone/src → syzygy `update`)

Ticket: [03-frontend-behavior-inventory](../issues/03-frontend-behavior-inventory.md)

Sources: `sone/src` at upstream `0488f97` (read-only). All paths below are relative to `sone/src/` unless they start with `src-tauri/`. Each claim cites `file:line`. Where sone's behavior looks accidental, it is marked **Quirk**, so syzygy can choose on purpose whether to keep it.

---

## 1. State model (Jotai atoms → future iced `State`)

### 1.1 Playback (`atoms/playback.ts`)

| Atom | Type / default | Persisted | Meaning |
|---|---|---|---|
| `isPlayingAtom` | `bool` = false | no | Whether audio is playing, from the UI's point of view (`:56`) |
| `currentTrackAtom` | `Track?` | session snapshot | The current track (`:57`) |
| `volumeAtom` | `f64` = 1.0 | localStorage `sone.volume.v1` | Linear amplitude 0..1 (`:58`) |
| `queueAtom` | `Track[]` | snapshot | The **context queue**: what's left of the playing source (`:61`) |
| `historyAtom` | `Track[]` | snapshot | Tracks already played, oldest first, capped at 500 (`:62`, `hooks/usePlaybackActions.ts:171`) |
| `streamInfoAtom` | `StreamInfo?` | no | Quality and codec of the live stream (`:63`) |
| `preMuteVolumeAtom` | `f64` = 0 | no | Volume to restore on unmute (`:64`) |
| `autoplayAtom` | `bool` = false | localStorage `sone.autoplay.v1` | Start a track radio when the queue runs out (`:65`) |
| `useTrackGainAtom` | `bool` = true | no | Replay gain: per track (true) or per album (false, album played in order) (`:67-68`) |
| `repeatAtom` | `0\|1\|2` | localStorage `sone.repeat.v1` | 0 = off, 1 = repeat-all, 2 = repeat-one (`:70`) |
| `shuffleAtom` | `bool` = false | localStorage `sone.shuffle.v1` | Global shuffle mode (`:71`) |
| `manualQueueAtom` | `Track[]` | snapshot | The **manual queue** filled by "Play next" and "Add to queue". It always plays before the context queue (`:72`) |
| `originalQueueAtom` | `Track[]?` | snapshot | The context queue in its unshuffled order. Non-null only while shuffle is on (`:73`) |
| `playbackSourceAtom` | `PlaybackSource?` | snapshot | Where the current track comes from (the "Playing from" label), plus the source's full track list (`:74`) |
| `contextSourceAtom` | `PlaybackSource?` | snapshot | The context source, stashed while a manual-queue track with a different `_source` plays (`:75`) |
| `allowExplicitAtom` | `bool` = true | localStorage `sone.allowExplicit.v1` | When false, explicit tracks are filtered out of every queue write (`:77`) |
| `exclusiveModeAtom`, `bitPerfectAtom`, `gaplessAtom`, `maxQualityAtom`, `exclusiveDeviceAtom`, `volumeNormalizationAtom` | | backend is the source of truth (hydrated with `get_*`) | Output settings (`:79-87`, hydrated at `components/AppInitializer.tsx:317-334`) |
| `bitPerfectPreviousStateAtom` | `{volume, volumeNormalization}?` | localStorage | What to restore when bit-perfect is turned off (`:94-100`) |
| `consecutiveFailCountAtom` | `u32` | no | How many unplayable tracks in a row the auto-advance has skipped (`:104`) |
| `userPausedAtom` | `bool` | no | Whether the user explicitly paused. Global, so a gapless advance never un-pauses (`:106-109`) |
| `signalPathAtom` | `SignalPath?` | no | Signal-path panel (out of scope) (`:54`) |

Runtime-only refs in `usePlaybackActions` that must become state fields in syzygy:
- `playGenerationRef`: a counter bumped on every play. A play result whose generation is stale is dropped (`hooks/usePlaybackActions.ts:204`, `:302`, `:339`).
- `autoplayIdsRef`: ids of the tracks autoplay put in the queue. They report `chosenByUser = false` (`:205`, `:1021`, `:1148`).
- `playNextLockRef`: a mutex shared by `playNext` and `playPrevious` (`:206`, `:837`, `:1188`).
- `lastPlayInvokeRef`: a 250 ms guard against re-entry. A second `playTrack` within 250 ms returns `transient` (`:172`, `:256-260`).
- `pendingNextRef` (the gapless slot), owned by AppInitializer (`components/AppInitializer.tsx:212`).

### 1.2 Track identity: `_qid`, `_source`, `_playingFrom`, `_contextFrom` (`types.ts:75-147`, `lib/qid.ts`)
- `QueuedTrack = Track + _qid + optional _source: ManualTrackSource` (`types.ts:135-147`).
- `_qid` is a per-instance queue id of the form `q<N>`, from a global counter (`lib/qid.ts:5-7`). It lets the same track id appear twice in a queue. `ensureQid` keeps an existing qid (`:13-16`). After a restore, `advanceCounterPast` moves the counter past every restored qid so new qids can't collide (`:18-26`, `components/AppInitializer.tsx:649-661`).
- `_source`: a source tag on a manual-queue entry (type, id, name, image, subtitle, mixType). It has no track list (`types.ts:135-142`).
- `_playingFrom` and `_contextFrom`: copies of the playback and context source, stamped on the current track at play time so `playPrevious` can restore "Playing from" (`hooks/usePlaybackActions.ts:321-323`). They are stripped before persisting (`components/AppInitializer.tsx:148-167`).
- `PlaybackSource = {type, id, name, image?, subtitle?, mixType?, tracks: QueuedTrack[]}` (`types.ts:661-669`). `tracks` holds the source's **full** track list and is used by repeat-all and by the history-less `playPrevious` (§2.6, §2.8).
- Source types in use: `album`, `playlist`, `playlist-recs`, `mix`, `artist`, `artist-tracks`, `favorites`, `radio`, `search`. Of these, all but `search` are navigable (`lib/playbackSource.ts:1-13`). No UI path in `src/` sets `radio`; it shows up only as a navigation case (`components/PlayerBar.tsx:300-306`).

### 1.3 Library (`atoms/favorites.ts`, `atoms/playlists.ts`)
- Sets of favorite ids: tracks, videos, albums, playlist uuids, followed artists, mix ids (`atoms/favorites.ts:5-10`).
- Optimistic overlays for the sidebar and library lists: `optimisticFavoriteAlbums`, `optimisticFollowedArtists` and `optimisticFavoriteMixes`, each with the newest first (`:11-13`).
- Sort preferences, persisted: album, artist and mix sorts default to `DATE DESC`, playlists default to `DATE_UPDATED DESC`, and a track sort is stored per playlist (`:17-31`).
- Playlists: `userPlaylistsAtom` (playlists at the root level), `allPlaylistsAtom` (a flat list of every playlist, used by "Add to playlist") (`atoms/playlists.ts:4-6`).
- Optimistic mutation overlays (`atoms/playlists.ts:7-26`):
  - `deletedPlaylistIds` and `deletedFolderIds`: sets that hide the item.
  - `movedPlaylists`: a map from uuid to the folder it was moved **from**, which hides it there.
  - `folderCountAdjustments`: a map from folder id to a count delta.
  - `addedToFolder`: a map from folder id to entries to prepend.
  - `renamedFolders`: a map from folder id to its new name.
  - `updatedPlaylists`: a map from uuid to `{title, description}`.
  - `allFolders` and `allFoldersFetched`: a cache of folders for the move menu, invalidated by setting the flag to false.

### 1.4 Auth, navigation and UI
- Auth: `isAuthenticated`, `isAuthChecking` (true until `load_saved_auth` returns), `authTokens`, `userName` (default `"TIDAL User"`), `currentUserAvatar` (`atoms/auth.ts:4-8`).
- Navigation: `currentViewAtom: AppView`, default `{type:"home"}` (`atoms/navigation.ts:4`). `AppView` is a union of views (`types.ts:213-283`).
- UI: `drawerOpen`, `drawerTab` (default `"queue"`), `maximizedPlayer`, `sidebarCollapsed` (`atoms/ui.ts:4-8`). The drawer tabs are `queue | suggested | lyrics | credits` (`components/NowPlayingDrawer.tsx:79`). `toggleDrawerTab(tab)` closes the drawer when that tab is already open, and otherwise switches to the tab and opens it (`hooks/useDrawer.ts:21-30`).

---

## 2. Playback rules (`hooks/usePlaybackActions.ts`)

### 2.1 `playTrack(track, {chosenByUser=true, skipHistoryPush, suppressUnplayableToast})` (`:242-405`)
1. Re-entry guard: a call within 250 ms returns `{ok:false, reason:"transient"}` (`:256-260`).
2. Videos are out of scope. When a video is current, audio playback stops it (`:263-300`).
3. Sets `userPaused = false`, bumps the generation, and gives the track a qid (`:301-303`).
4. **Updates the UI optimistically:** pushes the previous current track onto history unless `skipHistoryPush`, capped at 500 (`:312-320`). Stamps `_playingFrom` and `_contextFrom` (`:322-323`), sets `markPlaybackLoading(true)` (which freezes the position bar), and sets `currentTrack` (`:326-327`).
5. Invokes `play_tidal_track {trackId, useTrackGain}`. If the reply is `device_busy`, it retries every 500 ms up to 10 times, and on the first retry shows the toast "Preparing exclusive audio…" (`:177-198`, `:330-337`).
6. On success, if the generation still matches: sets `streamInfo`, `isPlaying = true` and `failCount = 0`, clears the loading gate, and sends `notify_track_started {payload}` (fire and forget) (`:342-356`).
7. On failure, if the generation still matches, rolls back `currentTrack` and `history` and sets `isPlaying = false` (`:361-366`). Errors are classified in this order:
   - `blocked`: proxy refused. Toast, then halt.
   - `network`: rate-limited "network-error" event.
   - `rate-limited`: HTTP 429. Schedules an automatic `playNext` after `retryAfterSecs + 1` s, unless the user paused or the generation changed.
   - `unplayable`: HTTP 404, 410 or 451, or a 401 with a terminal subStatus.
   - anything else is `transient` and shown as a "playback-error" toast.

   Sources: `:367-401`, `:214-228`, `lib/trackAvailability.ts:216-253`.

`isTrackUnavailable` checks metadata only: `streamReady === false`, `allowStreaming === false`, or a `streamStartDate` in the future. Videos are never unavailable (`lib/trackAvailability.ts:1-19`).

### 2.2 Pause, resume and toggle (`:407-516`)
- `pause`: `userPaused = true`, `pause_track`, `isPlaying = false`.
- `resume`: `userPaused = false`. If `is_track_finished` is true, the track is replayed from the top with `play_tidal_track` and the replay is scrobbled. Otherwise `resume_track` is called (`:451-478`).
- **Session-restore consequence:** after a restart the backend has no URI, so `is_finished` returns true (`src-tauri/src/audio.rs:2901-2905`). Pressing Play on a restored track therefore restarts it from 0:00. **The playback position is not restored.**
- `toggle`: depends on `isPlaying` (`:503-516`).

### 2.3 Manual queue
- `addToQueue(track, source?)`: rejects explicit tracks when explicit content isn't allowed, stamps a new qid and the optional `_source`, and **appends** the track to the manual queue (`:690-698`).
- `playNextInQueue(track, source?)`: does the same but **prepends** (`:700-708`). When "Play next" is used on a whole album, playlist or mix, the tracks are prepended in reverse order so they keep their original order (`components/MediaContextMenu.tsx:228-240`).
- Default `_source` tag:
  - From track context menus: the track's album (`components/TrackContextMenu.tsx:74-80`, `NowPlayingDrawer.tsx:~810`).
  - From media cards: the album, playlist or mix itself (`components/MediaContextMenu.tsx:129-155`).

### 2.4 Starting playback from a source (the entry points the screens call)
- **`setQueueTracks(tracks, {albumMode, source, reorder, manualCount})`** (`:710-761`):
  - With `reorder`, it only splits the combined list back into manual (the first `manualCount` entries) and context, keeping the qids. The drawer uses this for drag-and-drop.
  - Otherwise it does the following:
    - Filters out explicit tracks.
    - Sets `useTrackGain = !albumMode`.
    - Clears `originalQueue`, `manualQueue` and `contextSource`.
    - Sets `playbackSource` from `source`. Its `tracks` come from `source.allTracks`, each stamped with a qid.
    - Sets `queue = stamped(eligible)`.
  - **This clears the manual queue.**
- **`setShuffledQueue(tracks, opts)`** (`:1489-1532`):
  - Filters explicit tracks, clears the manual queue and the context source, and puts the tracks in the queue in Fisher-Yates order.
  - Sets `originalQueue = stamped` **only if global shuffle is already on**, and never turns global shuffle on (test `hooks/usePlaybackActions.test.tsx:50-79`).
  - In other words, a "Shuffle" button on a detail page plays one shuffled pass without changing the shuffle mode.
- **`playFromSource(track, allTracks, opts)`** (clicking a track row) (`:1576-1619`):
  - Builds `rest` as the tracks after the clicked one, **followed by the tracks before it** (wrap-around) (`:1597-1601`).
  - Uses `setShuffledQueue(rest)` if shuffle is on, otherwise `setQueueTracks(rest)`. Resets the fail count and calls `playTrack(track)`.
  - If the result is `unplayable`, calls `playNext({explicit:true})`. If it is `rate-limited`, calls `requeueHead(track)`.
- **`playAllFromSource(allTracks, opts)`** (the "Play" button) (`:1621-1662`):
  - Filters out unavailable and explicit tracks; returns if nothing is left.
  - If shuffle is on, picks a random first track and builds a shuffled queue from the rest. Otherwise the head plays and the tail goes through `setQueueTracks`.
  - Falls back the same way as `playFromSource`.
- **`useMediaPlay(item)`** (the play overlay on cards) (`hooks/useMediaPlay.ts:41-93`):
  - 250 ms guard, then `fetchMediaTracks(item)`: an album page, the playlist's tracks, a mix's items, or an artist's top tracks (`api/tidal.ts:785-815`).
  - Calls `setQueueTracks(rest, {source})`. **This ignores shuffle mode.** It then plays the first track; if that track is unavailable or unplayable it calls `playNext({explicit:true})`.
  - On `rate-limited` it puts the first track back with `setQueueTracks([first, ...rest])`.
  - Artists get **no** source (`buildSource` default, `:36-37`).
- **`appendToQueue(newTracks)`** (`:763-800`): the background paginator calls this as later playlist or favorites pages arrive (`components/PlaylistView.tsx:470-472`, `components/FavoritesView.tsx:488-491`).
  - Filters explicit tracks and appends them to `playbackSource.tracks`.
  - If shuffle is on, it also appends them to `originalQueue` and inserts each one at a random position in the queue. Otherwise it appends them to the queue.
  - **Quirk:** `playFromSource` wraps around, but later pages are appended after the wrapped earlier tracks. For a paginated playlist the queue order is therefore [after clicked] + [before clicked] + [pages not loaded yet].
- Album pages pass `albumMode: true`, which selects album gain (`components/AlbumView.tsx:166-193`). Album "Shuffle" shuffles locally, then calls `setShuffledQueue(rest, {albumMode:true})` and `playTrack(first)` (`:184-197`).
- Playlist and favorites "Shuffle" first await `fetchRemaining()` so the shuffle covers every track (`components/PlaylistView.tsx:521-536`, `components/FavoritesView.tsx:~512-529`).
- Search results: `playFromSource` with `{type:"search", id:query, name:"Search: <q>"}` (`components/SearchView.tsx:172-183`). A track direct-hit from the search bar or the top hit calls `setQueueTracks([])` and then `playTrack`, so it plays alone with no source (`components/SearchBar.tsx:531-532`, `components/SearchView.tsx:491-492`).
- `SourcePlayButton`: if `playbackSource.type/id` equals this page's source, the button toggles play and pause. Otherwise it calls `onPlay` (`components/SourcePlayButton.tsx:31-46`).

### 2.5 `playNext({explicit})`: auto-advance and the Next button (`:835-1183`)
It is serialized by `playNextLock`. It sets `userPaused = false` (`:837-839`), then:
1. **Repeat-one, when `explicit` is false:** replays the current track in place with `play_tidal_track`, without touching history, and calls `notifySeek(0)`. Errors are classified as in §2.1, and `rate-limited` schedules the resume (`:844-918`). An explicit Next skips this step even under repeat-one.
2. Calls `stop_track`, which prevents stale `track-finished` events (`:921`). An explicit call resets the fail count (`:925-927`).
3. **Drains the manual queue first** (`:946-1008`). For each head:
   - If the head is unavailable, it is dropped, counted as a failure, and the loop continues.
   - Otherwise it is popped. If it carries a `_source`:
     - The current `playbackSource` is stashed into `contextSource`, but only if that is empty.
     - `playbackSource` is set to the `_source` tag with `tracks: []`.
   - Then `playTrack(head, {chosenByUser: explicit, suppressUnplayableToast})` runs. On `unplayable`, the source change is rolled back, the failure is counted, and the loop continues. On any other failure, the source is rolled back, the **head is pushed back**, and playNext returns.
4. **When the manual queue is empty, restores the stashed context source** into `playbackSource` and clears `contextSource` (`:1011-1015`).
5. **Drains the context queue** (`:1018-1069`) the same way:
   - Each popped head is also removed from `originalQueue` **by `_qid`**.
   - The track is played with `chosenByUser = !autoplayIds.has(id)`.
   - On a non-unplayable failure the head goes back into both the queue and `originalQueue`.
6. If both queues are empty:
   - **Repeat-all** (`:1071-1127`): picks `contextSource ?? playbackSource`. If it has `tracks`, they become the new pool and **history is kept**. Otherwise the pool is history plus the current track, and **history is cleared**, with the first replay done with `skipHistoryPush` (test `usePlaybackActions.test.tsx:81-134`).
     - The pool is filtered for explicit content and availability, then restamped.
     - If shuffle is on, the pool is Fisher-Yates shuffled and `originalQueue` becomes the pool minus the first track. Otherwise `originalQueue = null`.
     - Then it plays the first track. On `unplayable` it releases the lock and recurses. On `rate-limited` it calls `requeueHead`.
     - If the pool is empty, `isPlaying = false`.
   - **Autoplay**, when repeat is off and autoplay is on (`:1128-1174`):
     - Requires `current.mixes.TRACK_MIX`; otherwise it does nothing. **Quirk:** this path returns early without setting `isPlaying = false` (`:1137`).
     - Calls `getMixItems(mixId)` and drops tracks already in history or current, explicit tracks if disallowed, and unavailable tracks.
     - `autoplayIds` becomes the ids of the rest, the queue becomes the rest, `useTrackGain = true`, and the first track plays with `chosenByUser:false`.
     - **Quirk:** `playbackSource` is not changed, so "Playing from" still shows the old source during autoplay.
     - On a network or 429 error it falls through to stop. A 429 schedules a resume.
   - Otherwise: `isPlaying = false`.
7. **Skip cap:** when the auto-advance drain hits 3 unplayable tracks in a row (`MAX_CONSECUTIVE_PLAY_FAILS`), it shows the toast "Multiple tracks failed to play — stopped", resets the count and stops. A drain shows the toast "Track unavailable — skipping" at most once (`:89`, `:928-943`).
8. A blocked proxy never enters the skip drain. The queues stay intact (test `usePlaybackActions.test.tsx:136-211`).

### 2.6 `playPrevious()` (`:1187-1463`)
- If the interpolated position is more than 3 s, it seeks to 0 and returns (`:1191-1195`). Otherwise it resets the fail count and calls `stop_track`.
- **When there is history:**
  - Pops the last entry.
  - **Pushes the current track to the front of the manual queue**, tagged with `_source` from the current `playbackSource`, so a later Next returns to it with the correct "Playing from" (`:1227-1245`).
  - Restores `playbackSource` and `contextSource` from the popped entry's `_playingFrom` and `_contextFrom` (`:1248-1252`). Sets `currentTrack` and calls `play_tidal_track`.
  - Unlike `playTrack`, nothing is pushed to history.
  - On error, restores a **full snapshot**: current track, history, queue, original queue, manual queue and both sources (`:1302-1309`).
- **When there is no history:**
  - Finds the current track in `playbackSource.tracks`. If it is at index > 0, the previous source track plays.
  - The current track is pushed back to the front of the manual queue (tagged) if the manual queue is non-empty. Otherwise it goes to the front of the context queue, and is also inserted into `originalQueue` at its position in source order (`:1331-1388`).
  - If the index is 0 or the track isn't in the source, it seeks to 0.
- Note: playPrevious never calls `playTrack`, so the 250 ms guard and history push don't apply. It calls `invokePlayWithRetry` directly.

### 2.7 Shuffle toggle (`:1465-1487`)
- **On:** `originalQueue = queue`, `queue = shuffle(queue)`, `shuffle = true`. Only the context queue is shuffled; the manual queue keeps its order.
- **Off:** `queue = originalQueue` filtered to the `_qid`s still in the queue (so tracks already played don't come back), then `originalQueue = null` and `shuffle = false`.
- The MPRIS `set-shuffle` handler calls toggle only when the requested state differs from the current one (`components/AppInitializer.tsx:1250-1254`).

### 2.8 Repeat (`components/PlayerBar.tsx:603`, `components/AppInitializer.tsx:1333-1335`)
- The button and the shortcut cycle `(repeat + 1) % 3`: off, then all, then one. MPRIS writes the value directly (`:1255-1257`).
- Repeat-one affects only the automatic `playNext`. An explicit Next skips to the next track (§2.5.1).
- The gapless predictor returns null under repeat-one, so the loop goes through the EOS path (§2.10).

### 2.9 Other queue operations
- `removeFromQueue(index)`: the index runs over the combined list, manual queue first. Removing a context entry also removes it from `originalQueue` by `_qid` (`:802-833`).
- `playFromQueue(index)`: refuses an unavailable track with a toast. Otherwise it resets the fail count, removes the entry from its queue (and from `originalQueue`), then calls `playTrack` (`:1534-1574`).
- `clearQueue()`: empties the queue, manual queue and `originalQueue`, and sets both sources to null. **It keeps history and the current track** (`:1664-1670`).
- Drawer drag-and-drop (`components/NowPlayingDrawer.tsx:255-285`):
  - Moves an item in the combined list.
  - Dragging an item from manual into context gives `manualCount - 1`; from context into manual, `+1`.
  - Then calls `setQueueTracks(reordered, {reorder:true, manualCount})`. This **does not update `originalQueue`**.
- History rows in the drawer show the last 10 entries, and clicking one calls `playTrack(track)` (`components/NowPlayingDrawer.tsx:384-398`). This pushes the current track to history; the clicked entry stays in history too.
- Changing the "Allow explicit content" setting stops playback and clears the current track, queue, manual queue, original queue, history and both sources (`components/settings/PlaybackTab.tsx:146-158`).

### 2.10 Gapless prefetch (`hooks/useGaplessPrefetch.ts`, `lib/gaplessPredict.ts`, `components/AppInitializer.tsx:860-936`)
- **What is predicted** (`lib/gaplessPredict.ts:16-30`, `usePlaybackActions.ts:522-529`): `pickGaplessNext`.
  - Returns null under repeat-one.
  - Otherwise the candidate is the manual head, or the context head if there is no manual head.
  - Returns null if the candidate is unavailable or a video, **or if it has a `_source` whose id differs from the current `playbackSource.id`** (a source switch is never gapless).
  - Autoplay radio and the repeat-all rebuild are never predicted, so they always take the EOS path.
- **When gapless is enabled** (`useGaplessPrefetch.ts:61-67`): `get_gapless_supported` (cached) and the gapless setting are both true, exclusive mode and bit-perfect are both off, no video is current, and there is a current track. It does **not** depend on `isPlaying`, so the slot stays armed while paused.
- **Arming the slot:**
  - Calls `set_next_track {trackId, qid, useTrackGain}`, which returns a `StreamInfo` stored as `pendingNext {trackId, qid, track, streamInfo}`.
  - Calls `clear_next_track` when the feature is disabled or nothing is predicted.
  - Dedup: skips the call when both trackId and qid are unchanged. A changed qid is sent again (`:86-92`).
  - Remembers a failed (trackId, qid) for 10 s (`:29`, `:96-104`).
  - Coalesces calls made while one is in flight (`:108-111`). Uses a generation guard (`:119`, `:128`).
- **Triggers:**
  - Debounced by 250 ms: changes to the manual queue, queue, current track (which also clears the failure memo), repeat, shuffle and autoplay.
  - Immediate: changes to exclusive mode, bit-perfect and gapless (`:171-191`).
  - A saved proxy setting resets the slot and re-arms it (`:161-169`).
- **`track-advanced {trackId, qid}`** (the backend already switched audio, so "audio reality wins") (`AppInitializer.tsx:860-936`):
  - This event is never gated by the playNext lock.
  - Removes **exactly one** entry with a matching qid from the manual queue; if none, from the context queue, falling back to the first entry with the same trackId. Context removals also drop the entry from `originalQueue` by qid and from the autoplay ids.
  - Then `advanceToTrack(pending.track, pending.streamInfo)` (`usePlaybackActions.ts:536-575`): bumps the generation and pushes the previous track to history. Does **not** call `markPlaybackLoading(true)`. Sets the current track and stream info, `isPlaying = !userPaused`, `failCount = 0`, and notifies with `chosenByUser = false`.
  - If the pending slot is missing or doesn't match, it calls `get_stream_info {trackId, useTrackGain}` and `get_track {trackId}`, then advances.
  - **advanceToTrack doesn't apply the manual `_source` switch**. That's safe because the predictor never arms a track from a different source.
- **`track-finished`** (end of stream with nothing armed): `streamInfo = null`, then `playNext()` with `explicit` false (`AppInitializer.tsx:943-951`).

### 2.11 Position interpolation (`lib/playbackPosition.ts`)
- While playing, polls `get_playback_position` every 2 s and interpolates locally between polls (`:31-70`, `:84-88`).
- On pause, the position freezes at its live value. On resume, the clock re-anchors, so the paused time doesn't count (`:~160-180`).
- A track change resets the position to 0 and opens a 3 s settle window. During that window a poll more than 2 s ahead of the interpolated value is ignored (it is a leftover from the gapless concat) (`:26-29`, `:46-52`).
- `notifySeek(t)` anchors the position at `t`, clears the loading gate and settle window, and re-polls after 300 ms (`:95-125`).
- `markPlaybackLoading` freezes the position during an explicit load (`:132-136`).
- syzygy can replace this with position events or a subscription, but should keep the semantics: freeze while loading, no backward jump on pause, ignore stale polls after a track change.

### 2.12 Volume and bit-perfect (`usePlaybackActions.ts:577-679`)
- `setVolume` does nothing while bit-perfect is on, and the slider is locked (`components/VolumeSlider.tsx:29`). Otherwise it calls `set_volume {level}`.
- **Bit-perfect on:**
  1. Save `{volume, normalization}`.
  2. Ramp the volume to 1.0: 12 steps over 300 ms, each a `set_volume` call.
  3. `set_volume_normalization {enabled:false}`.
  4. `set_bit_perfect {enabled:true}`.
- **Bit-perfect off:**
  1. `set_bit_perfect {false}`.
  2. Ramp back to the saved volume.
  3. Restore the saved normalization.
- Turning on bit-perfect from the shortcut also turns on exclusive mode. If no exclusive device is set, it picks the first one from `list_audio_devices` and calls `set_exclusive_device` (`components/AppInitializer.tsx:1365-1389`). Turning exclusive mode off also turns bit-perfect off (`components/UserMenu.tsx:173-187`).
- Mute toggle: if the volume is above 0, save it to `preMuteVolume` and set 0; otherwise restore `preMuteVolume`, or 0.5 if there is none (`AppInitializer.tsx:1314-1322`).
- At startup, the persisted volume is sent with `set_volume` only if `get_bit_perfect` returns false (`:830-851`).

---

## 3. Session restore and persistence (`components/AppInitializer.tsx`)

### 3.1 Auth bootstrap (`:275-400`)
1. `load_saved_auth` returns tokens or null. If null, `isAuthChecking = false` and the login screen shows.
2. If `user_id` is missing, `get_session_user_id` provides it. Then `authTokens` is set, `isAuthenticated = true` and `isAuthChecking = false`. Home shows right away and the rest loads in the background.
3. `consume_legacy_auth_notice` shows a "re-login with PKCE" toast. This doesn't apply to syzygy.
4. Hydrates the output settings: `get_exclusive_mode`, `get_bit_perfect`, `get_gapless`, `get_max_quality`, `get_exclusive_device`, `get_volume_normalization` (and `get_proxy_settings`, which is out of scope) (`:317-337`).
5. Loads root playlists with `get_playlist_folders {folderId:"root", offset:0, limit:50, ...}` and keeps only the playlists in `userPlaylists`.
   - On a 401 it calls `refresh_tidal_auth` and retries once. If the refresh fails, it logs out locally (`isAuthenticated = false`, tokens null) (`:339-388`).
6. Prefetches `getHomePage()` without waiting (`:391`).

### 3.2 After authentication (`:423-488`, runs on every login)
- `get_user_profile {userId}` returns `[name, …]` and sets the user name. `get_profile {userId}` provides the avatar.
- `get_all_favorite_ids {userId}` returns the track, album, artist and playlist id sets. Separate calls: `get_favorite_mix_ids` and `get_favorite_video_ids {userId}`.
- `get_feed {userId}` sets the unseen count (feed is out of scope).
- After 2 s, warms the cache: `get_favorite_tracks (0, 50)`, `get_favorite_artists (0, 20)`, `get_favorite_albums (0, 20)`.

### 3.3 Playback snapshot (`:562-784`)
- **Restore order:**
  1. `load_playback_queue`, which returns a JSON string or null and reads `queue.json` in the config directory (`src-tauri/src/commands/playback.rs:403-406`).
  2. If that fails or returns null, localStorage `sone.playback-state.v1` (`:664-681`).
- **`PlaybackSnapshot`** fields: `currentTrack`, `queue`, `history`, `originalQueue?`, `manualQueue?`, `playbackSource?`, `contextSource?`. The sources include their `tracks` (`types.ts:671-690`).
- **Sanitizing on restore** (`:574-662`):
  - Entries whose `id` isn't a number are dropped and runtime fields are stripped.
  - History is capped at 500.
  - `originalQueue`, `manualQueue` and the sources' `tracks` get `ensureQid`. Note that the **context queue keeps its persisted `_qid`s** and doesn't get `ensureQid`.
  - The qid counter is advanced past every restored qid.
  - `originalQueue` is set to null when it is absent.
- **Not persisted:** `isPlaying` (restore always starts paused), the position (§2.2), `streamInfo`, `useTrackGain` (resets to true, so album gain is lost after a restart), `autoplayIds`, `userPaused`. Shuffle, repeat, autoplay and volume are persisted separately in localStorage.
- **Persistence** starts only after restore finishes, to avoid a race (`:763-765`):
  - Any change to the 7 snapshot atoms marks the state dirty. A microtask writes it to localStorage immediately, and `save_playback_queue {snapshotJson}` runs 2 s after the last change (`:683-761`).
  - Pending writes are flushed on unmount (`:777-782`).
- syzygy should keep a single store, probably just the backend `queue.json`, and drop the localStorage fallback.

### 3.4 Logout (`hooks/useAuth.ts:237-290`)
- Sets `isPlaying = false`, `currentTrack = null` and `queue = []`, then calls `logout`.
- Clears the frontend cache, auth, name and avatar, history, streamInfo and `userPaused`.
- Clears every favorite set and optimistic overlay, every playlist and folder atom, and the scroll offsets, and sets the view to home.
- Removes these localStorage keys: playback state, volume, search history.
- **Quirk:** it doesn't clear `manualQueue`, `originalQueue`, `playbackSource` or `contextSource`. The persistence subscription can write them back to `queue.json`.

### 3.5 Other global listeners in AppInitializer that are in prototype scope
- `audio-error {kind, message}` (`:981-1007`):
  - Sets `isPlaying = false`.
  - `device_disconnected` and `playback_error` also clear `streamInfo`.
  - `device_busy` shows "Audio device is busy — close other apps using it". Any other kind shows the message, cut to 80 characters.
- `audio-resampled {from, to}` and `audio-bit-depth-changed {from, to}`: info toasts (`:1012-1047`).
- **MPRIS (in scope):**
  - Commands sent: `update_mpris_metadata {metadata:{…, userRating: fav ? 1 : 0}}`, `update_mpris_playback_status {isPlaying, positionSecs?}`, `update_mpris_shuffle {enabled}`, `update_mpris_loop_status {mode}`, `update_mpris_fullscreen` (`:1150-1299`).
  - Events handled:
    - `mpris:play` and `mpris:pause` act only on a state change. `mpris:stop` calls `stop_track` and sets `isPlaying = false`.
    - `mpris:seek` (relative): `get_playback_position`, then `seek_track`. `mpris:set-position` calls `seekTo`.
    - `mpris:set-volume` (clamped to 0..1), `mpris:set-shuffle`, `mpris:set-loop-status`.
    - `mpris:open-uri` (deep link, out of scope), `mpris:set-fullscreen`.
- Tray, global shortcuts, theme, deep links, scrobble, update check, MCP and overlay are out of scope. Keyboard shortcuts are listed at `:1306-1393`.

---

## 4. Library editing flows

All mutations are **optimistic with rollback**. On success, the frontend cache is invalidated by tag.

### 4.1 Favorites (`hooks/useFavorites.ts`)
| Action | Optimistic step | Command (args) | Rollback |
|---|---|---|---|
| Like track | add id; prepend to the cached favorite pages | `add_favorite_track {userId, trackId}` | remove id and the cached entry (`:56-78`) |
| Unlike track | remove id and cached entry | `remove_favorite_track {userId, trackId}` | re-add id (cached entry not restored) (`:80-101`) |
| Like album | add id; cache; prepend to `optimisticFavoriteAlbums` | `add_favorite_album {userId, albumId}` | undo all (`:151-184`) |
| Unlike album | remove id, cache entry and optimistic entry | `remove_favorite_album {userId, albumId}` | re-add id only (`:186-210`) |
| Like playlist | add uuid; cache | `add_favorite_playlist {userId, playlistUuid}` | undo (`:214-240`) |
| Unlike playlist | remove uuid; **add to `deletedPlaylistIds`** (hides it from the library); cache | `remove_favorite_playlist {userId, playlistUuid}` | re-add uuid only (`:242-265`) |
| Follow artist | add id; cache; prepend to `optimisticFollowedArtists` | `add_favorite_artist {userId, artistId}` | undo (`:269-302`) |
| Unfollow artist | remove id, cache entry and optimistic entry | `remove_favorite_artist {userId, artistId}` | re-add id (`:304-330`) |
| Like or unlike mix | add or remove id and the optimistic entry; `invalidateCache("fav-mixes")` | `add_favorite_mix {mixId}` / `remove_favorite_mix {mixId}` (no userId) | undo (`:334-386`) |

Favorite-video commands exist (`add_favorite_video` / `remove_favorite_video {userId, videoId}`) but videos are out of scope. Every call site checks `favoriteTrackIds.has(id)` to decide between like and unlike. Examples: the player bar heart (`components/PlayerBar.tsx:175-190`) and the like shortcut (`AppInitializer.tsx:1323-1331`). The sidebar and library lists merge the optimistic overlay into the paginated server list and filter by the live id set (`components/Sidebar.tsx:235-346`).

### 4.2 Playlists (`hooks/usePlaylists.ts`, `components/AddToPlaylistMenu.tsx`)
- **Create:**
  - `create_playlist {title, description, accessType}`. The access type is `"PUBLIC"` or `"UNLISTED"` and defaults to UNLISTED.
  - The returned playlist is prepended to `userPlaylists`, and the `user-playlists` cache is invalidated (`hooks/usePlaylists.ts:25-47`).
  - The create modal then calls `addTracksToPlaylist` if it was given tracks, and records the playlist as recent (`AddToPlaylistMenu.tsx:77-96`). Descriptions are capped at 500 characters (`:30`).
- **Edit:**
  - `update_playlist {playlistId, title, description, accessType}`.
  - Only the non-null fields of the response are merged into `userPlaylists` (because a 204 response returns no body).
  - Sets `updatedPlaylists[uuid] = {title, description}` (`hooks/usePlaylists.ts:49-88`).
- **Delete:**
  - Optimistically removes the playlist from `userPlaylists` and adds it to `deletedPlaylistIds`, then calls `delete_playlist {userId, playlistId}`.
  - On success it invalidates `playlist:<id>`, `playlist-page:<id>` and `user-playlists`. On failure it restores both (`:187-220`).
  - If the user is on that playlist's page, the view is replaced with home (`components/MediaContextMenu.tsx:340-357`).
  - Popstate skips history entries for deleted playlists (`AppInitializer.tsx:1436-1445`).
- **Add tracks:**
  - Optimistically adds N to the playlist's `numberOfTracks`, then calls `add_tracks_to_playlist {playlistId, trackIds}`.
  - On success: invalidates the playlist caches and `refreshUserPlaylists()`, which calls `get_playlist_folders` for the root and merges the result. It refreshes again after 3 s to pick up the cover the server generates (`:222-243`).
  - The single-track version is `add_track_to_playlist {playlistId, trackId}` (`:153-168`).
  - A 409 or "dupe" error shows "Track already in this playlist" (`AddToPlaylistMenu.tsx:472-497`).
  - After a successful add, `allPlaylistsAtom` is reset to `[]` so the next time the menu opens it fetches again (`:471`).
- **Remove a track:**
  - `remove_track_from_playlist {playlistId, index}`. The index is the **row index** in the playlist, not a track id.
  - It is optimistic on the count (−1) (`hooks/usePlaylists.ts:170-185`).
  - It is offered only in the user's own playlists (`components/TrackContextMenu.tsx:60`, `:155-160`).
- **Choosing a playlist:**
  - The "Add to playlist" menu loads every playlist with `get_all_playlists {userId, offset, limit:500, order:"DATE_UPDATED", orderDirection:"DESC"}`. It fetches the remaining pages in parallel and keeps them in `allPlaylistsAtom` (`AddToPlaylistMenu.tsx:395-422`).
  - It shows up to 8 recent playlists, stored in localStorage under `sone.recent-playlists.v1` (`:28-47`).
  - There is a title filter and a "Create new" button.

### 4.3 Folders (`hooks/useFolders.ts`, `components/MoveToFolderMenu.tsx`, `components/FolderContextMenu.tsx`)
- Folder ids are the TRN suffix: `trn:folder:<id>` (`api/tidal.ts:1023`). Playlists are addressed by TRN as `trn:playlist:<uuid>`.
- **Create:**
  - `create_playlist_folder {folderId: parentId ("root"), name, trns: "trn:playlist:<uuid>" or ""}`. A folder can be created **together with** the playlist that goes into it (`hooks/useFolders.ts:310-353`, `MoveToFolderMenu.tsx:68-90`).
  - Sets `allFoldersFetched = false`.
  - The new id is resolved from the response's `items[*].data.trn` where the name matches. If that fails, it refetches the root folders sorted by `DATE_UPDATED DESC` and takes the first folder with the same name.
  - The caller then optimistically adds a folder entry to the root with `totalNumberOfItems:1`, and hides the moved playlist from its source folder (`MoveToFolderMenu.tsx:590-621`).
- **Rename:** optimistically sets `renamedFolders[id] = newName`, then calls `rename_playlist_folder {folderTrn, name}` and invalidates the folder cache. On failure it restores the previous override (`useFolders.ts:355-378`).
- **Delete:** optimistically adds the id to `deletedFolderIds` and invalidates the cache, then calls `delete_playlist_folder {folderTrn}`. On failure it un-hides the folder (`:380-396`). If the user is viewing the deleted folder, the view is replaced with home (`FolderContextMenu.tsx:166-176`).
- **Move a playlist:**
  - The optimistic step:
    - Sets `movedPlaylists[uuid] = sourceFolderId`.
    - Adjusts the counts: −1 on the source folder, +1 on the target unless the target is root.
    - Appends a snapshot of the playlist to `addedToFolder[target]`.
  - Then calls `move_playlist_to_folder {folderId: target or "root", playlistTrn}`. On success, the target goes on the recent list (up to 8, localStorage `sone.recent-folders.v1`). Any failure rolls back everything (`useFolders.ts:398-489`, `:274-292`).
- **"New playlist in folder"** (folder context menu): creates the playlist, then calls `movePlaylistTo(target = folder)` (`FolderContextMenu.tsx:247-273`).
- The move menu lists every folder, fetched by paging the root with its cursor and sorted by name (`MoveToFolderMenu.tsx:207-245`). It only covers folders at the root level.
- **How the library list merges** (`components/LibraryViewAll.tsx:333-369`, the same pattern as `Sidebar.tsx:151-190`): from the server items, hide deleted folders, deleted playlists, and playlists moved away from this folder. Then prepend the entries from `addedToFolder[current]` that aren't already present, folders first. Folder names use `renamedFolders` when set.

---

## 5. Per-screen commands and events

Wrappers in `api/tidal.ts` cache results in a frontend LRU (150 MB) with TTLs: 2 min for search and suggestions, 2 h for playlists, favorites, mixes, lyrics and sections, 24 h for albums, artists and credits (`api/tidal.ts:41`, `:49-53`). A network error triggers a "network-error" event, at most once every 30 s (`:102-119`). syzygy needs to decide whether to keep this cache; the backend already has a disk cache.

| Screen | `invoke` commands (args) | Events listened to |
|---|---|---|
| **Login** (`components/Login.tsx`, `hooks/useAuth.ts`) | On mount: `get_saved_credentials`, `get_default_credentials`, `has_pkce_defaults` (`Login.tsx:147-167`). **Device code:** `start_device_auth {clientId, clientSecret}`, then the browser opens `verificationUriComplete ?? verificationUri`, then `poll_device_auth {deviceCode, clientId, clientSecret}` every `max(interval, 5)` s until it returns tokens (`Login.tsx:228-286`, `useAuth.ts:134-180`). **PKCE in the system browser** (uses the embedded creds): `start_pkce_browser_login` returns `{authorizeUrl, codeVerifier, clientUniqueKey}`; the browser opens the URL; **the user pastes the redirect URL**, and `code` is taken from its query string; then `complete_pkce_browser_login {code, codeVerifier, clientUniqueKey}` (`Login.tsx:322-374`). Advanced PKCE with custom creds: `start_pkce_auth {clientId}`, then `complete_pkce_auth {code, codeVerifier, clientUniqueKey, clientId, clientSecret}`. After tokens arrive: `get_session_user_id` if needed, then `get_playlist_folders` for the root. Out of scope: `start_pkce_login_window` (webview), `parse_token_data`, `import_session`. | `pkce-login-success`, `pkce-login-error`, `pkce-login-cancelled`. These only come from the embedded-webview PKCE, which is out of scope (`Login.tsx:191-213`) |
| **App bootstrap** | §3.1–3.3: `load_saved_auth`, `get_session_user_id`, `refresh_tidal_auth`, `get_*` settings, `get_user_profile {userId}`, `get_profile {userId}`, `get_all_favorite_ids {userId}`, `get_favorite_mix_ids`, `load_playback_queue`, `save_playback_queue {snapshotJson}`, `set_volume {level}`, `get_playback_position` | `track-advanced`, `track-finished`, `audio-error`, `audio-resampled`, `audio-bit-depth-changed`, `mpris:*` |
| **Home** (`components/Home.tsx`, `HomeSection.tsx`) | `get_home_page {feedType}` returns `{home:{sections, cursor, tabs}, isStale}`. If `isStale`, it shows the cached copy and then calls `refresh_home_page {feedType}` (stale-while-revalidate, `Home.tsx:189-243`). Infinite scroll: `get_home_page_more {cursor, feedType}` (`:316-340`); a cursor that failed is never retried. Refetches on window focus at most every 5 min, after `invalidateCache("home")` (`:284-303`). Feed tabs come from `home.tabs`. Card playback goes through `useMediaPlay` (§2.4) and `playFromSource` for track sections. | – |
| **Search** (`components/SearchView.tsx`, `SearchBar.tsx`) | `search_tidal {query, limit:50}` (`SearchView.tsx:156`). Typing calls `get_suggestions {query, limit:10}`, debounced 300 ms; failures return empty results (`SearchBar.tsx:100-122`, `api/tidal.ts:324-345`). The search history (10 entries) is kept in localStorage `sone.search-history` (`SearchBar.tsx:26-50`). | – |
| **Album** (`components/AlbumView.tsx`) | `get_album_page {albumId}` returns `{page:{tracks, …}}` (`:110`); a 404 shows a not-found state (`:117`). Playback uses `albumMode:true` with an album source (§2.4). Favoriting uses §4.1. | – |
| **Artist** (`components/ArtistPage.tsx`, `ArtistTracksPage.tsx`) | `get_artist_page {artistId}` (`ArtistPage.tsx:161`; the raw result is normalized at `api/tidal.ts:484-620`). Follow and unfollow use §4.1 (`:355-366`). Top tracks play with source `{type:"artist", allTracks: topTracks}` (`:291-345`). The "all tracks" view uses `get_artist_top_tracks_all {artistId, …}` with source type `artist-tracks` (`ArtistTracksPage.tsx`, `api/tidal.ts:650-684`). "View all" uses `get_artist_view_all`. The radio button navigates to the artist mix. | – |
| **Playlist** (`components/PlaylistView.tsx`) | `get_playlist_details {playlistId}` (`:117-147`; `publicPlaylist` maps to `accessType`, `api/tidal.ts:849-861`). Tracks are paged with `get_playlist_tracks_page {playlistId, offset, limit, order, orderDirection}`; the sort is stored per playlist in `trackSortPrefs` (`:287`, `:338`, `:378`). `fetchRemaining` keeps loading pages in the background after a play and appends each one with `appendToQueue` (`:331-367`, `:465-472`). Recommendations come from `get_playlist_recommendations {playlistId, offset, limit}` and play with source type `playlist-recs` (`:189`, `:476-501`). Editing uses §4.2. | – |
| **Favorites** (`components/FavoritesView.tsx`) | `get_favorite_tracks {userId, offset, limit:100, order (default "DATE"), orderDirection (default "DESC")}` (`:155-161`). Tracks play with source `{type:"favorites", id:"favorites", name:"Loved Tracks"}` (`:471-479`). The remaining pages load in the background and are appended with `appendToQueue` (`:488-506`). The videos tab (`get_favorite_videos`) is out of scope. | – |
| **Library / sidebar** (`components/Sidebar.tsx`, `LibraryViewAll.tsx`) | Playlists and folders: `get_playlist_folders {folderId ("root" or an id), includeOnly:"", offset, limit, order, orderDirection, cursor:""}`. Pagination uses the cursor, and the total is estimated as offset + count + 1 while a cursor exists (`LibraryViewAll.tsx:159-180`, `api/tidal.ts:975-993`). Albums: `get_favorite_albums {userId, offset, limit, order, orderDirection}`. Artists: `get_favorite_artists {…}`. Mixes: `get_favorite_mixes {offset, limit, order, orderDirection}` (`:181-208`). Filtering playlists loads the flat list with `get_all_flattened_playlists` (`:381-396`). Merging the optimistic overlays is covered in §4.3. Folder actions: §4.3. | – |
| **Player bar** (`components/PlayerBar.tsx`) | Controls: shuffle (`toggleShuffle`), previous, play/pause, next (explicit), repeat cycle (`:533-603`). Seeking goes through `useProgressScrub` and `seekTo`, which calls `seek_track {positionSecs}` and then `notifySeek` (`hooks/useProgressScrub.ts:102`, `usePlaybackActions.ts:681-688`). Volume calls `set_volume {level}`. The heart uses §4.1. The cover links to the album; the "Playing from" label navigates to the source when the source type is navigable (`:253-336`). Drawer toggles open lyrics or queue (`:631-651`). The quality badge opens the signal path, which is out of scope. | Indirect only: the state comes from atoms that the AppInitializer listeners write |
| **Now-playing drawer** (`components/NowPlayingDrawer.tsx`) | **Queue tab:** shows the last 10 history entries, the current track, and the manual + context queues as one list. The header reads "Next in queue" when the manual queue is non-empty; otherwise "Next up from <source>", which navigates if the source is navigable (`:380-460`). It supports drag-reordering, remove, play-from-queue and clear (§2.9). **Lyrics tab:** `get_track_lyrics {trackId}` returns `{lyrics, subtitles, isRightToLeft, lyricsProvider}`. If `subtitles` parses as LRC (`lib/lrc.ts:23-48`; multiple timestamps per line, sorted), the synced view highlights the active line using the interpolated position. Auto-scroll pauses after the user scrolls; a "Sync lyrics" button turns it back on. Otherwise the plain `lyrics` text shows; if there is neither, "No lyrics available" (`:960-1160`). Lines can't be clicked to seek. Suggested tab: `get_mix_items {mixId: TRACK_MIX}` (`:786-805`). Credits: `get_track_credits {trackId}`. Bio: `get_artist_bio {artistId}`. | – |
| **Settings** (`components/settings/PlaybackTab.tsx`, `QualityPicker.tsx`, `UserMenu.tsx`) | Quality: `set_max_quality {quality}`, one of `HI_RES_LOSSLESS`, `LOSSLESS` or `HIGH` (`QualityPicker.tsx:27`). `get_gapless_supported`, `set_gapless {enabled}`; the gapless toggle is disabled while gapless is unsupported or exclusive mode or bit-perfect is on (`PlaybackTab.tsx:39-44`, `:98`). `set_volume_normalization {enabled}`. Autoplay and explicit content are frontend-only (explicit also clears the queue, §2.9). Output settings (in the user menu): `set_exclusive_mode {enabled}`, `list_audio_devices`, `set_exclusive_device {device}`, bit-perfect via §2.12 (`UserMenu.tsx:132-312`). Logout: §3.4, `logout`. | – |

---

## 6. Candidate glossary for CONTEXT.md

| Term | Meaning |
|---|---|
| **Track** | A TIDAL audio item with an id, metadata and availability flags. |
| **Queue entry** | One occurrence of a track in a queue, identified by its qid, so the same track can appear twice. |
| **Qid** | A per-instance queue id (`q<N>`) that tells duplicate tracks apart in queues and in gapless matching. |
| **Playback source** | The container the current playback came from (album, playlist, mix, artist, favorites, search…). It drives "Playing from" and keeps the container's full track list. |
| **Context source** | The playback source stashed while manual-queue tracks from another source play. It is restored once the manual queue is empty. |
| **Context queue** | The tracks left from the playback source, in play order (possibly shuffled). |
| **Manual queue** | Tracks the user added with "Play next" or "Add to queue". It always plays before the context queue. |
| **Source tag** | A source label attached to a manual-queue entry. Playing that entry switches "Playing from" to it. |
| **Original queue** | The unshuffled order of the context queue, kept while shuffle is on and used when shuffle is turned off. |
| **History** | Tracks already played, oldest first, capped at 500. Previous walks back through it. |
| **Shuffle** | A global mode that randomizes the context queue (not the manual queue) and keeps the original order. |
| **Shuffle play** | A one-shot shuffled start from a detail page that doesn't turn the global shuffle mode on. |
| **Repeat mode** | Off, all or one. Repeat-all rebuilds the queue from the source's track list. Repeat-one replays the track on automatic advance only. |
| **Autoplay** | When the queues run out (and repeat is off), continue with the current track's track mix ("radio"). |
| **Explicit advance** | A user-initiated Next. It overrides repeat-one and resets the skip counter. |
| **Auto-advance** | The advance triggered by the end of a track (`track-finished`), as opposed to an explicit advance. |
| **Skip drain** | The loop that skips unplayable tracks during an advance. It stops after 3 in a row. |
| **Unavailable / unplayable** | Unavailable: the metadata says the track can't stream. Unplayable: the server refused it (404, 410, 451, or a terminal 401). Both are skipped. |
| **Transient failure** | A network, rate-limit or proxy error. The track stays at the head of the queue and playback halts (or resumes after a rate limit). |
| **Gapless slot** | The single prerolled next track registered with the backend (`set_next_track`). |
| **Gapless advance** | The backend switched tracks on its own (`track-advanced`). The UI reconciles the queue and adopts the track. |
| **Album mode** | Playing an album in order, which uses album replay gain instead of track gain. |
| **Playback snapshot** | The persisted queue state (current track, queues, history, sources) restored at startup. |
| **Bit-perfect** | Unaltered output through an exclusive device. Volume is locked at unity and normalization is off. |
| **Exclusive mode** | Direct ALSA output to a chosen device, bypassing the OS mixer. |
| **Max quality** | The highest stream quality requested: HI_RES_LOSSLESS, LOSSLESS or HIGH. |
| **Favorites / Loved tracks** | The user's liked tracks, albums, playlists and mixes, plus followed artists. |
| **Library** | The user's playlists and folders, favorite albums, followed artists and favorite mixes. |
| **Folder** | A TIDAL playlist folder (`trn:folder:<id>`). Folders contain playlists (and folders), under the root. |
| **TRN** | TIDAL resource name, used to address folders and playlists in folder commands. |
| **Optimistic overlay** | Client-side changes (hidden, moved, renamed, added, count delta) layered on server lists until the next refetch. |
| **Track mix / radio** | A TIDAL mix generated from one track (`mixes.TRACK_MIX`). Used by autoplay and the Suggested tab. |
| **Synced lyrics** | LRC-timed lyrics (`subtitles`) that highlight in time with playback. Plain lyrics are the fallback. |

---

## 7. Notes for the port

1. Several things syzygy needs to decide on purpose:
   - `useMediaPlay` ignores shuffle mode.
   - `playFromSource` wraps around, then pages are appended after the wrap.
   - Autoplay doesn't update "Playing from".
   - Drag-reordering doesn't update `originalQueue`.
   - Logout leaves the manual queue, original queue and sources behind.
   - The autoplay early return leaves `isPlaying` unchanged.
   - Restore doesn't keep the position or the album gain.
2. The concurrency guards (generation counter, playNext lock, 250 ms re-entry guard, gapless gen/in-flight/coalescing, failure memo) exist because Jotai and async code interleave. In iced they become explicit state fields checked in `update` when each async `Task` result comes back.
3. Tests that document intended behavior and should become `update`-level tests:
   - `hooks/usePlaybackActions.test.tsx`: shuffle play, repeat-all history, blocked proxy, the repeat-one error message.
   - `lib/gaplessPredict.test.ts`.
   - `hooks/useGaplessPrefetch.test.ts`: the failure memo and proxy re-arm.
   - `lib/trackAvailability.test.ts`.
   - `lib/playbackPosition.test.ts`.
   - `lib/playbackSource.test.ts`.
