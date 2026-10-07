use serde::Deserialize;
use serde_json::Value;

use super::*;
use crate::models::*;

impl TidalClient {
    pub async fn get_user_profile(&self, user_id: u64) -> Result<(String, Option<String>), Error> {
        let cc = self.country_code();
        let body = self
            .api_get_body(&format!("/users/{}", user_id), &[("countryCode", &cc)])
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UserProfile {
            #[serde(default)]
            first_name: Option<String>,
            #[serde(default)]
            last_name: Option<String>,
            #[serde(default)]
            username: Option<String>,
            #[serde(default)]
            profile_name: Option<String>,
        }

        let data: UserProfile =
            serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
        let username = data.username.clone();
        let name = data
            .profile_name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| match (&data.first_name, &data.last_name) {
                (Some(f), Some(l)) if !f.is_empty() => Some(format!("{} {}", f, l)),
                (Some(f), _) if !f.is_empty() => Some(f.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "TIDAL User".to_string());
        Ok((name, username))
    }

    /// Who is signed in, and where. Also sets the country for later calls.
    pub async fn get_session_info(&self) -> Result<SessionInfo, Error> {
        let body = self.api_get_body("/sessions", &[]).await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SessionResponse {
            user_id: u64,
            #[serde(default)]
            country_code: Option<String>,
        }

        let data: SessionResponse =
            serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;

        // Store the user's country code for all subsequent API calls
        if let Some(cc) = &data.country_code {
            self.set_country_code(cc.clone());
        }
        Ok(SessionInfo {
            user_id: data.user_id,
            country_code: data.country_code.filter(|cc| !cc.is_empty()),
        })
    }

    pub async fn get_user_playlists(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<PaginatedResponse<TidalPlaylist>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/playlists", user_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PlaylistResponse {
            items: Vec<TidalPlaylistRaw>,
            total_number_of_items: u32,
        }

        let data: PlaylistResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        let playlists: Vec<TidalPlaylist> = data.items.into_iter().map(|p| p.into()).collect();
        Ok(PaginatedResponse {
            items: playlists,
            total_number_of_items: data.total_number_of_items,
            offset,
            limit,
        })
    }

