# 30: Lyrics tab

**What to build:** the drawer's Lyrics tab shows synced lyrics with the active line highlighted and scrolled into view, pauses the auto-scroll when the user scrolls, and resumes it with "Sync lyrics". Without synced lyrics it shows plain ones, and otherwise "No lyrics available". See user stories 100–101 and the spec's "Lyrics".

**Blocked by:** 29

**Status:** ready-for-agent

- [x] Lyrics come from TIDAL's track lyrics endpoint through `syzygy-catalog`: plain lyrics, LRC `subtitles`, a right-to-left flag and the provider
- [x] `subtitles` is parsed as LRC (multiple timestamps per line, sorted). If parsing works, the synced view highlights the active line from the playback position and auto-scrolls
- [x] Auto-scroll pauses on user scroll and resumes with "Sync lyrics". Lines are not clickable
- [x] Otherwise plain text, otherwise "No lyrics available". Right-to-left lyrics align right

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `syzygy_catalog::Lyrics { plain, synced, right_to_left, provider }` from `Catalog::lyrics` (`/tracks/{id}/lyrics`, not cached). `synced` is the parsed LRC: `Vec<Line { at, text }>`, sorted. A line with several timestamps appears once per timestamp. Tags like `[ar:…]` and untimed lines are skipped. `None` if no line is timed. `Lyrics::active(position)` is the last line to have started. TIDAL's 404 for a track without lyrics lands as `Remote::NotFound`, which shows "No lyrics available".
- **Drawer.** `Tab::Lyrics` sits between Suggested and Credits, with its own `TabRead`. `Drawer::playing` now takes the position too, so it's told after every playback update, ticks included.
- **Following.** `Drawer::follow`, which the Shell runs after every drawer update and every `playing` call, picks the line sung. When that line changes, the line's container gets the `SUNG` id. `drawer::measure` (a widget `Operation`) reads its bounds and the scrollable's, and the drawer scrolls to centre it. Before the first line it scrolls to the top. Plain lyrics go to their top once.
- **User scrolls.** iced has no hook for wheel events over a scrollable (a `mouse_area` captures them), so a user scroll is any `Scrolled` away from the offset the tab last scrolled itself to, once that scroll has landed. That stops following and shows a floating "Sync lyrics" button, which resumes it. `Scrolled` carries the scroll limit too. When the limit changes (a resize, new content), the line is measured again, and an offset cut short by the new limit isn't taken as the user's.
- **Player bar.** The Lyrics toggle (Lucide `mic-vocal`) sits left of the Play queue toggle and is accented while it shows.
- **Not done.** Positions come from the 250 ms tick, not interpolated as sone does. A track change while plain lyrics show doesn't scroll back to the top until the new lyrics arrive (then it does).
- **Tested:** 8 new catalog tests (LRC parsing, the active line), 491 in the workspace. The drawer is Shell code, which the spec leaves untested. **Not run** in the window.
