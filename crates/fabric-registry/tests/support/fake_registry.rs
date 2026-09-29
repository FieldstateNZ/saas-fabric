//! A registry holding manifests, blobs and tags in memory, content-addressed
//! by real SHA-256 digests.

mod artifacts;
mod modes;
mod publish;
mod respond;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use crate::support::http_server::{self, RecordedRequest};

pub use artifacts::Listed;

/// The name repositories are published under, whatever socket answers.
pub const HOST: &str = "ghcr.io";

/// How the fake answers `HEAD` on a manifest.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Head {
    /// With the manifest's digest, as GHCR and Docker Hub do.
    #[default]
    WithDigest,

    /// With no digest header at all.
    WithoutDigest,

    /// `405`, as a registry that does not answer `HEAD` does.
    NotAllowed,
}

/// A reply a test fixed for a path, ahead of everything else.
#[derive(Clone)]
struct Fixed {
    method: String,
    path: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// How the fake behaves, beyond what it holds: each switched on by a test.
#[derive(Default)]
struct Modes {
    /// Whether the CDN redirects each blob once more, to itself.
    cdn_second_hop: bool,
    /// Whether every body is sent chunked, with no `Content-Length`.
    unlengthed: bool,
    /// Whether a manifest is `404` unless its media type was accepted.
    strict_accept: bool,
}

/// The fake's state.
#[derive(Default)]
struct State {
    /// `(repository path, tag)` to the digest it points at.
    tags: BTreeMap<(String, String), String>,
    /// `(repository path, digest)` to a manifest's bytes.
    manifests: BTreeMap<(String, String), String>,
    /// `(repository path, digest)` to a blob's bytes.
    blobs: BTreeMap<(String, String), String>,
    /// `(repository path, subject)` to the referrers API's entries.
    referrers: BTreeMap<(String, String), Vec<serde_json::Value>>,
    /// `(repository path, subject)` to the referrers tag schema's entries.
    tag_schema: BTreeMap<(String, String), Vec<serde_json::Value>>,
    /// Digests served as bytes that do not hash to them.
    corrupt: BTreeSet<String>,
    /// Tokens the fake has minted.
    mints: u64,
    /// A token the fake refuses, as an aged-out one would be.
    stale_token: Option<String>,
    /// Whether every tag listing is answered one tag at a time.
    paginate: bool,
    /// An origin to write `Link` targets under, instead of a bare path.
    link_origin: Option<String>,
    /// A status `tags/list` answers with instead of a listing.
    tags_status: Option<u16>,
    /// Whether the referrers API is served. GHCR does not serve it.
    serves_referrers: bool,
    /// How `HEAD` on a manifest is answered.
    head: Head,
    /// A `Docker-Content-Digest` to answer with instead of the real one.
    digest_header: Option<String>,
    /// The key a token response names its bearer under.
    token_key: Option<String>,
    /// Where blobs are redirected to, when a CDN is running.
    cdn: Option<String>,
    /// How it behaves, beyond what it holds.
    modes: Modes,
    /// Replies fixed by a test.
    fixed: Vec<Fixed>,
}

/// A registry answering over a socket, and optionally a CDN beside it.
pub struct FakeRegistry {
    /// Where it is listening.
    pub base_url: String,
    /// Its state.
    state: Arc<Mutex<State>>,
    /// Every request it received.
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    /// Every request its CDN received.
    cdn_requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl FakeRegistry {
    /// Starts an empty registry.
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responder_state = Arc::clone(&state);

        let base_url = http_server::start(
            Arc::new(move |request| respond::respond(&responder_state, request)),
            Arc::clone(&requests),
        )
        .await;

        Self {
            base_url,
            state,
            requests,
            cdn_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Starts a registry whose blobs are redirected to a second server, on
    /// another origin, as every hosted registry's are to a CDN.
    pub async fn start_with_cdn() -> Self {
        let fake = Self::start().await;
        let cdn_state = Arc::clone(&fake.state);
        let cdn = http_server::start(
            Arc::new(move |request| respond::cdn(&cdn_state, request)),
            Arc::clone(&fake.cdn_requests),
        )
        .await;
        fake.locked().cdn = Some(cdn);
        fake
    }

    /// Every request the registry received.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Every request the CDN received.
    pub fn cdn_requests(&self) -> Vec<RecordedRequest> {
        self.cdn_requests.lock().unwrap().clone()
    }

    /// Every request path the registry was asked for.
    pub fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|request| request.path).collect()
    }

    /// How many `method` requests had a path containing `fragment`.
    pub fn count(&self, method: &str, fragment: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.method == method && request.path.contains(fragment))
            .count()
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

/// A repository's API path: the name with the registry host stripped.
fn path_of(repository: &str) -> String {
    repository
        .strip_prefix(&format!("{HOST}/"))
        .unwrap_or(repository)
        .to_owned()
}

/// `sha256:` and the hex of `bytes`' SHA-256: the fake's own digests are
/// real, so an adapter that hashed nothing, or hashed wrongly, is caught.
pub fn sha256(bytes: &[u8]) -> String {
    let hash = ring::digest::digest(&ring::digest::SHA256, bytes);
    hash.as_ref().iter().fold("sha256:".to_owned(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}
