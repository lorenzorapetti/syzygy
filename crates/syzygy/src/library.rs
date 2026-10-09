//! The user's Library as the Shell keeps it: the Favorite id sets behind
//! the hearts, the sidebar's root playlists and Folders as last read, and
//! the edits the user made that TIDAL's reads may not show yet.
//!
//! An edit shows straight away: it's pending, and every list renders its
//! server items through [`apply`], which lays the pending edits over them.
//! Its mutation runs in the background. If TIDAL refuses it, the edit is
//! dropped, which is the rollback, and a toast says so. If TIDAL takes
//! it, the reads it touches are stale and read again, and the edit stays
//! until a `Fresh` read of one of its tags that started after the success
//! arrives: from then on the server is the truth.
//!
//! Edits are serialized per target: a second edit of the same track waits
//! for the first, while edits of different targets run at once. A failure
//! drops the edits waiting behind it too.
//!
//! Own playlists are created, edited and deleted the same way. A new
//! playlist shows at once as a placeholder that can't be opened yet, and
//! becomes the playlist TIDAL made when the mutation comes back. A deleted
//! one is gone at once, and its Pages with it; playback hears of it once
//! TIDAL has deleted it, since a fill can't be taken back.
//!
//! [`Library::update`] is pure: it changes the state and returns
//! [`Effect`]s for the Shell to run.

#[cfg(test)]
mod tests;

use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target};
use syzygy_catalog::library::Item;
use syzygy_catalog::{
    FavoriteId, FavoriteIds, LibrarySort, Playlist, PlaylistFields, Read, Shelf, Track,
};

use crate::page::paged::List;
use crate::playback::SourceRef;
use crate::settings::Settings;

/// How a new playlist's uuid starts until TIDAL has made it.
const PLACEHOLDER: &str = "new:";

/// The tag the root playlists and the Folders are read under.
const FOLDERS: &str = "folders";

/// When a read started or an edit landed, on the Library's own clock. A
/// read stamped after an edit landed shows that edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp(u64);

/// One pending edit, as its mutation's result names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditId(u64);

/// A Favorite as the lists show it.
#[derive(Debug, Clone)]
pub enum Favorite {
    /// A Loved track, as the Favorites Page lists it.
    Track(Track),
    /// An album, artist, playlist or mix, as the Library lists it.
    Item(FavoriteId, Item),
}

impl Favorite {
    pub fn track(track: &Track) -> Self {
        Favorite::Track(track.clone())
    }

    /// A card's album, artist, playlist or mix. `None` for what can't be
    /// a Favorite, such as a video's card or the Loved tracks'.
    pub fn card(card: &Card) -> Option<Self> {
        let id = match &card.target {
            Target::Album(id) => FavoriteId::Album(*id),
            Target::Artist(id) => FavoriteId::Artist(*id),
            Target::Playlist(uuid) => FavoriteId::Playlist(uuid.clone()),
            Target::Mix(id) => FavoriteId::Mix(id.clone()),
            Target::Favorites | Target::Track(_) | Target::Video(_) | Target::None => {
                return None;
            }
        };
        Some(Favorite::Item(id, Item::Card(card.clone())))
    }

    pub fn playlist(playlist: &Playlist) -> Self {
        Favorite::Item(
            FavoriteId::Playlist(playlist.uuid.clone()),
            Item::Playlist(playlist.clone()),
        )
    }

    pub fn id(&self) -> FavoriteId {
        match self {
            Favorite::Track(track) => FavoriteId::Track(track.id),
            Favorite::Item(id, _) => id.clone(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Favorite::Track(track) => &track.title,
            Favorite::Item(_, Item::Playlist(playlist)) => &playlist.title,
            Favorite::Item(_, Item::Card(card)) => &card.title,
            Favorite::Item(_, Item::Folder(folder)) => &folder.name,
        }
    }
}

/// A change to the Library the user made.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Like or unlike; follow or unfollow an artist.
    Favorite(Favorite, bool),
    /// A new Own playlist, as an [`Item::Playlist`]: a placeholder until
    /// TIDAL has made it, then the one TIDAL made.
    Create(Item),
    /// An Own playlist with new fields, as an [`Item::Playlist`].
    Change(Item),
    /// An Own playlist deleted.
    Delete(Playlist),
}

