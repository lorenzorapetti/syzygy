# 24: Background fills for long sources

**What to build:** playing a long playlist, a sorted playlist or Loved tracks starts immediately and keeps loading the rest in the background, even after the user navigates away. See user story 70 and the spec's "Playback model" (fills).

**Blocked by:** 23, 18, 19

**Status:** ready-for-agent

- [ ] The continuation is plain data (`SourceRef` + cursor/offset). A sorted playlist's sort is part of its `SourceRef`, so fills keep fetching in sorted order
- [ ] `Effect::StartFill { fill_id, continuation }`. The App holds the abort handle, so the fill outlives the Page. `PageArrived(fill_id, tracks)` with a stale id is dropped
- [ ] Arriving pages append to the end of the play order, or go into random positions in the unplayed tail when Shuffle is on
- [ ] A new source emits `CancelFill`. Repeat all uses what has loaded
- [ ] Seam 1 tests cover stale fill ids, Shuffle insertion and cancelling on a new source
