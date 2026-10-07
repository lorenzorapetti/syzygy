# Library editing interactions

Type: grilling
Status: resolved
Blocked by: 03, 07

## Question

How do Library edits work in syzygy: favorites (tracks, albums, playlists, mixes, followed artists), "Add to playlist", playlist create/edit/delete and track removal, and folder create/rename/delete/move?

The [spec](../spec.md) (section "Library editing") currently copies sone's flows from the [frontend behavior inventory](../research/frontend-behavior.md) §4 without them having been questioned. Confirm or change:
- **Ownership:** the shape of the `Library` state in the Shell. Which id sets and optimistic overlays it holds, and how lists (sidebar, Library view-all, Favorites) merge them with the server's pages.
- **Edits:** optimistic apply, rollback and toast on failure, and which `syzygy-catalog` tags each mutation invalidates.
- **Add to playlist:** the menu (load every playlist, recents, filter, "Create new"), the duplicate (409) handling, and when the loaded list is invalidated.
- **Recents:** where the recent-playlists and recent-folders lists live (the spec says `Settings`, 8 each).
- **Folders:** the folder picker covers only root-level folders. Keep that limitation or support nesting? Also: creating a folder together with the playlist moved into it, and "New playlist in folder".
- **Track removal:** removal by row index in the user's own playlists only. What happens when the playlist's sort order differs from the server's?
- **Deletion side effects:** deleting the playlist or folder being viewed (go Home, prune the Back stack), and its effect on a Playback source that is that playlist.
- **Context menus:** which surfaces get which menu items (`iced_aw::context_menu`).
- **Glossary:** Folder, TRN and Optimistic overlay are candidate `CONTEXT.md` terms.

Call `grilling` and `domain-modeling`. When resolved, update the spec's "Library editing" section to match.

## Answer

Resolved by grilling (2026-10-07). The spec's "Library editing" section, Seam 3 under Testing Decisions and user stories 47 and 54–65 now hold the full answer. In short:

- **Ownership:** the Shell's `Library` holds the Favorite id sets, the sidebar's root playlists and Folders, and one `Vec<PendingEdit>`. `library::apply(server_items, folder, &pending)` is the only merge, used by every list and picker. sone's eight overlay maps are not ported.
- **Edits:** a failure drops the pending edit (that's the rollback) and shows a toast. After a success the edit stays until a `Fresh` read of an affected tag that started later arrives. Edits are serialized per target, and a failure drops the edits queued behind it. Tags: `user:{id}`, `playlist:{id}`, `folders`, `fav-*`. `library::update` and `apply` become test Seam 3.
- **Favorite id sets** load when the Shell starts (`Cached` then `Fresh`), and hearts stay disabled until their set has loaded.
- **Add to playlist:** a normal catalog read of every playlist, not kept in Library state, rendered through `apply`. Adding one track: `onDupes=FAIL`, and a 409 shows "already in playlist". Adding a selection: `onDupes=SKIP`, then the playlist is re-read and a toast says "Added N (M already in playlist)".
- **Recents** (8 playlists, 8 Folders) and the per-playlist and Library sorts live in `Settings`, cleared on user switch.
- **Folders** are one level deep, and the pickers show only root-level Folders. A Folder can hold Own playlists and Favorites. Only an empty Folder can be deleted (checked in TIDAL's own app: it refuses to delete a non-empty folder). Create-with-playlist is one atomic call shown as a placeholder until the `Fresh` read, with no guessing the id by name. When a two-call edit (new playlist in a Folder, create with tracks) fails halfway, the playlist is kept and a toast explains, with no undo.
- **Track removal:** sone removes by the displayed row, which deletes the wrong track under a sort or filter. Sorting stays server-side, so a sorted Playback source can keep filling through its `SourceRef`. Client-side sorting was agreed at first, then dropped: it needs the whole playlist loaded before sorting or playing in sorted order, and its order wouldn't match TIDAL's. In own order, each row knows its index. In a sorted view, removal first does a fresh, uncached read in own order and matches the row on (track id, dateAdded). No match or several matches are refused with a toast. Removal is limited to Own playlists.
- **Deletion side effects:** a deleted Page goes Home, and its entries are pruned from both stacks. If the deleted playlist is the Playback source, playback gets `SourceDeleted`: it keeps playing what has loaded, cancels the fill, and "Playing from" no longer links.
- **Context menus:** the table in the spec. Track rows gain Go to album and Go to artist, and Share is dropped. Dialogs are a Shell `Dialog` overlay, pickers are popovers, and Pages open them through the new `Action::Library`.
- **Glossary:** Favorite, Own playlist and Folder added to `CONTEXT.md`. TRN and pending edits stay out of it as implementation terms.