impl Edit {
    /// What the edit changes. Edits of one target run one at a time.
    fn target(&self) -> FavoriteId {
        match self {
            Edit::Favorite(favorite, _) => favorite.id(),
            Edit::Create(item) | Edit::Change(item) => FavoriteId::Playlist(uuid(item).to_string()),
            Edit::Delete(playlist) => FavoriteId::Playlist(playlist.uuid.clone()),
        }
    }

    /// The reads it makes stale.
    fn tags(&self) -> Vec<String> {
        match self {
            Edit::Favorite(favorite, _) => vec![favorite.id().tag().to_string()],
            Edit::Create(_) => vec![FOLDERS.to_string()],
            Edit::Change(item) => playlist_tags(uuid(item)),
            Edit::Delete(playlist) => playlist_tags(&playlist.uuid),
        }
    }

    /// The playlist it shows in place of the listed one.
    fn shows(&self) -> Option<&Item> {
        match self {
            Edit::Create(item) | Edit::Change(item) => Some(item),
            Edit::Favorite(..) | Edit::Delete(_) => None,
        }
    }

    /// Whether it takes what it targets out of the lists.
    fn hides(&self) -> bool {
        matches!(self, Edit::Favorite(_, false) | Edit::Delete(_))
    }

    /// The toast when TIDAL refuses it.
    fn failure(&self) -> String {
        match self {
            Edit::Favorite(favorite, on) => {
                let name = short(favorite.name());
                match (favorite.id(), on) {
                    (FavoriteId::Artist(_), true) => format!("Couldn't follow {name}"),
                    (FavoriteId::Artist(_), false) => format!("Couldn't unfollow {name}"),
                    (_, true) => format!("Couldn't add \u{201c}{name}\u{201d} to your Favorites"),
                    (_, false) => {
                        format!("Couldn't remove \u{201c}{name}\u{201d} from your Favorites")
                    }
                }
            }
            Edit::Create(item) => format!("Couldn't create \u{201c}{}\u{201d}", title(item)),
            Edit::Change(item) => format!("Couldn't save \u{201c}{}\u{201d}", title(item)),
            Edit::Delete(playlist) => {
                format!("Couldn't delete \u{201c}{}\u{201d}", short(&playlist.title))
            }
        }
    }
}

/// The uuid of a playlist edit's item.
fn uuid(item: &Item) -> &str {
    match item {
        Item::Playlist(playlist) => &playlist.uuid,
        Item::Folder(_) | Item::Card(_) => "",
    }
}

fn title(item: &Item) -> String {
    match item {
        Item::Playlist(playlist) => short(&playlist.title),
        Item::Folder(folder) => short(&folder.name),
        Item::Card(card) => short(&card.title),
    }
}

/// The tags of the reads an edit of a playlist makes stale: the root
/// playlists and the Folders, and the playlist's own.
fn playlist_tags(uuid: &str) -> Vec<String> {
    vec![FOLDERS.to_string(), syzygy_catalog::playlist_tag(uuid)]
}

/// A new playlist TIDAL hasn't made yet. It can't be opened, edited or
/// deleted.
pub fn is_placeholder(playlist: &Playlist) -> bool {
    playlist.uuid.starts_with(PLACEHOLDER)
}

/// A dialog the user answers before an edit.
// The Folders' dialogs join these.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// The form for a new playlist.
    NewPlaylist,
    /// The form for an Own playlist's title, description and access.
    EditPlaylist(Playlist),
    /// Whether to delete an Own playlist.
    DeletePlaylist(Playlist),
}

