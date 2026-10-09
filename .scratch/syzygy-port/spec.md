# Spec: Port sone to syzygy (pure-Rust iced)

Status: ready-for-agent

Map: [map.md](map.md). Decisions come from tickets [01](issues/01-iced-capabilities-survey.md)–[10](issues/10-library-editing.md), ADRs [0001](../../docs/adr/0001-strip-proxy-support.md)–[0004](../../docs/adr/0004-playback-source-with-play-order.md) and the glossary in `CONTEXT.md`. Use the glossary's terms throughout.

## Problem Statement

sone is a good TIDAL client with bit-perfect output, but its UI is a Tauri webview running a React/TypeScript frontend. The playback rules (queue, Manual queue, Shuffle, Repeat mode, Autoplay, History, session restore) live in TypeScript atoms that drift apart: three copies of the upcoming tracks, stashed context sources, locks and re-entry guards that work around async interleaving. The user wants a desktop TIDAL client that is native Rust end to end, so the UI and the playback logic sit in one language and one process, the state is explicit, and the playback rules can be tested directly. They also want it to look and behave like sone, so nothing they rely on in daily listening is lost.

## Solution

syzygy: a new, separate desktop app built with iced 0.14 that replaces sone's webview. It reuses sone's Rust engine code (the TIDAL client, the disk cache and crypto, the GStreamer/ALSA audio engine, MPRIS and the TIDAL play reporter), refactored into a workspace of `syzygy-*` crates, and ports sone's TypeScript behaviour into the iced `update` and state. It keeps sone's layout and dark theme, though not pixel for pixel.

From the user's side, syzygy lets them:
- sign in with Browser login or Device-code login, and stay signed in across restarts;
- browse the whole Catalog: Home, Search, Album, Artist, Playlist, Mix, Favorites, Library, Explore, Feed and Profile;
- edit their Library: favorites, playlists and folders;
- play music bit-perfect, with the queue, Shuffle, Repeat mode, Autoplay, gapless advance and a session that comes back paused where they left off;
- see what's playing in the player bar and in the now-playing drawer with synced lyrics;
- change quality and output settings;
- control playback from the desktop through MPRIS;
- have plays reported to TIDAL so they show up in "Recently Played".

## User Stories

### Identity and startup

1. As a user, I want syzygy to be its own app with its own name, app ID, config and cache directories, so that it never touches sone's data if I have both installed.
2. As a user, I want syzygy to open straight into the main window at Home when I'm already signed in, with no wait for the network, so that startup feels instant.
3. As a user, I want syzygy to refresh my account details in the background after startup, so that my country and user id stay right without blocking me.
4. As a user, I want to see a clear fatal error screen if syzygy can't reach the system keyring or its fallback key file, so that it never stores my data unencrypted or silently forgets it.
5. As a user, I want the app to credit sone and its author, so that its origin is clear and the GPL-3.0 licence is respected.

### Login and Session

6. As a new user, I want a "Sign in with browser" button as the main action, so that I get the Login method that allows lossless playback.
7. As a user doing Browser login, I want syzygy to open TIDAL's sign-in page in my own browser, so that I sign in where my passwords and 2FA already live.
8. As a user whose browser didn't open, I want a "Copy link" fallback, so that I can open the sign-in page myself.
9. As a user doing Browser login, I want to paste the page I ended on (either the whole URL or just the code) into a field, so that syzygy can finish signing me in even though nothing can listen for the redirect.
10. As a user, I want a "Paste from clipboard" button, so that I don't have to click into the field and paste by hand.
11. As a user whose pasted URL contains an error from TIDAL, I want to see TIDAL's error description and a "Start over" button, so that I know what went wrong and can retry.
12. As a user whose code exchange failed, I want the error shown inline and the field left ready for another paste, so that one mistake doesn't send me back to the beginning.
13. As a user, I want a secondary "Use a code instead (no lossless)" link, so that I can sign in from another device when the browser flow doesn't work for me, knowing the tradeoff.
14. As a user doing Device-code login, I want the code in large type with a Copy button, an "Open link.tidal.com" button and a QR code, so that I can enter it on whichever device is handy.
15. As a user doing Device-code login, I want syzygy to keep checking until I've entered the code, and a Cancel button that stops it, so that I'm signed in as soon as I'm done or can back out.
16. As a user whose device code expired, I want a "Get a new code" button, so that I can try again without restarting.
17. As a user, I want my Session kept encrypted on disk and restored on every launch, so that I sign in once.
18. As a user, I want syzygy to renew my tokens on its own when TIDAL rejects an old one, without several renewals racing each other, so that I never notice the renewal.
19. As a user on a flaky network, I want network failures and TIDAL server errors treated as ordinary errors rather than Session expiry, so that I'm not sent back to log in for no reason.
20. As a user whose Session expired, I want to land on the login screen with a "Your session expired, sign in again" banner, so that I understand why.
21. As a user whose Session expired, I want my queue, cache and preferences kept when I sign back in as the same user, so that I pick up where I was.
22. As a user who signs in as a different account after expiry, I want the old account's cache and queue cleared, so that I don't see someone else's music.
23. As a user, I want Logout to stop playback and delete my Session, disk cache, saved queue and pending play reports, so that nothing of my account stays on the machine.
24. As a user, I want my audio and quality preferences kept after Logout, so that I don't have to set up my output again.

### Shell, navigation and Back stack

