//! Sign-in: the embedded credential pairs, Browser login (PKCE with a pasted
//! code) and Device-code login.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::client::TidalClient;
use crate::models::{AuthTokens, DeviceAuthResponse};
use crate::{Error, config};

/// Where TIDAL sends the browser after sign-in. It is fixed for the embedded
/// PKCE client, so the user pastes the resulting URL (or its code) back.
pub const REDIRECT_URI: &str = "https://tidal.com/android/login/auth";

/// Which embedded credential pair a Session's tokens belong to. Refreshes must
/// use the same pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginMethod {
    /// Browser login (PKCE). Can stream lossless and Hi-Res.
    Browser,
    /// Device-code login. No lossless.
    DeviceCode,
}

pub(crate) struct Credentials {
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
}

impl Credentials {
    /// An auth form: `client_id`, `fields`, the scope, and `client_secret`
    /// when there is one.
    pub(crate) fn form<'a>(&'a self, fields: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut form = vec![("client_id", self.client_id.as_str())];
        form.extend_from_slice(fields);
        form.push(("scope", "r_usr w_usr w_sub"));
        if !self.client_secret.is_empty() {
            form.push(("client_secret", self.client_secret.as_str()));
        }
        form
    }
}

impl LoginMethod {
    /// Whether this build carries credentials for this method.
    pub fn is_available(self) -> bool {
        match self {
            LoginMethod::Browser => config::has_pkce_keys(),
            LoginMethod::DeviceCode => config::has_stream_keys(),
        }
    }

    pub(crate) fn credentials(self) -> Result<Credentials, Error> {
        if !self.is_available() {
            return Err(Error::NotConfigured(format!(
                "no embedded credentials for {self:?}"
            )));
        }
        Ok(match self {
            LoginMethod::Browser => Credentials {
                client_id: config::stream_key_c(),
                client_secret: config::stream_key_d(),
            },
            LoginMethod::DeviceCode => Credentials {
                client_id: config::stream_key_a(),
                client_secret: config::stream_key_b(),
            },
        })
    }
}

/// One Browser login attempt: the URL to open, and the verifier the code
/// exchange needs. "Start over" generates a new one.
#[derive(Debug, Clone)]
pub struct PkceParams {
    pub authorize_url: String,
    code_verifier: String,
    client_unique_key: String,
}

impl PkceParams {
    pub fn generate() -> Result<Self, Error> {
        let client_id = LoginMethod::Browser.credentials()?.client_id;
        let mut rng = rand::rng();
        let random_bytes: [u8; 32] = rng.random();
        let code_verifier = URL_SAFE_NO_PAD.encode(random_bytes);
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let client_unique_key = format!("{:016x}", rng.random::<u64>());

        let authorize_url = format!(
            "https://login.tidal.com/authorize?response_type=code&redirect_uri={}&client_id={}&lang=EN&appMode=android&client_unique_key={}&code_challenge={}&code_challenge_method=S256&restrict_signup=true",
            "https%3A%2F%2Ftidal.com%2Fandroid%2Flogin%2Fauth",
            client_id,
            client_unique_key,
            code_challenge,
        );

        Ok(Self {
            authorize_url,
            code_verifier,
            client_unique_key,
        })
    }
}

/// Why pasted Browser-login input yielded no code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PastedInputError {
    /// Nothing was pasted.
    Empty,
    /// TIDAL redirected with `error=`. Holds its `error_description`, or the
    /// error code when there is none.
    Denied(String),
}

