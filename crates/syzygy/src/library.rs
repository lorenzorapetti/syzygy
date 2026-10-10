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
//! Tracks are added to Own playlists the same way: the playlist counts
//! them at once, and TIDAL's count replaces that guess when the add comes
//! back. One track is refused if the playlist has it already; a selection
//! skips what it has and the user is told how many that was.
//!
//! Tracks are removed from Own playlists by their index in the playlist's
//! own order, which is how TIDAL takes them. A row shown in that order
//! knows its index; a row shown sorted doesn't, so its removal first reads
//! the playlist in its own order and looks for the one row that's the same
//! track added at the same time. Until TIDAL has taken it, the row is
//! hidden and the rows after it move up; once it has, the Page drops it
//! from what it loaded.
//!
//! Folders are created, renamed and deleted the same way, and Own
//! playlists are moved between them. A new Folder shows at once as a
//! placeholder that can't be opened, with the playlist it was made for
//! already in it, until a read of the top level lists TIDAL's. A moved
//! playlist shows only where it went, and the Folders it left and joined
//! count it at once. Only an empty Folder can be deleted.
//!
//! [`Library::update`] is pure: it changes the state and returns
//! [`Effect`]s for the Shell to run.

#[cfg(test)]
mod tests;

use std::sync::Arc;
use syzygy_catalog::home_feed::{Card, Target as Leads};
use syzygy_catalog::library::{Folder, Item};
use syzygy_catalog::{
    Added, FavoriteId, FavoriteIds, Kind, LibrarySort, Playlist, PlaylistFields, Read, Shelf, Track,
};

use crate::page::paged::List;
use crate::playback::SourceRef;
use crate::settings::Settings;

/// How a new playlist's uuid, or a new Folder's id, starts until TIDAL
/// has made it.
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

/// What an edit changes: a Favorite, which is also how a track or an Own
/// playlist is named, or a Folder by its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    Favorite(FavoriteId),
    Folder(String),
}

impl Target {
    fn playlist(uuid: &str) -> Self {
        Target::Favorite(FavoriteId::Playlist(uuid.to_string()))
    }
}

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
            Leads::Album(id) => FavoriteId::Album(*id),
            Leads::Artist(id) => FavoriteId::Artist(*id),
            Leads::Playlist(uuid) => FavoriteId::Playlist(uuid.clone()),
            Leads::Mix(id) => FavoriteId::Mix(id.clone()),
            Leads::Favorites | Leads::Track(_) | Leads::Video(_) | Leads::None => {
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
    /// TIDAL has made it, then the one TIDAL made. What comes next (tracks
    /// going in, or a move to a Folder) happens once it's made, as an edit
    /// of its own.
    Create(Item, Then),
    /// An Own playlist with new fields, as an [`Item::Playlist`].
    Change(Item),
    /// An Own playlist deleted.
    Delete(Playlist),
    /// Tracks added to an Own playlist.
    Add(Adding),
    /// A track taken out of an Own playlist.
    Remove(Removing),
    /// A new Folder, as an [`Item::Folder`] with a placeholder id, and the
    /// playlist moved into it as it's made. It shows until a read of the
    /// top level lists the Folder TIDAL made: its id isn't guessed.
    CreateFolder {
        folder: Item,
        moving: Option<Moving>,
    },
    /// A Folder with a new name, and the name it had.
    RenameFolder { folder: Folder, was: String },
    /// An empty Folder deleted.
    DeleteFolder(Folder),
    /// An Own playlist moved to another Folder, or to the top level.
    Move(Moving),
}

/// What follows a new playlist once TIDAL has made it.
#[derive(Debug, Clone)]
pub enum Then {
    /// These tracks go in, if there are any.
    Add(Vec<Track>),
    /// It moves into this Folder.
    Move(Folder),
}

/// A playlist and the Folder it's listed in: `None` is the top level.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub playlist: Playlist,
    pub folder: Option<String>,
}

/// An Own playlist on its way to another Folder.
#[derive(Debug, Clone)]
pub struct Moving {
    /// The playlist as an [`Item::Playlist`], as last edited.
    item: Item,
    /// The Folder it leaves: `None` is the top level.
    from: Option<String>,
    /// The Folder it joins: `None` is the top level.
    to: Option<Folder>,
    /// The playlist was made for this Folder, just before.
    new_playlist: bool,
}

impl Moving {
    fn playlist(&self) -> &Playlist {
        match &self.item {
            Item::Playlist(playlist) => playlist,
            Item::Folder(_) | Item::Card(_) => unreachable!("playlists move"),
        }
    }

    fn to(&self) -> Option<&str> {
        self.to.as_ref().map(|folder| folder.id.as_str())
    }
}

/// A track to take out of an Own playlist, as its row's menu asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Removal {
    pub playlist: Playlist,
    pub track: Track,
    pub at: At,
}

/// Where the row to remove is in its playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    /// Its index in the playlist's own order, as the rows show it: with
    /// the removals still to land taken out.
    Index(usize),
    /// Somewhere in a sorted view, which doesn't say. It's found in the
    /// playlist's own order once the removal runs.
    Sorted,
}