25. As a user, I want a 280px sidebar with my Library (playlists, folders, albums, artists, mixes, with covers), so that my music is always one click away.
26. As a user, I want a header with back and forward buttons, search and my avatar, so that the main controls are always in the same place.
27. As a user, I want to go back and forward through the Pages I've visited, with header buttons, the mouse side buttons and Alt+←/→, so that navigation feels like a browser.
28. As a user, I want going back to put me at the same scroll position I left, so that I don't lose my place in long lists.
29. As a user, I want a Page to draw its header straight away from the card I clicked (title, cover, artist), so that navigation feels immediate while the rest loads.
30. As a user, I want tabs inside a Page (Home feed, Search, Library type, Artist view-all section) to be remembered when I go back, without each tab switch adding a step to the Back stack, so that Back takes me to the previous Page, not the previous tab.
31. As a user, I want navigating anywhere to close the now-playing drawer and the maximized player, so that I can see where I went.
32. As a user, I want Settings to open as a modal that is not part of the Back stack, so that Back never reopens it.
33. As a user who deleted a playlist or Folder, I want its Pages removed from back and forward, so that I never land on something that no longer exists.
34. As a user, I want a Page that doesn't exist to show a not-found message, and a Page that failed to load to show an inline error with Retry, so that I know what happened and can recover.
35. As a user, I want cached Pages to show straight away and update quietly when fresh data arrives, so that revisiting is instant but never stale for long.
36. As a user, I want a failed refresh to keep the cached content on screen, so that a network hiccup doesn't blank the Page.
37. As a user, I want content kept to a readable maximum width on very wide windows, so that titles and durations don't drift a screen apart.
38. As a user, I want covers to fade in once loaded, with a placeholder before, so that scrolling looks smooth.
39. As a user, I want playlists with thousands of tracks to scroll smoothly, so that big playlists aren't sluggish.

### Browsing the Catalog

40. As a user, I want Home with its feed tabs and sections, loading more as I scroll, so that I discover music the way TIDAL suggests it.
41. As a user, I want Home to refresh when I come back to the window after a while (at most every 5 minutes), so that recommendations stay current.
42. As a user, I want search suggestions in a dropdown as I type, debounced, without leaving the Page I'm on, so that I can search without losing my place.
43. As a user, I want submitting a search or picking a suggestion to open a Search Page with tabs, so that I can browse results by type.
44. As a user, I want my last 10 searches remembered, so that I can repeat them quickly.
45. As a user, I want an Album Page with its tracks, so that I can play or explore the album.
46. As a user, I want an Artist Page with top tracks, albums and sections, plus "all top tracks" and "view all" Pages, so that I can go deep on an artist.
47. As a user, I want a Playlist Page with its tracks, sortable and filterable, with my sort remembered per playlist, and the playlist's recommendations, so that I can play and extend it.
48. As a user, I want a Mix Page, so that I can play TIDAL's mixes.
49. As a user, I want a Favorites Page with my Loved tracks, so that I can play everything I've liked.
50. As a user, I want Library view-all Pages for playlists (with folders), albums, artists and mixes, so that I can see my whole Library.
51. As a user, I want Explore and its sub-pages, so that I can browse genres, moods and editorial content.
52. As a user, I want a Feed Page with new releases from artists I follow, marked seen when I open it, and an unseen badge in the sidebar, so that I notice new music.
53. As a user, I want a read-only Profile Page, and a Page listing that profile's playlists, so that I can see my own or another user's public profile.

### Library editing

54. As a user, I want to like and unlike tracks, albums, playlists and mixes, and follow and unfollow artists, from any track row, card, Page header or the player bar, with a heart I can't click until syzygy knows whether the item is already a Favorite, so that I can keep my Favorites up to date as I listen and never toggle one by accident.
55. As a user, I want Library edits to show immediately and roll back with an error if TIDAL refuses them, and quick repeated edits to the same item to apply in order, so that the app feels fast but never lies for long.
56. As a user, I want to create a playlist with a title, a description (up to 500 characters) and public/unlisted access, optionally with tracks already in it, and to keep the playlist with a clear message if adding the tracks fails, so that I can start a playlist from what I'm listening to.
57. As a user, I want to edit an Own playlist's title, description and access, so that I can fix it later.
58. As a user, I want to delete an Own playlist, be taken Home if I'm on its Page, and have the music keep playing if it's what I'm listening to, so that it's gone everywhere at once without cutting off a song.
59. As a user, I want "Add to playlist" from a track or a selection, with my recent playlists first, a title filter and "Create new", so that adding is quick even with many playlists.
60. As a user, I want to be told when a track is already in the playlist, and when I add a selection, to have the new tracks added and be told how many were already there, so that I don't add duplicates by accident.
61. As a user, I want to remove a track from my Own playlists (only mine), and have the right track removed however the list is sorted or filtered, so that I can tidy them up.
62. As a user, I want to create and rename Folders, and delete empty ones (a Folder with playlists in it can't be deleted, as in TIDAL's own app), so that I can organize my playlists.
63. As a user, I want to move a playlist into a Folder or back to the root, with recent Folders first, and to create a Folder and move a playlist into it in one step, so that organizing is fast.
64. As a user, I want "New playlist in folder" from a Folder's menu, and to keep the playlist at the root with a clear message if the move fails, so that I can create a playlist where it belongs.
65. As a user, I want right-click context menus on tracks, cards, playlists and Folders, with "Go to album" and "Go to artist" on tracks, so that every edit is reachable where I'm looking.

### Starting playback

66. As a user, I want clicking a track in a list to play from that track to the end of the list, so that I hear what comes after it, not the list wrapped around.
67. As a user, I want every play to remember where it came from (album, playlist, mix, artist, Loved tracks, search results, a single track or a Track radio), so that "Playing from" is always right.
68. As a user, I want the play button on a card to respect Shuffle, so that cards and Pages behave the same.
69. As a user, I want a Shuffle play button that starts a source in random order once without turning Shuffle on, so that I can shuffle one album without changing my mode.
70. As a user, I want playback of a long playlist or Loved tracks to start immediately and keep loading the rest in the background even after I navigate away, so that I never wait for the whole list.
71. As a user, I want a track played on its own (a search top hit) to still have a Playback source, so that Repeat and "Playing from" work for it too.

