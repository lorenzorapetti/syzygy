mod api;

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::Error;
use crate::auth::LoginMethod;
use crate::models::{AuthTokens, is_playbackinfo_sub_status};
use crate::rate_gate::{self, RateGate};

pub(crate) const TIDAL_AUTH_URL: &str = "https://auth.tidal.com/v1/oauth2";
pub(crate) const TIDAL_API_URL: &str = "https://api.tidal.com/v1";
pub(crate) const TIDAL_API_V2_URL: &str = "https://api.tidal.com/v2";
pub(crate) const TIDAL_OPENAPI_URL: &str = "https://openapi.tidal.com/v2";
pub(crate) const TIDAL_CLIENT_VERSION: &str = "2026.9.15";

/// Used until `get_session_info` reports the account's country.
const DEFAULT_COUNTRY_CODE: &str = "US";

/// What the client tells the app about the Session. Sent on the channel the
/// caller passed to [`TidalClient::new`].
#[derive(Debug, Clone)]
pub enum Event {
    /// A 401 led to a successful refresh. The app should save these tokens.
    TokensRefreshed(AuthTokens),
    /// The token endpoint answered `invalid_grant`. The client has dropped its
    /// tokens; the user has to sign in again.
    SessionExpired,
}

/// The TIDAL API client. Cheap to clone; every clone shares the same tokens,
/// rate gate and refresh lock.
#[derive(Clone)]
pub struct TidalClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    session: RwLock<SessionState>,
    /// Held for the whole refresh, so concurrent 401s refresh once.
    refresh: Mutex<()>,
    gate: RateGate,
    events: UnboundedSender<Event>,
}

struct SessionState {
    tokens: Option<AuthTokens>,
    login_method: LoginMethod,
    country_code: String,
}

