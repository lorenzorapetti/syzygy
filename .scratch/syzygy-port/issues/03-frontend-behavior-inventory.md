# Frontend behavior inventory

Type: research
Status: resolved

## Question

Which rules does sone's TypeScript frontend (`sone/src`) hold that syzygy must port into iced `update`? Cover: queue, manual queue, original queue, shuffle, repeat, autoplay, history, playback/context source, gapless prefetch, session restore (AppInitializer), and the flows for favorites, playlists and folders. For each prototype screen (home, search, album, artist, playlist, favorites, library, player bar, now-playing drawer with lyrics, settings, login), which Tauri commands it calls (`invoke`) and which events it listens to. Identify domain terms worth putting in CONTEXT.md.

## Answer

Playback state has two queues: a **manual queue** that always drains first, and a **context queue** that holds the rest of the **playback source**. Entries are told apart by a per-instance `_qid`. Shuffle saves an **original queue** and restores it when turned off. While manual entries from another source play, the original source is stashed as the **context source**.
`playNext` works in this order: repeat-one (on automatic advance only), then the manual queue (which can switch the source), then the context queue (skipping unplayable tracks and stopping after 3 failures), then repeat-all (rebuilt from `source.tracks`), then autoplay (track-mix radio). `playPrevious` puts the current track back at the head of the manual queue and restores the source from history. Gapless prefetch arms only a head from the same source, and `track-advanced` removes exactly one entry, matched by qid.
Session restore reads `queue.json` (7 atoms) and starts paused at 0:00. It doesn't keep the position or the album gain. Every library edit (favorites, playlists, folders) is optimistic with rollback, through overlay atoms.
The findings include the `invoke`/`listen` calls for each screen, a glossary of 30 terms, and a list of quirks for syzygy to decide on deliberately.

[findings](../research/frontend-behavior.md)
