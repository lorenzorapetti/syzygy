# 25: Manual queue and the track context menu

**What to build:** right-clicking any track row gives "Play next", "Add to queue", "Go to Track radio", "Go to album" and "Go to artist". Queued tracks play before the rest of the Playback source, the same track can be queued twice, and "Playing from" shows where a Manual queue entry came from while it plays. See user stories 72–74 and the spec's "Playback model" and "Library editing" (context menus).

**Blocked by:** 23

**Status:** ready-for-agent

- [x] The Manual queue is a separate list of Queue entries, each with its own id and a Source tag. Shuffle never reorders it
- [x] "Playing from" shows the entry's Source tag while it plays, and the Playback source otherwise
- [x] The Previous branch that isn't the previous play-order step pushes the current track to the front of the Manual queue with its tag, then plays the popped History entry under its own tag. History entries record their Source tag
- [x] `iced_aw::context_menu` on track rows (every list, search results) with Play next, Add to queue, Go to Track radio, Go to album and Go to artist. The menu is built so later tickets can add Library items
- [x] Seam 1 tests cover the Manual queue, Source tags and the Previous branch

## Comments

Implemented. Notes for later tickets:

- **`playback`.** `Listening` now has `manual: VecDeque<Item>`, and it's part of the rollback snapshot, so a failed play puts back a Queue entry it popped. Every `Item` has an `EntryId`. New messages: `PlayNext(Track, Source)` and `AddToQueue(Track, Source)`, where the `Source` is the Source tag. `advance` pops the Manual queue before the play order, Repeat all included, and Repeat one leaves it alone. `queued()` lists `(EntryId, &Track)`. `upcoming()` is now only what's left of the source after the Manual queue. A track queued while a play loads also goes into the rollback copy, so it survives that play failing.
- **Following sone.** A queued track's Source tag is its album (`page::queue_tag`), or the track itself when it has no album. Starting a new Playback source clears the Manual queue. Queueing with nothing current plays the track straight away (otherwise it would sit invisibly, with the player bar hidden). The Shell's toast still says "will play next" in that case.
- **Previous.** If the current track is the source's latest step, the cursor moves back as before. Otherwise (a Queue entry, or a track popped from History) the current track goes to the front of the Manual queue under its tag. This closes ticket 23's gap.
- **Catalog.** `Track::track_radio: Option<String>` comes from `mixes.TRACK_MIX`. Many list endpoints leave it out.
- **Menu.** `page/track_menu.rs` wraps every `track_list::marked` row (so every list, Search included) in `iced_aw::ContextMenu`. The menu is a `Vec` of sections of `Item { icon, label, link }`, and an item with no link is disabled (Go to album without an album, Go to artist without artists). Tickets 35/37/38 add their sections there. Drawer rows (ticket 29) get it if they're built with `track_list::marked`. The new `Link`s are `PlayNext`, `AddToQueue` and `TrackRadio`, and they map to `Action::Queue { track, next }` and `Action::TrackRadio`. "Go to Track radio" navigates straight away when the track knows its mix. Otherwise the Shell reads the track on its own (`radio_read`, aborted by any navigation) and toasts "Track radio unavailable" if there's still no mix. New Lucide icons: list-end, list-plus, radio, disc-3.
- **Tested:** 17 new Seam 1 tests and 2 catalog parsing tests. 380 in the workspace. **Not run** in the window for this ticket.
