//! Stream resolution: the quality cascade and the URI the audio engine plays.

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize};

use crate::Error;
use crate::client::TidalClient;
use crate::models::StreamInfo;

/// A TIDAL audio quality tier, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Quality {
    #[default]
    HiResLossless,
    HiRes,
    Lossless,
    High,
}

impl Quality {
    const ORDER: [Quality; 4] = [
        Quality::HiResLossless,
        Quality::HiRes,
        Quality::Lossless,
        Quality::High,
    ];

    /// TIDAL's name for the tier.
    pub fn as_str(self) -> &'static str {
        match self {
            Quality::HiResLossless => "HI_RES_LOSSLESS",
            Quality::HiRes => "HI_RES",
            Quality::Lossless => "LOSSLESS",
            Quality::High => "HIGH",
        }
    }

    /// Parse TIDAL's name. An unknown name is treated as the maximum.
    pub fn from_name(name: &str) -> Self {
        Self::ORDER
            .into_iter()
            .find(|q| q.as_str() == name)
            .unwrap_or_default()
    }

    fn is_hi_res(self) -> bool {
        matches!(self, Quality::HiResLossless | Quality::HiRes)
    }
}

/// Lenient: an unknown name becomes the maximum rather than failing the
/// whole document it sits in.
impl<'de> Deserialize<'de> for Quality {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(Self::from_name(&name))
    }
}

/// A resolved stream, ready for the audio engine.
#[derive(Debug, Clone)]
pub struct PlayableStream {
    /// The track that was asked for.
    pub track_id: u64,
    /// A direct URL (BTS), or the DASH manifest as a `data:` URI.
    pub uri: String,
    pub is_dash: bool,
    /// What TIDAL actually served: quality, codec, format and gains.
    pub info: StreamInfo,
}

/// Tiers to try, highest to lowest, under the user's `ceiling`. The two Hi-Res
/// tiers need a client secret and are dropped without one. Always ends in
/// `High`, so it is never empty.
fn quality_tiers(ceiling: Quality, has_secret: bool) -> Vec<Quality> {
    Quality::ORDER
        .into_iter()
        .skip_while(|&q| q != ceiling)
        .filter(|&q| has_secret || !q.is_hi_res())
        .collect()
}

impl TidalClient {
    /// Resolve `track_id` to something playable, trying each tier from
    /// `max_quality` down until one answers.
    pub async fn resolve_stream(
        &self,
        track_id: u64,
        max_quality: Quality,
    ) -> Result<PlayableStream, Error> {
        let has_secret = !self.login_method().credentials()?.client_secret.is_empty();
        let mut last_err = None;
        let mut info = None;
        for tier in quality_tiers(max_quality, has_secret) {
            match self.get_stream_url(track_id, tier).await {
                Ok(found) => {
                    info = Some(found);
                    break;
                }
                // A network failure, a rate limit or a terminal answer won't
                // change at a lower tier — over-requesting quality returns 200
                // with a downgraded audioQuality, never an error.
                Err(e) if e.is_network() || e.is_rate_limited() || e.is_terminal_unplayable() => {
                    return Err(e);
                }
                // The session is gone; lower tiers would fail the same way.
                Err(e @ (Error::SessionExpired | Error::NotAuthenticated)) => return Err(e),
                Err(e) => last_err = Some(e),
            }
        }
        let info = match info {
            Some(info) => info,
            // `quality_tiers` always yields at least `High`.
            None => return Err(last_err.expect("quality_tiers always yields High")),
        };

        log::debug!(
            "[resolve_stream]: track_id={} — quality={:?}, bitDepth={:?}, sampleRate={:?}, codec={:?}, dash={}",
            track_id,
            info.audio_quality,
            info.bit_depth,
            info.sample_rate,
            info.codec,
            info.manifest.is_some()
        );

        let is_dash = info.manifest.is_some();
        let uri = match &info.manifest {
            // DASH: the MPD manifest as a data URI for GStreamer's dashdemux.
            Some(mpd) => format!(
                "data:application/dash+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(mpd.as_bytes())
            ),
            None => info.url.clone(),
        };

        Ok(PlayableStream {
            track_id,
            uri,
            is_dash,
            info,
        })
    }

