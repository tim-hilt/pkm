//! OAuth 2.0 sign-in with PKCE (RFC 7636) and token refresh, shared by the
//! Cloud Providers that sign in (Google Drive, later OneDrive; ADR-0012).
//!
//! The client only shows the sign-in page and hands back the redirect; the
//! code exchange and every later refresh are here. A rotated refresh token is
//! handed to the client through a callback, which stores it in the keychain.

use std::fs::File;
use std::io::{self, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
use excalidraw_native_core::json;
use ureq::Agent;

use crate::percent::{decode, encode};
use crate::sha256::sha256;
use crate::storage::ConnectError;

/// An access token is refreshed this long before it expires.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// Hears of a refresh token that replaced the one in use.
pub(crate) type OnRotate = Arc<dyn Fn(&str) + Send + Sync>;

/// What differs between the OAuth providers.
#[derive(Debug, Clone)]
pub(crate) struct OAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub scope: String,
    pub redirect_uri: String,
    /// More query parameters of the sign-in page's URL.
    pub extra: Vec<(String, String)>,
}

/// `n` random bytes from the system, as base64url without padding.
fn random_token(n: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; n];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(bytes))
}

/// A sign-in under way: the page to show, and what is needed to finish it.
pub(crate) struct SignIn {
    config: OAuthConfig,
    verifier: String,
    state: String,
    url: String,
}

impl SignIn {
    pub fn start(config: OAuthConfig) -> io::Result<Self> {
        let verifier = random_token(32)?;
        let state = random_token(16)?;
        let challenge = BASE64_URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()));
        let mut query = vec![
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", &config.redirect_uri),
            ("response_type", "code"),
            ("scope", &config.scope),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
        ];
        query.extend(config.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let url = format!("{}?{}", config.auth_url, form(&query));
        Ok(Self {
            config,
            verifier,
            state,
            url,
        })
    }

    /// The sign-in page to show.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The URL scheme the page redirects to, for the client to listen for.
    pub fn callback_scheme(&self) -> String {
        let uri = &self.config.redirect_uri;
        uri.split(':').next().unwrap_or(uri).to_owned()
    }

    /// Finishes with the `redirect` the page led to: refusing consent is
    /// `AccessDenied`, a redirect that is not the answer to this sign-in is
    /// `SignInFailed`, and the code is exchanged for a `Session`.
    pub fn finish(
        self,
        agent: Agent,
        redirect: &str,
        on_rotate: OnRotate,
    ) -> Result<Session, ConnectError> {
        let query = redirect
            .strip_prefix(&self.config.redirect_uri)
            .and_then(|rest| rest.strip_prefix('?'))
            .ok_or(ConnectError::SignInFailed)?;
        let query = query.split('#').next().unwrap_or("");
        let param = |name: &str| {
            query.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == name).then(|| decode(&value.replace('+', " ")))
            })
        };
        if param("state").as_deref() != Some(&self.state) {
            return Err(ConnectError::SignInFailed);
        }
        if let Some(error) = param("error") {
            return Err(match error.as_str() {
                "access_denied" => ConnectError::AccessDenied,
                _ => ConnectError::SignInFailed,
            });
        }
        let code = param("code").ok_or(ConnectError::SignInFailed)?;
        let tokens = request_tokens(
            &agent,
            &self.config.token_url,
            &[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("code", &code),
                ("code_verifier", &self.verifier),
                ("redirect_uri", &self.config.redirect_uri),
            ],
        )
        .map_err(|e| match e {
            TokenError::Rejected(_) => ConnectError::SignInFailed,
            TokenError::Unreachable(_) => ConnectError::Unreachable,
        })?;
        let refresh_token = tokens.refresh_token.ok_or(ConnectError::SignInFailed)?;
        Ok(Session {
            agent,
            config: self.config,
            on_rotate,
            tokens: Mutex::new(Held {
                refresh_token,
                access: Some((tokens.access_token, tokens.expires)),
            }),
        })
    }
}

struct Tokens {
    access_token: String,
    expires: Instant,
    refresh_token: Option<String>,
}

enum TokenError {
    /// The server refused, with this `error` code.
    Rejected(String),
    Unreachable(io::Error),
}