### Queue, Shuffle, Repeat, Autoplay, History

72. As a user, I want "Play next" and "Add to queue" to put tracks in a Manual queue that plays before the rest of the Playback source, so that I can slip tracks in without losing my place.
73. As a user, I want the same track to be queueable twice, each copy its own Queue entry, so that duplicates behave independently.
74. As a user, I want "Playing from" to show where a Manual queue entry came from while it plays, and the Playback source the rest of the time, so that I always know why this track is playing.
75. As a user, I want Shuffle to randomize only what's left of the Playback source and never the Manual queue, so that my hand-picked order is kept.
76. As a user, I want turning Shuffle off to return the rest of the source to its original order, skipping what already played, so that I don't hear tracks twice.
77. As a user, I want Repeat all to start the Playback source over when it ends (reshuffled if Shuffle is on), so that I can loop an album or playlist.
78. As a user, I want Repeat one to replay the track when it ends but let Next move on, so that I can loop a track without getting stuck.
79. As a user, I want Autoplay, when on and Repeat is off, to continue with the last track's Track radio when nothing is left, and "Playing from" to show that radio, so that the music doesn't stop and I know what I'm hearing.
80. As a user, I want playback to stop cleanly when Autoplay finds no Track radio, so that the player never says it's playing when it isn't.
81. As a user, I want Previous to restart the track if I'm more than 3 seconds in, and otherwise go back through History, so that it works like every other player.
82. As a user, I want History capped at 500 tracks, so that it never grows without bound.
83. As a user, I want unplayable or unavailable tracks skipped automatically, with a message if 3 in a row fail, so that one bad track doesn't stop playback but a broken source doesn't spin forever.
84. As a user, I want album gain applied only when I play an album in album order, and track gain otherwise, so that loudness is right in both cases.
85. As a user, I want tracks to change with no silence when the next one is known (gapless advance), including Manual queue entries, Repeat one, Repeat all and an Autoplay radio, so that live albums and mixes flow.
86. As a user, I want a rate-limit from TIDAL to pause playback and resume on its own after the delay unless I did something in between, so that I don't have to babysit it.
87. As a user, I want a "device busy" message while syzygy retries a busy audio device, so that I know to close the other app.
88. As a user, I want rapid clicks on Next or on tracks to land on the last thing I chose, never on a stale one, so that the player keeps up with me.

### Explicit content

89. As a user, I want a setting that decides whether explicit tracks may play, so that I control what I hear.
90. As a user with explicit content off, I want starting an explicit track, a source containing one, or queueing one to ask me "Allow explicit content" or "Play without them", so that I decide in the moment.
91. As a user with explicit content off, I want explicit tracks reached any other way skipped silently and shown dimmed, without them counting as failures, so that playback goes on without nagging me.

### Now-playing drawer and player bar

92. As a user, I want a 90px player bar with cover, title, artist, heart, "Playing from", transport controls, a seek bar, volume and drawer toggles, so that playback is always under control.
93. As a user, I want clicking the cover or "Playing from" to navigate there, so that I can jump to what's playing.
94. As a user, I want seeking by dragging that only commits when I let go, so that scrubbing doesn't stutter the audio.
95. As a user, I want a mute toggle that restores my previous volume, so that I can silence quickly.
96. As a user, I want a now-playing drawer that slides up over the content with a large cover and Queue, Suggested, Lyrics and Credits tabs, so that I can dig into the current track.
97. As a user, I want the Queue tab to show recent History, the current track, the Manual queue ("Next in queue") and the rest of the source ("Next up from <source>"), so that I see everything that will play.
98. As a user, I want to pick any entry in the Queue tab to play it, drag entries within their section to reorder, remove entries and clear the queue, so that I can shape what comes next.
99. As a user, I want clearing the queue to keep the Playback source so Repeat all can start it over, so that clearing doesn't forget what I was listening to.
100. As a user, I want the Lyrics tab to show synced lyrics with the active line highlighted and auto-scrolling, pausing when I scroll and resuming with a "Sync lyrics" button, so that I can sing along or read freely.
101. As a user, I want plain lyrics when there are no synced ones, right-to-left lyrics laid out correctly, and "No lyrics available" otherwise, so that the tab is never confusing.
102. As a user, I want the Suggested tab to show the current track's Track radio, and the Credits tab its credits, so that I can find related music and who made it.
103. As a user, I want drawer tabs to load only for the current track and reload when it changes, so that they're always about what's playing.

### Session restore

104. As a user, I want my queue, Manual queue, History, current track and position restored on launch, paused, so that I resume exactly where I stopped without music blasting out.
105. As a user, I want a background load that didn't finish before I quit to resume after restore, so that long playlists are complete after a restart.
106. As a user, I want Shuffle, Repeat mode, Autoplay, volume and the explicit setting remembered, so that the player behaves the same every launch.

### Settings

107. As a user, I want to pick the maximum quality (Hi-Res lossless, Lossless, High), so that I control bandwidth and fidelity.
108. As a user, I want to turn on exclusive mode and pick an output device from a list, so that syzygy bypasses the system mixer.
109. As a user, I want a bit-perfect switch that locks volume at 100% and turns off normalization, ramping smoothly rather than jumping, and that turns exclusive mode on with it (and off when exclusive mode is turned off), so that I get untouched output without blowing my ears.
110. As a user, I want to turn gapless playback and volume normalization on or off, with gapless disabled when it isn't supported or exclusive mode or bit-perfect is on, so that I can't pick a broken combination.
111. As a user, I want Autoplay and the explicit content setting in Settings, so that every playback preference is in one place.
112. As a user, I want to turn play reporting to TIDAL on or off, so that I choose whether my plays show in "Recently Played".
113. As a user, I want Logout in Settings, so that I can sign out.
114. As a user, I want info messages when the audio engine resamples or changes bit depth, so that I know when output isn't bit-perfect.