impl TidalClient {
    pub fn new(http: reqwest::Client, events: UnboundedSender<Event>) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                session: RwLock::new(SessionState {
                    tokens: None,
                    login_method: LoginMethod::Browser,
                    country_code: DEFAULT_COUNTRY_CODE.to_string(),
                }),
                refresh: Mutex::new(()),
                gate: RateGate::new(),
                events,
            }),
        }
    }

    /// Restore a stored Session. `country_code` is the last one known, if any.
    pub fn restore_session(
        &self,
        tokens: AuthTokens,
        login_method: LoginMethod,
        country_code: Option<String>,
    ) {
        let mut session = self.write();
        session.tokens = Some(tokens);
        session.login_method = login_method;
        if let Some(cc) = country_code.filter(|cc| !cc.is_empty()) {
            session.country_code = cc;
        }
    }

    /// Forget the tokens and the account's country. Later calls fail with
    /// `Error::NotAuthenticated`.
    pub fn sign_out(&self) {
        let mut session = self.write();
        session.tokens = None;
        session.country_code = DEFAULT_COUNTRY_CODE.to_string();
    }

    pub fn tokens(&self) -> Option<AuthTokens> {
        self.read().tokens.clone()
    }

    /// Which credential pair the current tokens belong to.
    pub fn login_method(&self) -> LoginMethod {
        self.read().login_method
    }

    /// The account's country, sent with most API calls.
    pub fn country_code(&self) -> String {
        self.read().country_code.clone()
    }

    pub(crate) fn set_country_code(&self, country_code: String) {
        if !country_code.is_empty() {
            self.write().country_code = country_code;
        }
    }

    /// Install tokens from a completed sign-in.
    pub(crate) fn sign_in(&self, tokens: AuthTokens, login_method: LoginMethod) {
        let mut session = self.write();
        session.tokens = Some(tokens);
        session.login_method = login_method;
    }

    /// The bytes of a picture on TIDAL's image CDN. Unauthenticated and
    /// outside the rate gate: images aren't API traffic.
    pub async fn get_image(&self, url: &str) -> Result<Vec<u8>, Error> {
        let response = self.http().get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body: String::new(),
            });
        }
        Ok(response.bytes().await?.to_vec())
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    fn read(&self) -> RwLockReadGuard<'_, SessionState> {
        self.inner.session.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, SessionState> {
        self.inner
            .session
            .write()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn access_token(&self) -> Result<String, Error> {
        self.read()
            .tokens
            .as_ref()
            .map(|t| t.access_token.clone())
            .ok_or(Error::NotAuthenticated)
    }

    /// POST a form to an auth endpoint, bypassing the rate gate. Returns the
    /// status and body, whatever the status.
    pub(crate) async fn auth_post(
        &self,
        endpoint: &str,
        form: &[(&str, &str)],
    ) -> Result<(StatusCode, String), Error> {
        let response = self
            .http()
            .post(format!("{TIDAL_AUTH_URL}/{endpoint}"))
            .form(form)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.text().await.unwrap_or_default()))
    }

    /// Single egress point for API traffic. Consults the cooldown before
    /// sending and records a new one from any 429. Never sleeps.
    async fn send(&self, request: reqwest::Request) -> Result<reqwest::Response, Error> {
        if let Some(secs) = self.inner.gate.cooling_down() {
            return Err(Error::rate_limited(secs));
        }
        let resp = self.inner.http.execute(request).await?;
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            let secs = rate_gate::retry_after_or_default(resp.headers());
            self.inner.gate.trip(secs);
            // Report the gate's own view, not the raw header: `trip` clamps,
            // and a concurrent longer cooldown may already be in force.
            let secs = self.inner.gate.cooling_down().unwrap_or(secs);
            log::warn!("[send] rate limited, cooling down {secs}s");
            return Err(Error::rate_limited(secs));
        }
        Ok(resp)
    }

    /// Send with the access token. A 401 refreshes the token (once, however
    /// many requests hit it together) and retries the request once.
    pub(crate) async fn send_authed(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, Error> {
        let mut request = req.build()?;
        let retry = request.try_clone();
        let token = self.access_token()?;
        set_bearer(&mut request, &token)?;
        let response = self.send(request).await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        let url = response.url().to_string();
        let body = response.text().await.unwrap_or_default();
        // A 401 carrying a playbackinfo sub-status is not auth expiry, and a
        // refresh would never change the answer.
        if is_playbackinfo_sub_status(&body) {
            log::warn!(
                "[send_authed] {url} -> 401 playbackinfo sub-status, not retrying: {}",
                body.chars().take(200).collect::<String>()
            );
            return Err(Error::Api { status: 401, body });
        }
        let Some(mut retry) = retry else {
            return Err(Error::Api { status: 401, body });
        };
        log::debug!("Got 401 from {url}, refreshing");
        let token = self.refresh_after_401(&token).await?;
        set_bearer(&mut retry, &token)?;
        self.send(retry).await
    }

    /// Refresh the tokens after `stale` got a 401, and return the access token
    /// to retry with. If another request already refreshed while this one
    /// waited for the lock, its token is reused.
    async fn refresh_after_401(&self, stale: &str) -> Result<String, Error> {
        let _guard = self.inner.refresh.lock().await;
        let (current, login_method) = {
            let session = self.read();
            let tokens = session.tokens.clone().ok_or(Error::NotAuthenticated)?;
            (tokens, session.login_method)
        };
        if current.access_token != stale {
            return Ok(current.access_token);
        }

        let result = self.request_refresh(&current, login_method).await;
        let mut session = self.write();
        // A sign-out or another sign-in while the refresh was in flight wins
        // over whatever the refresh returned.
        let replaced = session
            .tokens
            .as_ref()
            .is_none_or(|t| t.refresh_token != current.refresh_token);
        if replaced {
            return session
                .tokens
                .as_ref()
                .map(|t| t.access_token.clone())
                .ok_or(Error::NotAuthenticated);
        }
        match result {
            Ok(tokens) => {
                session.tokens = Some(tokens.clone());
                drop(session);
                let access_token = tokens.access_token.clone();
                let _ = self.inner.events.send(Event::TokensRefreshed(tokens));
                Ok(access_token)
            }
            Err(Error::SessionExpired) => {
                session.tokens = None;
                drop(session);
                log::warn!("[refresh] refresh token rejected, Session expired");
                let _ = self.inner.events.send(Event::SessionExpired);
                Err(Error::SessionExpired)
            }
            Err(e) => Err(e),
        }
    }

    /// Exchange the refresh token for new tokens. `invalid_grant` is
    /// `Error::SessionExpired`; every other failure leaves the Session alone.
    async fn request_refresh(
        &self,
        current: &AuthTokens,
        login_method: LoginMethod,
    ) -> Result<AuthTokens, Error> {
        let credentials = login_method.credentials()?;
        let form = credentials.form(&[
            ("refresh_token", current.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ]);
        let (status, body) = self.auth_post("token", &form).await?;
        if !status.is_success() {
            if is_invalid_grant(&body) {
                return Err(Error::SessionExpired);
            }
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        // The refresh response may leave out refresh_token and user_id.
        #[derive(serde::Deserialize)]
        struct RefreshResponse {
            access_token: String,
            #[serde(default)]
            refresh_token: Option<String>,
            expires_in: u64,
            token_type: String,
            #[serde(default)]
            user_id: Option<u64>,
        }
        let parsed = serde_json::from_str::<RefreshResponse>(&body)
            .map_err(|e| Error::Parse(format!("refresh response: {e}")))?;
        Ok(AuthTokens {
            access_token: parsed.access_token,
            refresh_token: parsed
                .refresh_token
                .unwrap_or_else(|| current.refresh_token.clone()),
            expires_in: parsed.expires_in,
            token_type: parsed.token_type,
            user_id: parsed.user_id.or(current.user_id),
        })
    }

    /// Authenticated GET, status checked, JSON body deserialized into `T`.
    pub(crate) async fn api_get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, Error> {
        let body = self.api_get_body(path, query).await?;
        serde_json::from_str(&body)
            .map_err(|e| Error::Parse(format!("{} - Body: {}", e, &body[..body.len().min(500)])))
    }

    /// Authenticated GET, status checked, raw body returned. `path` is either
    /// a full URL or a path under the v1 API.
    pub(crate) async fn api_get_body(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<String, Error> {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{TIDAL_API_URL}{path}")
        };
        let response = self.authenticated_get(&url, query).await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            if url.contains("/users/") || url.contains("/sessions") {
                log::error!(
                    "[api_get_body] {url} -> status={status} body=<redacted: account endpoint>"
                );
            } else {
                log::error!(
                    "[api_get_body] {url} -> status={status} body={}",
                    &body[..body.len().min(500)]
                );
            }
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(body)
    }

    /// Authenticated GET. v2 endpoints also get the client-version header.
    pub(crate) async fn authenticated_get(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, Error> {
        let mut req = self.http().get(url).query(query);
        if url.contains("/v2/") {
            req = req.header("x-tidal-client-version", TIDAL_CLIENT_VERSION);
        }
        self.send_authed(req).await
    }
}

fn set_bearer(request: &mut reqwest::Request, token: &str) -> Result<(), Error> {
    let value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| Error::Parse("access token is not a valid header value".into()))?;
    request.headers_mut().insert(AUTHORIZATION, value);
    Ok(())
}

/// The token endpoint's answer to a refresh token it will never accept again.
fn is_invalid_grant(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.as_str().map(|e| e == "invalid_grant"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::is_invalid_grant;

    #[test]
    fn only_invalid_grant_is_session_expiry() {
        assert!(is_invalid_grant(
            r#"{"status":400,"error":"invalid_grant","sub_status":11101,"error_description":"Token could not be verified"}"#
        ));
        for body in [
            r#"{"status":400,"error":"invalid_request"}"#,
            r#"{"status":401,"error":"invalid_client"}"#,
            r#"{"status":500,"error":"server_error"}"#,
            r#"{"error_description":"invalid_grant"}"#,
            "invalid_grant",
            "",
        ] {
            assert!(!is_invalid_grant(body), "{body}");
        }
    }
}