/// `key=value` pairs as a URL query or a form body.
fn form(pairs: &[(&str, &str)]) -> String {
    let parts: Vec<_> = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect();
    parts.join("&")
}

fn request_tokens(agent: &Agent, url: &str, params: &[(&str, &str)]) -> Result<Tokens, TokenError> {
    let body = form(params);
    let mut response = agent
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes())
        .map_err(|e| TokenError::Unreachable(e.into_io()))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| TokenError::Unreachable(e.into_io()))?;
    let value = json::parse(&text).ok();
    let field = |name: &str| value.as_ref()?.get(name)?.as_str().map(str::to_owned);
    match status {
        200..=299 => {
            let access_token = field("access_token")
                .ok_or_else(|| TokenError::Unreachable(io::Error::other("no access token")))?;
            let seconds = value
                .as_ref()
                .and_then(|v| v.get("expires_in")?.as_f64())
                .unwrap_or(3600.0);
            Ok(Tokens {
                access_token,
                expires: Instant::now() + Duration::from_secs(seconds.max(0.0) as u64),
                refresh_token: field("refresh_token"),
            })
        }
        400..=499 => Err(TokenError::Rejected(field("error").unwrap_or_default())),
        _ => Err(TokenError::Unreachable(io::Error::other(format!(
            "token endpoint answered with status {status}"
        )))),
    }
}

struct Held {
    refresh_token: String,
    access: Option<(String, Instant)>,
}

/// A signed-in account: hands out access tokens, refreshing them when they
/// are about to expire.
pub(crate) struct Session {
    agent: Agent,
    config: OAuthConfig,
    on_rotate: OnRotate,
    tokens: Mutex<Held>,
}

impl Session {
    /// The session of an account signed in before, from its refresh token.
    /// Contacts nobody: the first access token is fetched on use.
    pub fn resume(
        agent: Agent,
        config: OAuthConfig,
        refresh_token: String,
        on_rotate: OnRotate,
    ) -> Self {
        Self {
            agent,
            config,
            on_rotate,
            tokens: Mutex::new(Held {
                refresh_token,
                access: None,
            }),
        }
    }

    /// The refresh token in use.
    pub fn refresh_token(&self) -> String {
        self.tokens.lock().unwrap().refresh_token.clone()
    }

    /// A valid access token. A refresh token the provider no longer accepts
    /// (access revoked) is `PermissionDenied`; anything else that fails, e.g.
    /// being offline, is another kind.
    pub fn access_token(&self) -> io::Result<String> {
        let mut held = self.tokens.lock().unwrap();
        if let Some((token, expires)) = &held.access
            && Instant::now() + EXPIRY_MARGIN < *expires
        {
            return Ok(token.clone());
        }
        let refreshed = request_tokens(
            &self.agent,
            &self.config.token_url,
            &[
                ("grant_type", "refresh_token"),
                ("client_id", &self.config.client_id),
                ("refresh_token", &held.refresh_token),
            ],
        )
        .map_err(|e| match e {
            TokenError::Rejected(code) if code == "invalid_grant" => {
                io::Error::new(io::ErrorKind::PermissionDenied, "access revoked")
            }
            TokenError::Rejected(code) => io::Error::other(format!("token refused: {code}")),
            TokenError::Unreachable(e) => e,
        })?;
        if let Some(rotated) = refreshed.refresh_token
            && rotated != held.refresh_token
        {
            held.refresh_token = rotated;
            (self.on_rotate)(&held.refresh_token);
        }
        held.access = Some((refreshed.access_token.clone(), refreshed.expires));
        Ok(refreshed.access_token)
    }

    /// The provider refused the access token handed out last: the next one
    /// is fetched anew.
    pub fn expire(&self) {
        self.tokens.lock().unwrap().access = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_one_of_rfc_7636() {
        // RFC 7636, appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            BASE64_URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes())),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_and_states_differ_every_time() {
        let (a, b) = (random_token(32).unwrap(), random_token(32).unwrap());
        assert_ne!(a, b);
        // 43 characters: the shortest verifier RFC 7636 allows.
        assert_eq!(a.len(), 43);
    }
}