#[derive(Debug)]
pub struct Pending {
    id: EditId,
    edit: Edit,
    state: Progress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Progress {
    /// Waiting for an edit of the same target to finish.
    Queued,
    /// Its mutation is running.
    Running,
    /// TIDAL took it at this time. Reads started later show it.
    Landed(Stamp),
}

/// What a mutation does at TIDAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    Favorite {
        user_id: u64,
        id: FavoriteId,
        on: bool,
    },
    /// Make a playlist. Its result comes back as [`Message::Created`].
    CreatePlaylist {
        user_id: u64,
        fields: PlaylistFields,
    },
    /// Set an Own playlist's title, description and access.
    UpdatePlaylist {
        user_id: u64,
        uuid: String,
        fields: PlaylistFields,
    },
    /// Delete an Own playlist.
    DeletePlaylist { user_id: u64, uuid: String },
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Like (`true`) or unlike a Favorite; follow or unfollow an artist.
    /// Dropped while its heart can't be clicked: until the Favorites have
    /// loaded, or when it's already so.
    Favorite(Favorite, bool),
    /// Open a dialog: for a new playlist, or to edit or delete an Own one.
    Ask(Ask),
    /// Make a playlist with these fields.
    CreatePlaylist(PlaylistFields),
    /// Set an Own playlist's fields.
    EditPlaylist(Playlist, PlaylistFields),
    /// Delete an Own playlist, as the user confirmed.
    DeletePlaylist(Playlist),
    /// An edit's mutation came back.
    Done(EditId, Result<(), Arc<syzygy_catalog::Error>>),
    /// A new playlist's mutation came back with the playlist TIDAL made.
    Created(EditId, Result<Playlist, Arc<syzygy_catalog::Error>>),
    /// A read of the Favorite ids, started at the stamp.
    FavoriteIds(Stamp, Read<FavoriteIds>),
    /// A read of these tags, started at the stamp, brought TIDAL's answer.
    Fresh(Stamp, Vec<String>),
}

/// What the Library asks the Shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Run an edit's mutation. Its result comes back as
    /// [`Message::Done`] with the edit's id.
    Mutate(EditId, Mutation),
    /// Read the Favorite ids, as of the stamp.
    ReadFavorites { user_id: u64, stamp: Stamp },
    /// An edit landed: what's read under these tags is stale, so lists on
    /// screen read it again.
    Refresh(Vec<String>),
    /// Tell the user an edit didn't happen.
    Toast(String),
    /// Open a dialog.
    Ask(Ask),
    /// The playlist with this uuid is gone: its Pages go too.
    Deleted(String),
    /// TIDAL deleted a playlist: playback stops reading it, should it be
    /// the Playback source.
    SourceDeleted(SourceRef),
}

pub struct Library {
    /// Who's signed in. Nothing is read or edited until TIDAL has said.
    user_id: Option<u64>,
    /// The Favorite id sets and the stamp they're as of. `None` until a
    /// read arrives, and hearts can't be clicked until then.
    favorites: Option<(Stamp, FavoriteIds)>,
    /// The sidebar's root playlists and Folders, as last read.
    pub root: Shelved,
    pending: Vec<Pending>,
    next_edit: u64,
    clock: u64,
}

/// One of the sidebar's lists: a Library type at the top level.
pub struct Shelved {
    /// The order it's wanted in.
    pub sort: LibrarySort,
    /// The order `items` was read in, once a read has arrived. `None`
    /// makes the next read replace the list, even one paged past its first
    /// page.
    pub read_in: Option<LibrarySort>,
    pub items: List<Item>,
    /// It has been asked for, so a new order reads it again.
    pub asked: bool,
}

impl Shelved {
    pub fn new(sort: LibrarySort) -> Self {
        Self {
            sort,
            read_in: None,
            items: List::new(),
            asked: false,
        }
    }
}

impl Library {
    /// The Library for `user_id`, with the read of its Favorites.
    pub fn new(user_id: Option<u64>, settings: &Settings) -> (Self, Vec<Effect>) {
        let mut library = Self {
            user_id: None,
            favorites: None,
            root: Shelved::new(settings.library_sort(syzygy_catalog::Kind::Playlists)),
            pending: Vec::new(),
            next_edit: 0,
            clock: 0,
        };
        let effects = user_id.map_or_else(Vec::new, |user_id| library.user_known(user_id));
        (library, effects)
    }

    /// TIDAL said who's signed in: read their Favorites.
    pub fn user_known(&mut self, user_id: u64) -> Vec<Effect> {
        if self.user_id.is_some() {
            return vec![];
        }
        self.user_id = Some(user_id);
        vec![self.read_favorites(user_id)]
    }

    /// A read is starting: the stamp to send back with what it brings.
    pub fn start_read(&mut self) -> Stamp {
        self.tick()
    }

    fn tick(&mut self) -> Stamp {
        self.clock += 1;
        Stamp(self.clock)
    }

