//! The `playback_session` event, from sone: its body, its headers, the SQS
//! batch form they ride in, and what TIDAL's answer means.

use base64::Engine;
use serde_json::{Value, json};

use crate::Source;

pub const EC_URL: &str = "https://ec.tidal.com/api/event-batch";
/// Identity of the TIDAL Android client whose `cid` syzygy authenticates
/// with. Events ride on that client's token, so they must describe that
/// client, not syzygy. TIDAL ships roughly weekly, so this pin drifts; bump
/// it occasionally.
const TIDAL_APP_VERSION: &str = "2.205.0";
const OS_NAME: &str = "Android";
const OS_VERSION: &str = "35";
const DEVICE_MODEL: &str = "Pixel 7";
const DEVICE_VENDOR: &str = "Google";
/// Max events per SQS SendMessageBatch.
pub const MAX_BATCH: usize = 10;

/// What the play was started from: the primary Recently Played attribution.
/// Values mirror the web player's `entityType.toUpperCase()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Album,
    Playlist,
    Artist,
    Mix,
    /// A track played on its own, attributed to itself.
    Item,
    /// The user's Loved tracks.
    MyItems,
}

impl SourceType {
    fn as_tidal(self) -> &'static str {
        match self {
            SourceType::Album => "ALBUM",
            SourceType::Playlist => "PLAYLIST",
            SourceType::Artist => "ARTIST",
            SourceType::Mix => "MIX",
            SourceType::Item => "ITEM",
            SourceType::MyItems => "MY_ITEMS",
        }
    }
}

/// What TIDAL attributes a play of `track_id` from `source` to. Containers
/// keep their id. ITEM carries the track's own id: TIDAL accepts any id
/// there but surfaces only that one. Loved tracks use the web player's
/// fixed id.
pub fn resolve_source(source: &Source, track_id: u64) -> (SourceType, String) {
    match source {
        Source::Album(id) => (SourceType::Album, id.to_string()),
        Source::Playlist(uuid) => (SourceType::Playlist, uuid.clone()),
        Source::Mix(id) => (SourceType::Mix, id.clone()),
        Source::Artist(id) => (SourceType::Artist, id.to_string()),
        Source::LovedTracks => (SourceType::MyItems, "MY_TRACKS".to_string()),
        Source::Item => (SourceType::Item, track_id.to_string()),
    }
}

/// JWT claims needed to attribute an event to the account.
#[derive(Debug, Default, Clone)]
pub struct Claims {
    pub uid: Option<u64>,
    pub cid: Option<u64>,
    pub sid: Option<String>,
}

/// Decode the middle JWT segment (base64url). No signature verification.
pub fn parse_claims(access_token: &str) -> Claims {
    let Some(seg) = access_token.split('.').nth(1) else {
        return Claims::default();
    };
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(seg)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(seg));
    let Ok(bytes) = decoded else {
        return Claims::default();
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return Claims::default();
    };
    let as_u64 = |k: &str| match v.get(k) {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    };
    Claims {
        uid: as_u64("uid"),
        cid: as_u64("cid"),
        sid: v.get("sid").and_then(|x| x.as_str()).map(String::from),
    }
}

/// The facts needed to build one `playback_session` event.
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub requested_product_id: u64,
    pub actual_product_id: String,
    pub quality: String,
    pub audio_mode: String,
    pub presentation: String,
    pub source: (SourceType, String),
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub end_asset_pos: f64,
}

/// Build the MessageBody JSON (mobile shape) for one event.
pub fn build_body(ev: &SessionEvent, claims: &Claims) -> String {
    let (source_type, source_id) = &ev.source;
    let payload = json!({
        "playbackSessionId": ev.session_id,
        "isPostPaywall": true,
        "productType": "TRACK",
        "requestedProductId": ev.requested_product_id.to_string(),
        "actualProductId": ev.actual_product_id,
        "actualAssetPresentation": ev.presentation,
        "actualAudioMode": ev.audio_mode,
        "actualQuality": ev.quality,
        "startTimestamp": ev.start_ts_ms,
        "endTimestamp": ev.end_ts_ms,
        "startAssetPosition": 0.0,
        "endAssetPosition": ev.end_asset_pos,
        // Interruptions only (pause/resume/seek). A straight play sends [];
        // session bounds live in start/endTimestamp and start/endAssetPosition.
        "actions": [],
        "sourceType": source_type.as_tidal(),
        "sourceId": source_id,
    });

    let body = json!({
        "group": "play_log",
        "version": 2,
        "ts": ev.end_ts_ms,
        "uuid": uuid::Uuid::new_v4().to_string(),
        // client.token is the `cid` claim as a string (per TIDAL's Android SDK).
        "user": {
            "id": claims.uid,
            "clientId": claims.cid,
            "sessionId": claims.sid,
        },
        "client": {
            "token": claims.cid.map(|c| c.to_string()).unwrap_or_default(),
            "deviceType": "mobile",
            "version": TIDAL_APP_VERSION,
            "platform": "android",
        },
        "payload": payload,
    });
    body.to_string()
}