/// A track coming out of an Own playlist.
#[derive(Debug, Clone)]
pub struct Removing {
    /// The playlist as an [`Item::Playlist`], without the track.
    item: Item,
    track: Track,
    at: At,
}

impl Removing {
    fn playlist(&self) -> &Playlist {
        match &self.item {
            Item::Playlist(playlist) => playlist,
            Item::Folder(_) | Item::Card(_) => unreachable!("tracks come out of a playlist"),
        }
    }
}

/// Whether two rows are the same entry of a playlist: the same track,
/// added at the same time. Two copies of a track were added apart.
fn same_entry(a: &Track, b: &Track) -> bool {
    a.id == b.id && a.date_added == b.date_added
}

/// Where `track` is among `rows`: at `index` when it's there, or else the
/// first row that's the same entry.
pub fn locate(rows: &[&Track], track: &Track, index: Option<usize>) -> Option<usize> {
    index
        .filter(|&index| rows.get(index).is_some_and(|row| same_entry(row, track)))
        .or_else(|| rows.iter().position(|row| same_entry(row, track)))
}

/// Tracks going into an Own playlist.
#[derive(Debug, Clone)]
pub struct Adding {
    /// The playlist as an [`Item::Playlist`], counting the tracks.
    item: Item,
    tracks: Vec<Track>,
    /// The playlist was made for them, just before.
    new_playlist: bool,
}

impl Adding {
    /// One track, which TIDAL refuses if the playlist has it already. A
    /// selection skips what it has.
    fn single(&self) -> Option<&Track> {
        match self.tracks.as_slice() {
            [track] => Some(track),
            _ => None,
        }
    }

    /// TIDAL counted the tracks: its count replaces the guess.
    fn counted(&mut self, added: &Added) {
        if let Item::Playlist(playlist) = &mut self.item {
            playlist.tracks = added.tracks;
        }
    }

    fn playlist(&self) -> &Playlist {
        match &self.item {
            Item::Playlist(playlist) => playlist,
            Item::Folder(_) | Item::Card(_) => unreachable!("tracks go into a playlist"),
        }
    }

    /// What the user is told once they're in.
    fn told(&self, added: Option<&Added>) -> String {
        let playlist = short(&self.playlist().title);
        match (self.single(), added) {
            (Some(track), _) => {
                format!("Added \u{201c}{}\u{201d} to playlist", short(&track.title))
            }
            (_, Some(added)) if added.skipped() > 0 => format!(
                "Added {} ({} already in playlist)",
                added.new,
                added.skipped()
            ),
            (None, added) => {
                let new = added.map_or(self.tracks.len(), |added| added.new);
                let noun = if new == 1 { "track" } else { "tracks" };
                format!("Added {new} {noun} to \u{201c}{playlist}\u{201d}")
            }
        }
    }
}

impl Edit {
    /// What the edit changes. Edits of one target run one at a time.
    /// A new Folder made with a playlist in it targets the playlist, so
    /// that a move of it waits.
    fn target(&self) -> Target {
        match self {
            Edit::Favorite(favorite, _) => Target::Favorite(favorite.id()),
            Edit::Create(item, _) | Edit::Change(item) => Target::playlist(uuid(item)),
            Edit::Delete(playlist) => Target::playlist(&playlist.uuid),
            Edit::Add(adding) => Target::playlist(&adding.playlist().uuid),
            Edit::Remove(removing) => Target::playlist(&removing.playlist().uuid),
            Edit::CreateFolder {
                moving: Some(moving),
                ..
            }
            | Edit::Move(moving) => Target::playlist(&moving.playlist().uuid),
            Edit::CreateFolder {
                folder: Item::Folder(folder),
                moving: None,
            }
            | Edit::RenameFolder { folder, .. }
            | Edit::DeleteFolder(folder) => Target::Folder(folder.id.clone()),
            Edit::CreateFolder { .. } => unreachable!("a new Folder is a Folder"),
        }
    }

    /// Where it puts the playlist it targets: `Some(None)` is the top
    /// level.
    fn moves_to(&self) -> Option<Option<&str>> {
        match self {
            Edit::Move(moving) => Some(moving.to()),
            Edit::CreateFolder {
                folder: Item::Folder(folder),
                moving: Some(_),
            } => Some(Some(&folder.id)),
            _ => None,
        }
    }

    /// The reads it makes stale.
    fn tags(&self) -> Vec<String> {
        match self {
            Edit::Favorite(favorite, _) => vec![favorite.id().tag().to_string()],
            Edit::Create(..) => vec![FOLDERS.to_string()],
            Edit::Change(item) => playlist_tags(uuid(item)),
            Edit::Delete(playlist) => playlist_tags(&playlist.uuid),
            Edit::Add(adding) => playlist_tags(&adding.playlist().uuid),
            Edit::Remove(removing) => playlist_tags(&removing.playlist().uuid),
            Edit::CreateFolder { .. }
            | Edit::RenameFolder { .. }
            | Edit::DeleteFolder(_)
            | Edit::Move(_) => vec![FOLDERS.to_string()],
        }
    }