    /// Fetch a flat list of ALL user playlists (v1 endpoint, ignores folder nesting).
    pub async fn get_all_playlists(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
    ) -> Result<PaginatedResponse<TidalPlaylist>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/playlists", user_id),
                &[
                    ("offset", &offset_str),
                    ("limit", &limit_str),
                    ("order", order),
                    ("orderDirection", order_direction),
                    ("countryCode", &cc),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Resp {
            items: Vec<TidalPlaylistRaw>,
            total_number_of_items: u32,
        }

        let data: Resp = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))?;
        let playlists: Vec<TidalPlaylist> = data.items.into_iter().map(|p| p.into()).collect();
        Ok(PaginatedResponse {
            items: playlists,
            total_number_of_items: data.total_number_of_items,
            offset,
            limit,
        })
    }

    pub async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        access_type: &str,
    ) -> Result<TidalPlaylist, Error> {
        let body = serde_json::json!({
            "data": {
                "type": "playlists",
                "attributes": {
                    "name": title,
                    "description": description,
                    "accessType": access_type
                }
            }
        });

        log::debug!(
            "[create_playlist]: url={}/playlists, body={}",
            TIDAL_OPENAPI_URL,
            body
        );

        let client = self.http();
        let request = client
            .post(format!("{}/playlists", TIDAL_OPENAPI_URL))
            .query(&[("countryCode", self.country_code().as_str())])
            .json(&body);
        let response = self.send_authed(request).await?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        log::debug!(
            "[create_playlist]: status={}, response={}",
            status,
            &body_text[..body_text.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body: body_text,
            });
        }

        let resp = serde_json::from_str::<OpenApiPlaylistResponse>(&body_text)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body_text)))?;

        Ok(resp.into())
    }

    pub async fn update_playlist(
        &self,
        playlist_id: &str,
        title: &str,
        description: &str,
        access_type: &str,
    ) -> Result<TidalPlaylist, Error> {
        let body = serde_json::json!({
            "data": {
                "id": playlist_id,
                "type": "playlists",
                "attributes": {
                    "name": title,
                    "description": description,
                    "accessType": access_type
                }
            }
        });

        log::debug!(
            "[update_playlist]: url={}/playlists/{}, body={}",
            TIDAL_OPENAPI_URL,
            playlist_id,
            body
        );

        let client = self.http();
        let request = client
            .patch(format!("{}/playlists/{}", TIDAL_OPENAPI_URL, playlist_id))
            .header("Content-Type", "application/vnd.api+json")
            .query(&[("countryCode", self.country_code().as_str())])
            .json(&body);
        let response = self.send_authed(request).await?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        log::debug!(
            "[update_playlist]: status={}, response={}",
            status,
            &body_text[..body_text.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body: body_text,
            });
        }

        // 204 No Content — update succeeded but no body returned
        if body_text.is_empty() {
            return Ok(TidalPlaylist {
                uuid: playlist_id.to_string(),
                title: title.to_string(),
                description: Some(description.to_string()),
                image: None,
                number_of_tracks: None,
                number_of_videos: None,
                creator: None,
                playlist_type: Some("USER".to_string()),
                duration: None,
                last_updated: None,
                access_type: Some(access_type.to_string()),
            });
        }

        let resp = serde_json::from_str::<OpenApiPlaylistResponse>(&body_text)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body_text)))?;

        Ok(resp.into())
    }

    pub async fn add_track_to_playlist(
        &self,
        playlist_id: &str,
        track_id: u64,
    ) -> Result<(), Error> {
        // First, get the playlist ETag which is required for modifications
        let client = self.http();
        let req = client
            .get(format!("{}/playlists/{}", TIDAL_API_URL, playlist_id))
            .query(&[("countryCode", self.country_code().as_str())]);
        let head_response = self.send_authed(req).await?;

        let etag = head_response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*")
            .to_string();

        // Add the track
        let client = self.http();
        let req = client
            .post(format!("{}/playlists/{}/items", TIDAL_API_URL, playlist_id))
            .header("If-None-Match", &etag)
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[
                ("trackIds", &track_id.to_string()),
                ("onDupes", &"FAIL".to_string()),
                ("onArtifactNotFound", &"FAIL".to_string()),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_track_from_playlist(
        &self,
        playlist_id: &str,
        index: u32,
    ) -> Result<(), Error> {
        // First, get the playlist ETag which is required for modifications
        let client = self.http();
        let req = client
            .get(format!("{}/playlists/{}", TIDAL_API_URL, playlist_id))
            .query(&[("countryCode", self.country_code().as_str())]);
        let head_response = self.send_authed(req).await?;

        let etag = head_response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*")
            .to_string();

        // Remove the track at the given index
        let client = self.http();
        let req = client
            .delete(format!(
                "{}/playlists/{}/items/{}",
                TIDAL_API_URL, playlist_id, index
            ))
            .header("If-None-Match", &etag)
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<(), Error> {
        // First, get the playlist ETag which is required for modifications
        let client = self.http();
        let req = client
            .get(format!("{}/playlists/{}", TIDAL_API_URL, playlist_id))
            .query(&[("countryCode", self.country_code().as_str())]);
        let head_response = self.send_authed(req).await?;

        let etag = head_response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*")
            .to_string();

        // Delete the playlist
        let client = self.http();
        let req = client
            .delete(format!("{}/playlists/{}", TIDAL_API_URL, playlist_id))
            .header("If-None-Match", &etag)
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn get_favorite_playlist_uuids(&self, user_id: u64) -> Result<Vec<String>, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/playlists",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavItem {
            item: TidalPlaylistRaw,
        }
        #[derive(Deserialize)]
        struct FavResponse {
            #[serde(default)]
            items: Vec<FavItem>,
        }

        let data =
            serde_json::from_str::<FavResponse>(&body).map_err(|e| Error::Parse(e.to_string()))?;

        Ok(data.items.into_iter().map(|f| f.item.uuid).collect())
    }

    pub async fn get_favorite_playlists(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<PaginatedResponse<TidalPlaylist>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/favorites/playlists", user_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        struct FavEntry {
            item: TidalPlaylistRaw,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavResponse {
            items: Vec<FavEntry>,
            total_number_of_items: u32,
        }

        let data: FavResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        let playlists: Vec<TidalPlaylist> = data.items.into_iter().map(|e| e.item.into()).collect();
        Ok(PaginatedResponse {
            items: playlists,
            total_number_of_items: data.total_number_of_items,
            offset,
            limit,
        })
    }

    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<TidalTrack>, Error> {
        // `/items` (not `/tracks`) returns the real entries: tracks AND videos,
        // each wrapped as `{ "item": {...}, "type": "track" | "video" }`.
        let path = format!("/playlists/{}/items", playlist_id);

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItemsResponse {
            items: Vec<Value>,
            total_number_of_items: u32,
        }

        let mut all_tracks: Vec<TidalTrack> = Vec::new();
        let mut offset: u32 = 0;
        let page_size: u32 = 100;

        loop {
            let cc = self.country_code();
            let offset_str = offset.to_string();
            let limit_str = page_size.to_string();
            let body = self
                .api_get_body(
                    &path,
                    &[
                        ("countryCode", &cc),
                        ("limit", &limit_str),
                        ("offset", &offset_str),
                    ],
                )
                .await?;

            let data: ItemsResponse = serde_json::from_str(&body)
                .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;

            let fetched = data.items.len() as u32;
            let mut tracks = parse_playlist_items(data.items)?;
            all_tracks.append(&mut tracks);

            if fetched == 0 || all_tracks.len() as u32 >= data.total_number_of_items {
                break;
            }
            offset += fetched;
        }

        Ok(all_tracks)
    }

    pub async fn get_playlist_tracks_page(
        &self,
        playlist_id: &str,
        offset: u32,
        limit: u32,
        order: Option<&str>,
        order_direction: Option<&str>,
    ) -> Result<PaginatedTracks, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("countryCode", &cc),
            ("limit", &limit_str),
            ("offset", &offset_str),
        ];
        if let Some(o) = order {
            params.push(("order", o));
        }
        if let Some(od) = order_direction {
            params.push(("orderDirection", od));
        }
        // `/items` returns tracks AND videos, each wrapped as `{ item, type }`.
        let body = self
            .api_get_body(&format!("/playlists/{}/items", playlist_id), &params)
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItemsResponse {
            items: Vec<Value>,
            total_number_of_items: u32,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            limit: u32,
        }

        let data: ItemsResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        let items = parse_playlist_items(data.items)?;
        Ok(PaginatedTracks {
            items,
            total_number_of_items: data.total_number_of_items,
            offset: data.offset,
            limit: data.limit,
        })
    }

    /// Fetch playlist recommendations. The API wraps each track in `{ item, type }`.
    pub async fn get_playlist_recommendations(
        &self,
        playlist_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<PaginatedTracks, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/playlists/{}/recommendations/items", playlist_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        struct WrappedItem {
            item: TidalTrack,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RecommendationsResponse {
            #[serde(default)]
            items: Vec<WrappedItem>,
            #[serde(default)]
            total_number_of_items: u32,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            limit: u32,
        }

        let data: RecommendationsResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))?;

        let mut tracks: Vec<TidalTrack> = data.items.into_iter().map(|w| w.item).collect();
        for t in &mut tracks {
            t.backfill_artist();
        }

        Ok(PaginatedTracks {
            items: tracks,
            total_number_of_items: data.total_number_of_items,
            offset: data.offset,
            limit: data.limit,
        })
    }

    pub async fn get_album_detail(&self, album_id: u64) -> Result<TidalAlbumDetail, Error> {
        let cc = self.country_code();
        self.api_get(&format!("/albums/{}", album_id), &[("countryCode", &cc)])
            .await
    }

    pub async fn get_album_tracks(
        &self,
        album_id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<PaginatedTracks, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/albums/{}/tracks", album_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct AlbumTracksResponse {
            items: Vec<TidalTrack>,
            total_number_of_items: u32,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            limit: u32,
        }

        let mut data: AlbumTracksResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        for t in &mut data.items {
            t.backfill_artist();
        }
        Ok(PaginatedTracks {
            items: data.items,
            total_number_of_items: data.total_number_of_items,
            offset: data.offset,
            limit: data.limit,
        })
    }

    pub async fn get_favorite_tracks(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
    ) -> Result<PaginatedTracks, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/favorites/tracks", user_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                    ("order", order),
                    ("orderDirection", order_direction),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        struct FavoriteTrackItem {
            item: TidalTrack,
            created: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavoriteTracksResponse {
            items: Vec<FavoriteTrackItem>,
            total_number_of_items: u32,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            limit: u32,
        }

        let data: FavoriteTracksResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        Ok(PaginatedTracks {
            items: data
                .items
                .into_iter()
                .map(|f| {
                    let mut t = f.item;
                    t.backfill_artist();
                    t.date_added = Some(f.created);
                    t
                })
                .collect(),
            total_number_of_items: data.total_number_of_items,
            offset: data.offset,
            limit: data.limit,
        })
    }

    pub async fn is_track_favorited(&self, user_id: u64, track_id: u64) -> Result<bool, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/tracks",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavoriteTrackItem {
            #[serde(default)]
            id: Option<u64>,
            #[serde(default)]
            item: Option<TidalTrack>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavoriteTracksResponse {
            #[serde(default)]
            items: Vec<FavoriteTrackItem>,
        }

        let data = serde_json::from_str::<FavoriteTracksResponse>(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;

        Ok(data.items.iter().any(|entry| {
            entry.id == Some(track_id)
                || entry
                    .item
                    .as_ref()
                    .is_some_and(|track| track.id == track_id)
        }))
    }

    pub async fn get_favorite_track_ids(&self, user_id: u64) -> Result<Vec<u64>, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/tracks",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavItem {
            item: TidalTrack,
        }

        #[derive(Deserialize)]
        struct FavResponse {
            #[serde(default)]
            items: Vec<FavItem>,
        }

        let data =
            serde_json::from_str::<FavResponse>(&body).map_err(|e| Error::Parse(e.to_string()))?;

        Ok(data.items.into_iter().map(|f| f.item.id).collect())
    }

    pub async fn add_favorite_track(&self, user_id: u64, track_id: u64) -> Result<(), Error> {
        let track_id_str = track_id.to_string();

        let client = self.http();
        let req = client
            .post(format!(
                "{}/users/{}/favorites/tracks",
                TIDAL_API_URL, user_id
            ))
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[("trackId", track_id_str.as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_favorite_track(&self, user_id: u64, track_id: u64) -> Result<(), Error> {
        let client = self.http();
        let req = client
            .delete(format!(
                "{}/users/{}/favorites/tracks/{}",
                TIDAL_API_URL, user_id, track_id
            ))
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn is_album_favorited(&self, user_id: u64, album_id: u64) -> Result<bool, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/albums",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavoriteAlbumItem {
            #[serde(default)]
            id: Option<u64>,
            #[serde(default)]
            item: Option<TidalAlbum>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavoriteAlbumsResponse {
            #[serde(default)]
            items: Vec<FavoriteAlbumItem>,
        }

        let data = serde_json::from_str::<FavoriteAlbumsResponse>(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;

        Ok(data.items.iter().any(|entry| {
            entry.id == Some(album_id)
                || entry
                    .item
                    .as_ref()
                    .is_some_and(|album| album.id == album_id)
        }))
    }

    pub async fn get_favorite_album_ids(&self, user_id: u64) -> Result<Vec<u64>, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/albums",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavAlbumItem {
            #[serde(default)]
            item: Option<TidalAlbum>,
        }

        #[derive(Deserialize)]
        struct FavAlbumResponse {
            #[serde(default)]
            items: Vec<FavAlbumItem>,
        }

        let data = serde_json::from_str::<FavAlbumResponse>(&body)
            .map_err(|e| Error::Parse(e.to_string()))?;

        Ok(data
            .items
            .into_iter()
            .filter_map(|f| f.item.map(|a| a.id))
            .collect())
    }

    pub async fn add_favorite_album(&self, user_id: u64, album_id: u64) -> Result<(), Error> {
        let album_id_str = album_id.to_string();

        let client = self.http();
        let req = client
            .post(format!(
                "{}/users/{}/favorites/albums",
                TIDAL_API_URL, user_id
            ))
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[("albumId", album_id_str.as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_favorite_album(&self, user_id: u64, album_id: u64) -> Result<(), Error> {
        let client = self.http();
        let req = client
            .delete(format!(
                "{}/users/{}/favorites/albums/{}",
                TIDAL_API_URL, user_id, album_id
            ))
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn add_favorite_playlist(
        &self,
        user_id: u64,
        playlist_uuid: &str,
    ) -> Result<(), Error> {
        let client = self.http();
        let req = client
            .post(format!(
                "{}/users/{}/favorites/playlists",
                TIDAL_API_URL, user_id
            ))
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[("uuid", playlist_uuid)]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_favorite_playlist(
        &self,
        user_id: u64,
        playlist_uuid: &str,
    ) -> Result<(), Error> {
        let client = self.http();
        let req = client
            .delete(format!(
                "{}/users/{}/favorites/playlists/{}",
                TIDAL_API_URL, user_id, playlist_uuid
            ))
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn get_favorite_artist_ids(&self, user_id: u64) -> Result<Vec<u64>, Error> {
        let client = self.http();
        let req = client
            .get(format!(
                "{}/users/{}/favorites/artists",
                TIDAL_API_URL, user_id
            ))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("limit", "2000"),
                ("offset", "0"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        #[derive(Deserialize)]
        struct FavItem {
            item: TidalArtistDetail,
        }
        #[derive(Deserialize)]
        struct FavResponse {
            #[serde(default)]
            items: Vec<FavItem>,
        }

        let data =
            serde_json::from_str::<FavResponse>(&body).map_err(|e| Error::Parse(e.to_string()))?;

        Ok(data.items.into_iter().map(|f| f.item.id).collect())
    }

    pub async fn get_all_favorite_ids(&self, user_id: u64) -> Result<AllFavoriteIds, Error> {
        let client = self.http();
        let req = client
            .get(format!("{}/users/{}/favorites/ids", TIDAL_API_URL, user_id))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("locale", "en_US"),
                ("deviceType", "BROWSER"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let raw: std::collections::HashMap<String, Vec<String>> =
            serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;

        let parse_u64s = |key: &str| -> Vec<u64> {
            raw.get(key)
                .map(|v| v.iter().filter_map(|s| s.parse::<u64>().ok()).collect())
                .unwrap_or_default()
        };

        Ok(AllFavoriteIds {
            tracks: parse_u64s("TRACK"),
            albums: parse_u64s("ALBUM"),
            artists: parse_u64s("ARTIST"),
            playlists: raw.get("PLAYLIST").cloned().unwrap_or_default(),
        })
    }

    pub async fn add_favorite_artist(&self, user_id: u64, artist_id: u64) -> Result<(), Error> {
        let artist_id_str = artist_id.to_string();

        let client = self.http();
        let req = client
            .post(format!(
                "{}/users/{}/favorites/artists",
                TIDAL_API_URL, user_id
            ))
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[("artistId", artist_id_str.as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_favorite_artist(&self, user_id: u64, artist_id: u64) -> Result<(), Error> {
        let client = self.http();
        let req = client
            .delete(format!(
                "{}/users/{}/favorites/artists/{}",
                TIDAL_API_URL, user_id, artist_id
            ))
            .query(&[("countryCode", self.country_code().as_str())]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn add_favorite_mix(&self, mix_id: &str) -> Result<(), Error> {
        log::debug!("[add_favorite_mix]: mix_id={}", mix_id);

        let client = self.http();
        let req = client
            .put(format!("{}/favorites/mixes/add", TIDAL_API_V2_URL))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("mixIds", mix_id),
                ("onArtifactNotFound", "FAIL"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[add_favorite_mix]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn remove_favorite_mix(&self, mix_id: &str) -> Result<(), Error> {
        log::debug!("[remove_favorite_mix]: mix_id={}", mix_id);

        let client = self.http();
        let req = client
            .put(format!("{}/favorites/mixes/remove", TIDAL_API_V2_URL))
            .query(&[
                ("countryCode", self.country_code().as_str()),
                ("mixIds", mix_id),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[remove_favorite_mix]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    /// Fetch favorite mix IDs from api.tidal.com/v2/favorites/mixes.
    pub async fn get_favorite_mix_ids(&self) -> Result<Vec<String>, Error> {
        let response = self.get_favorite_mixes(0, 50, "DATE", "DESC").await?;
        let ids: Vec<String> = response.items.iter().map(|m| m.id.clone()).collect();
        log::debug!("[get_favorite_mix_ids]: found {} mix IDs", ids.len());
        Ok(ids)
    }

    /// Fetch full favorite mix objects from api.tidal.com/v2/favorites/mixes.
    pub async fn get_favorite_mixes(
        &self,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
    ) -> Result<PaginatedResponse<TidalFavoriteMix>, Error> {
        let url = format!("{}/favorites/mixes", TIDAL_API_V2_URL);
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &url,
                &[
                    ("countryCode", &cc),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                    ("order", order),
                    ("orderDirection", order_direction),
                ],
            )
            .await?;

        log::debug!(
            "[get_favorite_mixes]: body_preview={}",
            &body[..body.len().min(500)]
        );

        // v2 response is a wrapper object { items: [...] }; extract the inner array as raw Values first
        let raw_items: Vec<serde_json::Value> =
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                arr
            } else if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&body) {
                obj.get("items")
                    .or_else(|| obj.get("data"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
            } else {
                log::warn!(
                    "[get_favorite_mixes]: parse failed - body: {}",
                    &body[..body.len().min(500)]
                );
                Vec::new()
            };

        // Deserialize each item, skipping any that fail to parse
        let items: Vec<TidalFavoriteMix> = raw_items
            .into_iter()
            .filter_map(|v| serde_json::from_value::<TidalFavoriteMix>(v).ok())
            .collect();

        let count = items.len() as u32;
        log::debug!("[get_favorite_mixes]: found {} mixes", count);
        // v2 API doesn't return totalNumberOfItems — this is a synthetic sentinel for hasMore logic only, not a displayable count
        let estimated_total = if count == limit {
            offset + count + 1
        } else {
            offset + count
        };
        Ok(PaginatedResponse {
            items,
            total_number_of_items: estimated_total,
            offset,
            limit,
        })
    }

    /// Fetch playlist folders from v2/my-collection/playlists/folders.
    /// Returns raw JSON so we can capture the full response shape.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_playlist_folders(
        &self,
        folder_id: &str,
        include_only: &str,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
        cursor: &str,
    ) -> Result<serde_json::Value, Error> {
        let url = format!("{}/my-collection/playlists/folders", TIDAL_API_V2_URL);
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let cc = self.country_code();
        let mut params: Vec<(&str, &str)> = vec![
            ("folderId", folder_id),
            ("offset", &offset_str),
            ("limit", &limit_str),
            ("order", order),
            ("orderDirection", order_direction),
            ("countryCode", &cc),
            ("locale", "en_US"),
            ("deviceType", "BROWSER"),
        ];
        if !include_only.is_empty() {
            params.push(("includeOnly", include_only));
        }
        if !cursor.is_empty() {
            params.push(("cursor", cursor));
        }
        let body = self.api_get_body(&url, &params).await?;

        log::debug!(
            "[get_playlist_folders]: body_preview={}",
            &body[..body.len().min(1000)]
        );

        serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))
    }

    /// Fetch ALL playlists across every folder via the flattened endpoint,
    /// paginating with the response `cursor` until exhausted. Returns the raw
    /// folder items (same shape `normalizeFolderItem` consumes).
    pub async fn get_all_flattened_playlists(&self) -> Result<Vec<serde_json::Value>, Error> {
        let url = format!(
            "{}/my-collection/playlists/folders/flattened",
            TIDAL_API_V2_URL
        );
        let cc = self.country_code();
        let mut accumulated: Vec<serde_json::Value> = Vec::new();
        let mut cursor = String::new();
        const MAX_PAGES: u32 = 40;

        for _ in 0..MAX_PAGES {
            let mut params: Vec<(&str, &str)> = vec![
                ("offset", "0"),
                ("limit", "50"),
                ("order", "DATE"),
                ("orderDirection", "DESC"),
                ("countryCode", &cc),
                ("locale", "en_US"),
                ("deviceType", "BROWSER"),
            ];
            if !cursor.is_empty() {
                params.push(("cursor", &cursor));
            }

            let body = self.api_get_body(&url, &params).await?;
            let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
                Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)]))
            })?;

            if let Some(items) = value.get("items").and_then(|v| v.as_array()) {
                accumulated.extend(items.iter().cloned());
            }

            match value.get("cursor").and_then(|c| c.as_str()) {
                Some(next) if !next.is_empty() => cursor = next.to_string(),
                _ => break,
            }
        }

        log::debug!(
            "[get_all_flattened_playlists]: accumulated {} items",
            accumulated.len()
        );
        Ok(accumulated)
    }

    pub async fn create_playlist_folder(
        &self,
        folder_id: &str,
        name: &str,
        trns: &str,
    ) -> Result<serde_json::Value, Error> {
        log::debug!(
            "[create_playlist_folder]: folder_id={}, name={}, trns={}",
            folder_id,
            name,
            trns
        );

        let cc = self.country_code();
        let mut params: Vec<(&str, &str)> = vec![
            ("folderId", folder_id),
            ("name", name),
            ("countryCode", cc.as_str()),
            ("locale", "en_US"),
            ("deviceType", "BROWSER"),
        ];
        if !trns.is_empty() {
            params.push(("trns", trns));
        }

        let client = self.http();
        let req = client
            .put(format!(
                "{}/my-collection/playlists/folders/create-folder",
                TIDAL_API_V2_URL
            ))
            .query(&params);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[create_playlist_folder]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(serde_json::from_str(&body).unwrap_or(serde_json::Value::Null))
    }

    pub async fn rename_playlist_folder(&self, folder_trn: &str, name: &str) -> Result<(), Error> {
        log::debug!(
            "[rename_playlist_folder]: folder_trn={}, name={}",
            folder_trn,
            name
        );

        let client = self.http();
        let req = client
            .put(format!(
                "{}/my-collection/playlists/folders/rename",
                TIDAL_API_V2_URL
            ))
            .query(&[
                ("trn", folder_trn),
                ("name", name),
                ("countryCode", self.country_code().as_str()),
                ("locale", "en_US"),
                ("deviceType", "BROWSER"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[rename_playlist_folder]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn delete_playlist_folder(&self, folder_trn: &str) -> Result<(), Error> {
        log::debug!("[delete_playlist_folder]: folder_trn={}", folder_trn);

        let client = self.http();
        let req = client
            .put(format!(
                "{}/my-collection/playlists/folders/remove",
                TIDAL_API_V2_URL
            ))
            .query(&[
                ("trns", folder_trn),
                ("countryCode", self.country_code().as_str()),
                ("locale", "en_US"),
                ("deviceType", "BROWSER"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[delete_playlist_folder]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn move_playlist_to_folder(
        &self,
        folder_id: &str,
        playlist_trn: &str,
    ) -> Result<(), Error> {
        log::debug!(
            "[move_playlist_to_folder]: folder_id={}, playlist_trn={}",
            folder_id,
            playlist_trn
        );

        let client = self.http();
        let req = client
            .put(format!(
                "{}/my-collection/playlists/folders/move",
                TIDAL_API_V2_URL
            ))
            .query(&[
                ("folderId", folder_id),
                ("trns", playlist_trn),
                ("countryCode", self.country_code().as_str()),
                ("locale", "en_US"),
                ("deviceType", "BROWSER"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[move_playlist_to_folder]: status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn add_tracks_to_playlist(
        &self,
        playlist_id: &str,
        track_ids: &[u64],
    ) -> Result<(), Error> {
        // Get the playlist ETag which is required for modifications
        let client = self.http();
        let req = client
            .get(format!("{}/playlists/{}", TIDAL_API_URL, playlist_id))
            .query(&[("countryCode", self.country_code().as_str())]);
        let head_response = self.send_authed(req).await?;

        let etag = head_response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*")
            .to_string();

        let ids_str = track_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let client = self.http();
        let req = client
            .post(format!("{}/playlists/{}/items", TIDAL_API_URL, playlist_id))
            .header("If-None-Match", &etag)
            .query(&[("countryCode", self.country_code().as_str())])
            .form(&[
                ("trackIds", ids_str.as_str()),
                ("onDupes", "SKIP"),
                ("onArtifactNotFound", "FAIL"),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    pub async fn get_track_lyrics(&self, track_id: u64) -> Result<TidalLyrics, Error> {
        let cc = self.country_code();
        self.api_get(
            &format!("/tracks/{}/lyrics", track_id),
            &[("countryCode", &cc)],
        )
        .await
    }

    pub async fn get_playlist_details(
        &self,
        playlist_id: &str,
    ) -> Result<serde_json::Value, Error> {
        let cc = self.country_code();
        self.api_get(
            &format!("/playlists/{}", playlist_id),
            &[("countryCode", &cc)],
        )
        .await
    }

    pub async fn get_track(&self, track_id: u64) -> Result<serde_json::Value, Error> {
        let cc = self.country_code();
        self.api_get(&format!("/tracks/{}", track_id), &[("countryCode", &cc)])
            .await
    }

    pub async fn get_track_credits(&self, track_id: u64) -> Result<Vec<TidalCredit>, Error> {
        let cc = self.country_code();
        self.api_get(
            &format!("/tracks/{}/credits", track_id),
            &[("countryCode", &cc)],
        )
        .await
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<TidalSearchResults, Error> {
        // Try the v2 API first (web app uses this, returns playlists properly)
        if let Ok(v2) = self.search_v2(query, limit).await {
            return Ok(v2);
        }

        // Fallback to v1 API
        self.search_v1(query, limit).await
    }

    async fn search_v2(&self, query: &str, limit: u32) -> Result<TidalSearchResults, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        // v2 uses a different base URL, so pass the full URL
        let body = self
            .api_get_body(
                &format!("{}/search", TIDAL_API_V2_URL),
                &[
                    ("query", query),
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("types", "ARTISTS,ALBUMS,TRACKS,PLAYLISTS,VIDEOS"),
                    ("includeContributors", "true"),
                    ("includeUserPlaylists", "true"),
                    ("includeDidYouMean", "true"),
                    ("supportsUserData", "true"),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await?;
        Self::parse_search_response(&body, query, "v2")
    }

    async fn search_v1(&self, query: &str, limit: u32) -> Result<TidalSearchResults, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let body = self
            .api_get_body(
                "/search",
                &[
                    ("query", query),
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", "0"),
                    ("types", "ARTISTS,ALBUMS,TRACKS,PLAYLISTS,VIDEOS"),
                    ("includeContributors", "true"),
                    ("includeUserPlaylists", "true"),
                    ("supportsUserData", "true"),
                ],
            )
            .await?;
        Self::parse_search_response(&body, query, "v1")
    }

    fn parse_search_response(
        body: &str,
        query: &str,
        tag: &str,
    ) -> Result<TidalSearchResults, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Sec<T> {
            items: Vec<T>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SR {
            #[serde(default)]
            artists: Option<Sec<TidalArtist>>,
            #[serde(default)]
            albums: Option<Sec<TidalAlbumDetail>>,
            #[serde(default)]
            tracks: Option<Sec<TidalTrack>>,
            #[serde(default)]
            playlists: Option<Sec<TidalPlaylistRaw>>,
            #[serde(default)]
            videos: Option<Sec<TidalVideo>>,
        }

        let data: SR = serde_json::from_str(body)
            .map_err(|e| Error::Parse(format!("search ({}): {}", tag, e)))?;

        // Parse topHits from the raw JSON (v2 returns an array of typed entities)
        let top_hits = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|json| {
                json.get("topHits")
                    .and_then(|v| v.as_array())
                    .map(|arr| DirectHitItem::parse_array(arr))
            })
            .unwrap_or_default();

        log::debug!(
            "search [{}]: t={} al={} ar={} pl={} v={} th={} [{}] for '{}'",
            tag,
            data.tracks.as_ref().map(|s| s.items.len()).unwrap_or(0),
            data.albums.as_ref().map(|s| s.items.len()).unwrap_or(0),
            data.artists.as_ref().map(|s| s.items.len()).unwrap_or(0),
            data.playlists.as_ref().map(|s| s.items.len()).unwrap_or(0),
            data.videos.as_ref().map(|s| s.items.len()).unwrap_or(0),
            top_hits.len(),
            top_hits
                .iter()
                .map(|h| h.hit_type.as_str())
                .collect::<Vec<_>>()
                .join(","),
            query
        );

        let mut tracks = data.tracks.map(|s| s.items).unwrap_or_default();
        for t in &mut tracks {
            t.backfill_artist();
        }

        let mut albums = data.albums.map(|s| s.items).unwrap_or_default();
        for a in &mut albums {
            a.backfill_artist();
        }

        Ok(TidalSearchResults {
            artists: data.artists.map(|s| s.items).unwrap_or_default(),
            albums,
            tracks,
            playlists: data
                .playlists
                .map(|s| s.items.into_iter().map(|p| p.into()).collect())
                .unwrap_or_default(),
            videos: data.videos.map(|s| s.items).unwrap_or_default(),
            top_hit_type: None,
            top_hits,
        })
    }

    /// Fetch suggestions from Tidal's v2 /suggestions/ endpoint.
    /// Returns a SuggestionsResponse with text suggestions AND direct hit entities,
    /// exactly as the webapp's mini-search dropdown uses.
    pub async fn get_suggestions(&self, query: &str, limit: u32) -> SuggestionsResponse {
        let empty = SuggestionsResponse {
            text_suggestions: vec![],
            direct_hits: vec![],
        };
        let url = format!("{}/suggestions/", TIDAL_API_V2_URL);
        let country_code = self.country_code();

        let resp = self
            .authenticated_get(
                &url,
                &[
                    ("query", query),
                    ("countryCode", &country_code),
                    ("explicit", "true"),
                    ("hybrid", "true"),
                ],
            )
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let body = r.text().await.unwrap_or_default();
                if let Some(result) = Self::parse_v2_suggestions_full(&body, limit) {
                    log::debug!(
                        "suggestions v2: {} text, {} hits for '{}'",
                        result.text_suggestions.len(),
                        result.direct_hits.len(),
                        query
                    );
                    return result;
                }
            }
            Ok(r) => log::debug!("suggestions v2: HTTP {} for '{}'", r.status(), query),
            Err(e) => log::debug!("suggestions v2: error: {} for '{}'", e, query),
        }

        empty
    }

    /// Parse the full v2 /suggestions/ response into SuggestionsResponse.
    /// Preserves directHits in exact API order (mixed entity types).
    fn parse_v2_suggestions_full(body: &str, limit: u32) -> Option<SuggestionsResponse> {
        let json: serde_json::Value = serde_json::from_str(body).ok()?;

        let mut text_suggestions = Vec::new();

        // Extract history items (source = "history")
        if let Some(arr) = json.get("history").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(q) = item.get("query").and_then(|v| v.as_str()) {
                    text_suggestions.push(SuggestionTextItem {
                        query: q.to_string(),
                        source: "history".to_string(),
                    });
                }
            }
        }

        // Extract suggestion items (source = "suggestion")
        if let Some(arr) = json.get("suggestions").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(q) = item.get("query").and_then(|v| v.as_str()) {
                    text_suggestions.push(SuggestionTextItem {
                        query: q.to_string(),
                        source: "suggestion".to_string(),
                    });
                }
            }
        }

        text_suggestions.truncate(limit as usize);

        // Extract directHits in exact API order using the shared helper
        let direct_hits = json
            .get("directHits")
            .and_then(|v| v.as_array())
            .map(|arr| DirectHitItem::parse_array(arr))
            .unwrap_or_default();

        Some(SuggestionsResponse {
            text_suggestions,
            direct_hits,
        })
    }

    // ==================== Home Page (Pages API) ====================

    /// Fetch the v2 home feed from api.tidal.com/v2/home/feed/{slug}.
    ///
    /// Errors are returned, never flattened into an empty feed: callers cache
    /// what comes back, and a silent empty is indistinguishable from "this
    /// account has no home content" — which is how one failed request used to
    /// blank Home for hours.
    pub async fn fetch_v2_home_feed(
        &self,
        feed_slug: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<HomeTab>, Vec<HomePageSection>, Option<String>), Error> {
        let url = format!("{}/home/feed/{}", TIDAL_API_V2_URL, feed_slug);
        let country_code = self.country_code();

        let mut params: Vec<(&str, &str)> = vec![
            ("countryCode", &country_code),
            ("locale", "en_US"),
            ("deviceType", "BROWSER"),
            ("platform", "WEB"),
        ];
        let cursor_owned;
        if let Some(c) = cursor {
            cursor_owned = c.to_string();
            params.push(("cursor", &cursor_owned));
        }

        let resp = self.authenticated_get(&url, &params).await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let body = r.text().await.unwrap_or_default();
                match serde_json::from_str::<Value>(&body) {
                    Ok(json) => {
                        let next_cursor = json
                            .get("page")
                            .and_then(|p| p.get("cursor"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());
                        let raw_count = json
                            .get("items")
                            .and_then(|i| i.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let result = Self::parse_page_response(&json).unwrap_or_default();
                        log::debug!(
                            "v2 home feed: cursor={:?}, raw items={}, parsed sections={}, next_cursor={:?}",
                            cursor.is_some(),
                            raw_count,
                            result.sections.len(),
                            next_cursor.is_some()
                        );
                        if result.sections.len() < raw_count {
                            log::debug!(
                                "v2 home feed: {} sections dropped during parsing",
                                raw_count - result.sections.len()
                            );
                        }
                        Ok((result.tabs, result.sections, next_cursor))
                    }
                    Err(e) => {
                        log::warn!("v2 home feed: parse error: {}", e);
                        Err(Error::Parse(format!("home feed JSON: {}", e)))
                    }
                }
            }
            Ok(r) => {
                let status = r.status().as_u16();
                let body = r.text().await.unwrap_or_default();
                log::warn!("v2 home feed: HTTP {}", status);
                Err(Error::Api { status, body })
            }
            Err(e) => {
                log::warn!("v2 home feed: request error: {}", e.log_safe());
                Err(e)
            }
        }
    }

    /// Fetch a single page endpoint. Handles both V1 and V2 response formats.
    async fn fetch_page_endpoint(&self, endpoint: &str) -> Result<Vec<HomePageSection>, Error> {
        let cc = self.country_code();
        let body = match self
            .api_get_body(
                &format!("/{}", endpoint),
                &[
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                    ("locale", "en_US"),
                ],
            )
            .await
        {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Page endpoint {} failed: {}", endpoint, e);
                return Ok(vec![]); // Don't fail the whole home page for one endpoint
            }
        };

        let json: Value = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} JSON: {}", endpoint, e)))?;

        let result = Self::parse_page_response(&json)?;
        log::debug!(
            "[{}]: parsed {} sections: {:?}",
            endpoint,
            result.sections.len(),
            result
                .sections
                .iter()
                .map(|s| format!("\"{}\" ({})", s.title, s.section_type))
                .collect::<Vec<_>>()
        );

        if result.sections.is_empty()
            && let Some(obj) = json.as_object()
        {
            log::debug!(
                "[{}]: 0 sections parsed, top-level keys: {:?}",
                endpoint,
                obj.keys().collect::<Vec<_>>()
            );
        }

        Ok(result.sections)
    }

    /// Build a dedup key from a section: uses title + first 3 item IDs.
    /// This ensures sections with the same title but different content are kept.
    fn section_dedup_key(s: &HomePageSection) -> String {
        let mut key = s.title.clone();
        if let Some(items) = s.items.as_array() {
            for item in items.iter().take(3) {
                let id = item
                    .get("id")
                    .and_then(|i| i.as_u64())
                    .map(|i| i.to_string())
                    .or_else(|| {
                        item.get("uuid")
                            .and_then(|u| u.as_str())
                            .map(|s| s.to_string())
                    })
                    .or_else(|| {
                        item.get("mixId")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                key.push('|');
                key.push_str(&id);
            }
        }
        key
    }

    /// Helper: add sections to the collection, deduplicating smartly.
    /// Skips sections with empty titles.
    /// Uses title + item IDs for dedup key so same-title sections with different content are kept.
    fn add_unique_sections(
        all: &mut Vec<HomePageSection>,
        seen: &mut std::collections::HashSet<String>,
        new_sections: Vec<HomePageSection>,
    ) {
        for s in new_sections {
            // Skip sections with empty/blank titles
            if s.title.trim().is_empty() {
                continue;
            }
            // Skip PAGE_LINKS navigation sections on the home page
            if s.section_type == "PAGE_LINKS_CLOUD" || s.section_type == "PAGE_LINKS" {
                continue;
            }
            let key = Self::section_dedup_key(&s);
            if seen.insert(key) {
                all.push(s);
            }
        }
    }

    /// Fetch the home page. Tries the v2 home/feed/static endpoint first
    /// (what the Tidal web app uses). Falls back to multi-endpoint v1 approach.
    /// Trusts Tidal's section ordering — no manual resorting.
    pub async fn get_home_page(&self, feed_slug: &str) -> Result<HomePageResponse, Error> {
        // Try v2 home feed first (single endpoint, personalized). A failure here
        // is held rather than raised: the v1 fallback below may still have
        // content, and only if that comes up empty too does the error stand.
        let (tabs, mut all_sections, cursor, v2_error) =
            match self.fetch_v2_home_feed(feed_slug, None).await {
                Ok((tabs, sections, cursor)) => (tabs, sections, cursor, None),
                Err(e) => {
                    log::warn!(
                        "[home v2]: home/feed/{} failed: {}",
                        feed_slug,
                        e.log_safe()
                    );
                    (vec![], vec![], None, Some(e))
                }
            };

        if !all_sections.is_empty() {
            log::debug!(
                "[home v2]: got {} sections from home/feed/{}",
                all_sections.len(),
                feed_slug
            );

            // Filter out non-content section types
            all_sections.retain(|s| {
                !s.title.trim().is_empty()
                    && s.section_type != "PAGE_LINKS_CLOUD"
                    && s.section_type != "PAGE_LINKS"
            });

            for s in &all_sections {
                log::debug!(
                    "[home v2] section: '{}' type={} items={}",
                    s.title,
                    s.section_type,
                    s.items.as_array().map(|a| a.len()).unwrap_or(0)
                );
            }
            log::debug!(
                "[home v2]: returning {} sections, cursor={:?}",
                all_sections.len(),
                cursor.is_some()
            );
            return Ok(HomePageResponse {
                tabs,
                sections: all_sections,
                cursor,
            });
        }

        // v1 fallback only applies to the default static feed; other tabs are
        // v2-only, so a v2 failure is the whole story for them.
        if feed_slug != "static" {
            if let Some(e) = v2_error {
                return Err(e);
            }
            return Ok(HomePageResponse {
                tabs,
                sections: all_sections,
                cursor,
            });
        }

        // v2 unavailable — fall back to v1 multi-endpoint approach
        log::debug!("[home v1]: v2 empty, falling back to v1 endpoints");
        let mut seen_titles = std::collections::HashSet::new();

        let home_sections = self.fetch_page_endpoint("pages/home").await?;
        Self::add_unique_sections(&mut all_sections, &mut seen_titles, home_sections);

        if let Ok(sections) = self.fetch_page_endpoint("pages/for_you").await {
            Self::add_unique_sections(&mut all_sections, &mut seen_titles, sections);
        }

        if let Ok(sections) = self
            .fetch_page_endpoint("pages/my_collection_my_mixes")
            .await
        {
            Self::add_unique_sections(&mut all_sections, &mut seen_titles, sections);
        }

        if let Ok(sections) = self.fetch_page_endpoint("pages/explore").await {
            Self::add_unique_sections(&mut all_sections, &mut seen_titles, sections);
        }

        if let Ok(sections) = self.fetch_page_endpoint("pages/rising").await {
            Self::add_unique_sections(&mut all_sections, &mut seen_titles, sections);
        }

        // Nothing anywhere. If v2 told us why, say so — an error gives the UI
        // something to show and a retry to offer, where a blank success does not.
        if all_sections.is_empty()
            && let Some(e) = v2_error
        {
            log::warn!("[home]: v2 failed and the v1 fallback found nothing");
            return Err(e);
        }

        log::debug!("[home v1]: returning {} sections", all_sections.len());
        Ok(HomePageResponse {
            tabs: vec![],
            sections: all_sections,
            cursor: None,
        })
    }

    /// Extract the tab list from a v2 home feed response: `header.vibes.items[]`.
    fn parse_home_tabs(json: &Value) -> Vec<HomeTab> {
        json.get("header")
            .and_then(|h| h.get("vibes"))
            .and_then(|v| v.get("items"))
            .and_then(|i| i.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|it| {
                        let name = it.get("name").and_then(|n| n.as_str())?;
                        let tab_type = it.get("type").and_then(|t| t.as_str())?;
                        Some(HomeTab {
                            name: name.to_string(),
                            tab_type: tab_type.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parse a pages API response, supporting V1, V2, and tab/category formats.
    fn parse_page_response(json: &Value) -> Result<HomePageResponse, Error> {
        let tabs = Self::parse_home_tabs(json);
        let mut sections = Vec::new();

        // ---- V1 format: { rows: [ { modules: [ { type, title, pagedList, ... } ] } ] }
        if let Some(rows) = json.get("rows").and_then(|r| r.as_array()) {
            for row in rows {
                if let Some(modules) = row.get("modules").and_then(|m| m.as_array()) {
                    for module in modules {
                        if let Some(sec) = Self::parse_v1_module(module) {
                            sections.push(sec);
                        }
                    }
                }
            }
        }

        // ---- V2 format: { items: [ { type, title, items: [...], viewAll, ... } ] }
        if sections.is_empty()
            && let Some(top_items) = json.get("items").and_then(|i| i.as_array())
        {
            // Check if ANY item looks like a V2 section (objects with type/title/items)
            // vs just being raw content items (e.g. flat track/album objects)
            let looks_like_sections = top_items.iter().any(|f| {
                f.get("items").is_some()
                    || f.get("type")
                        .and_then(|t| t.as_str())
                        .map(|t| {
                            t.contains("LIST")
                                || t.contains("GRID")
                                || t.contains("SHORTCUT")
                                || t == "PAGE_LINKS_CLOUD"
                                || t == "PAGE_LINKS"
                                || t == "HIGHLIGHT_MODULE"
                        })
                        .unwrap_or(false)
                    || f.get("titleTextInfo").is_some()
            });

            if looks_like_sections {
                for item in top_items {
                    if let Some(sec) = Self::parse_v2_section(item) {
                        sections.push(sec);
                    }
                }
            }
        }

        // ---- Tab format: { tabs: [ { title, items: [...] } ] }
        // The explore page often uses a tabs-based structure
        if sections.is_empty()
            && let Some(tabs) = json.get("tabs").and_then(|t| t.as_array())
        {
            for tab in tabs {
                // Each tab may contain rows (V1) or items (V2) inside it
                if let Some(rows) = tab.get("rows").and_then(|r| r.as_array()) {
                    for row in rows {
                        if let Some(modules) = row.get("modules").and_then(|m| m.as_array()) {
                            for module in modules {
                                if let Some(sec) = Self::parse_v1_module(module) {
                                    sections.push(sec);
                                }
                            }
                        }
                    }
                }
                if let Some(items) = tab.get("items").and_then(|i| i.as_array()) {
                    for item in items {
                        if let Some(sec) = Self::parse_v2_section(item) {
                            sections.push(sec);
                        }
                    }
                }
            }
        }

        // ---- Categories format: { categories: [...] } or { sections: [...] }
        if sections.is_empty() {
            let containers = [
                json.get("categories").and_then(|c| c.as_array()),
                json.get("sections").and_then(|s| s.as_array()),
            ];
            for container in containers.into_iter().flatten() {
                for item in container {
                    // Try V1 module parsing first, then V2
                    if let Some(sec) = Self::parse_v1_module(item) {
                        sections.push(sec);
                    } else if let Some(sec) = Self::parse_v2_section(item) {
                        sections.push(sec);
                    }
                }
            }
        }

        // ---- Fallback: if the response itself looks like a single section
        //      e.g. { title, items: [...] } from a "view all" endpoint
        if sections.is_empty()
            && let Some(items) = json.get("items").and_then(|i| i.as_array())
        {
            let page_title = json
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Results")
                .to_string();
            // Unwrap {type, data} wrappers (v2 view-all format)
            let unwrapped: Vec<Value> = items
                .iter()
                .map(|item| {
                    if let Some(data) = item.get("data") {
                        let mut merged = data.clone();
                        if let Some(obj) = merged.as_object_mut()
                            && let Some(item_type) = item.get("type").and_then(|t| t.as_str())
                        {
                            obj.entry("_itemType".to_string())
                                .or_insert(Value::String(item_type.to_string()));
                        }
                        merged
                    } else {
                        item.clone()
                    }
                })
                .collect();
            sections.push(HomePageSection {
                title: page_title,
                section_type: "MIXED_LIST".to_string(),
                items: Value::Array(unwrapped),
                has_more: false,
                api_path: None,
            });
        }

        Ok(HomePageResponse {
            tabs,
            sections,
            cursor: None,
        })
    }

    /// Parse a V1 module (from rows/modules format).
    fn parse_v1_module(module: &Value) -> Option<HomePageSection> {
        let section_type = module
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Only skip truly non-content promotional types. MULTIPLE_TOP_PROMOTIONS
        // is content (a "Featured" row of playlist/video promos handled by the
        // frontend). FEATURED_PROMOTIONS is a distinct type the UI does not render —
        // letting it through emits id-less cards, so skip it.
        if section_type == "TEXT_BLOCK"
            || section_type == "SOCIAL"
            || section_type == "ARTICLE_LIST"
            || section_type == "FEATURED_PROMOTIONS"
        {
            return None;
        }

        // Get title - check multiple possible fields
        let title = module
            .get("title")
            .and_then(|t| t.as_str())
            .or_else(|| module.get("header").and_then(|h| h.as_str()))
            .unwrap_or("")
            .to_string();

        // PAGE_LINKS are navigation sections (explore categories) — allow them through
        // so the explore page can use them. The home page filters them out in add_unique_sections.

        // Allow sections even with empty titles if they have items
        // (some sections have descriptions but no title)

        // Extract items from pagedList, highlights, listItems, or other containers
        let items = if let Some(paged_list) = module.get("pagedList") {
            paged_list
                .get("items")
                .cloned()
                .unwrap_or(Value::Array(vec![]))
        } else if let Some(highlights) = module.get("highlights") {
            if let Some(arr) = highlights.as_array() {
                let unwrapped: Vec<Value> =
                    arr.iter().filter_map(|h| h.get("item").cloned()).collect();
                Value::Array(unwrapped)
            } else {
                Value::Array(vec![])
            }
        } else if let Some(list_items) = module.get("listItems").and_then(|l| l.as_array()) {
            // Some modules use "listItems" instead of "pagedList"
            Value::Array(list_items.clone())
        } else if module.get("mix").is_some() {
            // MIX_HEADER type - single mix as an item
            Value::Array(vec![module.get("mix").cloned().unwrap_or(Value::Null)])
        } else {
            // Last resort: look for any array field that looks like items
            let mut found = Value::Array(vec![]);
            if let Some(obj) = module.as_object() {
                for (key, val) in obj {
                    if key == "type"
                        || key == "title"
                        || key == "header"
                        || key == "showMore"
                        || key == "viewAll"
                        || key == "description"
                        || key == "id"
                        || key == "selfLink"
                    {
                        continue;
                    }
                    if let Some(arr) = val.as_array()
                        && !arr.is_empty()
                        && arr[0].is_object()
                    {
                        found = val.clone();
                        break;
                    }
                }
            }
            found
        };

        // Skip truly empty sections
        if items.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            return None;
        }

        // Extract "showMore" or "viewAll" api path - check multiple locations
        let api_path = module
            .get("showMore")
            .and_then(|sm| sm.get("apiPath"))
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                module
                    .get("pagedList")
                    .and_then(|pl| pl.get("dataApiPath"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                module.get("viewAll").and_then(|va| {
                    if let Some(s) = va.as_str() {
                        Some(s.to_string())
                    } else {
                        va.get("apiPath")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    }
                })
            });

        let has_more = api_path.is_some();

        Some(HomePageSection {
            title,
            section_type,
            items,
            has_more,
            api_path,
        })
    }

    /// Parse a V2 section (from the flat items format).
    /// V2 sections look like:
    /// { "type": "HORIZONTAL_LIST", "moduleId": "...", "title": "...",
    ///   "items": [ { "type": "ALBUM", "data": { ... } }, ... ],
    ///   "viewAll": "pages/..." }
    fn parse_v2_section(section: &Value) -> Option<HomePageSection> {
        let section_type = section
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Get title — v2 may use string, object {"text": "..."}, or titleTextInfo
        let title = section
            .get("title")
            .and_then(|t| {
                t.as_str().map(|s| s.to_string()).or_else(|| {
                    t.get("text")
                        .and_then(|tx| tx.as_str())
                        .map(|s| s.to_string())
                })
            })
            .or_else(|| {
                section
                    .get("titleTextInfo")
                    .and_then(|ti| ti.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        if title.is_empty() {
            log::debug!(
                "parse_v2_section: dropping section with empty title, type={}",
                section_type
            );
            return None;
        }

        // V2 items can be in "items" array, where each has { type, data }
        let raw_items = section.get("items").and_then(|i| i.as_array());

        let items = if let Some(raw) = raw_items {
            // Unwrap the "data" field from each item if present,
            // but keep the item type info by merging it
            let unwrapped: Vec<Value> = raw
                .iter()
                .map(|item| {
                    if let Some(data) = item.get("data") {
                        // Merge item-level type into data for identification
                        let mut merged = data.clone();
                        if let Some(obj) = merged.as_object_mut()
                            && let Some(item_type) = item.get("type").and_then(|t| t.as_str())
                        {
                            obj.entry("_itemType".to_string())
                                .or_insert(Value::String(item_type.to_string()));
                        }
                        merged
                    } else {
                        // No "data" wrapper — item is already flat
                        item.clone()
                    }
                })
                .collect();
            Value::Array(unwrapped)
        } else {
            Value::Array(vec![])
        };

        // V2 viewAll is either a string or an object
        let api_path = section
            .get("viewAll")
            .and_then(|va| {
                if let Some(s) = va.as_str() {
                    Some(s.to_string())
                } else {
                    va.get("apiPath")
                        .and_then(|p| p.as_str())
                        .map(|s| s.to_string())
                }
            })
            .or_else(|| {
                section
                    .get("showMore")
                    .and_then(|sm| sm.get("apiPath"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            });

        let has_more = api_path.is_some();

        // Map V2 section types to something our frontend understands
        let mapped_type = match section_type.as_str() {
            "SHORTCUT_LIST" => "SHORTCUT_LIST",
            "HORIZONTAL_LIST" | "HORIZONTAL_LIST_WITH_CONTEXT" => {
                // Try to detect the content type from items
                if let Some(arr) = items.as_array() {
                    if let Some(first) = arr.first() {
                        let item_type = first
                            .get("_itemType")
                            .or_else(|| first.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");
                        match item_type {
                            "MIX" => "MIX_LIST",
                            "ALBUM" => "ALBUM_LIST",
                            "PLAYLIST" => "PLAYLIST_LIST",
                            "ARTIST" => "ARTIST_LIST",
                            "TRACK" => "TRACK_LIST",
                            _ => {
                                // Detect by data shape
                                if first.get("mixType").is_some()
                                    || first.get("mixImages").is_some()
                                {
                                    "MIX_LIST"
                                } else if first.get("uuid").is_some() {
                                    "PLAYLIST_LIST"
                                } else if first.get("cover").is_some()
                                    || first.get("numberOfTracks").is_some()
                                {
                                    "ALBUM_LIST"
                                } else if first.get("picture").is_some()
                                    && first.get("cover").is_none()
                                {
                                    "ARTIST_LIST"
                                } else {
                                    "MIXED_TYPES_LIST"
                                }
                            }
                        }
                    } else {
                        "MIXED_TYPES_LIST"
                    }
                } else {
                    "MIXED_TYPES_LIST"
                }
            }
            "TRACK_LIST" => "TRACK_LIST",
            other => other,
        };

        Some(HomePageSection {
            title,
            section_type: mapped_type.to_string(),
            items,
            has_more,
            api_path,
        })
    }

    pub async fn get_favorite_artists(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
    ) -> Result<PaginatedResponse<TidalArtistDetail>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/favorites/artists", user_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                    ("order", order),
                    ("orderDirection", order_direction),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        struct FavEntry {
            item: TidalArtistDetail,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavResponse {
            items: Vec<FavEntry>,
            total_number_of_items: u32,
        }

        let data: FavResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))?;
        let artists: Vec<TidalArtistDetail> = data.items.into_iter().map(|f| f.item).collect();
        log::debug!(
            "[get_favorite_artists]: got {} artists (total={})",
            artists.len(),
            data.total_number_of_items
        );
        Ok(PaginatedResponse {
            items: artists,
            total_number_of_items: data.total_number_of_items,
            offset,
            limit,
        })
    }

    /// Fetch user's favorite albums as structured data for the sidebar.
    pub async fn get_favorite_albums(
        &self,
        user_id: u64,
        offset: u32,
        limit: u32,
        order: &str,
        order_direction: &str,
    ) -> Result<PaginatedResponse<TidalAlbumDetail>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &format!("/users/{}/favorites/albums", user_id),
                &[
                    ("countryCode", &cc),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                    ("order", order),
                    ("orderDirection", order_direction),
                ],
            )
            .await?;

        #[derive(Deserialize)]
        struct FavEntry {
            item: TidalAlbumDetail,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct FavResponse {
            items: Vec<FavEntry>,
            total_number_of_items: u32,
        }

        let data: FavResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, body)))?;
        let albums: Vec<TidalAlbumDetail> = data.items.into_iter().map(|e| e.item).collect();
        log::debug!(
            "[get_favorite_albums]: got {} albums (total={})",
            albums.len(),
            data.total_number_of_items
        );
        Ok(PaginatedResponse {
            items: albums,
            total_number_of_items: data.total_number_of_items,
            offset,
            limit,
        })
    }

    // ==================== Artist Detail ====================

    /// Fetch full artist detail (name, picture, etc.)
    pub async fn get_artist_detail(&self, artist_id: u64) -> Result<TidalArtistDetail, Error> {
        let cc = self.country_code();
        self.api_get(&format!("/artists/{}", artist_id), &[("countryCode", &cc)])
            .await
    }

    // ==================== Mix / Radio Items ====================

    /// Parse a `pages/mix` JSON response body into a `MixPageResult`.
    /// Extracts mix metadata from `MIX_HEADER` and tracks from `TRACK_LIST`.
    fn parse_mix_page(mix_id: &str, body: &str) -> Option<MixPageResult> {
        let json: Value = serde_json::from_str(body).ok()?;
        let rows = json.get("rows")?.as_array()?;

        let mut title: Option<String> = None;
        let mut subtitle: Option<String> = None;
        let mut mix_type: Option<String> = None;
        let mut image: Option<String> = None;
        let mut tracks: Vec<TidalTrack> = Vec::new();

        for row in rows {
            let modules = row.get("modules").and_then(|m| m.as_array());
            let Some(modules) = modules else { continue };
            for module in modules {
                let mod_type = module.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match mod_type {
                    "MIX_HEADER" => {
                        if let Some(mix) = module.get("mix") {
                            title = mix.get("title").and_then(|t| t.as_str()).map(String::from);
                            subtitle = mix
                                .get("subTitle")
                                .and_then(|s| s.as_str())
                                .map(String::from);
                            mix_type = mix
                                .get("mixType")
                                .and_then(|t| t.as_str())
                                .map(String::from);
                            // Extract image URL from images.LARGE.url (or MEDIUM, SMALL)
                            if let Some(images) = mix.get("images") {
                                image = images
                                    .get("LARGE")
                                    .or_else(|| images.get("MEDIUM"))
                                    .or_else(|| images.get("SMALL"))
                                    .and_then(|img| img.get("url"))
                                    .and_then(|u| u.as_str())
                                    .map(String::from);
                            }
                        }
                    }
                    "TRACK_LIST" => {
                        if let Some(items) = module
                            .get("pagedList")
                            .and_then(|p| p.get("items"))
                            .and_then(|i| i.as_array())
                        {
                            tracks = items
                                .iter()
                                .filter_map(|item| {
                                    serde_json::from_value::<TidalTrack>(item.clone()).ok()
                                })
                                .collect();
                            for t in &mut tracks {
                                t.backfill_artist();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if tracks.is_empty() {
            return None;
        }

        Some(MixPageResult {
            mix_id: mix_id.to_string(),
            mix_type,
            title,
            subtitle,
            image,
            tracks,
        })
    }

    /// Fetch the tracks in a mix (custom mixes, radio stations, etc.)
    /// Tries `pages/mix` first, falls back to legacy `/mixes/{id}/items`.
    pub async fn get_mix_items(&self, mix_id: &str) -> Result<MixPageResult, Error> {
        let cc = self.country_code();

        // Primary: pages/mix endpoint
        if let Ok(body) = self
            .api_get_body(
                "/pages/mix",
                &[
                    ("mixId", mix_id),
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                    ("locale", "en_US"),
                ],
            )
            .await
            && let Some(result) = Self::parse_mix_page(mix_id, &body)
        {
            return Ok(result);
        }

        // Fallback: legacy /mixes/{id}/items (no metadata available)
        let tracks = self.get_mix_items_legacy(mix_id).await?;
        Ok(MixPageResult {
            mix_id: mix_id.to_string(),
            mix_type: None,
            title: None,
            subtitle: None,
            image: None,
            tracks,
        })
    }

    /// Legacy mix endpoint: `/mixes/{id}/items`
    async fn get_mix_items_legacy(&self, mix_id: &str) -> Result<Vec<TidalTrack>, Error> {
        let cc = self.country_code();
        let body = self
            .api_get_body(&format!("/mixes/{}/items", mix_id), &[("countryCode", &cc)])
            .await?;

        let json: Value = serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
        if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
            let mut tracks: Vec<TidalTrack> = items
                .iter()
                .filter_map(|entry| {
                    entry
                        .get("item")
                        .and_then(|item| serde_json::from_value::<TidalTrack>(item.clone()).ok())
                })
                .collect();
            for t in &mut tracks {
                t.backfill_artist();
            }
            Ok(tracks)
        } else {
            Ok(vec![])
        }
    }

    // ==================== Artist Page ====================

    /// Fetch an artist's top tracks
    pub async fn get_artist_top_tracks(
        &self,
        artist_id: u64,
        limit: u32,
    ) -> Result<Vec<TidalTrack>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let body = self
            .api_get_body(
                &format!("/artists/{}/toptracks", artist_id),
                &[("countryCode", &cc), ("limit", &limit_str), ("offset", "0")],
            )
            .await?;

        #[derive(Deserialize)]
        struct Resp {
            items: Vec<TidalTrack>,
        }

        let mut data: Resp = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))?;
        for t in &mut data.items {
            t.backfill_artist();
        }
        Ok(data.items)
    }

    /// Fetch an artist's albums
    pub async fn get_artist_albums(
        &self,
        artist_id: u64,
        limit: u32,
    ) -> Result<Vec<TidalAlbumDetail>, Error> {
        let cc = self.country_code();
        let limit_str = limit.to_string();
        let body = self
            .api_get_body(
                &format!("/artists/{}/albums", artist_id),
                &[("countryCode", &cc), ("limit", &limit_str), ("offset", "0")],
            )
            .await?;

        #[derive(Deserialize)]
        struct Resp {
            items: Vec<TidalAlbumDetail>,
        }

        let data: Resp = serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))?;
        Ok(data.items)
    }

    /// Fetch artist bio text
    pub async fn get_artist_bio(&self, artist_id: u64) -> Result<String, Error> {
        let cc = self.country_code();
        match self
            .api_get_body(
                &format!("/artists/{}/bio", artist_id),
                &[("countryCode", &cc)],
            )
            .await
        {
            Ok(body) => {
                let json: Value = serde_json::from_str(&body).unwrap_or_default();
                Ok(json
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string())
            }
            Err(_) => Ok(String::new()), // Bio not always available
        }
    }

    pub async fn get_artist_page(&self, artist_id: u64) -> Result<Value, Error> {
        let cc = self.country_code();
        // Try v2 first
        let v2_url = format!("{}/artist/{}", TIDAL_API_V2_URL, artist_id);
        match self
            .api_get_body(
                &v2_url,
                &[
                    ("countryCode", &cc),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                    ("platform", "WEB"),
                ],
            )
            .await
        {
            Ok(body) => {
                return serde_json::from_str(&body)
                    .map_err(|e| Error::Parse(format!("artist page v2 JSON: {}", e)));
            }
            Err(e) => {
                log::warn!(
                    "[get_artist_page] v2 failed for artist {}: {:?}, falling back to v1",
                    artist_id,
                    e
                );
            }
        }
        // Fallback to v1
        let body = self
            .api_get_body(
                &format!("/pages/artist?artistId={}", artist_id),
                &[
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                    ("locale", "en_US"),
                ],
            )
            .await?;
        serde_json::from_str(&body).map_err(|e| Error::Parse(format!("artist page v1 JSON: {}", e)))
    }

    pub async fn get_artist_top_tracks_all(
        &self,
        artist_id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<Value, Error> {
        let url = format!("{}/artist/ARTIST_TOP_TRACKS/view-all", TIDAL_API_V2_URL);
        let cc = self.country_code();
        let id_str = artist_id.to_string();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        let body = self
            .api_get_body(
                &url,
                &[
                    ("artistId", &id_str),
                    ("locale", "en_US"),
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                    ("platform", "WEB"),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                ],
            )
            .await?;
        serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("artist top tracks JSON: {}", e)))
    }

    pub async fn get_artist_view_all(
        &self,
        artist_id: u64,
        view_all_path: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Value, Error> {
        let cc = self.country_code();
        let id_str = artist_id.to_string();
        let limit_str = limit.to_string();
        let offset_str = offset.to_string();
        // viewAll paths from v2 API are relative like "artist/ARTIST_ALBUMS/view-all?artistId=123"
        // They need the v2 base URL, and may already contain query params
        let url = if view_all_path.starts_with("http") {
            view_all_path.to_string()
        } else {
            let path = view_all_path.trim_start_matches('/');
            format!("{}/{}", TIDAL_API_V2_URL, path)
        };
        // The path may already contain ?artistId=... — reqwest .query() appends correctly
        let body = self
            .api_get_body(
                &url,
                &[
                    ("artistId", &id_str),
                    ("locale", "en_US"),
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                    ("platform", "WEB"),
                    ("limit", &limit_str),
                    ("offset", &offset_str),
                ],
            )
            .await?;
        serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("artist view-all JSON: {}", e)))
    }

    pub fn parse_album_page(&self, body: &str) -> Result<AlbumPageResponse, Error> {
        let json: Value = serde_json::from_str(body)
            .map_err(|e| Error::Parse(format!("album page JSON: {}", e)))?;

        let rows = json
            .get("rows")
            .and_then(|r| r.as_array())
            .ok_or_else(|| Error::Parse("album page: missing rows".into()))?;

        let mut album: Option<TidalAlbumDetail> = None;
        let mut tracks: Vec<TidalTrack> = Vec::new();
        let mut total_tracks: u32 = 0;
        let mut credits: Vec<TidalCredit> = Vec::new();
        let mut review: Option<TidalReview> = None;
        let mut sections: Vec<AlbumPageSection> = Vec::new();
        let mut vibrant_color: Option<String> = None;
        let mut copyright: Option<String> = None;

        for row in rows {
            let modules = match row.get("modules").and_then(|m| m.as_array()) {
                Some(m) => m,
                None => continue,
            };

            for module in modules {
                let mtype = module.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match mtype {
                    "ALBUM_HEADER" => {
                        if let Some(album_val) = module.get("album")
                            && let Ok(mut detail) =
                                serde_json::from_value::<TidalAlbumDetail>(album_val.clone())
                        {
                            detail.backfill_artist();
                            copyright = detail.copyright.clone();
                            album = Some(detail);
                        }
                        // Credits
                        if let Some(creds) = module.get("credits").and_then(|c| c.as_array()) {
                            for c in creds {
                                if let Ok(credit) = serde_json::from_value::<TidalCredit>(c.clone())
                                {
                                    credits.push(credit);
                                }
                            }
                        }
                        // Review
                        if let Some(rev) = module.get("review")
                            && let Ok(r) = serde_json::from_value::<TidalReview>(rev.clone())
                            && r.text.is_some()
                        {
                            review = Some(r);
                        }
                    }
                    "ALBUM_ITEMS" => {
                        if let Some(paged) = module.get("pagedList") {
                            total_tracks = paged
                                .get("totalNumberOfItems")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0) as u32;

                            if let Some(items) = paged.get("items").and_then(|i| i.as_array()) {
                                for item_wrapper in items {
                                    // Items are wrapped as {item: {...}, type: "track"}
                                    let track_val =
                                        item_wrapper.get("item").unwrap_or(item_wrapper);
                                    if let Ok(mut track) =
                                        serde_json::from_value::<TidalTrack>(track_val.clone())
                                    {
                                        track.backfill_artist();
                                        // Extract vibrant color from first track's album
                                        if vibrant_color.is_none()
                                            && let Some(ref alb) = track.album
                                        {
                                            vibrant_color = alb.vibrant_color.clone();
                                        }
                                        tracks.push(track);
                                    }
                                }
                            }
                        }
                    }
                    "ALBUM_LIST" | "ARTIST_LIST" => {
                        let title = module
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let mut items: Vec<Value> = Vec::new();

                        if let Some(paged) = module.get("pagedList")
                            && let Some(arr) = paged.get("items").and_then(|i| i.as_array())
                        {
                            items = arr.clone();
                        }

                        let api_path = module
                            .get("showMore")
                            .and_then(|sm| sm.get("apiPath"))
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string());

                        if !items.is_empty() {
                            sections.push(AlbumPageSection {
                                title,
                                section_type: mtype.to_string(),
                                items,
                                api_path,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let album =
            album.ok_or_else(|| Error::Parse("album page: no ALBUM_HEADER found".into()))?;

        let video_cover = album.video_cover.clone();

        Ok(AlbumPageResponse {
            album,
            tracks,
            total_tracks,
            vibrant_color,
            video_cover,
            copyright,
            credits,
            review,
            sections,
        })
    }

    pub async fn get_album_page(&self, album_id: u64) -> Result<AlbumPageResponse, Error> {
        let cc = self.country_code();
        let id_str = album_id.to_string();
        let body = self
            .api_get_body(
                "/pages/album",
                &[
                    ("albumId", &id_str),
                    ("countryCode", &cc),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await?;
        self.parse_album_page(&body)
    }

    pub async fn get_page(&self, api_path: &str) -> Result<HomePageResponse, Error> {
        let cc = self.country_code();
        // Route v2 paths (home/*, artist/*, feed/*) through v2 base URL,
        // v1 paths (pages/*) through v1
        let path = if api_path.starts_with("http") {
            api_path.to_string()
        } else {
            let trimmed = api_path.trim_start_matches('/');
            if trimmed.starts_with("pages/") {
                format!("/{}", trimmed)
            } else {
                format!("{}/{}", TIDAL_API_V2_URL, trimmed)
            }
        };
        let is_v2 = path.contains("/v2/");
        let body = if is_v2 {
            self.api_get_body(
                &path,
                &[
                    ("countryCode", &cc),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                    ("platform", "WEB"),
                ],
            )
            .await?
        } else {
            self.api_get_body(&path, &[("countryCode", &cc), ("deviceType", "BROWSER")])
                .await?
        };

        let json: Value = serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
        Self::parse_page_response(&json)
    }

    /// Resolve the user's artistId from `/v1/users/{id}`. Returns `Ok(None)`
    /// when the account has no associated artist profile.
    pub async fn get_user_artist_id(&self, user_id: u64) -> Result<Option<u64>, Error> {
        let cc = self.country_code();
        let body = self
            .api_get_body(&format!("/users/{}", user_id), &[("countryCode", &cc)])
            .await?;
        let json: Value = serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
        Ok(json.get("artistId").and_then(|v| v.as_u64()))
    }

    /// Full read-only profile. Falls back to a minimal profile (name/handle from
    /// the user record, no artist data) when the account has no artistId.
    pub async fn get_profile(&self, user_id: u64) -> Result<Profile, Error> {
        let cc = self.country_code();

        let Some(artist_id) = self.get_user_artist_id(user_id).await? else {
            let (name, username) = self.get_user_profile(user_id).await?;
            return Ok(Profile {
                user_id,
                artist_id: None,
                name,
                handle: username,
                bio: None,
                bio_id: None,
                picture_files: Vec::new(),
                artwork_id: None,
                blur_hash: None,
                palette: Vec::new(),
                external_links: Vec::new(),
                fan_count: None,
                public_playlists: Vec::new(),
            });
        };

        let artist_id_str = artist_id.to_string();
        let artist_body = self
            .api_get_body(
                &format!("{}/artists/{}", TIDAL_OPENAPI_URL, artist_id),
                &[
                    ("include", "profileArt,biography,owners"),
                    ("countryCode", &cc),
                ],
            )
            .await?;
        let parts = parse_artist_profile(&artist_body)?;

        let playlists_body = self
            .api_get_body(
                &format!("{}/playlists", TIDAL_OPENAPI_URL),
                &[
                    ("filter[owners.id]", &user_id.to_string()),
                    ("include", "coverArt"),
                    ("countryCode", &cc),
                ],
            )
            .await?;
        let public_playlists = parse_public_playlists(&playlists_body)?;

        let fan_count = self.fetch_fan_count(user_id, &artist_id_str).await.ok();

        Ok(Profile {
            user_id,
            artist_id: Some(artist_id),
            name: parts.name,
            handle: parts.handle,
            bio: parts.bio,
            bio_id: parts.bio_id,
            picture_files: parts.picture_files,
            artwork_id: parts.artwork_id,
            blur_hash: parts.blur_hash,
            palette: parts.palette,
            external_links: parts.external_links,
            fan_count,
            public_playlists,
        })
    }

    /// Best-effort follower/fan count. Tries the social-host profile endpoint
    /// first, then the openapi followers relationship.
    async fn fetch_fan_count(&self, user_id: u64, artist_id: &str) -> Result<u32, Error> {
        let primary = self
            .api_get_body(
                &format!("https://api.tidal.com/v2/profiles/{}", user_id),
                &[],
            )
            .await;
        if let Ok(body) = primary
            && let Ok(json) = serde_json::from_str::<Value>(&body)
            && let Some(n) = json.get("numberOfFollowers").and_then(|v| v.as_u64())
        {
            return Ok(n as u32);
        }

        let body = self
            .api_get_body(
                &format!(
                    "{}/artists/{}/relationships/followers",
                    TIDAL_OPENAPI_URL, artist_id
                ),
                &[("countryCode", &self.country_code())],
            )
            .await?;
        let json: Value = serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
        let count = json
            .get("data")
            .and_then(|d| d.as_array())
            .map(|a| a.len() as u32)
            .ok_or_else(|| Error::Parse("followers: missing data array".into()))?;
        Ok(count)
    }

    /// Fetch the activity feed. `user_id` comes from the caller — `self.tokens`
    /// may carry `None` on the token-import path.
    pub async fn fetch_feed(&self, user_id: u64) -> Result<FeedResponse, Error> {
        let url = format!("{}/feed/activities", TIDAL_API_V2_URL);
        let country_code = self.country_code();
        let user_id_str = user_id.to_string();

        let body = self
            .api_get_body(
                &url,
                &[
                    ("userId", user_id_str.as_str()),
                    ("countryCode", country_code.as_str()),
                    ("locale", "en_US"),
                    ("deviceType", "BROWSER"),
                    ("platform", "WEB"),
                ],
            )
            .await?;

        let feed = parse_feed_body(&body).map_err(|e| {
            log::error!("[fetch_feed] parse error: {}", e);
            Error::Parse(format!("feed parse error: {}", e))
        })?;

        log::debug!(
            "[fetch_feed] items={} unseen={}",
            feed.items.len(),
            feed.unseen_count
        );

        Ok(feed)
    }

    /// Mark every feed activity as seen.
    ///
    /// Hand-rolled because v2 returns 400/404 without the client-version
    /// header. Callers treat failure as non-fatal.
    pub async fn mark_feed_seen(&self, user_id: u64) -> Result<(), Error> {
        log::debug!("[mark_feed_seen] user_id={}", user_id);

        let user_id_str = user_id.to_string();
        let client = self.http();
        let req = client
            .put(format!("{}/feed/activities/seen", TIDAL_API_V2_URL))
            .header("x-tidal-client-version", TIDAL_CLIENT_VERSION)
            .query(&[
                ("userId", user_id_str.as_str()),
                ("countryCode", self.country_code().as_str()),
            ]);
        let response = self.send_authed(req).await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        log::debug!(
            "[mark_feed_seen] status={}, body={}",
            status,
            &body[..body.len().min(500)]
        );

        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod home_tab_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn search_parses_video_section() {
        let body = json!({
            "tracks": { "items": [] },
            "videos": { "items": [
                { "id": 111, "title": "A Music Video", "duration": 215,
                  "imageId": "abc", "artists": [{ "id": 5, "name": "Some Artist" }] },
                { "id": 222, "title": "Another" }
            ]}
        })
        .to_string();
        let res = TidalClient::parse_search_response(&body, "q", "test").expect("parses");
        assert_eq!(res.videos.len(), 2);
        assert_eq!(res.videos[0].id, 111);
        assert_eq!(res.videos[0].title, "A Music Video");
        // A missing videos section yields an empty vec, never an error.
        let none = TidalClient::parse_search_response(
            &json!({ "tracks": { "items": [] } }).to_string(),
            "q",
            "test",
        )
        .expect("parses");
        assert!(none.videos.is_empty());
    }

    #[test]
    fn direct_hit_parses_video_type() {
        let item = json!({
            "type": "VIDEOS",
            "value": { "id": 999, "title": "Top Video", "duration": 180,
                       "imageId": "img-uuid", "artists": [{ "id": 1, "name": "Star" }] }
        });
        let hit = DirectHitItem::from_typed_value(&item).expect("video hit parses");
        assert_eq!(hit.hit_type, "VIDEOS");
        assert_eq!(hit.id, Some(999));
        assert_eq!(hit.title.as_deref(), Some("Top Video"));
        assert_eq!(hit.image.as_deref(), Some("img-uuid"));
        assert_eq!(hit.artist_name.as_deref(), Some("Star"));
        assert_eq!(hit.duration, Some(180));
    }

    #[test]
    fn parses_tabs_from_vibes() {
        let body = json!({
            "header": { "vibes": { "items": [
                { "name": "For you", "type": "STATIC" },
                { "name": "Staff Picks", "type": "EDITORIAL" },
                { "name": "Uploads", "type": "UPLOADS" }
            ]}},
            "items": []
        });
        let tabs = TidalClient::parse_home_tabs(&body);
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[0].name, "For you");
        assert_eq!(tabs[0].tab_type, "STATIC");
        assert_eq!(tabs[2].tab_type, "UPLOADS");
    }

    #[test]
    fn returns_empty_when_no_vibes() {
        let body = json!({ "items": [] });
        assert!(TidalClient::parse_home_tabs(&body).is_empty());
    }

    #[test]
    fn parse_playlist_items_tolerates_null_or_missing_duration() {
        let items = vec![
            serde_json::json!({ "type": "track", "item": { "id": 1, "title": "A", "duration": 100 } }),
            serde_json::json!({ "type": "video", "item": { "id": 2, "title": "V", "duration": null } }),
            serde_json::json!({ "type": "video", "item": { "id": 3, "title": "V2" } }),
        ];
        let out = super::parse_playlist_items(items).expect("must not abort on null duration");
        assert_eq!(out.len(), 3, "no item should be dropped");
        assert_eq!(out[1].duration, 0);
        assert_eq!(out[2].duration, 0);
    }

    #[test]
    fn parse_v1_module_skips_featured_promotions() {
        let module = serde_json::json!({
            "type": "FEATURED_PROMOTIONS",
            "pagedList": { "items": [ { "header": "X", "artifactId": "1", "type": "PLAYLIST" } ] }
        });
        assert!(
            super::TidalClient::parse_v1_module(&module).is_none(),
            "FEATURED_PROMOTIONS must be skipped, not rendered as cards"
        );
    }

    #[test]
    fn parse_v1_module_keeps_multiple_top_promotions() {
        let module = serde_json::json!({
            "type": "MULTIPLE_TOP_PROMOTIONS",
            "title": "Featured",
            "pagedList": { "items": [ { "header": "X", "artifactId": "1", "type": "PLAYLIST" } ] }
        });
        assert!(super::TidalClient::parse_v1_module(&module).is_some());
    }

    #[test]
    fn artist_structs_carry_artwork_fallback_fields() {
        let body = json!({
            "id": 42,
            "name": "Killigrew",
            "picture": null,
            "artworkId": "art-1",
            "selectedAlbumCoverFallback": "cover-1"
        })
        .to_string();

        let artist: TidalArtist = serde_json::from_str(&body).expect("parses");
        assert_eq!(artist.picture, None);
        assert_eq!(artist.artwork_id.as_deref(), Some("art-1"));
        assert_eq!(
            artist.selected_album_cover_fallback.as_deref(),
            Some("cover-1")
        );

        let detail: TidalArtistDetail = serde_json::from_str(&body).expect("parses");
        assert_eq!(detail.artwork_id.as_deref(), Some("art-1"));
        assert_eq!(
            detail.selected_album_cover_fallback.as_deref(),
            Some("cover-1")
        );

        // Absent fields must not fail the parse.
        let bare = json!({ "id": 1, "name": "Bare" }).to_string();
        let bare_artist: TidalArtist = serde_json::from_str(&bare).expect("parses");
        assert_eq!(bare_artist.artwork_id, None);
        assert_eq!(bare_artist.selected_album_cover_fallback, None);
    }

    #[test]
    fn direct_hit_artist_carries_artwork_fallback() {
        let item = json!({
            "type": "ARTISTS",
            "value": {
                "id": 7,
                "name": "Jacoo",
                "picture": null,
                "artworkId": "art-9",
                "selectedAlbumCoverFallback": "cover-9"
            }
        });
        let hit = DirectHitItem::from_typed_value(&item).expect("artist hit parses");
        assert_eq!(hit.picture, None);
        assert_eq!(hit.artwork_id.as_deref(), Some("art-9"));
        assert_eq!(
            hit.selected_album_cover_fallback.as_deref(),
            Some("cover-9")
        );
    }
}