/// The per-event `Headers` MessageAttribute (JSON string). Key set mirrors
/// TIDAL's `HeadersUtils.kt`.
pub fn build_headers(oauth_client_id: &str, access_token: &str, now_ms: i64) -> String {
    json!({
        "client-id": oauth_client_id,
        "app-version": TIDAL_APP_VERSION,
        "os-name": OS_NAME,
        "os-version": OS_VERSION,
        "device-model": DEVICE_MODEL,
        "device-vendor": DEVICE_VENDOR,
        "consent-category": "NECESSARY",
        "requested-sent-timestamp": now_ms.to_string(),
        "authorization": access_token,
    })
    .to_string()
}

/// Encode events as an SQS `SendMessageBatch` form body. `events` is a slice
/// of (body_json, headers_json); at most `MAX_BATCH` per call.
pub fn sqs_form(events: &[(String, String)]) -> Vec<(String, String)> {
    let mut form = Vec::with_capacity(events.len() * 8);
    for (i, (body, headers)) in events.iter().enumerate() {
        let n = i + 1;
        let p = |suffix: &str| format!("SendMessageBatchRequestEntry.{n}.{suffix}");
        form.push((p("Id"), uuid::Uuid::new_v4().to_string()));
        form.push((p("MessageBody"), body.clone()));
        form.push((p("MessageAttribute.1.Name"), "Name".into()));
        form.push((
            p("MessageAttribute.1.Value.StringValue"),
            "playback_session".into(),
        ));
        form.push((p("MessageAttribute.1.Value.DataType"), "String".into()));
        form.push((p("MessageAttribute.2.Name"), "Headers".into()));
        form.push((p("MessageAttribute.2.Value.StringValue"), headers.clone()));
        form.push((p("MessageAttribute.2.Value.DataType"), "String".into()));
    }
    form
}

/// Outcome of a batch POST, from the HTTP status and SQS XML body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// All events accepted.
    Accepted,
    /// Auth rejected: refresh and retry once, then keep them for later.
    AuthFailed,
    /// TIDAL failed (5xx): keep them for a later send, one attempt nearer
    /// being dropped.
    Retryable,
    /// TIDAL wasn't reached (offline, timed out, signed out): keep them for
    /// a later send. Plays made offline still count, so this isn't an
    /// attempt.
    Unreachable,
    /// Malformed events (SenderFault): drop them, never resend.
    SenderFault,
}