    /// The playlist it shows in place of the listed one.
    fn shows(&self) -> Option<&Item> {
        match self {
            Edit::Create(item, _) | Edit::Change(item) => Some(item),
            Edit::Add(adding) => Some(&adding.item),
            Edit::Remove(removing) => Some(&removing.item),
            Edit::Move(moving) => Some(&moving.item),
            Edit::Favorite(..)
            | Edit::Delete(_)
            | Edit::CreateFolder { .. }
            | Edit::RenameFolder { .. }
            | Edit::DeleteFolder(_) => None,
        }
    }

    /// Whether it takes what it targets out of the lists.
    fn hides(&self) -> bool {
        matches!(
            self,
            Edit::Favorite(_, false) | Edit::Delete(_) | Edit::DeleteFolder(_)
        )
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
            Edit::Create(item, _) => format!("Couldn't create \u{201c}{}\u{201d}", title(item)),
            Edit::Change(item) => format!("Couldn't save \u{201c}{}\u{201d}", title(item)),
            Edit::Delete(playlist) => {
                format!("Couldn't delete \u{201c}{}\u{201d}", short(&playlist.title))
            }
            Edit::Add(adding) => {
                let playlist = short(&adding.playlist().title);
                match (adding.new_playlist, adding.single()) {
                    (true, _) => {
                        format!("Created \u{201c}{playlist}\u{201d}, but couldn't add the tracks")
                    }
                    (false, Some(track)) => format!(
                        "Couldn't add \u{201c}{}\u{201d} to \u{201c}{playlist}\u{201d}",
                        short(&track.title)
                    ),
                    (false, None) => {
                        format!("Couldn't add the tracks to \u{201c}{playlist}\u{201d}")
                    }
                }
            }
            Edit::Remove(removing) => format!(
                "Couldn't remove \u{201c}{}\u{201d} from \u{201c}{}\u{201d}",
                short(&removing.track.title),
                short(&removing.playlist().title)
            ),
            Edit::CreateFolder { folder, .. } => {
                format!("Couldn't create \u{201c}{}\u{201d}", title(folder))
            }
            Edit::RenameFolder { was, .. } => {
                format!("Couldn't rename \u{201c}{}\u{201d}", short(was))
            }
            Edit::DeleteFolder(folder) => {
                format!("Couldn't delete \u{201c}{}\u{201d}", short(&folder.name))
            }
            Edit::Move(moving) => {
                let playlist = short(&moving.playlist().title);
                match (&moving.to, moving.new_playlist) {
                    (Some(folder), true) => format!(
                        "Created \u{201c}{playlist}\u{201d}, but couldn't move it to \u{201c}{}\u{201d}",
                        short(&folder.name)
                    ),
                    (Some(folder), false) => format!(
                        "Couldn't move \u{201c}{playlist}\u{201d} to \u{201c}{}\u{201d}",
                        short(&folder.name)
                    ),
                    (None, _) => {
                        format!("Couldn't move \u{201c}{playlist}\u{201d} out of its folder")
                    }
                }
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
pub fn playlist_tags(uuid: &str) -> Vec<String> {
    vec![FOLDERS.to_string(), syzygy_catalog::playlist_tag(uuid)]
}

/// A new playlist TIDAL hasn't made yet. It can't be opened, edited or
/// deleted.
pub fn is_placeholder(playlist: &Playlist) -> bool {
    playlist.uuid.starts_with(PLACEHOLDER)
}

/// A new Folder TIDAL hasn't made yet. It can't be opened, right-clicked
/// or moved into.
pub fn is_placeholder_folder(folder: &Folder) -> bool {
    folder.id.starts_with(PLACEHOLDER)
}

/// A dialog the user answers before an edit.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// The form for a new playlist, which these tracks go into.
    NewPlaylist(Vec<Track>),
    /// The form for a new playlist that goes into this Folder.
    NewPlaylistIn(Folder),
    /// The form for an Own playlist's title, description and access.
    EditPlaylist(Playlist),
    /// Whether to delete an Own playlist.
    DeletePlaylist(Playlist),
    /// A new Folder's name, and the playlist that goes into it.
    NewFolder(Option<Placed>),
    /// A Folder's new name.
    RenameFolder(Folder),
    /// Whether to delete an empty Folder.
    DeleteFolder(Folder),
}

/// Tracks for a playlist, as a menu has them.
#[derive(Debug, Clone, PartialEq)]
pub enum Tracks {
    These(Vec<Track>),
    /// All of what a card leads to, read when they're wanted.
    Card(Card),
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
    /// Add one track to an Own playlist, refused if it's there already
    /// (`onDupes=FAIL`).
    AddTrack {
        user_id: u64,
        uuid: String,
        track: u64,
    },
    /// Add tracks to an Own playlist, skipping those it has
    /// (`onDupes=SKIP`). Its result comes back as [`Message::Added`].
    AddTracks {
        user_id: u64,
        uuid: String,
        tracks: Vec<u64>,
    },
    /// Take the track at `index` in an Own playlist's own order out.
    RemoveTrack {
        user_id: u64,
        uuid: String,
        index: usize,
    },
    /// Make a Folder at the top level, with this playlist in it.
    CreateFolder {
        user_id: u64,
        name: String,
        playlist: Option<String>,
    },
    /// Give a Folder a new name.
    RenameFolder {
        user_id: u64,
        id: String,
        name: String,
    },
    /// Delete an empty Folder.
    DeleteFolder { user_id: u64, id: String },
    /// Move an Own playlist into a Folder, or to the top level (`None`).
    Move {
        user_id: u64,
        uuid: String,
        folder: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Like (`true`) or unlike a Favorite; follow or unfollow an artist.
    /// Dropped while its heart can't be clicked: until the Favorites have
    /// loaded, or when it's already so.
    Favorite(Favorite, bool),
    /// Open a dialog: for a new playlist, or to edit or delete an Own one.
    Ask(Ask),
    /// Open "Add to playlist" for these tracks, in a popover beside `at`:
    /// the menu item or button that asked, on screen.
    Pick { tracks: Tracks, at: iced::Rectangle },
    /// Open "Move to folder" for an Own playlist, in a popover beside
    /// `at`.
    PickFolder {
        playlist: Placed,
        at: iced::Rectangle,
    },
    /// Make a playlist with these fields, and put these tracks in it.
    CreatePlaylist(PlaylistFields, Vec<Track>),
    /// Make a playlist with these fields, and move it into this Folder.
    CreatePlaylistIn(PlaylistFields, Folder),
    /// Make a Folder with this name, with this playlist in it.
    CreateFolder(String, Option<Placed>),
    /// Give a Folder this name.
    RenameFolder(Folder, String),
    /// Delete an empty Folder, as the user confirmed.
    DeleteFolder(Folder),
    /// Move an Own playlist into a Folder, or to the top level (`None`).
    Move(Placed, Option<Folder>),
    /// Set an Own playlist's fields.
    EditPlaylist(Playlist, PlaylistFields),
    /// Delete an Own playlist, as the user confirmed.
    DeletePlaylist(Playlist),
    /// Add tracks to an Own playlist.
    AddTracks(Playlist, Vec<Track>),
    /// Take a track out of an Own playlist.
    RemoveTrack(Removal),
    /// An edit's mutation came back.
    Done(EditId, Result<(), Arc<syzygy_catalog::Error>>),
    /// A new playlist's mutation came back with the playlist TIDAL made.
    Created(EditId, Result<Playlist, Arc<syzygy_catalog::Error>>),
    /// Tracks went into a playlist, as TIDAL counted them.
    Added(EditId, Result<Added, Arc<syzygy_catalog::Error>>),
    /// A sorted removal's playlist, read in its own order.
    Order(EditId, Result<Vec<Track>, Arc<syzygy_catalog::Error>>),
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
    /// Read the playlist with this uuid in its own order, to find where a
    /// sorted removal's row is. It comes back as [`Message::Order`].
    ReadOrder(EditId, String),
    /// Read the Favorite ids, as of the stamp.
    ReadFavorites { user_id: u64, stamp: Stamp },
    /// An edit landed: what's read under these tags is stale, so lists on
    /// screen read it again.
    Refresh(Vec<String>),
    /// Tell the user an edit didn't happen.
    Toast(String),
    /// Tell the user how an edit went, when there's more to say than that
    /// it showed.
    Inform(String),
    /// Tracks are going into this playlist: it's one of the recent ones.
    Recent(String),
    /// Tracks went into this playlist: TIDAL makes its cover a while
    /// later, so its reads are read again then.
    Cover(String),
    /// Open a dialog.
    Ask(Ask),
    /// Open "Add to playlist".
    Pick { tracks: Tracks, at: iced::Rectangle },
    /// Open "Move to folder".
    PickFolder {
        playlist: Placed,
        at: iced::Rectangle,
    },
    /// A playlist is going into this Folder: it's one of the recent ones.
    RecentFolder(String),
    /// The playlist with this uuid is gone: its Pages go too.
    Deleted(String),
    /// The Folder with this id is gone: its Pages go too.
    FolderDeleted(String),
    /// TIDAL took the track at `index` of the playlist's own order out:
    /// a Page showing it drops the row from what it loaded.
    Removed {
        uuid: String,
        track: Track,
        index: usize,
    },
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
                    Ask::NewPlaylist(_) | Ask::NewFolder(None) => self.user_id.is_some(),
                    Ask::EditPlaylist(playlist) | Ask::DeletePlaylist(playlist) => {
                        self.editable(playlist)
                    }
                    Ask::NewFolder(Some(placed)) => self.editable(&placed.playlist),
                    Ask::NewPlaylistIn(folder) | Ask::RenameFolder(folder) => {
                        self.user_id.is_some() && !is_placeholder_folder(folder)
                    }
                    Ask::DeleteFolder(folder) => self.deletable(folder),
                };
                if allowed {
                    vec![Effect::Ask(ask)]
                } else {
                    vec![]
                }
            }
            Message::Pick { tracks, at } => match self.user_id {
                Some(_) => vec![Effect::Pick { tracks, at }],
                None => vec![],
            },
            Message::PickFolder { playlist, at } => {
                if !self.editable(&playlist.playlist) {
                    return vec![];
                }
                vec![Effect::PickFolder { playlist, at }]
            }
            Message::CreatePlaylist(fields, tracks) => {
                let count = tracks.len() as u32;
                self.create_playlist(fields, count, Then::Add(tracks))
            }
            Message::CreatePlaylistIn(fields, folder) => {
                if is_placeholder_folder(&folder) {
                    return vec![];
                }
                self.create_playlist(fields, 0, Then::Move(folder))
            }
            Message::CreateFolder(name, playlist) => {
                let name = name.trim().to_string();
                let allowed = match &playlist {
                    Some(placed) => self.editable(&placed.playlist),
                    None => self.user_id.is_some(),
                };
                if !allowed || name.is_empty() {
                    return vec![];
                }
                let folder = Folder {
                    id: format!("{PLACEHOLDER}{}", self.next_edit),
                    name,
                    playlists: Some(playlist.is_some().into()),
                };
                let moving =
                    playlist.map(|placed| self.moving(placed, Some(folder.clone()), false));
                self.push(Edit::CreateFolder {
                    folder: Item::Folder(folder),
                    moving,
                })
            }
            Message::RenameFolder(folder, name) => {
                let name = name.trim().to_string();
                let was = self.folder(&folder).name;
                if self.user_id.is_none()
                    || is_placeholder_folder(&folder)
                    || name.is_empty()
                    || name == was
                {
                    return vec![];
                }
                self.push(Edit::RenameFolder {
                    folder: Folder { name, ..folder },
                    was,
                })
            }
            Message::DeleteFolder(folder) => {
                if !self.deletable(&folder) {
                    return vec![];
                }
                let id = folder.id.clone();
                let mut effects = self.push(Edit::DeleteFolder(folder));
                effects.push(Effect::FolderDeleted(id));
                effects
            }
            Message::Move(placed, to) => self.move_to(placed, to, false),
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
            Message::AddTracks(playlist, tracks) => {
                if !self.editable(&playlist) || tracks.is_empty() {
                    return vec![];
                }
                self.add(playlist, tracks, false)
            }
            Message::RemoveTrack(Removal {
                playlist,
                track,
                at,
            }) => {
                if !self.editable(&playlist) {
                    return vec![];
                }
                let shown = self.playlist(&playlist);
                let without = Playlist {
                    tracks: shown.tracks.saturating_sub(1),
                    duration: shown.duration.saturating_sub(track.duration),
                    ..shown.clone()
                };
                self.push(Edit::Remove(Removing {
                    item: Item::Playlist(without),
                    track,
                    at,
                }))
            }
            Message::Done(id, Ok(())) => self.landed(id, None),
            Message::Done(id, Err(e))
            | Message::Created(id, Err(e))
            | Message::Added(id, Err(e))
            | Message::Order(id, Err(e)) => self.failed(id, &e),
            Message::Order(id, Ok(order)) => self.found(id, &order),
            Message::Created(id, Ok(made)) => {
                // From now on it's the playlist TIDAL made, which can be
                // opened.
                let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
                    return vec![];
                };
                let Edit::Create(_, then) = &mut pending.edit else {
                    return vec![];
                };
                let then = std::mem::replace(then, Then::Add(vec![]));
                pending.edit = Edit::Create(Item::Playlist(made.clone()), Then::Add(vec![]));
                let mut effects = self.landed(id, None);
                match then {
                    Then::Add(tracks) if tracks.is_empty() => {}
                    Then::Add(tracks) => effects.extend(self.add(made, tracks, true)),
                    Then::Move(folder) => {
                        let placed = Placed {
                            playlist: made,
                            folder: None,
                        };
                        effects.extend(self.move_to(placed, Some(folder), true));
                    }
                }
                effects
            }
            Message::Added(id, Ok(added)) => self.landed(id, Some(added)),
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

