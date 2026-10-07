# 19: Library reading: sidebar, Library view-all and Favorites

**What to build:** the user sees their Library: the sidebar lists their playlists, Folders, albums, artists and mixes with covers, the Library view-all Pages show each type (with Folders, which open to their playlists), and the Favorites Page shows their Loved tracks. Read-only in this ticket. See user stories 25, 49–50.

**Blocked by:** 17

**Status:** ready-for-agent

- [ ] The sidebar's Library lists, with covers. The Library type is a tab in the route
- [ ] Library view-all Pages for playlists (root playlists and Folders), albums, artists and mixes, each with its sort saved in `Settings` and applied by the server
- [ ] Opening a Folder shows its playlists (a `folderId` in the route)
- [ ] The Favorites Page lists Loved tracks
- [ ] The Shell holds the sidebar's root playlists and Folders as last read, so the sidebar is always populated
