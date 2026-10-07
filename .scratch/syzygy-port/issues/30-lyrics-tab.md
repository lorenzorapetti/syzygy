# 30: Lyrics tab

**What to build:** the drawer's Lyrics tab shows synced lyrics with the active line highlighted and scrolled into view, pauses the auto-scroll when the user scrolls, and resumes it with "Sync lyrics". Without synced lyrics it shows plain ones, and otherwise "No lyrics available". See user stories 100–101 and the spec's "Lyrics".

**Blocked by:** 29

**Status:** ready-for-agent

- [ ] Lyrics come from TIDAL's track lyrics endpoint through `syzygy-catalog`: plain lyrics, LRC `subtitles`, a right-to-left flag and the provider
- [ ] `subtitles` is parsed as LRC (multiple timestamps per line, sorted). If parsing works, the synced view highlights the active line from the playback position and auto-scrolls
- [ ] Auto-scroll pauses on user scroll and resumes with "Sync lyrics". Lines are not clickable
- [ ] Otherwise plain text, otherwise "No lyrics available". Right-to-left lyrics align right