    /// Whether the user can delete `folder`: one TIDAL has made, with no
    /// playlists in it once the pending edits are counted.
    pub fn deletable(&self, folder: &Folder) -> bool {
        self.user_id.is_some()
            && !is_placeholder_folder(folder)
            && self.folder(folder).playlists == Some(0)
    }

    /// Make a playlist with `count` tracks to come, and then what follows.
    fn create_playlist(&mut self, fields: PlaylistFields, count: u32, then: Then) -> Vec<Effect> {
        let Some(user_id) = self.user_id else {
            return vec![];
        };
        let uuid = format!("{PLACEHOLDER}{}", self.next_edit);
        let placeholder = Playlist {
            tracks: count,
            ..fields.playlist(uuid, user_id)
        };
        self.push(Edit::Create(Item::Playlist(placeholder), then))
    }

    /// A placed playlist on its way to `to`, as last edited.
    fn moving(&self, placed: Placed, to: Option<Folder>, new_playlist: bool) -> Moving {
        Moving {
            item: Item::Playlist(self.playlist(&placed.playlist).clone()),
            from: placed.folder,
            to,
            new_playlist,
        }
    }

    /// Move a playlist to `to`, unless it's there already or `to` isn't
    /// made yet.
    fn move_to(&mut self, placed: Placed, to: Option<Folder>, new_playlist: bool) -> Vec<Effect> {
        let to_id = to.as_ref().map(|folder| folder.id.as_str());
        if !self.editable(&placed.playlist)
            || placed.folder.as_deref() == to_id
            || to.as_ref().is_some_and(is_placeholder_folder)
        {
            return vec![];
        }
        let recent = to
            .as_ref()
            .map(|folder| Effect::RecentFolder(folder.id.clone()));
        let moving = self.moving(placed, to, new_playlist);
        let mut effects = self.push(Edit::Move(moving));
        effects.extend(recent);
        effects
    }