    async fn get_stream_url(&self, track_id: u64, quality: Quality) -> Result<StreamInfo, Error> {
        let cc = self.country_code();
        let body = self
            .api_get_body(
                &format!("/tracks/{}/playbackinfopostpaywall", track_id),
                &[
                    ("countryCode", &cc),
                    ("audioquality", quality.as_str()),
                    ("playbackmode", "STREAM"),
                    ("assetpresentation", "FULL"),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PlaybackInfo {
            manifest_mime_type: String,
            manifest: String,
            #[serde(default)]
            audio_quality: Option<String>,
            #[serde(default)]
            bit_depth: Option<u32>,
            #[serde(default)]
            sample_rate: Option<u32>,
            #[serde(default)]
            album_replay_gain: Option<f64>,
            #[serde(default)]
            album_peak_amplitude: Option<f64>,
            #[serde(default)]
            track_replay_gain: Option<f64>,
            #[serde(default)]
            track_peak_amplitude: Option<f64>,
        }

        let data = serde_json::from_str::<PlaybackInfo>(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        let manifest_bytes = base64::engine::general_purpose::STANDARD
            .decode(&data.manifest)
            .map_err(|e| Error::Parse(format!("Failed to decode manifest: {}", e)))?;
        let manifest_str = String::from_utf8(manifest_bytes)
            .map_err(|e| Error::Parse(format!("Invalid manifest encoding: {}", e)))?;

        let mut codec: Option<String> = None;

        // Handle BTS format (JSON with urls array)
        let url = if data.manifest_mime_type.contains("vnd.tidal.bts") {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            #[allow(dead_code)]
            struct BtsManifest {
                urls: Vec<String>,
                codecs: Option<String>,
                mime_type: Option<String>,
                encryption_type: Option<String>,
            }

            let manifest_data = serde_json::from_str::<BtsManifest>(&manifest_str)
                .map_err(|e| Error::Parse(format!("{} - Manifest: {}", e, manifest_str)))?;

            codec = manifest_data
                .codecs
                .map(|c| c.to_uppercase().split('.').next().unwrap_or("").to_string());

            manifest_data
                .urls
                .into_iter()
                .next()
                .ok_or(Error::Parse("No URL in BTS manifest".into()))?
        }
        // Handle DASH/MPD format — return raw manifest for GStreamer
        else if data.manifest_mime_type.contains("dash+xml") {
            // Extract codec from manifest
            if let Some(codecs_start) = manifest_str.find("codecs=\"") {
                let start = codecs_start + 8;
                if let Some(codecs_end) = manifest_str[start..].find("\"") {
                    let raw = &manifest_str[start..start + codecs_end];
                    codec = Some(if raw.contains("flac") {
                        "FLAC".to_string()
                    } else {
                        raw.to_uppercase()
                    });
                }
            }

            return Ok(StreamInfo {
                url: String::new(),
                codec,
                bit_depth: data.bit_depth,
                sample_rate: data.sample_rate,
                audio_quality: data.audio_quality.clone(),
                audio_mode: None,
                asset_presentation: None,
                manifest: Some(manifest_str),
                manifest_mime_type: None,
                manifest_hash: None,
                track_id: None,
                album_replay_gain: data.album_replay_gain,
                album_peak_amplitude: data.album_peak_amplitude,
                track_replay_gain: data.track_replay_gain,
                track_peak_amplitude: data.track_peak_amplitude,
            });
        }
        // JSON fallback
        else {
            #[derive(Deserialize)]
            struct JsonManifest {
                urls: Option<Vec<String>>,
            }

            if let Ok(manifest_data) = serde_json::from_str::<JsonManifest>(&manifest_str) {
                if let Some(urls) = manifest_data.urls {
                    if let Some(u) = urls.into_iter().next() {
                        u
                    } else {
                        return Err(Error::Parse("Empty URL list in manifest".into()));
                    }
                } else {
                    return Err(Error::Parse("No urls in JSON manifest".into()));
                }
            } else {
                return Err(Error::Parse(format!(
                    "Unknown manifest format '{}': {}",
                    data.manifest_mime_type,
                    &manifest_str[..manifest_str.len().min(300)]
                )));
            }
        };

        Ok(StreamInfo {
            url,
            codec,
            bit_depth: data.bit_depth,
            sample_rate: data.sample_rate,
            audio_quality: data.audio_quality.clone(),
            audio_mode: None,
            asset_presentation: None,
            manifest: None,
            manifest_mime_type: None,
            manifest_hash: None,
            track_id: None,
            album_replay_gain: data.album_replay_gain,
            album_peak_amplitude: data.album_peak_amplitude,
            track_replay_gain: data.track_replay_gain,
            track_peak_amplitude: data.track_peak_amplitude,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Quality::{self, *};
    use super::quality_tiers;

    #[test]
    fn ceiling_max_with_secret_is_full_cascade() {
        assert_eq!(
            quality_tiers(HiResLossless, true),
            vec![HiResLossless, HiRes, Lossless, High]
        );
    }

    #[test]
    fn ceiling_max_without_secret_drops_hires() {
        assert_eq!(quality_tiers(HiResLossless, false), vec![Lossless, High]);
    }

    #[test]
    fn ceiling_lossless_caps_below_hires() {
        assert_eq!(quality_tiers(Lossless, true), vec![Lossless, High]);
        assert_eq!(quality_tiers(Lossless, false), vec![Lossless, High]);
    }

    #[test]
    fn ceiling_high_is_only_high() {
        assert_eq!(quality_tiers(High, true), vec![High]);
        assert_eq!(quality_tiers(High, false), vec![High]);
    }

    #[test]
    fn unknown_ceiling_falls_back_to_max() {
        assert_eq!(Quality::from_name("GARBAGE"), HiResLossless);
        let parsed: Quality = serde_json::from_str(r#""GARBAGE""#).unwrap();
        assert_eq!(
            quality_tiers(parsed, true),
            vec![HiResLossless, HiRes, Lossless, High]
        );
    }

    #[test]
    fn names_round_trip() {
        for q in [HiResLossless, HiRes, Lossless, High] {
            let json = serde_json::to_string(&q).unwrap();
            assert_eq!(json, format!("\"{}\"", q.as_str()));
            assert_eq!(serde_json::from_str::<Quality>(&json).unwrap(), q);
        }
    }

    #[test]
    fn always_includes_high_so_never_empty() {
        for ceiling in [HiResLossless, HiRes, Lossless, High] {
            for has_secret in [true, false] {
                let tiers = quality_tiers(ceiling, has_secret);
                assert!(
                    tiers.contains(&High),
                    "ceiling={ceiling:?} secret={has_secret}"
                );
            }
        }
    }
}
