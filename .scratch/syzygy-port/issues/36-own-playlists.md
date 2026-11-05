# 36: Own playlists: create, edit and delete

**What to build:** the user creates a playlist (title, a description of up to 500 characters, public or unlisted), edits an Own playlist, and deletes one after confirming. Deletion removes the playlist everywhere: if they're on its Page they go Home, its Pages leave back and forward, and if it's what's playing, the music carries on. See user stories 33, 56–58 and the spec's "Library editing".

**Blocked by:** 35, 18

**Status:** ready-for-agent

- [x] A Shell `Dialog` overlay next to the settings and explicit-consent modals: one dialog open at a time, Esc closes it. Pages open dialogs by returning `Action::Library(library::Message)`
- [x] The create/edit form defaults to UNLISTED and caps the description at 500 characters. A new playlist shows at once through a pending edit
- [x] Edit and Delete are in the card and sidebar menus only for Own playlists. Delete asks for confirmation ("This can't be undone")
- [x] Deleting the playlist being viewed replaces the Page with Home and prunes its entries from both stacks
- [x] Deleting the Playback source sends playback `SourceDeleted(SourceRef)`: playback continues with what has loaded, answers with `CancelFill`, Repeat all uses what's loaded, and "Playing from" keeps the name but no longer links
- [x] Success invalidates `user:{id}` and `playlist:{id}`
- [x] Seam 3 test: `SourceDeleted` goes out when the Playback source is deleted. Seam 1 test: `SourceDeleted` cancels the fill and playback continues

## Comments

Implemented. Notes for later tickets:

- **Catalog.** `Playlist` gains `public` (from `publicPlaylist`, `#[serde(default)]` so old cache entries read as unlisted) and `PartialEq`. `PlaylistFields { title, description, public }`: `new()` is unlisted, `of(playlist)`, `trimmed()` cuts the description to `DESCRIPTION_LIMIT` (500), `applied(playlist)`, `playlist(uuid, user_id)`. New `create_playlist` (invalidates `user:{id}`, returns the made `Playlist` as an Own one), `update_playlist` and `delete_playlist` (invalidate `user:{id}` and `playlist:{uuid}`). `playlist_tag(uuid)` is the one spelling of that tag.
- **Library.** `Edit` gains `Create(Item)`, `Change(Item)` (both an `Item::Playlist`, so `apply` can hand out references) and `Delete(Playlist)`. A new playlist is a placeholder with uuid `new:{n}` (`library::is_placeholder`): it can't be opened, right-clicked, edited or deleted. `Message::Created` swaps in TIDAL's playlist. `apply` now also hides deleted playlists, shows edited ones as edited, and puts new ones first at the top level of the playlists. `Library::playlist(&p)` is a playlist as last edited, for its Page and the edit form. The Refresh/settle tags for playlist edits are `folders` (plus `playlist:{uuid}` for edit and delete), not `user:{id}`, so a landed edit re-reads the playlists and Folders rather than every list. Edit and delete are dropped unless the playlist is Own and not a placeholder.
- **Dialogs.** `Message::Ask(Ask)` → `Effect::Ask` → the Shell's `dialog: Option<Dialog>` (`shell/dialog.rs`), drawn after the consent and settings modals. Esc or a click outside closes it, and so does navigating. `Ask` has `NewPlaylist`, `EditPlaylist`, `DeletePlaylist`; ticket 39's Folder dialogs belong there too. Pages use `Link::Ask`.
- **Deletion.** `Effect::Deleted(uuid)` goes out at once: the Shell prunes `Route::Playlist { uuid }` from both stacks (`BackStack::remove`) and opens Home if the Page shows it. `Effect::SourceDeleted(SourceRef)` goes out only once TIDAL has deleted it, for every deleted playlist, and playback decides whether it's its source (`SourceRef::same_place` ignores the sort). This is a choice: a cancelled fill can't come back if TIDAL refuses. Playback keeps the deleted sources (`Playback::is_deleted`), so `page::source_route` gives no link. That list is cleared on `Reset` but not saved: after a restart, "Playing from" a deleted playlist links again, to a not-found Page.
- **Entry points.** Own playlists' menus (sidebar rows, Library tiles, the Playlist Page's "…") have Edit playlist and Delete playlist (`menu::own_playlist`). New playlist is a "+" in the sidebar's Library header and a button on the Library Playlists tab. New icons: plus, pencil, trash-2.
- **Not done.** Home-feed and search cards for an Own playlist still get the plain card menu (Like, no Edit or Delete): a `Card` doesn't say who made the playlist. A dialog asked for while the consent or settings modal is open waits behind it.
- **Tested:** 14 Seam 3 tests (create, the placeholder and its swap, rollback, edit, Own-only, deletion with `Deleted` at once and `SourceDeleted` on success, per-target ordering), 6 Seam 1 tests (`SourceDeleted` cancels the fill and playback continues, sort-independent, other playlists ignored, Repeat all with what's loaded, "Playing from" stops linking, reset), 4 catalog tests. 616 in the workspace. **Run:** the app starts with the stored Session and loads Home and the sidebar. I didn't click through the dialogs, and I didn't create, edit or delete a playlist on the live account.