### MPRIS and desktop integration

115. As a desktop user, I want syzygy to appear in my desktop's media controls with the current track, cover, status, position, Shuffle, loop status and volume, so that I can see what's playing from anywhere.
116. As a desktop user, I want media keys and the desktop's controls (play, pause, stop, next, previous, seek, set position, volume, Shuffle, loop) to act exactly like the in-app buttons, so that both stay in sync.
117. As a desktop user, I want "Raise" to focus the window and "Quit" to quit cleanly, so that the desktop can manage syzygy.
118. As a user, I want quitting to stop playback, flush pending play reports (within about 2 s) and save my settings and queue, so that nothing is lost on exit.

### Reporting

119. As a TIDAL user, I want my plays reported to TIDAL, so that they show up in "Recently Played" and inform my recommendations.
120. As a TIDAL user, I want tracks from an Autoplay radio reported as not chosen by me, so that TIDAL's stats stay honest.
121. As a user, I want unsent reports kept across restarts, so that plays made offline still count.

### Errors and diagnostics

122. As a user, I want transient errors (failed loads, failed edits, audio errors) shown as short toasts, and Page-level failures inline, so that I'm informed without being interrupted.
123. As a user, I want syzygy to write logs I can attach to a bug report, so that problems can be diagnosed.

### Developer

124. As a developer, I want the playback rules in a pure `update` that returns plain-data effects, so that I can test what plays next without GStreamer, D-Bus or TIDAL.
125. As a developer, I want one crate per backend subsystem with its own error type and no Tauri, so that each part is understandable and testable on its own.
126. As a developer, I want sone's existing Rust tests to come along with their modules, so that the port doesn't lose coverage.

## Implementation Decisions

### Workspace and identity

- A virtual workspace with every crate under `crates/`: `syzygy-tidal`, `syzygy-store`, `syzygy-audio`, `syzygy-mpris`, `syzygy-catalog`, `syzygy-report`, and the `syzygy` binary. Versions live in `[workspace.dependencies]`, and dependencies are bumped to current stable as they're ported (reqwest 0.12+). The existing `sone-iced` package goes away.
- `sone/` (upstream at `0488f97`) is a gitignored, read-only reference. Only the modules syzygy needs are copied. This is not a fork and has no upstream sync.
- GPL-3.0, crediting sone (lullabyX).
- Engine crates hardcode no app identity. The binary owns the identity constants (app name `syzygy`, app ID `com.lorenzorapetti.syzygy`, config and cache dirs under `syzygy`, MPRIS bus name `org.mpris.MediaPlayer2.syzygy` and identity `Syzygy`, keyring service `syzygy` / `master-key`) and passes them in.
- Linux on Wayland only: iced is built without its `x11` feature, so there's no X11 or XWayland backend. The Wayland `application_id` (`com.lorenzorapetti.syzygy`) is what ties the window to its desktop file and icon.
- iced 0.14.0 with features `tokio, image, svg, lazy, advanced`. iced owns the tokio runtime, so no `#[tokio::main]`. Context menus come from `iced_aw` 0.14.1.

### Crate responsibilities

| Crate | Responsibility | Depends on |
|---|---|---|
| `syzygy-tidal` | `TidalClient` split into client/auth/models, rate gate, embedded config; auth helpers (credentials, PKCE params, the fixed redirect URI, parsing `code` from pasted input); `resolve_stream(track_id, max_quality) -> PlayableStream` with the quality cascade; emits `TokensRefreshed` and `SessionExpired` | none |
| `syzygy-store` | crypto, `DiskCache` (generic tiers and tags), encrypted JSON file I/O with `SYZY` magic | none |
| `syzygy-audio` | `AudioPlayer` (GStreamer and DirectAlsa) minus proxy, `SignalPathTracker` without emits, `pipeline_probe` parsers, `compute_norm_gain`, `PositionCell`; a typed `Event` enum | none |
| `syzygy-mpris` | MPRIS server; position comes from a closure; Raise and Quit become events; a typed `Event` enum | none |
| `syzygy-catalog` | the binary's only way to reach the Catalog: stale-while-revalidate policy, cache keys, invalidation tags per mutation, browse reads, search and suggestions, lyrics, credits, Library reads and edits, image bytes | tidal, store |
| `syzygy-report` | TIDAL play reporting with a persisted queue and its own flush loop | tidal, store |
| `syzygy` (bin) | the iced app, `Settings`, logging, `GST_PLUGIN_PATH` selection, the image cache | all |

- No core crate. Each crate has its own error type; `Serialize` on errors and the Scrobble, MCP and ProxyBlocked variants are dropped.
- Proxy is stripped (ADR 0001): `TidalClient` takes a plain `reqwest::Client`, and `AudioProxy` and route plumbing leave audio.
- sone's `AppState`, `commands/*` and `lib.rs` listeners are not ported. The out-of-scope service calls (Discord, scrobble, MCP, idle-inhibit) disappear with that layer.

### Bridge between engines and iced

