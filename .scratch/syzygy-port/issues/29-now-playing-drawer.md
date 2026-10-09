# 29: Now-playing drawer: Queue, Suggested and Credits

**What to build:** the user opens a drawer over the content with a large cover and tabs. In the Queue tab they see everything that played and will play, and can pick, reorder, remove and clear entries. Suggested shows the current track's Track radio, and Credits shows who made it. See user stories 31, 96–99, 102–103 and the spec's "Playback model" (drawer operations) and "Visual system".

**Blocked by:** 25

**Status:** ready-for-agent

- [x] The drawer slides up over everything above the player bar: 45% cover in an explicit square box (a 640px or larger image), 55% tabs, an 80% black backdrop. The maximized player is included
- [x] Navigating anywhere closes the drawer and the maximized player
- [x] The Queue tab lists recent History, the current track, the Manual queue ("Next in queue") and the rest of the source ("Next up from <source>")
- [x] Picking a source entry jumps the cursor, and skipped entries don't enter History. Picking a Manual entry plucks it and plays it. A History row plays under its own tag without changing what's upcoming
- [x] Drag reorders only within its section. Remove works on single entries. Clear empties the Manual queue and the rest of the play order and cancels the fill, keeping the source for Repeat all
- [x] Suggested and Credits are each a `Remote<_>` for the current track only, loaded while visible and reloaded when the track changes
- [x] Seam 1 tests cover the drawer operations

## Comments

Implemented. Notes for later tickets:

- **`playback`.** New messages `Pick(Entry)`, `Remove(Entry)`, `Move(Entry, usize)` and `Clear`. `Entry` is a Queue tab row: `Played(EntryId)`, `Queued(EntryId)` or `Upcoming(Slot)`. A `Slot` is (play, index into the source's tracks), so it stays valid while the play order around it changes. Accessors: `upcoming()` now yields `(Slot, &Track)`, `history()` yields `(EntryId, &Track)`, and there's a new `source()` for the Playback source.
- **Picks.** A source pick sets the cursor to the slot and steps, so skipped steps sit behind the cursor and never enter History. A History pick replays a copy under a new id with no step, so Previous treats it like a queued entry. Picking an unavailable row toasts "Track unavailable". Picking an explicit row while explicit content isn't allowed asks for consent, as other choices do.
- **Edits during a load.** Remove (matched by id) also applies to the rollback copy. Move and Clear apply only to the live state, because a drag index is relative to what's on screen. If the play fails, a Move is undone rather than misplaced. Clear keeps the source; Repeat all and Autoplay carry on from it.
- **Drawer** (`shell/drawer.rs`). Tabs: Queue, Suggested, Credits. Ticket 30 adds Lyrics as a fourth `Tab` with its own `TabRead`.
  - `App::track_changed` runs after every playback update and calls `Shell::playing`, which tells the drawer the current track. The visible tab reads its data when that track has none, and a result for another track is dropped.
  - Suggested uses `radio::fetch`, which `radio::read` now wraps. Credits use the new `Catalog::credits`, which isn't cached.
  - Clicking a Suggested row plays the Track radio from that row.
  - The Queue and Suggested lists are windowed on the drawer's scrollable (`drawer::SCROLL`), which scrolls back to the top on a tab switch.
  - Drag starts from a row's grip (a `mouse_area`), and `on_enter` tracks the target row while dragging. The global left-button release (`shell_events`) drops it.
  - The slide uses an `Animation`, and the App subscribes to frames while it runs.
- **Maximized player** (`shell/maximized.rs`). A whole-window cover, title and artists over the player bar's own `controls` (shared scrub state). Opened from the player bar or the drawer. Escape, Minimize or any navigation closes it.
- **Player bar.** Play-queue toggle (accented while open) and maximize, left of mute. Ticket 30's Lyrics toggle goes next to them.
- **Catalog.** `track::Credit { role, contributors }`, from `/tracks/{id}/credits`.
- **Icons.** Lucide 0.563: grip-vertical, list-music, maximize-2, minimize-2, sparkles.
- **Not done.** No artist bio on Credits (sone has one; not in the spec). Once fully open, the panel covers the whole area, so the 80% backdrop only shows while it slides, as in sone.
- **Tested:** 27 new Seam 1 tests and 1 catalog test, 483 in the workspace. **Not run** in the window: there was no way to drive the GUI from this session.
