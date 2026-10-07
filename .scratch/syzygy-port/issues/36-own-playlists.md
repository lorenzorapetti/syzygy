# 36: Own playlists: create, edit and delete

**What to build:** the user creates a playlist (title, a description of up to 500 characters, public or unlisted), edits an Own playlist, and deletes one after confirming. Deletion removes the playlist everywhere: if they're on its Page they go Home, its Pages leave back and forward, and if it's what's playing, the music carries on. See user stories 33, 56–58 and the spec's "Library editing".

**Blocked by:** 35, 18

**Status:** ready-for-agent

- [ ] A Shell `Dialog` overlay next to the settings and explicit-consent modals: one dialog open at a time, Esc closes it. Pages open dialogs by returning `Action::Library(library::Message)`
- [ ] The create/edit form defaults to UNLISTED and caps the description at 500 characters. A new playlist shows at once through a pending edit
- [ ] Edit and Delete are in the card and sidebar menus only for Own playlists. Delete asks for confirmation ("This can't be undone")
- [ ] Deleting the playlist being viewed replaces the Page with Home and prunes its entries from both stacks
- [ ] Deleting the Playback source sends playback `SourceDeleted(SourceRef)`: playback continues with what has loaded, answers with `CancelFill`, Repeat all uses what's loaded, and "Playing from" keeps the name but no longer links
- [ ] Success invalidates `user:{id}` and `playlist:{id}`
- [ ] Seam 3 test: `SourceDeleted` goes out when the Playback source is deleted. Seam 1 test: `SourceDeleted` cancels the fill and playback continues