pub fn classify(status: reqwest::StatusCode, body: &str) -> SendOutcome {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return SendOutcome::AuthFailed;
    }
    if status.is_server_error() {
        return SendOutcome::Retryable;
    }
    if !status.is_success() {
        // Other 4xx: treated as permanent to avoid retry storms.
        return SendOutcome::SenderFault;
    }
    if body.contains("<BatchResultErrorEntry") {
        SendOutcome::SenderFault
    } else {
        SendOutcome::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn sample(source: (SourceType, String)) -> SessionEvent {
        SessionEvent {
            session_id: "sess-123".into(),
            requested_product_id: 42,
            actual_product_id: "42".into(),
            quality: "LOSSLESS".into(),
            audio_mode: "STEREO".into(),
            presentation: "FULL".into(),
            source,
            start_ts_ms: 1_000,
            end_ts_ms: 201_000,
            end_asset_pos: 200.0,
        }
    }

    fn item() -> (SourceType, String) {
        (SourceType::Item, "42".into())
    }

    fn claims() -> Claims {
        Claims {
            uid: Some(1),
            cid: Some(8017),
            sid: Some("sid-x".into()),
        }
    }

    fn body(ev: &SessionEvent) -> Value {
        serde_json::from_str(&build_body(ev, &claims())).unwrap()
    }

    // The mobile shape (user + client objects) is what surfaces in Recently
    // Played; guard against a regression to the web shape.
    #[test]
    fn body_is_mobile_shape() {
        let v = body(&sample(item()));
        assert_eq!(v["group"], "play_log");
        assert_eq!(v["version"], 2);
        assert!(v.get("user").is_some(), "user object required");
        assert!(v.get("client").is_some(), "client object required");
        // client.token is the cid claim as a string.
        assert_eq!(v["client"]["token"], "8017");
        assert_eq!(v["client"]["platform"], "android");
        assert_eq!(v["client"]["deviceType"], "mobile");
        assert_eq!(v["user"]["id"], 1);
        assert_eq!(v["client"]["version"], "2.205.0");
        assert_eq!(v["payload"]["playbackSessionId"], "sess-123");
        // Event.kt marks `name` @Transient: it rides as the SQS attribute only.
        assert!(v.get("name").is_none(), "body must not carry a name key");
        // The SDK appends actions only for interruptions; a straight play sends [].
        assert_eq!(v["payload"]["actions"].as_array().unwrap().len(), 0);
    }

    // HeadersUtils.kt's exact key set. It sends no app-name, so emitting one
    // is a key no real client produces.
    #[test]
    fn headers_match_sdk_key_set() {
        let h: Value = serde_json::from_str(&build_headers("cid-x", "tok-y", 123)).unwrap();
        assert!(h.get("app-name").is_none(), "SDK sends no app-name");
        assert_eq!(h.as_object().unwrap().len(), 9, "exactly nine header keys");
        assert_eq!(h["app-version"], "2.205.0");
        assert_eq!(h["os-name"], "Android");
        assert_eq!(h["os-version"], "35");
        assert_eq!(h["device-model"], "Pixel 7");
        assert_eq!(h["device-vendor"], "Google");
        assert_eq!(h["client-id"], "cid-x");
        assert_eq!(h["consent-category"], "NECESSARY");
        assert_eq!(h["requested-sent-timestamp"], "123");
        // Bare token here; the Bearer prefix lives on the HTTP header.
        assert_eq!(h["authorization"], "tok-y");
    }

    #[test]
    fn container_source_is_mapped() {
        let v = body(&sample((SourceType::Playlist, "pl-9".into())));
        assert_eq!(v["payload"]["sourceType"], "PLAYLIST");
        assert_eq!(v["payload"]["sourceId"], "pl-9");
    }

    // Live-verified 2026-09-12: the web player reports a track played outside
    // any container as ITEM, and TIDAL surfaces it as a Recently played TRACK
    // row only when sourceId is the track's own id (a search query does not).
    #[test]
    fn a_track_outside_a_container_carries_its_own_id() {
        let source = resolve_source(&Source::Item, 401317294);
        assert_eq!(source, (SourceType::Item, "401317294".to_string()));
        let v = body(&sample(resolve_source(&Source::Item, 42)));
        assert_eq!(v["payload"]["sourceType"], "ITEM");
        assert_eq!(v["payload"]["sourceId"], "42");
    }

    // Loved tracks mirror the web player's My Tracks context, which surfaces
    // as the "My Tracks" shortcut in Recently played.
    #[test]
    fn loved_tracks_map_to_my_items() {
        let source = resolve_source(&Source::LovedTracks, 7);
        assert_eq!(source, (SourceType::MyItems, "MY_TRACKS".to_string()));
        let v = body(&sample(source));
        assert_eq!(v["payload"]["sourceType"], "MY_ITEMS");
    }

    // Real containers keep their own id.
    #[test]
    fn container_sources_keep_their_id() {
        let cases = [
            (Source::Album(1765476), SourceType::Album, "1765476"),
            (Source::Playlist("pl".into()), SourceType::Playlist, "pl"),
            (Source::Mix("mix".into()), SourceType::Mix, "mix"),
            (Source::Artist(9), SourceType::Artist, "9"),
        ];
        for (source, kind, id) in cases {
            assert_eq!(resolve_source(&source, 42), (kind, id.to_string()));
        }
    }

    #[test]
    fn classify_outcomes() {
        use reqwest::StatusCode;
        let ok = "<SendMessageBatchResponse><SendMessageBatchResultEntry><Id>x</Id></SendMessageBatchResultEntry></SendMessageBatchResponse>";
        assert_eq!(classify(StatusCode::OK, ok), SendOutcome::Accepted);
        assert_eq!(
            classify(
                StatusCode::OK,
                "<BatchResultErrorEntry><SenderFault>true</SenderFault></BatchResultErrorEntry>"
            ),
            SendOutcome::SenderFault
        );
        assert_eq!(
            classify(StatusCode::UNAUTHORIZED, ""),
            SendOutcome::AuthFailed
        );
        assert_eq!(
            classify(StatusCode::INTERNAL_SERVER_ERROR, ""),
            SendOutcome::Retryable
        );
        assert_eq!(
            classify(StatusCode::BAD_REQUEST, ""),
            SendOutcome::SenderFault
        );
    }

    #[test]
    fn parse_claims_reads_uid_cid_sid() {
        // {"uid":173234555,"cid":8017,"sid":"abc"} as base64url, unsigned JWT.
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"uid":173234555,"cid":8017,"sid":"abc"}"#);
        let token = format!("h.{payload}.s");
        let c = parse_claims(&token);
        assert_eq!(c.uid, Some(173234555));
        assert_eq!(c.cid, Some(8017));
        assert_eq!(c.sid.as_deref(), Some("abc"));
    }

    // Nothing in a sent event may identify syzygy.
    #[test]
    fn event_carries_no_syzygy_fingerprint() {
        let headers = build_headers("cid-x", "tok-y", 123);
        let body = build_body(&sample((SourceType::Album, "7".into())), &claims());
        for payload in [&headers, &body] {
            assert!(
                !payload.to_lowercase().contains("syzygy"),
                "leaked app name: {payload}"
            );
            assert!(
                !payload.contains(env!("CARGO_PKG_VERSION")),
                "leaked syzygy version: {payload}"
            );
        }
    }
}