    fn read_favorites(&mut self, user_id: u64) -> Effect {
        Effect::ReadFavorites {
            user_id,
            stamp: self.tick(),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Favorite(favorite, on) => {
                if self.favorite(&favorite.id()) != Some(!on) {
                    return vec![];
                }
                self.push(Edit::Favorite(favorite, on))
            }
            Message::Ask(ask) => {
                let allowed = match &ask {
                    Ask::NewPlaylist => self.user_id.is_some(),
                    Ask::EditPlaylist(playlist) | Ask::DeletePlaylist(playlist) => {
                        self.editable(playlist)
                    }
                };
                if allowed {
                    vec![Effect::Ask(ask)]
                } else {
                    vec![]
                }
            }
            Message::CreatePlaylist(fields) => {
                let Some(user_id) = self.user_id else {
                    return vec![];
                };
                let uuid = format!("{PLACEHOLDER}{}", self.next_edit);
                let placeholder = fields.playlist(uuid, user_id);
                self.push(Edit::Create(Item::Playlist(placeholder)))
            }
            Message::EditPlaylist(playlist, fields) => {
                if !self.editable(&playlist) {
                    return vec![];
                }
                let edited = fields.applied(self.playlist(&playlist));
                self.push(Edit::Change(Item::Playlist(edited)))
            }
            Message::DeletePlaylist(playlist) => {
                if !self.editable(&playlist) {
                    return vec![];
                }
                let uuid = playlist.uuid.clone();
                let mut effects = self.push(Edit::Delete(playlist));
                effects.push(Effect::Deleted(uuid));
                effects
            }
            Message::Done(id, Ok(())) => self.landed(id),
            Message::Done(id, Err(e)) | Message::Created(id, Err(e)) => self.failed(id, &e),
            Message::Created(id, Ok(made)) => {
                // From now on it's the playlist TIDAL made, which can be
                // opened.
                if let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) {
                    pending.edit = Edit::Create(Item::Playlist(made));
                }
                self.landed(id)
            }
            Message::FavoriteIds(stamp, read) => {
                // They settle no edit: a landed edit is in them already,
                // and the lists still need it until their own reads catch
                // up.
                let ids = match read {
                    Read::Cached(ids) | Read::Fresh(Ok(ids)) => ids,
                    Read::Fresh(Err(e)) => {
                        log::warn!("Could not read the Favorites: {e}");
                        return vec![];
                    }
                };
                // An older read than what's shown, which may lack an edit
                // already taken in.
                if self.favorites.as_ref().is_some_and(|(at, _)| *at > stamp) {
                    return vec![];
                }
                self.favorites = Some((stamp, ids));
                vec![]
            }
            Message::Fresh(stamp, tags) => {
                self.settle(stamp, &tags);
                vec![]
            }
        }
    }

    /// Whether the user can edit or delete `playlist`: one of their Own
    /// playlists that TIDAL has made.
    fn editable(&self, playlist: &Playlist) -> bool {
        playlist.is_own(self.user_id) && !is_placeholder(playlist)
    }

    /// Show the edit now, and run it unless one of its target is before it.
    fn push(&mut self, edit: Edit) -> Vec<Effect> {
        let id = EditId(self.next_edit);
        self.next_edit += 1;
        let target = edit.target();
        let waits = self.pending.iter().any(|pending| {
            pending.edit.target() == target && !matches!(pending.state, Progress::Landed(_))
        });
        self.pending.push(Pending {
            id,
            edit,
            state: Progress::Queued,
        });
        if waits { vec![] } else { self.run(id) }
    }

    /// Run a queued edit's mutation.
    fn run(&mut self, id: EditId) -> Vec<Effect> {
        let Some(user_id) = self.user_id else {
            return vec![];
        };
        let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
            return vec![];
        };
        pending.state = Progress::Running;
        let mutation = match &pending.edit {
            Edit::Favorite(favorite, on) => Mutation::Favorite {
                user_id,
                id: favorite.id(),
                on: *on,
            },
            Edit::Create(Item::Playlist(playlist)) => Mutation::CreatePlaylist {
                user_id,
                fields: PlaylistFields::of(playlist),
            },
            Edit::Change(Item::Playlist(playlist)) => Mutation::UpdatePlaylist {
                user_id,
                uuid: playlist.uuid.clone(),
                fields: PlaylistFields::of(playlist),
            },
            Edit::Create(_) | Edit::Change(_) => return vec![],
            Edit::Delete(playlist) => Mutation::DeletePlaylist {
                user_id,
                uuid: playlist.uuid.clone(),
            },
        };
        vec![Effect::Mutate(id, mutation)]
    }

    /// TIDAL took an edit: its reads are read again, and the next edit of
    /// its target runs.
    fn landed(&mut self, id: EditId) -> Vec<Effect> {
        let at = self.tick();
        let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
            return vec![];
        };
        pending.state = Progress::Landed(at);
        let (target, tags) = (pending.edit.target(), pending.edit.tags());
        // TIDAL has it now, so the id sets do too: a later read that lacks
        // it would have to be from before.
        if let (Edit::Favorite(favorite, on), Some((stamp, ids))) =
            (&pending.edit, &mut self.favorites)
        {
            ids.set(&favorite.id(), *on);
            *stamp = at;
        }
        let deleted = match &pending.edit {
            Edit::Delete(playlist) => Some(SourceRef::Playlist {
                uuid: playlist.uuid.clone(),
                sort: None,
            }),
            _ => None,
        };
        let mut effects = self.next(&target);
        effects.push(Effect::Refresh(tags));
        effects.extend(deleted.map(Effect::SourceDeleted));
        effects
    }

    /// TIDAL refused an edit: it's dropped, with the edits waiting behind
    /// it, and the user is told once.
    fn failed(&mut self, id: EditId, error: &syzygy_catalog::Error) -> Vec<Effect> {
        let Some(index) = self.pending.iter().position(|pending| pending.id == id) else {
            return vec![];
        };
        let failed = self.pending.remove(index);
        log::warn!("TIDAL refused {:?}: {error}", failed.edit);
        let target = failed.edit.target();
        self.pending
            .retain(|pending| pending.edit.target() != target || pending.state != Progress::Queued);
        vec![Effect::Toast(failed.edit.failure())]
    }

    /// Run the first edit waiting on `target`.
    fn next(&mut self, target: &FavoriteId) -> Vec<Effect> {
        let queued = self
            .pending
            .iter()
            .find(|pending| pending.edit.target() == *target && pending.state == Progress::Queued)
            .map(|pending| pending.id);
        queued.map_or_else(Vec::new, |id| self.run(id))
    }

    /// A read started at `stamp` brought TIDAL's answer for `tags`: the
    /// landed edits it shows are no longer needed.
    fn settle(&mut self, stamp: Stamp, tags: &[String]) {
        self.pending.retain(|pending| {
            let Progress::Landed(at) = pending.state else {
                return true;
            };
            let shown = at < stamp && pending.edit.tags().iter().any(|tag| tags.contains(tag));
            !shown
        });
    }

    /// Whether the user has this Favorite, with their edits: `None` until
    /// the Favorites have loaded, when its heart can't be clicked.
    pub fn favorite(&self, id: &FavoriteId) -> Option<bool> {
        let (_, ids) = self.favorites.as_ref()?;
        let edited = self
            .pending
            .iter()
            .rev()
            .find_map(|pending| match &pending.edit {
                Edit::Favorite(favorite, on) if favorite.id() == *id => Some(*on),
                _ => None,
            });
        Some(edited.unwrap_or_else(|| ids.contains(id)))
    }

    /// Whether the track is a Loved track: `None` until that's known.
    pub fn liked(&self, track: &Track) -> Option<bool> {
        self.favorite(&FavoriteId::Track(track.id))
    }

    /// An Own playlist as the user last edited it, for its Page.
    pub fn playlist<'a>(&'a self, playlist: &'a Playlist) -> &'a Playlist {
        let id = FavoriteId::Playlist(playlist.uuid.clone());
        self.pending
            .iter()
            .rev()
            .filter(|pending| pending.edit.target() == id)
            .find_map(|pending| match pending.edit.shows() {
                Some(Item::Playlist(edited)) => Some(edited),
                _ => None,
            })
            .unwrap_or(playlist)
    }

    /// A list's server items with the pending edits laid over them.
    pub fn apply<'a, T: Listed>(&'a self, server: &'a [T], listing: Listing<'_>) -> Vec<&'a T> {
        apply(server, listing, &self.pending)
    }
}