    /// Put tracks in a playlist: it counts them at once.
    fn add(&mut self, playlist: Playlist, tracks: Vec<Track>, new_playlist: bool) -> Vec<Effect> {
        let shown = self.playlist(&playlist);
        let counted = Playlist {
            tracks: shown.tracks + tracks.len() as u32,
            ..shown.clone()
        };
        let uuid = playlist.uuid.clone();
        let mut effects = self.push(Edit::Add(Adding {
            item: Item::Playlist(counted),
            tracks,
            new_playlist,
        }));
        effects.push(Effect::Recent(uuid));
        effects
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
            Edit::Create(Item::Playlist(playlist), _) => Mutation::CreatePlaylist {
                user_id,
                fields: PlaylistFields::of(playlist),
            },
            Edit::Change(Item::Playlist(playlist)) => Mutation::UpdatePlaylist {
                user_id,
                uuid: playlist.uuid.clone(),
                fields: PlaylistFields::of(playlist),
            },
            Edit::Create(..) | Edit::Change(_) => return vec![],
            Edit::Delete(playlist) => Mutation::DeletePlaylist {
                user_id,
                uuid: playlist.uuid.clone(),
            },
            Edit::Add(adding) => {
                let uuid = adding.playlist().uuid.clone();
                match adding.single() {
                    Some(track) => Mutation::AddTrack {
                        user_id,
                        uuid,
                        track: track.id,
                    },
                    None => Mutation::AddTracks {
                        user_id,
                        uuid,
                        tracks: adding.tracks.iter().map(|track| track.id).collect(),
                    },
                }
            }
            Edit::Remove(removing) => match removing.at {
                At::Index(index) => Mutation::RemoveTrack {
                    user_id,
                    uuid: removing.playlist().uuid.clone(),
                    index,
                },
                At::Sorted => {
                    return vec![Effect::ReadOrder(id, removing.playlist().uuid.clone())];
                }
            },
            Edit::CreateFolder {
                folder: Item::Folder(folder),
                moving,
            } => Mutation::CreateFolder {
                user_id,
                name: folder.name.clone(),
                playlist: moving.as_ref().map(|moving| moving.playlist().uuid.clone()),
            },
            Edit::CreateFolder { .. } => return vec![],
            Edit::RenameFolder { folder, .. } => Mutation::RenameFolder {
                user_id,
                id: folder.id.clone(),
                name: folder.name.clone(),
            },
            Edit::DeleteFolder(folder) => Mutation::DeleteFolder {
                user_id,
                id: folder.id.clone(),
            },
            Edit::Move(moving) => Mutation::Move {
                user_id,
                uuid: moving.playlist().uuid.clone(),
                folder: moving.to().map(str::to_string),
            },
        };
        vec![Effect::Mutate(id, mutation)]
    }

    /// TIDAL took an edit: its reads are read again, and the next edit of
    /// its target runs. Tracks `added` to a playlist are as TIDAL counted
    /// them.
    fn landed(&mut self, id: EditId, added: Option<Added>) -> Vec<Effect> {
        let at = self.tick();
        let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
            return vec![];
        };
        pending.state = Progress::Landed(at);
        let told = match &mut pending.edit {
            Edit::Add(adding) => {
                if let Some(added) = &added {
                    adding.counted(added);
                }
                Some(adding.told(added.as_ref()))
            }
            _ => None,
        };
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
        let removed = match &pending.edit {
            Edit::Remove(removing) => match removing.at {
                At::Index(index) => Some(Effect::Removed {
                    uuid: removing.playlist().uuid.clone(),
                    track: removing.track.clone(),
                    index,
                }),
                // A removal runs once its index is known.
                At::Sorted => None,
            },
            _ => None,
        };
        let mut effects = self.next(&target);
        effects.extend(removed);
        effects.push(Effect::Refresh(tags));
        effects.extend(deleted.map(Effect::SourceDeleted));
        if let (Some(told), Target::Favorite(FavoriteId::Playlist(uuid))) = (told, target) {
            effects.push(Effect::Inform(told));
            effects.push(Effect::Cover(uuid));
        }
        effects
    }

    /// TIDAL refused an edit: it's dropped, with the edits waiting behind
    /// it, and the user is told once.
    fn failed(&mut self, id: EditId, error: &syzygy_catalog::Error) -> Vec<Effect> {
        let Some(failed) = self.drop_edit(id) else {
            return vec![];
        };
        log::warn!("TIDAL refused {:?}: {error}", failed.edit);
        // A playlist just made has no tracks to clash with.
        let duplicate = matches!(&failed.edit, Edit::Add(adding)
            if adding.single().is_some() && !adding.new_playlist)
            && error.is_duplicate();
        if duplicate {
            return vec![Effect::Inform("Track already in this playlist".to_string())];
        }
        vec![Effect::Toast(failed.edit.failure())]
    }

    /// Take an edit out, with the edits waiting behind it.
    fn drop_edit(&mut self, id: EditId) -> Option<Pending> {
        let index = self.pending.iter().position(|pending| pending.id == id)?;
        let dropped = self.pending.remove(index);
        let target = dropped.edit.target();
        self.pending
            .retain(|pending| pending.edit.target() != target || pending.state != Progress::Queued);
        Some(dropped)
    }

    /// A sorted removal's playlist came in its own order: the removal goes
    /// by the one row that's the same entry, and is refused when there's
    /// none or more than one.
    fn found(&mut self, id: EditId, order: &[Track]) -> Vec<Effect> {
        let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
            return vec![];
        };
        let Edit::Remove(removing) = &mut pending.edit else {
            return vec![];
        };
        let mut matching = order
            .iter()
            .enumerate()
            .filter(|(_, row)| same_entry(row, &removing.track))
            .map(|(index, _)| index);
        let refusal = match (matching.next(), matching.next()) {
            (Some(index), None) => {
                removing.at = At::Index(index);
                return self.run(id);
            }
            (None, _) => format!(
                "Couldn't find \u{201c}{}\u{201d} in \u{201c}{}\u{201d}",
                short(&removing.track.title),
                short(&removing.playlist().title)
            ),
            (Some(_), Some(_)) => format!(
                "Couldn't tell which \u{201c}{}\u{201d} to remove. \
                 Sort the playlist by # and remove it there",
                short(&removing.track.title)
            ),
        };
        log::warn!("Not removing {:?}: {refusal}", removing.track);
        self.drop_edit(id);
        vec![Effect::Toast(refusal)]
    }

    /// Run the first edit waiting on `target`.
    fn next(&mut self, target: &Target) -> Vec<Effect> {
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
        let id = Target::playlist(&playlist.uuid);
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

    /// A Folder as the user last edited it: its new name, and its count
    /// with the playlists moved in and out of it.
    pub fn folder(&self, folder: &Folder) -> Folder {
        let mut shown = folder.clone();
        let mut change: i64 = 0;
        for pending in &self.pending {
            match &pending.edit {
                Edit::RenameFolder {
                    folder: renamed, ..
                } if renamed.id == folder.id => {
                    shown.name = renamed.name.clone();
                }
                Edit::Move(moving) => {
                    if moving.from.as_deref() == Some(folder.id.as_str()) {
                        change -= 1;
                    }
                    if moving.to() == Some(folder.id.as_str()) {
                        change += 1;
                    }
                }
                // A new Folder counts its playlist already: only the Folder
                // it left is counted here.
                Edit::CreateFolder {
                    moving: Some(moving),
                    ..
                } if moving.from.as_deref() == Some(folder.id.as_str()) => change -= 1,
                _ => {}
            }
        }
        shown.playlists = folder
            .playlists
            .map(|n| (i64::from(n) + change).max(0) as u32);
        shown
    }

    /// The rows of a playlist's loaded `tracks` that aren't on their way
    /// out, by their place in `tracks`. In the playlist's own order (not
    /// `sorted`), a row's place among them is its index in the playlist.
    pub fn rows(&self, uuid: &str, tracks: &[Track], sorted: bool) -> Vec<usize> {
        let mut rows: Vec<usize> = (0..tracks.len()).collect();
        let removing = self
            .pending
            .iter()
            .filter_map(|pending| match &pending.edit {
                Edit::Remove(removing)
                    if removing.playlist().uuid == uuid
                        && !matches!(pending.state, Progress::Landed(_)) =>
                {
                    Some(removing)
                }
                _ => None,
            });
        // In the order they were asked for, as each index counts the
        // removals before it.
        for removing in removing {
            let index = match removing.at {
                At::Index(index) if !sorted => Some(index),
                At::Index(_) | At::Sorted => None,
            };
            let shown: Vec<&Track> = rows.iter().map(|&position| &tracks[position]).collect();
            if let Some(at) = locate(&shown, &removing.track, index) {
                rows.remove(at);
            }
        }
        rows
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
    /// Every Own playlist, wherever its Folder: what tracks can be added
    /// to. Likes and moves don't change it.
    Own,
}

impl Listing<'_> {
    /// Whether the edit changes what the list shows.
    fn shows(&self, edit: &Edit) -> bool {
        !matches!((self, edit), (Listing::Own, Edit::Favorite(..)))
    }

    /// Whether a like of this Favorite, or a new playlist, goes into the
    /// list. A liked or new playlist goes to the top level, not into a
    /// Folder.
    fn adds(&self, id: &Target) -> bool {
        let Target::Favorite(id) = id else {
            return false;
        };
        match self {
            Listing::Loved => matches!(id, FavoriteId::Track(_)),
            Listing::Shelf(shelf) => shelf.folder.is_none() && shelf.lists(id),
            Listing::Own => matches!(id, FavoriteId::Playlist(_)),
        }
    }

    /// The Folder whose playlists these are, for a list of playlists and
    /// Folders: `Some(None)` is the top level. Where a playlist was moved
    /// to decides which of these lists it's in.
    fn folder(&self) -> Option<Option<&str>> {
        match self {
            Listing::Shelf(shelf) if shelf.kind == Kind::Playlists => Some(shelf.folder.as_deref()),
            _ => None,
        }
    }
}

