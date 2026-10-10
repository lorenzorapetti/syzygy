# 39: Folders

**What to build:** the user creates, renames and deletes Folders (empty ones only, as in TIDAL's own app), moves playlists into a Folder or back to the root with recent Folders first, creates a Folder together with the playlist moved into it, and uses "New playlist in folder". See user stories 33, 62–64 and the spec's "Library editing" (Folders).

**Blocked by:** 36

**Status:** ready-for-agent

- [x] Folders are addressed by TRN (`trn:folder:<id>`, `trn:playlist:<uuid>`) and are one level deep
- [x] Folder menu (sidebar, Library view-all): New playlist in folder, Rename, Delete. Delete shows only for an empty Folder (its count with pending edits applied). Otherwise it's disabled with "Move or delete its playlists first"
- [x] Move to folder ▸ on playlist cards and sidebar items: a popover of root-level Folders sorted by name, with up to 8 recent Folders (in `Settings`) first, plus root. Moves update counts optimistically
- [x] Create a Folder with a playlist is one call. A placeholder Folder (temporary id, name, count 1) shows and the playlist is hidden where it was. The placeholder can't be opened or right-clicked. The next `Fresh` root read replaces it. No guessing the id by name
- [x] "New playlist in folder" is create then move. If the move fails, the playlist stays at the root and a toast says so
- [x] Deleting the Folder being viewed goes Home and prunes its entries from both stacks. Success invalidates `folders` and `user:{id}`
- [x] Seam 3 tests: the Folder placeholder and its replacement, moves and counts, and empty-only deletion

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `Folder` is `Serialize`/`Deserialize`. New `root_folders(user_id)` reads every page of the top level by cursor (NAME ASC, up to 40 pages) and keeps the Folders. It's cached under `root-folders:{id}` and tagged `folders` and `user:{id}`. `create_folder` (with an optional playlist uuid sent as `trns`), `rename_folder`, `delete_folder` and `move_to_folder` (where `None` is `root`) each invalidate `folders` and `user:{id}`. TRNs are built in `library::{folder_trn, playlist_trn}`.
- **Library.** Edits now target `library::Target`, which is `Favorite(FavoriteId)` or `Folder(id)`.
  - New edits: `CreateFolder { folder, moving }`, `RenameFolder { folder, was }`, `DeleteFolder(Folder)` and `Move(Moving)`.
  - A Folder made with a playlist targets the playlist, so a move of that playlist waits for it.
  - The placeholder Folder's id is `new:{n}` (`is_placeholder_folder`). It can't be opened, right-clicked, renamed, deleted, filled, or moved into. It's dropped by the next Fresh `folders` read that started after it landed. Its id isn't guessed.
  - `apply` puts new Folders first at the top level of the playlists. A moved playlist (or one made into a new Folder) shows only in the shelf of the Folder it went to.
  - `Library::folder(&Folder)` gives a Folder's name and count with the pending renames and moves applied. Every Folder view uses it. `apply` still returns references, so it can't make new values.
  - `Library::deletable` is true only for a non-placeholder Folder whose count is known and 0. Pass it the Folder as read, not as shown, or the pending edits are counted twice.
  - `Edit::Create` carries `Then::Add(tracks)` or `Then::Move(folder)` for "New playlist in folder". A failed move toasts "Created "X", but couldn't move it to "F"".
  - New effects: `PickFolder`, `RecentFolder` (sent when the move is pushed, as `Recent` is for playlists) and `FolderDeleted`.
- **Shell.** `shell/mover.rs` is the "Move to folder" popover. It's placed like the picker and lists root Folders by name. Up to 8 recents (`Settings::recent_folders`, cleared by `forget_account`) come first. It also has "Your Library" when the playlist is in a Folder, and "New folder" (which makes a Folder with the playlist in it). The current Folder and placeholders are left out. `FolderDeleted` prunes `Route::Folder` from both stacks and goes Home if it's on screen (`Shell::gone`, shared with playlist deletion).
- **Dialogs.** `Ask` gains `NewPlaylistIn(Folder)`, `NewFolder(Option<Placed>)`, `RenameFolder(Folder)` and `DeleteFolder(Folder)`. The dialog has a Folder name form and a delete confirmation.
- **Entry points.**
  - Folder menus (sidebar rows and Library tiles, via `page::library::folder_menu`) have New playlist in folder, Rename folder and Delete folder. Delete folder is disabled with the hint "Move or delete its playlists first".
  - Own playlist menus in the sidebar and Library tiles have Move to folder ▸ between Edit and Delete.
  - The Library Playlists tab has a "New folder" button. A Folder's Page has "New playlist" and shows its name as renamed.
  - New icons: folder-input and folder-plus.
- **Not done.**
  - The Playlist Page's "…" has no Move to folder, because the Page doesn't know which Folder the playlist is in.
  - A deleted playlist doesn't lower its Folder's count until the Folders are read again, so Delete folder stays disabled until then.
  - A Folder Page's total line is TIDAL's.
  - A Fresh root read that started before a Folder was made but arrives after can show the placeholder next to TIDAL's Folder until the next read.
  - `mover.rs` repeats much of `picker.rs`'s popover shell. A shared one would be a later refactor.
- **Tested:**
  - 30 Seam 3 tests: the placeholder, its replacement and its rollback; moves and counts (into, out of, between, refused, landed, edited, serialized, and leaving a Folder through a new one); empty-only deletion with pending moves; rename; New playlist in folder and its failure; the picker's opening.
  - A mover test (recents, then by name), a settings test for recent Folders and a catalog TRN test.
  - 676 tests in the workspace.
  - **Run:** the app starts. I didn't create, move or delete Folders on the live account.
