# 24: Background fills for long sources

**What to build:** playing a long playlist, a sorted playlist or Loved tracks starts immediately and keeps loading the rest in the background, even after the user navigates away. See user story 70 and the spec's "Playback model" (fills).

**Blocked by:** 23, 18, 19

**Status:** ready-for-agent

- [x] The continuation is plain data (`SourceRef` + cursor/offset). A sorted playlist's sort is part of its `SourceRef`, so fills keep fetching in sorted order
- [x] `Effect::StartFill { fill_id, continuation }`. The App holds the abort handle, so the fill outlives the Page. `PageArrived(fill_id, tracks)` with a stale id is dropped
- [x] Arriving pages append to the end of the play order, or go into random positions in the unplayed tail when Shuffle is on
- [x] A new source emits `CancelFill`. Repeat all uses what has loaded
- [x] Seam 1 tests cover stale fill ids, Shuffle insertion and cancelling on a new source

## Comments

Implemented. Notes for later tickets:

- **`playback`.** `PlayRequest` has `continuation: Option<Continuation>`, and `Continuation { source: SourceRef, offset }` is plain data. `SourceRef::Playlist { uuid, sort }` and `SourceRef::LovedTracks(sort)` now carry their sort, so a fill reads in the order the play started in. The fill lives on the source play (`Fill { id, continuation }`), and each page moves `offset` on. New effects: `StartFill { fill_id, continuation }`, which replaces any running fill, and `CancelFill`. New messages: `PageArrived(FillId, Vec<Track>)` and `FillEnded(FillId)`. Pages for any other id are dropped.
- **Where pages go.** They go on the end of the play order. If the order is shuffled (Shuffle on, or a Shuffle play) they go at random places in the upcoming tracks, but never ahead of the next one, which may be prepared for a gapless advance or put back there by Previous. Turning Shuffle off sorts them back into source order. Repeat all starts over with what has loaded, and a fill still running keeps adding to the new round.
- **Rollback.** Pages also reach the copy that a failed play would roll back to. If a new source fails to play, its fill is cancelled. The old source's fill, if it hadn't finished, starts again under a fresh id from where it got to. **Ticket 28:** restoring the snapshot should go through the same path: save `Continuation` (add `Serialize`), then emit `StartFill` for an unfinished one.
- **Runner.** `fill::pages` streams pages from the `Catalog` (`more_playlist_tracks`, `more_loved_tracks`, `more_artist_tracks`). The App holds `fill_task` (abort on drop) and sends `FillEnded` when the stream ends. A read that fails is logged and ends the fill, with no retry.
- **Who fills.** The Playlist, Favorites and Artist tracks Pages, through `page::with_rest(request, list.has_more())`, plus card plays of playlists and Loved tracks. Artist tracks wasn't asked for, but it pages the same way. The offset is the loaded row count, as the Pages use. Lists that drop rows they can't parse can drift from TIDAL's offset, as the Pages already do.
- **Side effect of the sort in `SourceRef`.** If the user re-sorts the playing playlist on its Page, the Page's Play button stops being Pause/Resume, because the Page now shows a different order and Play starts that one.
- **Not covered.** If the source runs out while a page is still loading, playback stops. Pages that arrive afterwards are added, but playback doesn't resume on its own.
- **Tested:** 16 new Seam 1 tests: starting a fill and its sorted continuation, no fill for a full source, pages appended in order, stale ids, shuffled insertion (Shuffle on, after a Shuffle play, never ahead of the next track across 20 seeds), Shuffle off re-sorting, cancelling on a new source (with or without a fill of its own), nothing left to cancel after `FillEnded`, Repeat all with what loaded, pages kept through a failed Next, and both rollback paths of a failed new source. That's 361 tests in the workspace. **Not run** in the window for this ticket.