/// What a list holds, as far as [`apply`] goes.
pub trait Listed {
    /// The Favorite it is, if it's one.
    fn favorite(&self) -> Option<FavoriteId>;
    /// What its edits target, if it can be edited.
    fn target(&self) -> Option<Target> {
        self.favorite().map(Target::Favorite)
    }
    /// What a pending like adds to a list of these.
    fn liked(favorite: &Favorite) -> Option<&Self>;
    /// What an edited or new playlist, or a new Folder, shows as in a list
    /// of these.
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

    fn target(&self) -> Option<Target> {
        match self {
            Item::Folder(folder) => Some(Target::Folder(folder.id.clone())),
            _ => self.favorite().map(Target::Favorite),
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
/// them. An unliked Favorite, a deleted playlist or Folder is hidden, and
/// an edited playlist shows as edited. A moved playlist shows only in the
/// Folder it went to. New Folders go first at the top level, then a liked
/// Favorite, a new playlist or a moved one not yet among them, the latest
/// edit first. The last edit of a target is the one that counts. The one
/// merge every list goes through.
pub fn apply<'a, T: Listed>(
    server: &'a [T],
    listing: Listing<'_>,
    pending: &'a [Pending],
) -> Vec<&'a T> {
    let shown = || pending.iter().rev().filter(|p| listing.shows(&p.edit));
    // Each target's last edit, latest first.
    let mut last: Vec<(Target, &'a Edit)> = Vec::new();
    for pending in shown() {
        let id = pending.edit.target();
        if !last.iter().any(|(seen, _)| *seen == id) {
            last.push((id, &pending.edit));
        }
    }
    let edit = |id: &Target| {
        last.iter()
            .find(|(seen, _)| seen == id)
            .map(|(_, edit)| *edit)
    };
    let created = |id: &Target| {
        pending
            .iter()
            .any(|pending| matches!(pending.edit, Edit::Create(..)) && pending.edit.target() == *id)
    };
    let listed = |id: &Target| server.iter().any(|item| item.target().as_ref() == Some(id));
    // Where the target was last moved to, if it was.
    let moved_to = |id: &Target| {
        shown()
            .filter(|pending| pending.edit.target() == *id)
            .find_map(|pending| pending.edit.moves_to())
    };
    // Whether it's in this list, if it's moved; else whether a like or a
    // new playlist goes into it.
    let belongs = |id: &Target| match (listing.folder(), moved_to(id)) {
        (Some(here), Some(to)) => here == to,
        _ => listing.adds(id),
    };
    // The target as last edited.
    let latest = |id: &Target| {
        shown()
            .filter(|pending| pending.edit.target() == *id)
            .find_map(|pending| pending.edit.shows())
    };
    let folders = shown()
        .filter(|_| listing.folder() == Some(None))
        .filter_map(|pending| match &pending.edit {
            Edit::CreateFolder { folder, .. } => T::edited(folder),
            _ => None,
        });
    let added = last
        .iter()
        .filter(|(id, _)| !listed(id) && belongs(id))
        .filter_map(|(id, edit)| match edit {
            Edit::Favorite(favorite, true) => T::liked(favorite),
            _ if edit.hides() => None,
            Edit::Favorite(..) => None,
            _ if created(id) || moved_to(id).is_some() => latest(id).and_then(T::edited),
            _ => None,
        });
    let kept = server.iter().filter_map(|item| {
        let Some(id) = item.target() else {
            return Some(item);
        };
        if let (Some(here), Some(to)) = (listing.folder(), moved_to(&id))
            && here != to
        {
            return None;
        }
        let Some(edit) = edit(&id) else {
            return Some(item);
        };
        if edit.hides() {
            return None;
        }
        Some(edit.shows().and_then(T::edited).unwrap_or(item))
    });
    folders.chain(added).chain(kept).collect()
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