/// Which list [`apply`] renders.
#[derive(Debug, Clone, Copy)]
pub enum Listing<'a> {
    /// The Favorites Page's Loved tracks.
    Loved,
    /// A Library shelf: a type at the top level, or a Folder's playlists.
    Shelf(&'a Shelf),
}

impl Listing<'_> {
    /// Whether a like of this Favorite adds it to the list. A liked
    /// playlist goes to the top level, not into a Folder.
    fn adds(&self, id: &FavoriteId) -> bool {
        match self {
            Listing::Loved => matches!(id, FavoriteId::Track(_)),
            Listing::Shelf(shelf) => shelf.folder.is_none() && shelf.lists(id),
        }
    }
}

/// What a list holds, as far as [`apply`] goes.
pub trait Listed {
    /// The Favorite it is, if it's one.
    fn favorite(&self) -> Option<FavoriteId>;
    /// What a pending like adds to a list of these.
    fn liked(favorite: &Favorite) -> Option<&Self>;
    /// What an edited or new playlist shows as in a list of these.
    fn edited(item: &Item) -> Option<&Self>;
}

impl Listed for Track {
    fn favorite(&self) -> Option<FavoriteId> {
        Some(FavoriteId::Track(self.id))
    }

    fn liked(favorite: &Favorite) -> Option<&Self> {
        match favorite {
            Favorite::Track(track) => Some(track),
            Favorite::Item(..) => None,
        }
    }

