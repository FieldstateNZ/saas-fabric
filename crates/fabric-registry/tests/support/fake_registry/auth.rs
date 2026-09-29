//! How the fake asks to be authenticated, and the realm that issues its
//! tokens: on its own origin, or on a server of its own.

use std::sync::Arc;

use serde_json::json;

use super::{FakeRegistry, State};
use crate::support::http_server::{self, RecordedRequest, Reply};

/// What the registry answers a request with nothing acceptable to present.
#[derive(Clone, Debug, Default)]
pub enum Challenge {
    /// `401` with a `Bearer` challenge naming this realm and service; any
    /// bearer but a stale one is accepted.
    Bearer {
        /// The realm the challenge names.
        realm: String,
        /// The service it names.
        service: String,
    },

    /// `401` with a `Basic` challenge; only the expected credential is
    /// accepted.
    Basic,

    /// Never challenges: everything is read anonymously. Replaced with a
    /// `Bearer` challenge as soon as the fake has an address to name.
    #[default]
    None,
}

impl FakeRegistry {
    /// Starts a registry whose realm runs on a server of its own, on another
    /// loopback port, as Docker Hub's does on `auth.docker.io`.
    pub async fn start_with_realm() -> Self {
        let fake = Self::start().await;
        let state = Arc::clone(&fake.state);
        let realm = http_server::start(
            Arc::new(move |request| realm(&mut state.lock().unwrap(), request)),
            Arc::clone(&fake.realm_requests),
        )
        .await;
        fake.challenge(Challenge::Bearer {
            realm: format!("{realm}/token"),
            service: "registry.test".to_owned(),
        });
        fake
    }

    /// Answers every unauthenticated request with `challenge`.
    pub fn challenge(&self, challenge: Challenge) {
        self.locked().challenge = challenge;
    }

    /// The challenge's realm, if it names one.
    pub fn realm(&self) -> String {
        match &self.locked().challenge {
            Challenge::Bearer { realm, .. } => realm.clone(),
            Challenge::Basic | Challenge::None => String::new(),
        }
    }

    /// Accepts only this username and secret, at the realm and as `Basic`.
    pub fn expect_credential(&self, username: &str, secret: &str) {
        self.locked().credential = Some((username.to_owned(), secret.to_owned()));
    }

    /// Has the realm refuse every credential it is sent, with `401`.
    pub fn realm_refuses(&self) {
        self.locked().realm.refuses = true;
    }

    /// Has the realm answer `403 DENIED` to every repository scope, whoever
    /// asks — as GHCR's does for a repository it will not grant.
    pub fn realm_declines_scopes(&self) {
        self.locked().realm.declines_scopes = true;
    }

    /// Has the realm answer `403 DENIED` to an anonymous request for no
    /// scope, as GHCR's does.
    pub fn realm_declines_anonymous_unscoped(&self) {
        self.locked().realm.declines_anonymous_unscoped = true;
    }

    /// Every request the separate realm received.
    pub fn realm_requests(&self) -> Vec<RecordedRequest> {
        self.realm_requests.lock().unwrap().clone()
    }
}

/// What the separate realm answers.
fn realm(state: &mut State, request: &RecordedRequest) -> Reply {
    if request.path.starts_with("/token") {
        return token(state, request);
    }
    Reply::json(404, "{}")
}

/// A token, a `401` for a credential the realm will not take, or a `403`
/// for a scope it declines. An anonymous request otherwise always gets one.
pub(super) fn token(state: &mut State, request: &RecordedRequest) -> Reply {
    let denied = || Reply::json(403, r#"{"errors":[{"code":"DENIED","message":"denied"}]}"#);
    let scoped = request.path.contains("scope=");
    if scoped && state.realm.declines_scopes {
        return denied();
    }
    if !scoped && request.authorization.is_none() && state.realm.declines_anonymous_unscoped {
        return denied();
    }
    if let Some(presented) = &request.authorization {
        let expected = state
            .credential
            .as_ref()
            .map(|(username, secret)| basic(username, secret));
        if state.realm.refuses || expected.as_deref() != Some(presented.as_str()) {
            return Reply::json(401, "{}");
        }
    }
    state.mints += 1;
    let key = state.token_key.clone().unwrap_or_else(|| "token".to_owned());
    Reply::json(200, json!({ key: format!("token-{}", state.mints) }).to_string())
}

/// The challenge for a request that presents nothing the registry accepts,
/// or `None` when it may be answered.
pub(super) fn challenged(state: &State, request: &RecordedRequest) -> Option<Reply> {
    let presented = request.authorization.as_deref();
    let (accepted, header) = match &state.challenge {
        Challenge::None => return None,
        Challenge::Bearer { realm, service } => {
            let refused = state.stale_token.as_ref().map(|token| format!("Bearer {token}"));
            let accepted = presented.is_some_and(|presented| {
                presented.starts_with("Bearer ") && Some(presented) != refused.as_deref()
            });
            (accepted, format!(r#"Bearer realm="{realm}",service="{service}""#))
        }
        Challenge::Basic => {
            let expected = state
                .credential
                .as_ref()
                .map(|(username, secret)| basic(username, secret));
            (
                presented.is_some() && presented == expected.as_deref(),
                r#"Basic realm="fake""#.to_owned(),
            )
        }
    };
    (!accepted).then(|| Reply::json(401, "{}").with("WWW-Authenticate", header))
}

/// `Basic <base64(username:secret)>`, as a client sends it.
pub fn basic(username: &str, secret: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = format!("{username}:{secret}").into_bytes();
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let triple = chunk.iter().enumerate().fold(0_u32, |acc, (index, byte)| {
            acc | (u32::from(*byte) << (16 - 8 * index))
        });
        for index in 0..4 {
            if index <= chunk.len() {
                encoded.push(ALPHABET[((triple >> (18 - 6 * index)) & 63) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    format!("Basic {encoded}")
}
