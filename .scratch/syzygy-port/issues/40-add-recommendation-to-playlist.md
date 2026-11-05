# 40: Add a recommendation to the playlist on screen

**What to build:** on an Own playlist's Page, each recommended track has a one-click button that adds it to that playlist, as sone's does. The track leaves the recommendations at once and goes back if the add fails. This is not the "Add to playlist ▸" picker from ticket 37: it always adds to the playlist being viewed. See user story 47 ("so that I can play and extend it"), the spec's "Library editing", and sone's `handleAddRecToPlaylist` (`sone/src/components/PlaylistView.tsx:236`, the `onAddToCurrentPlaylist` button in `TrackList.tsx:396`).

**Blocked by:** 37

**Status:** ready-for-agent

- [x] Recommendation rows on an Own playlist's Page get an "Add to this playlist" button (Lucide `list-plus`) at the end of the row, where other lists put the heart and "…". Someone else's playlist shows recommendations without it, because TIDAL only lets the user add to their own. sone shows it everywhere; this is a deliberate difference
- [x] Clicking it takes the track out of the recommendations straight away, and the next one moves up to keep ten on screen when there are more. It adds the track through ticket 37's single-track path, a pending edit with `onDupes=FAIL`
- [x] Success toasts `Added "<title>" to playlist`. The playlist's tracks show it through the normal pending-edit and `Fresh` read flow (`playlist:{uuid}` invalidated), not by appending it to the Page's list by hand
- [x] A failure puts the track back in the recommendations (unless it's already there) and toasts "Failed to add track". A duplicate (409 or "dupe") toasts "Track already in this playlist" and leaves it out of the recommendations
- [x] Seam 3 test: the add goes out as a pending edit for the playlist on screen, and is dropped on failure. The recommendations' optimistic remove and put-back are Page state and are checked by running the app

## Comments

Implemented. Notes for later tickets:

- **Library.** `Message::AddRecommendation(playlist, track)` is ticket 37's single-track add (`Mutation::AddTrack`, `onDupes=FAIL`, a pending edit on the playlist) with `Origin::Recommended`. `Adding`'s `new_playlist: bool` became `Origin { Picked, NewPlaylist, Recommended }`. A refused recommendation toasts "Failed to add track" and emits `Effect::NotAdded { uuid, track }`; a duplicate informs "Track already in this playlist" and doesn't. `drop_edit` now returns the queued edits it drops, so recommendations queued behind a refused add (or a refused sorted removal) get `NotAdded` too, with no toast of their own. A recommendation for a playlist that isn't editable gets `NotAdded` straight away.
- **Page.** On an Own playlist each recommendation row ends with a `list-plus` button (`track_list::with_add`, header `header_with_add`). Clicking it takes the track out of `Recommendations` (`take`; the ones after move up, and if the last of a later ten goes, the ten before show) and sends the add. `NotAdded` reaches the current Page through `Page::not_added`, which puts the track back last unless it's there. No tooltip ("Add to this playlist"): the app has none yet.
- **Not handled.** A `NotAdded` for a Page that's no longer current is dropped (its recommendations are read again on return). A duplicate still drops the non-recommendation adds queued behind it, as in ticket 37.
- **Tested:** 5 Seam 3 tests (the add for the playlist on screen and its success, refusal with put-back, duplicate kept out, queued recommendations put back, Own only), 3 `Recommendations` tests (take, step back, put back once). 684 in the workspace. **Run:** the app starts. I didn't add a recommendation on the live account.
