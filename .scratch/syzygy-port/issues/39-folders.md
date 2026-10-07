# 39: Folders

**What to build:** the user creates, renames and deletes Folders (empty ones only, as in TIDAL's own app), moves playlists into a Folder or back to the root with recent Folders first, creates a Folder together with the playlist moved into it, and uses "New playlist in folder". See user stories 33, 62–64 and the spec's "Library editing" (Folders).

**Blocked by:** 36

**Status:** ready-for-agent

- [ ] Folders are addressed by TRN (`trn:folder:<id>`, `trn:playlist:<uuid>`) and are one level deep
- [ ] Folder menu (sidebar, Library view-all): New playlist in folder, Rename, Delete. Delete shows only for an empty Folder (its count with pending edits applied). Otherwise it's disabled with "Move or delete its playlists first"
- [ ] Move to folder ▸ on playlist cards and sidebar items: a popover of root-level Folders sorted by name, with up to 8 recent Folders (in `Settings`) first, plus root. Moves update counts optimistically
- [ ] Create a Folder with a playlist is one call. A placeholder Folder (temporary id, name, count 1) shows and the playlist is hidden where it was. The placeholder can't be opened or right-clicked. The next `Fresh` root read replaces it. No guessing the id by name
- [ ] "New playlist in folder" is create then move. If the move fails, the playlist stays at the root and a toast says so
- [ ] Deleting the Folder being viewed goes Home and prunes its entries from both stacks. Success invalidates `folders` and `user:{id}`
- [ ] Seam 3 tests: the Folder placeholder and its replacement, moves and counts, and empty-only deletion
