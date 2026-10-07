# The Playback source owns its tracks, and playback keeps a play order over them

sone keeps three lists of upcoming tracks that drift apart: `queue` (what's left, possibly shuffled), `originalQueue` (the same entries unshuffled, while shuffle is on) and `playbackSource.tracks` (the full list, used by repeat-all and by Previous when there's no history). Patching each place where they diverge (drag-reordering doesn't update `originalQueue`, and pages that load later can't go where they belong) would carry the bug class into syzygy. Instead, the Playback source owns its tracks in source order, and playback keeps a play order (indices into those tracks) plus a cursor. The upcoming tracks are the play order after the cursor.
- **Shuffle on:** shuffles that tail.
- **Shuffle off:** rebuilds the tail in source order, skipping what was already played or removed.
- **Repeat all:** rebuilds the play order.
- **Drag and remove:** edit the play order.
- **Pages that load later:** their indices are inserted into the play order.

The Manual queue stays a separate list of Queue entries, each with a Source tag. Because the Playback source stays put while Manual queue entries play, sone's stashed "context source" also goes away.

## Considered Options

- **Port sone's three lists literally and fix each quirk one by one.** Rejected: the quirks come from keeping duplicated state in sync, and every new queue operation would have to update all three lists correctly.

## Consequences

- **The snapshot format:** `queue.json` stores the source's tracks, the play order, the cursor and the unfinished fill, not a flat queue.
- **Drawer drags:** they can't cross between the Manual queue and the source's part of the queue, because a Manual queue entry has no place in the source's play order.
