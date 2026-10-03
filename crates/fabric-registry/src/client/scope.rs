//! What a request is for, and what it has been told to present.
//!
//! # Why tokens are held by repository and by credential
//!
//! A token is issued for one scope, and — when a credential was presented
//! for it — carries that credential's authority. Holding it under the
//! repository path *and* whether it was credentialed means an anonymous
//! request never borrows a credentialed token, and a token never outlives
//! its credential: replacing one builds a new client, with nothing held.

use std::collections::BTreeMap;
use std::sync::Mutex;

use reqwest::RequestBuilder;

use crate::client::OciRegistry;

/// What a request is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope<'a> {
    /// Proving the registry's `/v2/` endpoint: a token with no scope.
    Registry,

    /// Reading one repository, by API path: `repository:<path>:pull`.
    Repository(&'a str),
}

/// What a request presents, once a challenge has asked for it.
#[derive(Clone)]
pub(crate) enum Held {
    /// A token from the realm.
    Bearer(String),

    /// The credential itself, as HTTP `Basic`, to a distribution registry's
    /// own origin.
    Basic,
}

/// What is held, by repository path and whether it was credentialed.
pub(crate) type Holdings = Mutex<BTreeMap<(String, bool), Held>>;

impl OciRegistry {
    /// What is held for `scope`, presenting or not. Nothing for proving,
    /// which is asked fresh every time.
    pub(super) fn held(&self, scope: Scope<'_>, presents: bool) -> Option<Held> {
        let Scope::Repository(path) = scope else {
            return None;
        };
        self.tokens
            .lock()
            .ok()?
            .get(&(path.to_owned(), presents))
            .cloned()
    }

    /// Holds `held` for `scope`, presenting or not.
    pub(super) fn hold(&self, scope: Scope<'_>, presents: bool, held: &Held) {
        if let (Scope::Repository(path), Ok(mut tokens)) = (scope, self.tokens.lock()) {
            tokens.insert((path.to_owned(), presents), held.clone());
        }
    }

    /// `request` with `held` attached, if anything is.
    ///
    /// Only ever called for a request to the registry's own origin: a caller
    /// following a redirect decides that first.
    pub(super) fn authorize(&self, request: RequestBuilder, held: Option<&Held>) -> RequestBuilder {
        match (held, &self.credential) {
            (Some(Held::Bearer(token)), _) => request.bearer_auth(token),
            (Some(Held::Basic), Some(credential)) => {
                request.basic_auth(&credential.username, Some(credential.secret.expose()))
            }
            (Some(Held::Basic), None) | (None, _) => request,
        }
    }
}