- `Services` is a struct of cheap `Clone` handles (player, `PositionCell`, MPRIS handle, catalog, reporter, TIDAL client, store), built in `boot` and stored in `App`. No globals.
- `TidalClient` is `Clone` with `&self` methods. Tokens sit behind an internal `RwLock`, and refresh is single-flight, so concurrent 401s refresh once.
- The audio API is async (replies over `oneshot`) and runs as `Task::perform`. Nothing blocking runs on the UI thread.
- Engine events (ADR 0003): `boot` creates each tokio unbounded channel and engine, and keeps the receiver in an `EventSource<T>`. `subscription()` always returns one `Subscription::run_with` per engine, mapped to `Message::Audio`, `Message::Mpris` and `Message::Tidal`. Engine subscriptions are never conditional on app state.
- Catalog reads return a stream of `Cached(v)` then `Fresh(v)` (or one value), run with `Task::run`. Library mutations are plain async fns that invalidate tags.
- `update` is the only writer of `Settings` and of the Session file, and it saves through `Task`s that encrypt and write on `spawn_blocking`.
- Position: `PositionCell` is lock-free and read without a round-trip. A 250 ms `time::every` runs only while playing, and MPRIS's position closure reads the same cell. Track end comes only from `audio::Event::TrackFinished`.
- Messages carry `Result<T, Arc<CrateError>>`.
- Shutdown: iced's exit-on-close is off. Close requests and `mpris::Event::Quit` map to `Message::Quit`, which batches stop, report flush (~2 s timeout), settings save and snapshot save, then chains `iced::exit()`. MPRIS `Raise` maps to `window::gain_focus`.

### Auth and Session

- Browser login is the default; Device-code login is secondary and labelled "no lossless". Only the embedded client ID pairs are used; `AuthMethod` records which pair the tokens belong to.
- Browser login: the redirect URI is fixed (`https://tidal.com/android/login/auth`), so the user pastes back the URL or code. Parsing: trim; take `code` from the query string if present, else use the whole input; `error=` shows `error_description` and "Start over", which generates a new verifier. The `open` crate launches the browser.
- Device-code login polls at the server's interval as an abortable `Task`; Cancel aborts; expiry shows "Get a new code". The screen shows the code, a Copy button, an "Open link.tidal.com" button and an iced `qr_code`.
- The Session (tokens, `AuthMethod`, `user_id`, `country_code`) lives in its own encrypted `session.json`. `Settings` holds only preferences. A keyring failure falls back to a 0600 key file; if both fail, `boot` enters `Phase::Fatal`.
- Tokens refresh only after a 401. Only `invalid_grant` from the token endpoint is Session expiry. On expiry: stop playback, clear tokens, keep `user_id`, `country_code`, queue, cache and preferences, show login with a banner. A different `user_id` signing in clears cache and queue.
- Logout: stop playback, delete `session.json`, the disk cache, `queue.json` and the report queue; keep preferences.

### App state, messages and navigation

- `App { services, settings, playback, images, phase }` with `enum Phase { Fatal(Error), Login(login::State), Shell(Shell) }`. Playback, settings and images outlive the Shell.
- `Shell` holds the Back stack, the current Page, overlays (sidebar, header search and suggestions, drawer, maximized player, settings modal, explicit-consent modal, the Library `Dialog`, toasts) and the global `Library` state.
- Each Page is a module with `State`, `Message`, `update(&mut self, msg) -> Action` and `view`, in `enum Page`. Pages never touch playback, `Services` or the `Library` state; they return `Action`s: `Navigate(Route)`, `Play(PlayRequest)`, `Library(library::Message)`, `FetchImages(urls)`, `Run(Task)`, `None`.
- The 15 Pages: Home, Search, Album, Artist, Artist tracks, Artist view-all, Playlist, Mix, Favorites, Library view-all, Explore, ExplorePage, Feed, Profile, ProfilePlaylists.
- Top-level message shape (from ticket 07):
  ```rust
  enum Message {
      Login(login::Message),
      Page(PageId, page::Message),
      Navigate(Route),
      Back, Forward,
      Shell(shell::Message),
      Library(library::Message),
      Playback(playback::Message),
      Images(images::Message),
      Audio(audio::Event), Mpris(mpris::Event), Tidal(tidal::Event),
      WindowFocused, CloseRequested, Quit,
  }
  ```
- Back stack: entries are `Route` + scroll offset, capped at 50. Navigating somewhere new clears forward. Routes are plain data; Album, Playlist, Artist and Mix routes carry an optional `Preview`. Tabs are part of the route and switching replaces the entry. Deleting a playlist or folder removes its entries from both stacks. The app opens at Home, and a new sign-in starts an empty stack at Home.
- Each navigation stamps a new `PageId`; messages for other ids are dropped. A Page holds `abort_on_drop` handles for its loads.
- Page data is `enum Remote<T> { Loading, Loaded(T), NotFound, Failed(Arc<Error>) }`. A `Fresh` failure after `Cached` keeps the cached data and only logs. No second in-memory Catalog cache in the binary.
- Search suggestions are debounced 300 ms in a shell-owned dropdown; submitting pushes `Search { query, tab }`. The 10-entry search history is in `Settings`.
- Feed calls mark-seen on open; the sidebar badge comes from one feed check after login. Profile is read-only.
- Drawer tabs (Queue, Suggested, Lyrics, Credits) are drawer state, each a `Remote<_>` for the current track only, loaded while visible.
- Images: an App-level LRU with a byte cap, URL → `Loading | Ready(Handle) | Failed`, bytes fetched through the catalog (disk-cached), about 6 at a time, deduplicated.

### Playback model