/// Find the authorization code in what the user pasted: the redirect URL, its
/// query string, or the bare code. Surrounding whitespace is ignored. A query
/// string without `code` is taken whole, and the exchange will reject it.
pub fn parse_pasted_input(input: &str) -> Result<String, PastedInputError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(PastedInputError::Empty);
    }

    let query = match input.split_once('?') {
        Some((_, query)) => Some(query),
        None if input.contains('=') => Some(input),
        None => None,
    };
    let Some(query) = query else {
        return Ok(input.to_string());
    };
    let query = query.split('#').next().unwrap_or_default();
    let pairs: Vec<(String, String)> = form_urlencoded_pairs(query);
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());

    if let Some(error) = get("error") {
        let description = get("error_description").filter(|d| !d.is_empty());
        return Err(PastedInputError::Denied(description.unwrap_or(error)));
    }
    match get("code").filter(|c| !c.is_empty()) {
        Some(code) => Ok(code),
        None => Ok(input.to_string()),
    }
}

/// Decode an `application/x-www-form-urlencoded` query string.
fn form_urlencoded_pairs(query: &str) -> Vec<(String, String)> {
    // Any base works; only the query is read back.
    match reqwest::Url::parse(&format!("http://localhost/?{query}")) {
        Ok(url) => url.query_pairs().into_owned().collect(),
        Err(_) => Vec::new(),
    }
}

impl TidalClient {
    /// Start Device-code login. Poll with [`TidalClient::poll_device_token`]
    /// at the returned interval.
    pub async fn start_device_auth(&self) -> Result<DeviceAuthResponse, Error> {
        let credentials = LoginMethod::DeviceCode.credentials()?;
        let form = credentials.form(&[]);
        let (status, body) = self.auth_post("device_authorization", &form).await?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }
        serde_json::from_str::<DeviceAuthResponse>(&body)
            .map_err(|e| Error::Parse(format!("device authorization response: {e}")))
    }

    /// Poll Device-code login once. `Ok(None)` means the user hasn't approved
    /// yet. On success the client is signed in with these tokens.
    pub async fn poll_device_token(&self, device_code: &str) -> Result<Option<AuthTokens>, Error> {
        let credentials = LoginMethod::DeviceCode.credentials()?;
        let form = credentials.form(&[
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ]);
        let (status, body) = self.auth_post("token", &form).await?;

        if status.as_u16() == 400
            && (body.contains("authorization_pending") || body.contains("slow_down"))
        {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let tokens = serde_json::from_str::<AuthTokens>(&body)
            .map_err(|e| Error::Parse(format!("token response: {e}")))?;
        self.sign_in(tokens.clone(), LoginMethod::DeviceCode);
        Ok(Some(tokens))
    }

    /// Finish Browser login with the code from [`parse_pasted_input`]. On
    /// success the client is signed in with these tokens.
    pub async fn exchange_pkce_code(
        &self,
        code: &str,
        params: &PkceParams,
    ) -> Result<AuthTokens, Error> {
        let credentials = LoginMethod::Browser.credentials()?;
        // No client secret here, and sone's literal `+` scope: this is the
        // exchange sone ships and TIDAL accepts.
        let form = [
            ("code", code),
            ("client_id", credentials.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", "r_usr+w_usr+w_sub"),
            ("code_verifier", params.code_verifier.as_str()),
            ("client_unique_key", params.client_unique_key.as_str()),
        ];
        let (status, body) = self.auth_post("token", &form).await?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let tokens = serde_json::from_str::<AuthTokens>(&body)
            .map_err(|e| Error::Parse(format!("token response: {e}")))?;
        self.sign_in(tokens.clone(), LoginMethod::Browser);
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_credential_pairs_are_embedded() {
        assert!(LoginMethod::Browser.is_available());
        assert!(LoginMethod::DeviceCode.is_available());
    }

    #[test]
    fn pkce_params_are_fresh_each_time() {
        let a = PkceParams::generate().expect("browser credentials");
        let b = PkceParams::generate().expect("browser credentials");
        // A new verifier means a new challenge in the URL.
        assert_ne!(a.authorize_url, b.authorize_url);
        assert!(
            a.authorize_url
                .starts_with("https://login.tidal.com/authorize?")
        );
        assert!(a.authorize_url.contains("code_challenge_method=S256"));
    }
}
