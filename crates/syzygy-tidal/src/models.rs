use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Playbackinfo sub-statuses occupy the 4xxx range. Auth failures use a
/// different namespace (11002/11003 token, 6001 session, 1002 pending), so a
/// 4xxx code on a 401 is never fixed by refreshing the token.
const PLAYBACKINFO_SUB_STATUS_RANGE: std::ops::RangeInclusive<u64> = 4000..=4999;

/// Sub-statuses meaning "this track will not play, move on". Deliberately
/// excludes 4006 (streaming privileges lost — recovers) and 4033 (subscription
/// up-sell — the user can fix it), which must NOT delete the track.
const TERMINAL_SUB_STATUSES: &[u64] = &[4005, 4010, 4030, 4031, 4032, 4034, 4035];

fn sub_status(body: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("subStatus").cloned())
        .and_then(|s| match s.as_u64() {
            Some(n) => Some(n),
            // A float-encoded whole number (4005.0) is still a sub-status; the
            // frontend's `typeof sub === "number"` accepts it, so we must too.
            None => s
                .as_f64()
                .filter(|f| f.is_finite() && f.fract() == 0.0 && *f >= 0.0)
                .map(|f| f as u64),
        })
}

/// True when a response body carries a playbackinfo sub-status. Drives the
/// "do not refresh the token" decision.
pub fn is_playbackinfo_sub_status(body: &str) -> bool {
    sub_status(body).is_some_and(|s| PLAYBACKINFO_SUB_STATUS_RANGE.contains(&s))
}

/// True when the sub-status means the track itself is unplayable.
pub fn is_terminal_sub_status(body: &str) -> bool {
    sub_status(body).is_some_and(|s| TERMINAL_SUB_STATUSES.contains(&s))
}

/// OAuth tokens. `Debug` leaves the token values out, so they never reach a
/// log.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
    #[serde(default)]
    pub user_id: Option<u64>,
}