- ADR 0004: the Playback source owns its tracks in source order; playback keeps a play order (indices) and a cursor. The Manual queue is a separate list of Queue entries with Source tags. No `originalQueue`, no stashed context source.
- ADR 0002: `playback::update` is pure. It changes state and returns `Vec<Effect>`; a thin runner turns effects into `Task`s against `Services`. Never call engines from `update`.
- Effects: `Play`, `Pause`, `Resume`, `Stop`, `Seek`, `ArmNext`/`ClearNext`, `SetVolume`/`RampVolume`, `StartFill`/`CancelFill`, `FetchTrackRadio`, `ResumeAfter`, `Mpris(diff)`, `Report…`, `SaveSnapshot`.
- Every start goes through `PlayRequest { source, first_page, continuation }`. No wrap-around: track k plays k…end. Card play respects Shuffle. Shuffle play shuffles once.
- Shuffle on shuffles the unplayed tail; off rebuilds it in source order minus played/removed. Randomness is a seedable `SmallRng` in playback state, seeded from entropy in `boot` and fixed in tests.
- Repeat one replays on auto-advance only. Repeat all rebuilds the play order (reshuffled if Shuffle) and keeps History.
- Autoplay makes the Track radio the Playback source; no radio or a failed fetch stops playback.
- Previous: >3 s seeks to 0; otherwise pop History (move the cursor back if it's the previous play-order step, else push the current track to the front of the Manual queue with its tag and play the popped entry under its own tag); empty History seeks to 0. History entries record the entry and its Source tag; cap 500.
- Album gain is derived: Album source, album order, no Shuffle, not a Shuffle play. Not persisted.
- Explicit content: `playback::update` returns `Outcome::NeedsExplicitConsent(pending)`; the Shell shows the modal and on Allow sends `Settings(AllowExplicit)` then the pending message. Explicit entries reached during an advance are skipped silently and don't count toward the skip cap. Turning the setting off wipes nothing.
- Drawer operations: picking a source entry jumps the cursor (skipped entries don't enter History); picking a Manual entry plucks and plays it; a History row plays it under its tag without changing what's upcoming; drag reorders only within its section; Clear empties the Manual queue and the rest of the play order and cancels the fill, keeping the source.
- Fills: the continuation is plain data (`SourceRef` + cursor/offset) and is saved in the snapshot. `StartFill { fill_id, continuation }`; the App holds the abort handle; `PageArrived(fill_id, tracks)` with stale ids dropped. Pages append to the end of the play order, or at random positions in the unplayed tail with Shuffle on. A new source emits `CancelFill`. Repeat all uses what has loaded.
- Concurrency: each `Play` carries a `PlayToken` echoed by its result; stale results are dropped. The skip drain is a state machine stopping after 3 unplayable/unavailable in a row, with a toast. No locks or re-entry guards. The current track changes optimistically and rolls back on failure.
- Failures: device busy retries inside the runner's `Play` task, surfacing `AudioBusyRetrying`. A 429 puts the entry back at the head, pauses, and emits `ResumeAfter(token, delay)`.
- Status is `Stopped | Loading(token) | Playing | Paused`. Position lives in state from the tick; while Loading it shows the target position. Play while Stopped with a current track plays from the stored position.
- Gapless arms whatever comes next when state knows it (Manual queue head, next play-order entry, Repeat one, the first entry of the precomputed repeat-all rebuild, the Track radio fetched when the last track starts), never a track that will be skipped. `TrackAdvanced` is reconciled by entry id and applies the Source tag switch.
- MPRIS: after each `update` that touches playback, derive an `MprisView`, diff it against the last one sent, and emit `Effect::Mpris(diff)` only on change. MPRIS commands go through the same playback messages as the UI.
- Reporting: "chosen by user" is false only for tracks from an Autoplay-started Track radio. The reporter is reached only via `Report…` effects (non-blocking enqueues); report enablement is a `Settings` preference.
- Volume, mute and bit-perfect are playback state. `RampVolume { to, over }` replaces sone's 12-step loop. Turning exclusive mode or bit-perfect on or off changes the other; enabling bit-perfect with no exclusive device picks the first listed device. Mute saves and restores the pre-mute volume (0.5 if none). The volume slider is locked while bit-perfect is on.
- Persistence: preferences (Shuffle, Repeat mode, Autoplay, volume, allow-explicit) live in `Settings`. `queue.json` (in `syzygy-store`, no fallback store) holds the source (tracks, play order, cursor, unfinished fill), Manual queue, History, current track and position; saved via `SaveSnapshot`, debounced ~2 s and on Quit. Restore comes back paused at the saved position and restarts an unfinished fill.

### Library editing

- **State.** The `Library` state lives in the Shell. It holds the Favorite id sets (tracks, albums, playlist uuids, artists, mixes), the sidebar's root playlists and Folders as last read, and one list of **pending edits** (`Vec<PendingEdit>`, each with an id: like/unlike, create/edit/delete playlist, add/remove tracks, create/rename/delete Folder, move). sone's eight overlay maps are not ported.
- **One merge.** `library::apply(server_items, folder, &pending)` produces what every list shows: the sidebar, Library view-all, Favorites, the add-to-playlist and move-to-folder pickers, and Favorite state for hearts. No list repeats the merge by hand.
- **Edit lifecycle.** `library::update` pushes the pending edit and emits the `syzygy-catalog` mutation as a `Task`. On failure the edit is dropped (that is the rollback) and a toast is shown. On success the mutation invalidates its tags (`user:{id}`, `playlist:{id}`, `folders`, `fav-*`), and the edit stays until a `Fresh` read of an affected tag that *started after* the success arrives; then it's dropped and the server is the truth.
- **Ordering.** Edits are serialized per target (the same track, playlist or Folder). A second edit on a target waits for the first, edits on different targets run concurrently, and a failure drops the edits queued behind it with one toast.
- **Favorite id sets** load when the Shell starts, as catalog reads (`Cached` then `Fresh`). A heart is disabled until its set has loaded.
- **Playlists.** Create, edit and delete apply to Own playlists only (`creator.id == user_id`). Create defaults to UNLISTED and caps descriptions at 500 characters. Adding one track uses `onDupes=FAIL`, and a 409 or "dupe" shows "Track already in this playlist". Adding a selection uses `onDupes=SKIP`, then re-reads the playlist and toasts "Added N (M already in playlist)" when counts differ; the optimistic count is not trusted. sone's second refresh after 3 s (for the generated cover) is kept.
- **Track removal.** Removal is by the index in the playlist's own order, which TIDAL's API requires, and is offered only in Own playlists. Sorting stays server-side as in sone (so a sorted Playback source fills through its `SourceRef`), and the filter runs on the loaded rows.
  - **Own order:** each loaded row knows its index, filtered or not, and a removal shifts later indices down by one.
  - **Sorted view:** a sorted response carries no own-order index, so removal first does a fresh (uncached) read of the playlist in own order and finds the row matching **(track id, dateAdded)**, which tells apart two copies of the same track. No match or more than one match refuses with a toast.
  - sone's removal by displayed row (wrong under a sort or filter) is not ported.
- **Folders** are one level deep and addressed by TRN (`trn:folder:<id>`, `trn:playlist:<uuid>`). The move-to-folder picker lists root-level Folders sorted by name; a nested Folder from the server can still be opened in Library view-all. A Folder can hold Own playlists and Favorites.
  - **Create with a playlist** is one atomic call (`create_playlist_folder` with `trns`). The pending edit shows a placeholder Folder (temporary id, the name, count 1) and hides the playlist where it was; the placeholder can't be opened or right-clicked. On success `folders` is invalidated and the next `Fresh` root read replaces it. sone's id-by-name guessing is not ported.
  - **Delete** is offered only for an empty Folder (its count with pending edits applied); otherwise the item is disabled with "Move or delete its playlists first", matching TIDAL's own app. A server refusal rolls back with a toast.
  - **"New playlist in folder"** is create then move, and **"Create with tracks"** is create then add, each step its own pending edit. If the second step fails, the playlist is kept (at the root, or empty) and a toast says what didn't happen. Nothing is undone.
- **Recents and sorts.** Recent playlists and recent Folders (8 each), the per-playlist track sort and the Library view-all sorts (all applied by the server) live in `Settings` and are cleared when a different `user_id` signs in, along with the cache and queue.
- **Add-to-playlist list.** Opening the menu runs a normal catalog read of every playlist (`DATE_UPDATED DESC`, paged in parallel, tagged `user:{id}`), rendered through `library::apply`. No list is kept in Library state, so nothing is invalidated by hand.
- **Deletion side effects.** Deleting the playlist or Folder the user is viewing replaces the Page with Home and prunes its entries from both stacks. Deleting the playlist that is the Playback source sends playback `SourceDeleted(SourceRef)`: playback continues with what has loaded, answers with `CancelFill`, Repeat all uses what's loaded, and "Playing from" keeps the name but no longer links. Folder deletion never touches playback, because only empty Folders can be deleted.
- **Dialogs.** The create/edit playlist form, rename Folder, and the delete confirmations are one Shell `Dialog` overlay next to the settings and explicit-consent modals: one open at a time, Esc closes. The add-to-playlist and move-to-folder pickers are popovers anchored to the menu item. Pages open them by returning `Action::Library(library::Message)`.
- **Context menus** use `iced_aw::context_menu`:

  | Surface | Items |
  |---|---|
  | Track row (any list, drawer rows, search results) | Play next · Add to queue · Like/Unlike · Add to playlist ▸ · Go to Track radio · Go to album · Go to artist · Remove from playlist (Own playlists only) |
  | Album, mix, playlist and artist cards; sidebar items | Play now · Play next · Add to queue · Like/Unlike (Follow/Unfollow for artists) · Add to playlist ▸ (not artists) · for an Own playlist also Edit · Move to folder ▸ · Delete |
  | Folder (sidebar, Library view-all) | New playlist in folder · Rename · Delete (empty only) |
  | Page header | the heart, and a "…" button opening that item's card menu |
  | Player bar | heart and Add to playlist; no context menu |

  sone's Share (copy link) is not ported.

### Lyrics

- The lyrics come from TIDAL's track lyrics endpoint through `syzygy-catalog`: plain lyrics, LRC `subtitles`, a right-to-left flag and the provider.
- `subtitles` is parsed as LRC (multiple timestamps per line, sorted). If parsing works, the synced view highlights the active line from the playback position in state and auto-scrolls; auto-scroll pauses on user scroll and resumes with "Sync lyrics". Otherwise plain text; otherwise "No lyrics available". Lines are not clickable. Right-to-left lyrics align right.

### Settings modal

- Contents: max quality (Hi-Res lossless, Lossless, High); exclusive mode with the output device list; bit-perfect; gapless (disabled while unsupported or while exclusive mode or bit-perfect is on); volume normalization; Autoplay; allow explicit content; report plays to TIDAL; Logout.
- Each control sends a message that changes `Settings` (and playback state where it applies) and returns the matching effects and a save `Task`.
- `Settings` is trimmed from sone's: Session data moved out, no migrations, no proxy, no theme.

### Visual system

- Layout A (faithful sone): 280px sidebar with covers, 64px header (back/forward, search, avatar), Page hero, 60px track rows with 40px covers, a 90px player bar split 30/40/30, and a now-playing drawer that slides up over everything above the player bar (45% cover in an explicit square box, 55% tabs, 80% black backdrop).
- Content is capped at a maximum width on wide windows.
- Styling: `const` tokens from sone's `deriveTheme("#A855F7", "#130F1A")` output; `Theme::custom` covers iced's defaults; one plain `fn(&Theme, Status) -> Style` per styled widget. No custom `Theme` type or `Catalog` impls.
- Fixed-height bars use `container(..).center_y(h)`, not `.height(h)`.
- Track lists are windowed: fixed row height, spacers, ±8 overscan, driven by `scrollable::on_scroll(Viewport)`. Lists request images only for rows near the viewport.
- Covers: a `sensor().on_show` placeholder, off-thread decode, `image::allocate`, an `Animation` fade-in, and an LRU keyed by cover id. Request 640px or larger covers for the drawer. `window::frames()` is subscribed only while something animates.
- Icons: Lucide 0.563 SVGs embedded with `include_str!`, one cached `svg::Handle` per icon, tinted via `svg::Style::color`, with filled icons swapped to `currentColor`.

### Errors, toasts and logging

- Toasts are Shell state: a short stack of info/error messages with auto-dismiss. They're used for failed Library edits, skip-drain stops, device busy, audio errors (cut to 80 characters), resampled/bit-depth info and rate-limit pauses.
- Page-level failures render inline through `Remote::Failed` with Retry. `Phase::Fatal` is only for unrecoverable startup failures (crypto/keyring).
- Logging uses sone's logging module, ported into the binary: logs go to a file under syzygy's cache or state dir, with the level set by an env var. Background refresh failures are logged, not toasted.

## Testing Decisions

- **A good test checks external behaviour only:** given a state and a message, what's the new observable state and which effects went out. Tests don't reach into private fields or helper functions, and don't use fakes or traits over engines (ADR 0002).
- **Seam 1: `playback::update`.** This is the main seam. Build a playback state with a fixed RNG seed, send `playback::Message`s and engine events (`TrackFinished`, `TrackAdvanced`, play results with tokens, `PageArrived`), and assert on the resulting state (current entry, upcoming entries, Manual queue, History, status, "Playing from") and the `Vec<Effect>`. Cover:
  - starting a source (no wrap, Shuffle respected, Shuffle play);
  - the Manual queue and Source tags;
  - Shuffle on and off;
  - Repeat one versus explicit Next, Repeat all rebuild;
  - Autoplay with and without a radio;
  - Previous in all its branches;
  - the skip drain and its cap;
  - explicit consent outcomes and silent skips;
  - fills, including stale ids, Shuffle insertion and cancel on a new source;
  - stale play tokens, 429 and `ResumeAfter`;
  - gapless arming choices, and `TrackAdvanced` reconciliation;
  - drawer operations;
  - MPRIS diffs only on change;
  - the "chosen by user" flag on reports;
  - bit-perfect and exclusive linkage, mute;
  - snapshot save and restore round-trip (restores paused at position, fill restarts).
  sone's TS tests (`usePlaybackActions.test.tsx`, `gaplessPredict.test.ts`, `useGaplessPrefetch.test.ts`, `trackAvailability.test.ts`, `playbackPosition.test.ts`, `playbackSource.test.ts`) are the prior art for which cases matter; port their intent, not their structure.
- **Seam 2: the engine crates' public APIs.** sone's Rust tests come along with their modules:
  - `syzygy-tidal`: TIDAL JSON parsing and the quality cascade. New tests for parsing pasted redirect input (full URL, bare code, `error=`).
  - `syzygy-audio`: the `pipeline_probe` parsers (24 tests).
  - `syzygy-catalog`: the home-cache encode/decode tests.
  The proxy and `source_guards` tests are dropped with proxy.
- **Seam 3: `library::update` and `library::apply`.** Build a `Library` state with server lists, send `library::Message`s and mutation results, and assert on what `apply` renders and which mutations went out. Cover: optimistic apply and rollback on failure; an edit kept after success until a `Fresh` read that started later; per-target ordering and dropping queued edits on failure; the Folder placeholder and its replacement; moves and count changes; empty-only Folder deletion; selection adds with skipped duplicates; own-order indices shifting after a removal, and resolving a sorted-view removal by (track id, dateAdded) with no or several matches refused; `SourceDeleted` going out when the Playback source is deleted.
- **Not tested:** views, Pages, the Shell, navigation and the effect runner. These stay thin and are checked by running the app.
- Prior art in this repo: none yet (fresh workspace). sone's Rust `#[cfg(test)]` modules are the style reference.

## Out of Scope

- Video playback, the miniplayer, animated polish (tilt covers, video covers).
- MCP server, overlay server, Discord, scrobbling, tray, global shortcuts.
- Proxy support and `webview_proxy_auth` (ADR 0001).
- Theme presets, custom `theme.json`, light theme, session import, the PKCE webview login window, custom client credentials.
- Update check, idle-inhibit, the signal-path panel, deep links (including MPRIS OpenUri), single-instance.
- Profile editing (name, handle, bio, links, picture). Profile is read-only.
- The artist bio drawer tab, and favorite videos.
- Disk cache stats and clear-cache controls, and the logging toggle in Settings.
- Tests for views, Pages and the Shell.

## Further Notes

- The Settings modal contents, lyrics, toasts and logging were accepted as written without grilling. Profile editing is out of scope.
- The "Library editing" section comes from [ticket 10](issues/10-library-editing.md).
- Implementation tickets: [11](issues/11-workspace-store-app-skeleton.md)–[39](issues/39-folders.md). The visual-system prototype on branch `prototype/visual-system` is the reference for widget and style code.
- TRN and pending edits are implementation terms and stay out of `CONTEXT.md`. It doesn't define Synced lyrics, Bit-perfect, Exclusive mode or Max quality yet. Add them through `domain-modeling` when the tickets that use them are grilled or built.