    fn edited(_item: &Item) -> Option<&Self> {
        None
    }
}

impl Listed for Item {
    fn favorite(&self) -> Option<FavoriteId> {
        match self {
            Item::Folder(_) => None,
            Item::Playlist(playlist) => Some(FavoriteId::Playlist(playlist.uuid.clone())),
            Item::Card(card) => Favorite::card(card).map(|favorite| favorite.id()),
        }
    }

    fn liked(favorite: &Favorite) -> Option<&Self> {
        match favorite {
            Favorite::Item(_, item) => Some(item),
            Favorite::Track(_) => None,
        }
    }

    fn edited(item: &Item) -> Option<&Self> {
        Some(item)
    }
}

/// What a list shows: its server items with the pending edits laid over
/// them. An unliked Favorite or a deleted playlist is hidden, and an
/// edited playlist shows as edited. A liked Favorite or a new playlist not
/// yet among them goes first, the latest edit first. The last edit of a
/// target is the one that counts. The one merge every list goes through.
pub fn apply<'a, T: Listed>(
    server: &'a [T],
    listing: Listing<'_>,
    pending: &'a [Pending],
) -> Vec<&'a T> {
    // Each target's last edit, latest first.
    let mut last: Vec<(FavoriteId, &'a Edit)> = Vec::new();
    for pending in pending.iter().rev() {
        let id = pending.edit.target();
        if !last.iter().any(|(seen, _)| *seen == id) {
            last.push((id, &pending.edit));
        }
    }
    let edit = |id: &FavoriteId| {
        last.iter()
            .find(|(seen, _)| seen == id)
            .map(|(_, edit)| *edit)
    };
    let created = |id: &FavoriteId| {
        pending
            .iter()
            .any(|pending| matches!(pending.edit, Edit::Create(_)) && pending.edit.target() == *id)
    };
    let listed = |id: &FavoriteId| {
        server
            .iter()
            .any(|item| item.favorite().as_ref() == Some(id))
    };
    let added = last
        .iter()
        .filter(|(id, _)| listing.adds(id) && !listed(id))
        .filter_map(|(id, edit)| match edit {
            Edit::Favorite(favorite, true) => T::liked(favorite),
            Edit::Create(item) | Edit::Change(item) if created(id) => T::edited(item),
            _ => None,
        });
    let kept = server.iter().filter_map(|item| {
        let Some(edit) = item.favorite().and_then(|id| edit(&id)) else {
            return Some(item);
        };
        if edit.hides() {
            return None;
        }
        Some(edit.shows().and_then(T::edited).unwrap_or(item))
    });
    added.chain(kept).collect()
}

/// A name short enough for a toast, as sone cuts it.
pub fn short(name: &str) -> String {
    if name.chars().count() > 30 {
        let cut: String = name.chars().take(28).collect();
        format!("{cut}\u{2026}")
    } else {
        name.to_string()
    }
}