impl std::fmt::Debug for AuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthTokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .field("token_type", &self.token_type)
            .field("user_id", &self.user_id)
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalTrack {
    pub id: u64,
    pub title: String,
    pub duration: u32,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    /// Some endpoints return `artists` (plural array) instead of / in addition to `artist`.
    #[serde(default)]
    pub artists: Option<Vec<TidalArtist>>,
    #[serde(default)]
    pub album: Option<TidalAlbum>,
    #[serde(default)]
    pub audio_quality: Option<String>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub volume_number: Option<u32>,
    #[serde(default)]
    pub date_added: Option<String>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub explicit: Option<bool>,
    #[serde(default)]
    pub popularity: Option<u32>,
    #[serde(default)]
    pub replay_gain: Option<f64>,
    #[serde(default)]
    pub peak: Option<f64>,
    #[serde(default)]
    pub copyright: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub stream_ready: Option<bool>,
    #[serde(default)]
    pub allow_streaming: Option<bool>,
    #[serde(default)]
    pub premium_streaming_only: Option<bool>,
    #[serde(default)]
    pub stream_start_date: Option<String>,
    #[serde(default)]
    pub audio_modes: Option<Vec<String>>,
    #[serde(default)]
    pub media_metadata: Option<MediaMetadata>,
    /// Present on track detail responses — contains mix IDs like `TRACK_MIX`.
    #[serde(default)]
    pub mixes: Option<Value>,
    /// From the `/playlists/{id}/items` wrapper `type` — "track" or "video".
    #[serde(default)]
    pub item_type: Option<String>,
    /// Video thumbnail UUID (videos carry `imageId` instead of `album.cover`).
    #[serde(default)]
    pub image_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixPageResult {
    pub mix_id: String,
    pub mix_type: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub image: Option<String>,
    pub tracks: Vec<TidalTrack>,
}

impl TidalTrack {
    /// If `artist` is None but `artists` has entries, fill from the first element.
    pub fn backfill_artist(&mut self) {
        if self.artist.is_none()
            && let Some(ref artists) = self.artists
            && let Some(first) = artists.first()
        {
            self.artist = Some(first.clone());
        }
    }
}

/// Parse `/playlists/{id}/items` wrapper entries (`{ item, type }`) into TidalTracks.
/// A video's inner `item` deserializes cleanly (its missing track-only fields are
/// all `#[serde(default)]`); we stamp `item_type` from the wrapper and copy `imageId`.
pub(crate) fn parse_playlist_items(items: Vec<Value>) -> Result<Vec<TidalTrack>, Error> {
    let mut out = Vec::with_capacity(items.len());
    for entry in items {
        let item_type = entry
            .get("type")
            .and_then(|t| t.as_str())
            .map(|s| s.to_lowercase());
        let Some(inner) = entry.get("item") else {
            continue;
        };
        // A playlist can interleave tracks and videos; a video item may carry a
        // null or absent `duration`, which fails TidalTrack's required u32 and
        // would abort the ENTIRE playlist. Default it to 0 so one such item can't
        // blank the list — the video player reads the real length from the stream.
        let mut inner = inner.clone();
        // NOTE (verified): use `is_none_or`, NOT `map_or(true, …)` — clippy's
        // `unnecessary_map_or` lint (warn-by-default on this repo's rustc 1.95)
        // would fail `cargo clippy -- -D warnings`. `id`/`title` remain hard-required
        // (the only other non-default TidalTrack fields); real video items always
        // carry them, so defaulting `duration` alone resolves the observed abort.
        if inner.get("duration").is_none_or(|d| d.is_null())
            && let Some(obj) = inner.as_object_mut()
        {
            obj.insert("duration".to_string(), Value::from(0u32));
        }
        let mut track: TidalTrack = serde_json::from_value(inner.clone())
            .map_err(|e| Error::Parse(format!("{} - Item: {}", e, inner)))?;
        if track.image_id.is_none() {
            track.image_id = inner
                .get("imageId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        track.item_type = item_type;
        track.backfill_artist();
        out.push(track);
    }
    Ok(out)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbumDetail {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub vibrant_color: Option<String>,
    #[serde(default)]
    pub video_cover: Option<String>,
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    /// v2 API returns "artists" (plural array) instead of "artist" (singular)
    #[serde(default)]
    pub artists: Option<Vec<TidalArtist>>,
    #[serde(default)]
    pub number_of_tracks: Option<u32>,
    #[serde(default)]
    pub number_of_videos: Option<u32>,
    #[serde(default)]
    pub number_of_volumes: Option<u32>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub upc: Option<String>,
    /// "ALBUM" | "EP" | "SINGLE"
    #[serde(default, rename = "type")]
    pub album_type: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
    #[serde(default)]
    pub explicit: Option<bool>,
    #[serde(default)]
    pub popularity: Option<u32>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub audio_quality: Option<String>,
    #[serde(default)]
    pub stream_ready: Option<bool>,
    #[serde(default)]
    pub allow_streaming: Option<bool>,
    #[serde(default)]
    pub stream_start_date: Option<String>,
    #[serde(default)]
    pub audio_modes: Option<Vec<String>>,
    #[serde(default)]
    pub media_metadata: Option<MediaMetadata>,
}

impl TidalAlbumDetail {
    /// Backfill `artist` from `artists[0]` if `artist` is None (v2 API uses plural `artists`)
    pub fn backfill_artist(&mut self) {
        if self.artist.is_none()
            && let Some(ref artists) = self.artists
            && let Some(first) = artists.first()
        {
            self.artist = Some(first.clone());
        }
    }
}

// ==================== Album Page types ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPageResponse {
    pub album: TidalAlbumDetail,
    pub tracks: Vec<TidalTrack>,
    pub total_tracks: u32,
    pub vibrant_color: Option<String>,
    pub video_cover: Option<String>,
    pub copyright: Option<String>,
    pub credits: Vec<TidalCredit>,
    pub review: Option<TidalReview>,
    pub sections: Vec<AlbumPageSection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalReview {
    pub source: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPageSection {
    pub title: String,
    // Written back as `sectionType`, so a cached copy reads back too.
    #[serde(
        rename(deserialize = "type", serialize = "sectionType"),
        alias = "sectionType"
    )]
    pub section_type: String,
    pub items: Vec<Value>,
    pub api_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedTracks {
    pub items: Vec<TidalTrack>,
    pub total_number_of_items: u32,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AllFavoriteIds {
    pub tracks: Vec<u64>,
    pub albums: Vec<u64>,
    pub artists: Vec<u64>,
    pub playlists: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total_number_of_items: u32,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtist {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub artwork_id: Option<String>,
    #[serde(default)]
    pub selected_album_cover_fallback: Option<String>,
    /// "MAIN" | "FEATURED" — present on embedded artist refs in tracks/albums
    #[serde(default, rename = "type")]
    pub artist_type: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalAlbum {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub vibrant_color: Option<String>,
    #[serde(default)]
    pub video_cover: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalPlaylistCreator {
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalPlaylistRaw {
    pub uuid: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub square_image: Option<String>,
    #[serde(default)]
    pub number_of_tracks: Option<u32>,
    #[serde(default)]
    pub number_of_videos: Option<u32>,
    #[serde(default)]
    pub creator: Option<TidalPlaylistCreator>,
    /// "USER" | "EDITORIAL" | "ARTIST"
    #[serde(default, rename = "type")]
    pub playlist_type: Option<String>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub popularity: Option<u32>,
    #[serde(default)]
    pub public_playlist: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub last_item_added_at: Option<String>,
}

/// OpenAPI v2 playlist response envelope
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPlaylistResponse {
    pub data: OpenApiPlaylistData,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPlaylistData {
    pub id: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: OpenApiPlaylistAttributes,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPlaylistAttributes {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub access_type: Option<String>,
    #[serde(default)]
    pub playlist_type: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub last_modified_at: Option<String>,
}

impl From<OpenApiPlaylistResponse> for TidalPlaylist {
    fn from(resp: OpenApiPlaylistResponse) -> Self {
        let d = resp.data;
        let a = d.attributes;
        TidalPlaylist {
            uuid: d.id,
            title: a.name,
            description: a.description,
            image: None,
            number_of_tracks: Some(0),
            number_of_videos: Some(0),
            creator: None,
            playlist_type: a.playlist_type,
            duration: Some(0),
            last_updated: a.last_modified_at,
            access_type: a.access_type,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalPlaylist {
    pub uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub number_of_tracks: Option<u32>,
    pub number_of_videos: Option<u32>,
    pub creator: Option<TidalPlaylistCreator>,
    /// "USER" | "EDITORIAL" | "ARTIST"
    #[serde(default)]
    pub playlist_type: Option<String>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub access_type: Option<String>,
}

impl From<TidalPlaylistRaw> for TidalPlaylist {
    fn from(raw: TidalPlaylistRaw) -> Self {
        TidalPlaylist {
            uuid: raw.uuid,
            title: raw.title,
            description: raw.description,
            // Prefer squareImage, fallback to image
            image: raw.square_image.or(raw.image),
            number_of_tracks: raw.number_of_tracks,
            number_of_videos: raw.number_of_videos,
            creator: raw.creator,
            playlist_type: raw.playlist_type,
            duration: raw.duration,
            last_updated: raw.last_updated,
            access_type: raw.public_playlist.map(|p| {
                if p {
                    "PUBLIC".to_string()
                } else {
                    "UNLISTED".to_string()
                }
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalLyrics {
    #[serde(default)]
    pub track_id: Option<u64>,
    #[serde(default)]
    pub lyrics_provider: Option<String>,
    #[serde(default)]
    pub provider_commontrack_id: Option<String>,
    #[serde(default)]
    pub provider_lyrics_id: Option<String>,
    #[serde(default)]
    pub lyrics: Option<String>,
    #[serde(default)]
    pub subtitles: Option<String>,
    #[serde(default)]
    pub is_right_to_left: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TidalContributor {
    pub name: String,
    #[serde(default)]
    pub id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TidalCredit {
    #[serde(rename(deserialize = "type", serialize = "creditType"))]
    pub credit_type: String,
    #[serde(default)]
    pub contributors: Vec<TidalContributor>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub url: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub bit_depth: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub audio_quality: Option<String>,
    /// "STEREO" | "DOLBY_ATMOS"
    #[serde(default)]
    pub audio_mode: Option<String>,
    /// "FULL" | "PREVIEW"
    #[serde(default)]
    pub asset_presentation: Option<String>,
    /// Raw MPD/DASH manifest XML when the stream is DASH.
    /// `None` for BTS (single-URL) streams.
    #[serde(default)]
    pub manifest: Option<String>,
    /// "application/dash+xml" | "application/vnd.tidal.bts"
    #[serde(default)]
    pub manifest_mime_type: Option<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,
    #[serde(default)]
    pub track_id: Option<u64>,
    #[serde(default)]
    pub album_replay_gain: Option<f64>,
    #[serde(default)]
    pub album_peak_amplitude: Option<f64>,
    #[serde(default)]
    pub track_replay_gain: Option<f64>,
    #[serde(default)]
    pub track_peak_amplitude: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoStreamInfo {
    pub url: String,
    pub video_quality: String,
    pub manifest_mime_type: String,
    pub video_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalVideo {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub vibrant_color: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default, rename = "type")]
    pub video_type: Option<String>,
    #[serde(default)]
    pub explicit: Option<bool>,
    #[serde(default)]
    pub ads_pre_paywall_only: Option<bool>,
    #[serde(default)]
    pub artist: Option<TidalArtist>,
    #[serde(default)]
    pub artists: Option<Vec<TidalArtist>>,
}

// ==================== Feed (activity notifications) ====================

/// Which entity a feed activity carries. The payload key in
/// `followableActivity` is the real discriminant; this is its typed form.
///
/// `rename_all` must live on the enum — a struct-level `rename_all` does not
/// rename enum variants, and the wire contract is lowercase.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedItemKind {
    Mix,
    Album,
    Unknown,
}

/// One flattened feed row. `item` is the raw payload, passed through untouched
/// so the frontend's existing item helpers can render and play it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    pub kind: FeedItemKind,
    pub activity_type: String,
    pub occurred_at: String,
    pub seen: bool,
    pub item: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedResponse {
    pub items: Vec<FeedItem>,
    pub unseen_count: u32,
}

#[derive(Debug, Deserialize)]
struct FeedActivitiesEnvelope {
    #[serde(default)]
    activities: Vec<FeedActivityEntry>,
    #[serde(default)]
    stats: Option<FeedStats>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedStats {
    #[serde(default)]
    total_not_seen_activities: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedActivityEntry {
    #[serde(default)]
    followable_activity: Option<serde_json::Value>,
    #[serde(default)]
    seen: bool,
}

/// Keys inside `followableActivity` that are metadata, not the payload.
const FEED_META_KEYS: [&str; 2] = ["activityType", "occurredAt"];

/// Flatten one `followableActivity` object into a `FeedItem`.
///
/// The object holds exactly one payload, keyed by content type
/// (`historyMix`, `album`, …). Returns `None` when no payload object is
/// present at all — such an entry has nothing to render.
pub fn flatten_feed_activity(seen: bool, activity: &serde_json::Value) -> Option<FeedItem> {
    let obj = activity.as_object()?;

    let activity_type = obj
        .get("activityType")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let occurred_at = obj
        .get("occurredAt")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let (key, payload) = obj
        .iter()
        .find(|(k, v)| !FEED_META_KEYS.contains(&k.as_str()) && v.is_object())?;

    let kind = match key.as_str() {
        "historyMix" => FeedItemKind::Mix,
        "album" => FeedItemKind::Album,
        other => {
            log::warn!(
                "[feed] unrecognized payload key '{}' (activityType={})",
                other,
                activity_type
            );
            FeedItemKind::Unknown
        }
    };

    Some(FeedItem {
        kind,
        activity_type,
        occurred_at,
        seen,
        item: payload.clone(),
    })
}

/// Parse a feed response body into flattened rows.
pub fn parse_feed_body(body: &str) -> Result<FeedResponse, serde_json::Error> {
    let envelope: FeedActivitiesEnvelope = serde_json::from_str(body)?;

    let items: Vec<FeedItem> = envelope
        .activities
        .iter()
        .filter_map(|entry| {
            entry
                .followable_activity
                .as_ref()
                .and_then(|a| flatten_feed_activity(entry.seen, a))
        })
        .collect();

    let unseen_count = envelope
        .stats
        .as_ref()
        .map(|s| s.total_not_seen_activities)
        .unwrap_or(0);

    Ok(FeedResponse {
        items,
        unseen_count,
    })
}

// ==================== v2 Home Feed MIX types ====================
// These structs document the v2 MIX shape. Not yet consumed by backend code
// (home feed items pass through as raw Value), but available for future typed parsing.

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MixTextInfo {
    #[serde(default)]
    pub color: Option<String>,
    pub text: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixImage {
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixImageRef {
    #[serde(default)]
    pub image_uuid: Option<String>,
    #[serde(default)]
    pub vibrant_color: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixArtistRef {
    #[serde(default)]
    pub artist_id: Option<u64>,
    #[serde(default)]
    pub artist_name: Option<String>,
    #[serde(default)]
    pub artist_image: Option<MixImageRef>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MixTrackRef {
    #[serde(default)]
    pub track_id: Option<u64>,
    #[serde(default)]
    pub track_title: Option<String>,
    #[serde(default)]
    pub track_group: Option<String>,
    #[serde(default)]
    pub track_image: Option<MixImageRef>,
}

/// v2 home feed MIX entity — completely unique shape from other Tidal entities.
/// Returned in home/feed sections with type "MIX".
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalMix {
    pub id: String,
    /// "TRACK_MIX" | "ARTIST_MIX" | "HISTORY_ALLTIME_MIX" | "HISTORY_MONTHLY_MIX" | "HISTORY_YEARLY_MIX"
    #[serde(default, rename = "type")]
    pub mix_type: Option<String>,
    #[serde(default)]
    pub title_text_info: Option<MixTextInfo>,
    #[serde(default)]
    pub subtitle_text_info: Option<MixTextInfo>,
    #[serde(default)]
    pub short_subtitle_text_info: Option<MixTextInfo>,
    #[serde(default)]
    pub description: Option<MixTextInfo>,
    #[serde(default)]
    pub mix_images: Option<Vec<MixImage>>,
    #[serde(default)]
    pub detail_mix_images: Option<Vec<MixImage>>,
    #[serde(default)]
    pub artist: Option<MixArtistRef>,
    #[serde(default)]
    pub track: Option<MixTrackRef>,
    #[serde(default)]
    pub content_behavior: Option<String>,
    #[serde(default)]
    pub country_code: Option<String>,
    #[serde(default)]
    pub is_stable_id: Option<bool>,
    #[serde(default)]
    pub sort_type: Option<String>,
    #[serde(default)]
    pub updated: Option<u64>,
    #[serde(default)]
    pub artifact_id_type: Option<String>,
}

// ==================== v2 Favorite Mixes types ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FavoriteMixImageUrl {
    pub url: String,
}

/// Shape returned by /v2/favorites/mixes — different from the home feed MIX entity.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalFavoriteMix {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sub_title: Option<String>,
    #[serde(default)]
    pub mix_type: Option<String>,
    #[serde(default)]
    pub images: Option<FavoriteMixImages>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub struct FavoriteMixImages {
    #[serde(default)]
    pub small: Option<FavoriteMixImageUrl>,
    #[serde(default)]
    pub medium: Option<FavoriteMixImageUrl>,
    #[serde(default)]
    pub large: Option<FavoriteMixImageUrl>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalSearchResults {
    pub artists: Vec<TidalArtist>,
    pub albums: Vec<TidalAlbumDetail>,
    pub tracks: Vec<TidalTrack>,
    pub playlists: Vec<TidalPlaylist>,
    #[serde(default)]
    pub videos: Vec<TidalVideo>,
    #[serde(default)]
    pub top_hit_type: Option<String>,
    /// Ordered top hits from the v2 search API (mixed entity types, ranked by relevance)
    #[serde(default)]
    pub top_hits: Vec<DirectHitItem>,
}

// ==================== Suggestions / Mini-search ====================

/// A single direct hit from the v2 /suggestions/ endpoint.
/// Each hit is a typed entity (artist, album, track, playlist) rendered in API order.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DirectHitItem {
    pub hit_type: String, // "ARTISTS", "ALBUMS", "TRACKS", "PLAYLISTS"
    // Common
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_album_cover_fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    // For tracks/albums: artist info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_name: Option<String>,
    // For tracks: album info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_tracks: Option<u32>,
    /// The complete track entity for TRACKS hits. The payload is a full track —
    /// `artists[]`, `explicit`, `album.vibrantColor`, `mediaMetadata`, `mixes` —
    /// so carry it whole rather than re-projecting it onto the flat fields above
    /// and losing everything they have no room for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TidalTrack>,
    /// The complete video entity for VIDEOS hits, for the same reason as `track`:
    /// the flat fields cannot express `explicit` or more than one artist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<TidalVideo>,
}

impl DirectHitItem {
    /// Parse a JSON item with { "type": "ARTISTS"|"ALBUMS"|..., "value": {...} } into a DirectHitItem.
    /// Returns None if the type is unrecognized or value is missing.
    pub fn from_typed_value(item: &serde_json::Value) -> Option<Self> {
        let hit_type = item
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let val = item.get("value")?;

        match hit_type.as_str() {
            "ARTISTS" => Some(DirectHitItem {
                hit_type,
                id: val.get("id").and_then(|v| v.as_u64()),
                uuid: None,
                name: val.get("name").and_then(|v| v.as_str()).map(String::from),
                title: None,
                picture: val
                    .get("picture")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                artwork_id: val
                    .get("artworkId")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                selected_album_cover_fallback: val
                    .get("selectedAlbumCoverFallback")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                cover: None,
                image: None,
                artist_name: None,
                album_id: None,
                album_title: None,
                album_cover: None,
                duration: None,
                number_of_tracks: None,
                track: None,
                video: None,
            }),
            "ALBUMS" => {
                let artist_name = val
                    .get("artists")
                    .and_then(|a| a.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    .or_else(|| {
                        val.get("artist")
                            .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    })
                    .map(String::from);
                Some(DirectHitItem {
                    hit_type,
                    id: val.get("id").and_then(|v| v.as_u64()),
                    uuid: None,
                    name: None,
                    title: val.get("title").and_then(|v| v.as_str()).map(String::from),
                    picture: None,
                    artwork_id: None,
                    selected_album_cover_fallback: None,
                    cover: val.get("cover").and_then(|v| v.as_str()).map(String::from),
                    image: None,
                    artist_name,
                    album_id: None,
                    album_title: None,
                    album_cover: None,
                    duration: val
                        .get("duration")
                        .and_then(|v| v.as_u64())
                        .map(|d| d as u32),
                    number_of_tracks: val
                        .get("numberOfTracks")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32),
                    track: None,
                    video: None,
                })
            }
            "TRACKS" => {
                let artist_name = val
                    .get("artists")
                    .and_then(|a| a.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    .or_else(|| {
                        val.get("artist")
                            .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    })
                    .map(String::from);
                let album = val.get("album");
                // Deserialize the whole entity; the flat fields below stay as a
                // fallback for a payload too partial to satisfy TidalTrack.
                let track = serde_json::from_value::<TidalTrack>(val.clone())
                    .ok()
                    .map(|mut t| {
                        t.backfill_artist();
                        t
                    });
                Some(DirectHitItem {
                    hit_type,
                    id: val.get("id").and_then(|v| v.as_u64()),
                    uuid: None,
                    name: None,
                    title: val.get("title").and_then(|v| v.as_str()).map(String::from),
                    picture: None,
                    artwork_id: None,
                    selected_album_cover_fallback: None,
                    cover: None,
                    image: None,
                    artist_name,
                    album_id: album.and_then(|a| a.get("id").and_then(|v| v.as_u64())),
                    album_title: album
                        .and_then(|a| a.get("title").and_then(|v| v.as_str()))
                        .map(String::from),
                    album_cover: album
                        .and_then(|a| a.get("cover").and_then(|v| v.as_str()))
                        .map(String::from),
                    duration: val
                        .get("duration")
                        .and_then(|v| v.as_u64())
                        .map(|d| d as u32),
                    number_of_tracks: None,
                    track,
                    video: None,
                })
            }
            "VIDEOS" => {
                let artist_name = val
                    .get("artists")
                    .and_then(|a| a.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    .or_else(|| {
                        val.get("artist")
                            .and_then(|a| a.get("name").and_then(|v| v.as_str()))
                    })
                    .map(String::from);
                // Deserialize the whole entity; the flat fields below stay as a
                // fallback for a payload too partial to satisfy TidalVideo.
                let video = serde_json::from_value::<TidalVideo>(val.clone()).ok();
                Some(DirectHitItem {
                    hit_type,
                    id: val.get("id").and_then(|v| v.as_u64()),
                    uuid: None,
                    name: None,
                    title: val.get("title").and_then(|v| v.as_str()).map(String::from),
                    picture: None,
                    artwork_id: None,
                    selected_album_cover_fallback: None,
                    cover: None,
                    // Videos carry a thumbnail UUID under `imageId`, not album cover.
                    image: val
                        .get("imageId")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    artist_name,
                    album_id: None,
                    album_title: None,
                    album_cover: None,
                    duration: val
                        .get("duration")
                        .and_then(|v| v.as_u64())
                        .map(|d| d as u32),
                    number_of_tracks: None,
                    track: None,
                    video,
                })
            }
            "PLAYLISTS" => Some(DirectHitItem {
                hit_type,
                id: None,
                uuid: val.get("uuid").and_then(|v| v.as_str()).map(String::from),
                name: None,
                title: val.get("title").and_then(|v| v.as_str()).map(String::from),
                picture: None,
                artwork_id: None,
                selected_album_cover_fallback: None,
                cover: None,
                image: val
                    .get("squareImage")
                    .and_then(|v| v.as_str())
                    .or_else(|| val.get("image").and_then(|v| v.as_str()))
                    .map(String::from),
                artist_name: None,
                album_id: None,
                album_title: None,
                album_cover: None,
                duration: None,
                number_of_tracks: val
                    .get("numberOfTracks")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
                track: None,
                video: None,
            }),
            _ => None,
        }
    }

    /// Parse an array of typed value items into Vec<DirectHitItem>, preserving order.
    pub fn parse_array(arr: &[serde_json::Value]) -> Vec<Self> {
        arr.iter().filter_map(Self::from_typed_value).collect()
    }
}

/// A text suggestion item (history or autocomplete).
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionTextItem {
    pub query: String,
    pub source: String, // "history" or "suggestion"
}

/// Full response from the suggestions endpoint, powering the mini-search dropdown.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionsResponse {
    pub text_suggestions: Vec<SuggestionTextItem>,
    pub direct_hits: Vec<DirectHitItem>,
}

// ==================== Home Page / Pages API ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistRole {
    pub category: String,
    pub category_id: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TidalArtistDetail {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub artwork_id: Option<String>,
    #[serde(default)]
    pub selected_album_cover_fallback: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub user_id: Option<u64>,
    #[serde(default)]
    pub popularity: Option<u32>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub spotlighted: Option<bool>,
    #[serde(default)]
    pub artist_types: Option<Vec<String>>,
    #[serde(default)]
    pub artist_roles: Option<Vec<TidalArtistRole>>,
    #[serde(default)]
    pub mixes: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomePageSection {
    pub title: String,
    pub section_type: String,
    pub items: Value,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub api_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomeTab {
    pub name: String,
    /// Raw tab type from the API, e.g. "STATIC", "EDITORIAL", "UPLOADS".
    /// The feed slug is `tab_type.to_lowercase()`.
    pub tab_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomePageResponse {
    #[serde(default)]
    pub tabs: Vec<HomeTab>,
    pub sections: Vec<HomePageSection>,
    pub cursor: Option<String>,
}

/// Who is signed in, from `/sessions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub user_id: u64,
    /// `None` when TIDAL leaves it out.
    pub country_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

// ==================== Profile ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileArtFile {
    pub href: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLink {
    pub href: String,
    pub link_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePlaylist {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub access_type: Option<String>,
    #[serde(default)]
    pub number_of_tracks: Option<u32>,
    #[serde(default)]
    pub cover_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub user_id: u64,
    #[serde(default)]
    pub artist_id: Option<u64>,
    pub name: String,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub bio_id: Option<String>,
    pub picture_files: Vec<ProfileArtFile>,
    #[serde(default)]
    pub artwork_id: Option<String>,
    #[serde(default)]
    pub blur_hash: Option<String>,
    pub palette: Vec<String>,
    pub external_links: Vec<ExternalLink>,
    #[serde(default)]
    pub fan_count: Option<u32>,
    pub public_playlists: Vec<ProfilePlaylist>,
}

/// Parsed pieces of the openapi `/artists/{id}` JSON:API response, before they
/// are merged into a `Profile`.
#[derive(Debug, Clone)]
pub struct ArtistProfileParts {
    pub name: String,
    pub handle: Option<String>,
    pub bio: Option<String>,
    pub bio_id: Option<String>,
    pub picture_files: Vec<ProfileArtFile>,
    pub artwork_id: Option<String>,
    pub blur_hash: Option<String>,
    pub palette: Vec<String>,
    pub external_links: Vec<ExternalLink>,
}

/// Find an entry in a JSON:API `included[]` array matching `type` + `id`.
fn resolve_included<'a>(included: &'a [Value], typ: &str, id: &str) -> Option<&'a Value> {
    included.iter().find(|e| {
        e.get("type").and_then(|t| t.as_str()) == Some(typ)
            && e.get("id").and_then(|i| i.as_str()) == Some(id)
    })
}

/// Pull `{href, meta:{width,height}}` art files out of an `artworks` included
/// object, sorted DESC by width.
fn art_files_from_artwork(artwork: &Value) -> Vec<ProfileArtFile> {
    let mut files: Vec<ProfileArtFile> = artwork
        .get("attributes")
        .and_then(|a| a.get("files"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    let href = f.get("href").and_then(|h| h.as_str())?.to_string();
                    let meta = f.get("meta");
                    let width = meta
                        .and_then(|m| m.get("width"))
                        .and_then(|w| w.as_u64())
                        .map(|w| w as u32);
                    let height = meta
                        .and_then(|m| m.get("height"))
                        .and_then(|h| h.as_u64())
                        .map(|h| h as u32);
                    Some(ProfileArtFile {
                        href,
                        width,
                        height,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|f| std::cmp::Reverse(f.width.unwrap_or(0)));
    files
}

pub(crate) fn parse_artist_profile(body: &str) -> Result<ArtistProfileParts, Error> {
    let json: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let data = json
        .get("data")
        .ok_or_else(|| Error::Parse("artist profile: missing data".into()))?;
    let attrs = data.get("attributes");

    let name = attrs
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_default()
        .to_string();
    let handle = attrs
        .and_then(|a| a.get("handle"))
        .and_then(|h| h.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let external_links = attrs
        .and_then(|a| a.get("externalLinks"))
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let href = item.get("href").and_then(|h| h.as_str())?.to_string();
                    let link_type = item
                        .get("meta")
                        .and_then(|m| m.get("type"))
                        .and_then(|t| t.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Some(ExternalLink { href, link_type })
                })
                .collect()
        })
        .unwrap_or_default();

    let empty: Vec<Value> = Vec::new();
    let included = json
        .get("included")
        .and_then(|i| i.as_array())
        .unwrap_or(&empty);

    let relationships = data.get("relationships");

    // profileArt is to-many: take data[0].
    let (picture_files, artwork_id, blur_hash, palette) = relationships
        .and_then(|r| r.get("profileArt"))
        .and_then(|p| p.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| {
            let id = first.get("id").and_then(|i| i.as_str())?;
            let artwork = resolve_included(included, "artworks", id)?;
            let aid = Some(id.to_string());
            let bh = artwork
                .get("attributes")
                .and_then(|a| a.get("blurHash"))
                .and_then(|b| b.as_str())
                .map(String::from);
            let pal = artwork
                .get("attributes")
                .and_then(|a| a.get("palette"))
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some((art_files_from_artwork(artwork), aid, bh, pal))
        })
        .unwrap_or((Vec::new(), None, None, Vec::new()));

    // biography is to-one.
    let (bio, bio_id) = relationships
        .and_then(|r| r.get("biography"))
        .and_then(|b| b.get("data"))
        .and_then(|d| {
            let id = d.get("id").and_then(|i| i.as_str())?;
            let bio_obj = resolve_included(included, "artistBiographies", id)?;
            let text = bio_obj
                .get("attributes")
                .and_then(|a| a.get("text"))
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            Some((text, Some(id.to_string())))
        })
        .unwrap_or((None, None));

    Ok(ArtistProfileParts {
        name,
        handle,
        bio,
        bio_id,
        picture_files,
        artwork_id,
        blur_hash,
        palette,
        external_links,
    })
}

/// Pick the file href closest to ~320px wide from an `artworks` included object.
fn cover_url_320(artwork: &Value) -> Option<String> {
    let files = art_files_from_artwork(artwork);
    files
        .iter()
        .min_by_key(|f| (f.width.unwrap_or(0) as i64 - 320).abs())
        .map(|f| f.href.clone())
}

pub(crate) fn parse_public_playlists(body: &str) -> Result<Vec<ProfilePlaylist>, Error> {
    let json: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let empty: Vec<Value> = Vec::new();
    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .unwrap_or(&empty);
    let included = json
        .get("included")
        .and_then(|i| i.as_array())
        .unwrap_or(&empty);

    let mut out = Vec::new();
    for pl in data {
        let attrs = pl.get("attributes");
        let access_type = attrs
            .and_then(|a| a.get("accessType"))
            .and_then(|t| t.as_str())
            .map(String::from);
        if access_type.as_deref() != Some("PUBLIC") {
            continue;
        }
        let id = pl
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or_default()
            .to_string();
        let title = attrs
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string();
        let number_of_tracks = attrs
            .and_then(|a| a.get("numberOfItems"))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32);
        let cover_url = pl
            .get("relationships")
            .and_then(|r| r.get("coverArt"))
            .and_then(|c| c.get("data"))
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("id").and_then(|i| i.as_str()))
            .and_then(|aid| resolve_included(included, "artworks", aid))
            .and_then(cover_url_320);
        out.push(ProfilePlaylist {
            id,
            title,
            access_type,
            number_of_tracks,
            cover_url,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use serde_json::json;

    fn artist_body_full() -> String {
        json!({
            "data": {
                "type": "artists",
                "id": "12345",
                "attributes": { "name": "Test Artist", "handle": "testartist" },
                "relationships": {
                    "profileArt": { "data": [{ "type": "artworks", "id": "art-1" }] },
                    "biography": { "data": { "type": "artistBiographies", "id": "bio-1" } },
                    "owners": { "data": [{ "type": "users", "id": "999" }] }
                }
            },
            "included": [
                {
                    "type": "artworks",
                    "id": "art-1",
                    "attributes": {
                        "blurHash": "L6Pj0^jE.AyE_3t7t7R**0o#DgR4",
                        "palette": ["#112233", "#445566"],
                        "files": [
                            { "href": "https://img/320.jpg", "meta": { "width": 320, "height": 320 } },
                            { "href": "https://img/1280.jpg", "meta": { "width": 1280, "height": 1280 } },
                            { "href": "https://img/640.jpg", "meta": { "width": 640, "height": 640 } }
                        ]
                    }
                },
                {
                    "type": "artistBiographies",
                    "id": "bio-1",
                    "attributes": { "text": "A short bio." }
                }
            ]
        })
        .to_string()
    }

    #[test]
    fn parse_artist_profile_full() {
        let parts = parse_artist_profile(&artist_body_full()).unwrap();
        assert_eq!(parts.name, "Test Artist");
        assert_eq!(parts.handle.as_deref(), Some("testartist"));
        assert_eq!(parts.bio.as_deref(), Some("A short bio."));
        assert_eq!(parts.bio_id.as_deref(), Some("bio-1"));
        assert_eq!(parts.artwork_id.as_deref(), Some("art-1"));
        assert_eq!(
            parts.blur_hash.as_deref(),
            Some("L6Pj0^jE.AyE_3t7t7R**0o#DgR4")
        );
        assert_eq!(parts.palette, vec!["#112233", "#445566"]);
        let widths: Vec<u32> = parts
            .picture_files
            .iter()
            .map(|f| f.width.unwrap())
            .collect();
        assert_eq!(widths, vec![1280, 640, 320]);
    }

    #[test]
    fn parse_artist_profile_no_bio_no_handle() {
        let body = json!({
            "data": {
                "type": "artists",
                "id": "12345",
                "attributes": { "name": "No Bio Artist" },
                "relationships": {
                    "profileArt": { "data": [] }
                }
            },
            "included": []
        })
        .to_string();
        let parts = parse_artist_profile(&body).unwrap();
        assert_eq!(parts.name, "No Bio Artist");
        assert_eq!(parts.handle, None);
        assert_eq!(parts.bio, None);
        assert_eq!(parts.bio_id, None);
        assert!(parts.picture_files.is_empty());
    }

    #[test]
    fn parse_public_playlists_filters_to_public() {
        let body = json!({
            "data": [
                {
                    "type": "playlists",
                    "id": "pub-uuid",
                    "attributes": { "name": "My Public Mix", "accessType": "PUBLIC", "numberOfItems": 17 },
                    "relationships": {
                        "coverArt": { "data": [{ "type": "artworks", "id": "cover-pub" }] }
                    }
                },
                {
                    "type": "playlists",
                    "id": "unlisted-uuid",
                    "attributes": { "name": "Secret", "accessType": "UNLISTED", "numberOfItems": 3 },
                    "relationships": {
                        "coverArt": { "data": [{ "type": "artworks", "id": "cover-unl" }] }
                    }
                }
            ],
            "included": [
                {
                    "type": "artworks",
                    "id": "cover-pub",
                    "attributes": {
                        "files": [
                            { "href": "https://cov/160.jpg", "meta": { "width": 160 } },
                            { "href": "https://cov/320.jpg", "meta": { "width": 320 } },
                            { "href": "https://cov/750.jpg", "meta": { "width": 750 } }
                        ]
                    }
                }
            ]
        })
        .to_string();
        let playlists = parse_public_playlists(&body).unwrap();
        assert_eq!(playlists.len(), 1);
        let p = &playlists[0];
        assert_eq!(p.id, "pub-uuid");
        assert_eq!(p.title, "My Public Mix");
        assert_eq!(p.access_type.as_deref(), Some("PUBLIC"));
        assert_eq!(p.number_of_tracks, Some(17));
        assert_eq!(p.cover_url.as_deref(), Some("https://cov/320.jpg"));
    }

    #[test]
    fn resolve_included_matches_type_and_id() {
        let included = vec![
            json!({ "type": "artworks", "id": "a1", "attributes": {} }),
            json!({ "type": "artistBiographies", "id": "b1", "attributes": {} }),
        ];
        let found = resolve_included(&included, "artistBiographies", "b1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().get("id").unwrap(), "b1");
        assert!(resolve_included(&included, "artworks", "missing").is_none());
    }

    #[test]
    fn parse_artist_profile_reads_external_links() {
        let body = json!({
            "data": {
                "type": "artists",
                "id": "12345",
                "attributes": {
                    "name": "Linked Artist",
                    "externalLinks": [
                        { "href": "https://instagram.com/me", "meta": { "type": "INSTAGRAM" } },
                        { "href": "https://me.com", "meta": { "type": "OFFICIAL_HOMEPAGE" } }
                    ]
                },
                "relationships": { "profileArt": { "data": [] } }
            },
            "included": []
        })
        .to_string();
        let parts = parse_artist_profile(&body).unwrap();
        assert_eq!(parts.external_links.len(), 2);
        assert_eq!(parts.external_links[0].href, "https://instagram.com/me");
        assert_eq!(parts.external_links[0].link_type, "INSTAGRAM");
        assert_eq!(parts.external_links[1].link_type, "OFFICIAL_HOMEPAGE");
    }
}

#[cfg(test)]
mod sub_status_tests {
    use super::{is_playbackinfo_sub_status, is_terminal_sub_status};

    #[test]
    fn terminal_codes_are_terminal() {
        for code in [4005u64, 4010, 4030, 4031, 4032, 4034, 4035] {
            let body = format!(r#"{{"status":401,"subStatus":{}}}"#, code);
            assert!(is_terminal_sub_status(&body), "{} should be terminal", code);
            assert!(is_playbackinfo_sub_status(&body));
        }
    }

    #[test]
    fn retryable_playbackinfo_codes_are_not_terminal() {
        // 4006 = privileges lost, 4033 = subscription up-sell. Both recover.
        for code in [4006u64, 4033] {
            let body = format!(r#"{{"status":401,"subStatus":{}}}"#, code);
            assert!(
                !is_terminal_sub_status(&body),
                "{} must not be terminal",
                code
            );
            // …but still must NOT trigger a token refresh.
            assert!(is_playbackinfo_sub_status(&body));
        }
    }

    #[test]
    fn auth_sub_statuses_and_junk_are_neither() {
        for body in [
            r#"{"status":401,"subStatus":11003}"#,
            r#"{"status":401,"subStatus":6001}"#,
            r#"{"subStatus":"4005"}"#, // string-typed, not a number
            "",
            "not json",
        ] {
            assert!(!is_playbackinfo_sub_status(body), "{body}");
            assert!(!is_terminal_sub_status(body), "{body}");
        }
    }

    #[test]
    fn float_encoded_sub_status_is_terminal() {
        // serde_json reads 4005.0 as f64, so as_u64() alone says None while the
        // frontend's `typeof sub === "number"` accepts it. Keep the two agreeing.
        let body = r#"{"status":401,"subStatus":4005.0}"#;
        assert!(is_terminal_sub_status(body));
        assert!(is_playbackinfo_sub_status(body));
    }
}

#[cfg(test)]
mod direct_hit_tests {
    use super::*;

    /// A real `directHits` entry captured from the suggestions endpoint. The
    /// value is a complete track entity, which is why the flat projection alone
    /// silently dropped the second artist, the explicit flag and the album's
    /// vibrant color.
    fn tv_off_hit() -> serde_json::Value {
        serde_json::json!({
            "type": "TRACKS",
            "value": {
                "id": 401317294,
                "title": "tv off",
                "duration": 221,
                "explicit": true,
                "artists": [
                    { "id": 3816041, "name": "Kendrick Lamar", "type": "MAIN",
                      "picture": "84d81b7a-a12e-4a3e-bda4-d0527cb1c8cf" },
                    { "id": 40179705, "name": "Lefty Gunplay", "type": "FEATURED",
                      "picture": null }
                ],
                "album": {
                    "id": 401317276,
                    "title": "GNX",
                    "cover": "faef7f4f-e362-484b-a46b-4e633c2a1ca3",
                    "vibrantColor": "#FFFFFF",
                    "releaseDate": "2024-11-22"
                },
                "audioQuality": "LOSSLESS",
                "mediaMetadata": { "tags": ["LOSSLESS", "HIRES_LOSSLESS"] },
                "mixes": { "TRACK_MIX": "001d13d399e86e948d03c21bcd15db" },
                "replayGain": -7.92,
                "peak": 0.959098,
                "trackNumber": 7,
                "volumeNumber": 1,
                "isrc": "USUG12408493"
            }
        })
    }

    #[test]
    fn track_hit_carries_every_artist() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        let artists = hit
            .track
            .as_ref()
            .and_then(|t| t.artists.as_ref())
            .expect("full track must carry artists[]");
        let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Kendrick Lamar", "Lefty Gunplay"]);
        assert_eq!(artists[1].artist_type.as_deref(), Some("FEATURED"));
    }

    #[test]
    fn track_hit_carries_explicit_flag() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        assert_eq!(hit.track.and_then(|t| t.explicit), Some(true));
    }

    #[test]
    fn track_hit_carries_album_vibrant_color() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        let album = hit
            .track
            .and_then(|t| t.album)
            .expect("full track must carry its album");
        assert_eq!(album.vibrant_color.as_deref(), Some("#FFFFFF"));
    }

    /// The payload has no singular `artist`, so it must be backfilled the same
    /// way every other track parse path does it.
    #[test]
    fn track_hit_backfills_the_singular_artist() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        let artist = hit
            .track
            .and_then(|t| t.artist)
            .expect("artist must be backfilled from artists[0]");
        assert_eq!(artist.name, "Kendrick Lamar");
    }

    /// The flat fields stay populated so the frontend fallback keeps working.
    #[test]
    fn track_hit_still_populates_the_flat_projection() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        assert_eq!(hit.artist_name.as_deref(), Some("Kendrick Lamar"));
        assert_eq!(hit.album_id, Some(401317276));
        assert_eq!(hit.album_title.as_deref(), Some("GNX"));
        assert_eq!(hit.duration, Some(221));
    }

    /// A value too partial to satisfy TidalTrack must not abort the hit — the
    /// flat projection is the fallback.
    #[test]
    fn partial_track_value_falls_back_to_the_flat_projection() {
        let partial = serde_json::json!({
            "type": "TRACKS",
            "value": { "id": 1, "title": "No Duration Here" }
        });
        let hit = DirectHitItem::from_typed_value(&partial).expect("hit must still parse");
        assert!(hit.track.is_none(), "TidalTrack needs a duration");
        assert_eq!(hit.title.as_deref(), Some("No Duration Here"));
    }

    /// The wire contract src/types.ts DirectHitItem.track relies on: the
    /// serialized hit must actually expose the three fields the flat projection
    /// dropped, under the camelCase names the frontend reads.
    #[test]
    fn serialized_hit_exposes_the_recovered_fields_to_the_frontend() {
        let hit = DirectHitItem::from_typed_value(&tv_off_hit()).expect("TRACKS hit must parse");
        let wire: serde_json::Value =
            serde_json::to_value(&hit).expect("hit must serialize for the frontend");

        let track = &wire["track"];
        assert_eq!(track["explicit"], serde_json::json!(true));
        assert_eq!(track["album"]["vibrantColor"], serde_json::json!("#FFFFFF"));
        let artists = track["artists"]
            .as_array()
            .expect("artists[] must survive to the wire");
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[1]["name"], serde_json::json!("Lefty Gunplay"));
        // TidalArtist renames artist_type to `type`, so the wire carries `type`
        // (not `artistType`) — matching every other track path in the app.
        assert_eq!(artists[1]["type"], serde_json::json!("FEATURED"));
    }

    /// Non-track hits must not pay for a `track` key on the wire.
    #[test]
    fn serialized_non_track_hit_omits_the_track_key() {
        let album = serde_json::json!({
            "type": "ALBUMS",
            "value": { "id": 401317276, "title": "GNX", "cover": "faef7f4f" }
        });
        let hit = DirectHitItem::from_typed_value(&album).expect("ALBUMS hit must parse");
        let wire: serde_json::Value = serde_json::to_value(&hit).expect("must serialize");
        assert!(wire.get("track").is_none());
    }

    /// Shape recorded in docs/superpowers/plans/2026-07-13-video-search.md. Only
    /// `id`/`title` are required by TidalVideo, so a leaner payload still parses.
    fn video_hit(extra: serde_json::Value) -> serde_json::Value {
        let mut value = serde_json::json!({
            "id": 12345678,
            "title": "Not Like Us",
            "duration": 274,
            "imageId": "aabbccdd-1122-3344-5566-778899aabbcc",
            "artists": [
                { "id": 3816041, "name": "Kendrick Lamar", "type": "MAIN" },
                { "id": 40179705, "name": "Someone Else", "type": "FEATURED" }
            ]
        });
        if let (Some(base), Some(more)) = (value.as_object_mut(), extra.as_object()) {
            for (k, v) in more {
                base.insert(k.clone(), v.clone());
            }
        }
        serde_json::json!({ "type": "VIDEOS", "value": value })
    }

    #[test]
    fn video_hit_carries_every_artist() {
        let hit = DirectHitItem::from_typed_value(&video_hit(serde_json::json!({})))
            .expect("VIDEOS hit must parse");
        let artists = hit
            .video
            .as_ref()
            .and_then(|v| v.artists.as_ref())
            .expect("full video must carry artists[]");
        let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Kendrick Lamar", "Someone Else"]);
    }

    /// The plan doc lists `artists` but not `explicit` for VIDEOS top-hits, so
    /// both cases must behave: present means carried, absent means simply None.
    #[test]
    fn video_hit_carries_explicit_when_the_payload_has_it() {
        let hit = DirectHitItem::from_typed_value(&video_hit(serde_json::json!({
            "explicit": true
        })))
        .expect("VIDEOS hit must parse");
        assert_eq!(hit.video.and_then(|v| v.explicit), Some(true));
    }

    #[test]
    fn video_hit_without_explicit_still_parses() {
        let hit = DirectHitItem::from_typed_value(&video_hit(serde_json::json!({})))
            .expect("VIDEOS hit must parse");
        let video = hit.video.expect("video entity must still be carried");
        assert!(video.explicit.is_none());
        assert_eq!(video.title, "Not Like Us");
    }

    #[test]
    fn video_hit_still_populates_the_flat_projection() {
        let hit = DirectHitItem::from_typed_value(&video_hit(serde_json::json!({})))
            .expect("VIDEOS hit must parse");
        assert_eq!(hit.hit_type, "VIDEOS");
        assert_eq!(hit.artist_name.as_deref(), Some("Kendrick Lamar"));
        assert_eq!(
            hit.image.as_deref(),
            Some("aabbccdd-1122-3344-5566-778899aabbcc")
        );
        assert_eq!(hit.duration, Some(274));
        assert!(hit.track.is_none(), "a video is not a track");
    }

    /// The wire contract src/types.ts DirectHitItem.video relies on.
    #[test]
    fn serialized_video_hit_exposes_artists_and_explicit() {
        let hit = DirectHitItem::from_typed_value(&video_hit(serde_json::json!({
            "explicit": true
        })))
        .expect("VIDEOS hit must parse");
        let wire: serde_json::Value = serde_json::to_value(&hit).expect("must serialize");
        let video = &wire["video"];
        assert_eq!(video["explicit"], serde_json::json!(true));
        assert_eq!(
            video["imageId"],
            serde_json::json!("aabbccdd-1122-3344-5566-778899aabbcc")
        );
        let artists = video["artists"].as_array().expect("artists[] on the wire");
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[1]["name"], serde_json::json!("Someone Else"));
        assert!(wire.get("track").is_none(), "a video carries no track key");
    }

    /// A value too partial to satisfy TidalVideo must not abort the hit.
    #[test]
    fn partial_video_value_falls_back_to_the_flat_projection() {
        let partial = serde_json::json!({
            "type": "VIDEOS",
            "value": { "title": "No Id Here", "artists": [{ "id": 1, "name": "A" }] }
        });
        let hit = DirectHitItem::from_typed_value(&partial).expect("hit must still parse");
        assert!(hit.video.is_none(), "TidalVideo needs an id");
        assert_eq!(hit.artist_name.as_deref(), Some("A"));
    }

    /// Non-track hit types have no track entity to carry.
    #[test]
    fn non_track_hits_carry_no_track() {
        let album = serde_json::json!({
            "type": "ALBUMS",
            "value": { "id": 401317276, "title": "GNX", "cover": "faef7f4f" }
        });
        let hit = DirectHitItem::from_typed_value(&album).expect("ALBUMS hit must parse");
        assert!(hit.track.is_none());
    }
}

#[cfg(test)]
mod feed_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn feed_item_kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&FeedItemKind::Mix).unwrap(),
            "\"mix\""
        );
        assert_eq!(
            serde_json::to_string(&FeedItemKind::Album).unwrap(),
            "\"album\""
        );
        assert_eq!(
            serde_json::to_string(&FeedItemKind::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    #[test]
    fn flattens_history_mix_activity() {
        let activity = json!({
            "historyMix": {
                "id": "0011112222333344445555666677",
                "mixType": "HISTORY_MONTHLY_MIX",
                "title": "July 2026",
                "subTitle": "Some Artist and more",
                "images": {
                    "MEDIUM": { "width": 533, "height": 533, "url": "https://example.invalid/a.jpg" }
                }
            },
            "activityType": "NEW_HISTORY_MIX",
            "occurredAt": "2026-08-01T00:00:00.000Z"
        });

        let item = flatten_feed_activity(true, &activity).expect("should flatten");

        assert!(matches!(item.kind, FeedItemKind::Mix));
        assert_eq!(item.activity_type, "NEW_HISTORY_MIX");
        assert_eq!(item.occurred_at, "2026-08-01T00:00:00.000Z");
        assert!(item.seen);
        assert_eq!(item.item["mixType"], "HISTORY_MONTHLY_MIX");
        assert_eq!(
            item.item["images"]["MEDIUM"]["url"],
            "https://example.invalid/a.jpg"
        );
    }

    #[test]
    fn flattens_album_activity() {
        let activity = json!({
            "album": {
                "id": 1234,
                "title": "Some Single",
                "type": "SINGLE",
                "cover": "00000000-1111-2222-3333-444444444444",
                "artists": [ { "id": 9, "name": "Some Artist" } ]
            },
            "activityType": "NEW_ALBUM_RELEASE",
            "occurredAt": "2026-06-05T00:00:00.000Z"
        });

        let item = flatten_feed_activity(false, &activity).expect("should flatten");

        assert!(matches!(item.kind, FeedItemKind::Album));
        assert!(!item.seen);
        assert_eq!(item.item["id"], 1234);
        assert_eq!(item.item["artists"][0]["name"], "Some Artist");
    }

    #[test]
    fn unrecognized_payload_key_becomes_unknown() {
        let activity = json!({
            "somethingNew": { "id": 7, "title": "Mystery" },
            "activityType": "NEW_MYSTERY_THING",
            "occurredAt": "2026-01-01T00:00:00.000Z"
        });

        let item = flatten_feed_activity(true, &activity).expect("should still flatten");

        assert!(matches!(item.kind, FeedItemKind::Unknown));
        assert_eq!(item.item["title"], "Mystery");
    }

    #[test]
    fn activity_without_payload_object_is_dropped() {
        let activity = json!({
            "activityType": "NEW_NOTHING",
            "occurredAt": "2026-01-01T00:00:00.000Z"
        });

        assert!(flatten_feed_activity(true, &activity).is_none());
    }

    #[test]
    fn parses_envelope_with_both_kinds_and_unseen_count() {
        let body = json!({
            "activities": [
                {
                    "followableActivity": {
                        "historyMix": { "id": "abc", "mixType": "HISTORY_MONTHLY_MIX" },
                        "activityType": "NEW_HISTORY_MIX",
                        "occurredAt": "2026-08-01T00:00:00.000Z"
                    },
                    "seen": false
                },
                {
                    "followableActivity": {
                        "album": { "id": 1, "title": "T" },
                        "activityType": "NEW_ALBUM_RELEASE",
                        "occurredAt": "2026-06-05T00:00:00.000Z"
                    },
                    "seen": true
                }
            ],
            "stats": { "totalNotSeenActivities": 3 }
        })
        .to_string();

        let feed = parse_feed_body(&body).expect("should parse");

        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.unseen_count, 3);
        assert!(matches!(feed.items[0].kind, FeedItemKind::Mix));
        assert!(!feed.items[0].seen);
        assert!(matches!(feed.items[1].kind, FeedItemKind::Album));
    }

    #[test]
    fn missing_stats_yields_zero_unseen() {
        let body = json!({ "activities": [] }).to_string();
        let feed = parse_feed_body(&body).expect("should parse");
        assert_eq!(feed.unseen_count, 0);
        assert!(feed.items.is_empty());
    }
}
