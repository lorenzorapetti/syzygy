# syzygy

A desktop TIDAL client: browse TIDAL's music, manage the user's collection, and play it with bit-perfect output.

## Language

### Content

**Catalog**:
Everything browsable from TIDAL: home, search results, albums, artists, playlists, mixes, and the user's Library.
_Avoid_: content, browse

**Library**:
The user's own collection on TIDAL: favorites (tracks, albums, artists, playlists, mixes), their playlists, and playlist folders. A part of the Catalog.
_Avoid_: collection, my music

**Favorite**:
A track, album, playlist or mix the user has liked, or an artist they follow. Favorites are part of the Library.
_Avoid_: saved, like (except as the UI verb)

**Own playlist**:
A playlist the user created. Only Own playlists can be edited, deleted, or have tracks added or removed. A playlist by someone else enters the Library only as a Favorite.
_Avoid_: user playlist

**Folder**:
A named group of playlists in the Library, either Own playlists or Favorites. Folders are one level deep: a Folder never holds another Folder. Only an empty Folder can be deleted, so deleting a Folder never deletes a playlist.
_Avoid_: playlist folder (except in code), directory

### Account

**Session**:
The user being signed in to TIDAL: who they are, their country, and the tokens that let syzygy act for them. It survives restarts until logout or Session expiry.
_Avoid_: auth, login state

**Login method**:
A way to start a Session. There are two: Browser login and Device-code login.

**Browser login**:
The default Login method. The user signs in on TIDAL's website in their own browser, then pastes the page they end on back into syzygy. Allows lossless playback.
_Avoid_: PKCE (except in code)

**Device-code login**:
The secondary Login method. syzygy shows a short code, and the user enters it on link.tidal.com from any device. Can't play lossless.
_Avoid_: login code

**Session expiry**:
TIDAL refusing to renew a Session, which sends the user back to log in. A network failure is not Session expiry.
_Avoid_: logged out (that's the user's own choice)

### Navigation

**Page**:
A place in the Catalog the user navigates to: an Album page, Artist page, Playlist page, and so on. Login and the fatal-error screen are not Pages, because you can't navigate to them.
_Avoid_: view, screen, route (except in code)

**Back stack**:
The Pages the user can go back and forward through. Settings is not in it.
_Avoid_: history (that's the tracks already played), navigation history

### Playback

**Playback source**:
The album, playlist, mix, artist, Loved tracks, search results, single track or Track radio that playback is working through. Every play has one, and it stays in place while the Manual queue plays.
_Avoid_: context, context source, container

**Queue entry**:
One occurrence of a track waiting to play, so the same track can be queued twice and each copy is still its own entry.
_Avoid_: qid (except in code)

**Manual queue**:
Queue entries the user added with "Play next" or "Add to queue". They play before anything left in the Playback source.
_Avoid_: user queue

**Source tag**:
The label for where a Manual queue entry comes from. "Playing from" shows it while that entry plays, and shows the Playback source the rest of the time.

**Shuffle**:
A lasting mode that plays the rest of the Playback source in random order. It never reorders the Manual queue.

**Shuffle play**:
Starting a Playback source in random order once, without turning Shuffle on.
_Avoid_: shuffle (for the one-off button)

**Repeat mode**:
Off, all or one. Repeat all starts the Playback source over when it runs out. Repeat one replays the current track when it ends, but Next still moves on.

**Autoplay**:
A setting: when nothing is left to play and Repeat mode is off, playback continues with the last track's Track radio.

**Track radio**:
TIDAL's mix generated from one track. When Autoplay starts one, it becomes the Playback source.
_Avoid_: track mix (except in code)

**History**:
Tracks already played, oldest first, capped at 500. Previous walks back through it.
_Avoid_: back stack (that's Pages)

**Album gain**:
Loudness levelled across a whole album rather than per track. It applies only while an album plays in album order.
_Avoid_: album mode

**Gapless advance**:
Moving to the next track with no silence, because it was prepared before the current one ended.

**Explicit content**:
Tracks TIDAL marks explicit. A setting decides whether they may play. Starting one while the setting is off asks the user to allow it. Explicit tracks reached any other way are skipped.
