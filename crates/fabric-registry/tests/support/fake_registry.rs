//! A registry holding manifests, blobs and tags in memory.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::support::http_server::{self, RecordedRequest, Reply};

/// The name repositories are published under, whatever socket answers.
pub const HOST: &str = "ghcr.io";

/// One published tag.
#[derive(Clone)]
struct Tagged {
    /// The manifest body.
    manifest: String,

    /// Its digest, as the registry reports it.
    digest: String,

    /// Config blobs this manifest can reach, by digest.
    blobs: BTreeMap<String, String>,
}

/// The fake's state.
#[derive(Default)]
struct State {
    /// `(repository path, tag or digest)` to what it serves.
    published: BTreeMap<(String, String), Tagged>,

    /// Tokens the fake has minted.
    mints: u64,

    /// A token the fake refuses, as an aged-out one would be.
    stale_token: Option<String>,

    /// Whether every tag listing is answered one tag at a time.
    paginate: bool,

    /// A status `tags/list` answers with instead of a listing.
    tags_status: Option<u16>,
}

/// A registry answering over a socket.
pub struct FakeRegistry {
    /// Where it is listening.
    pub base_url: String,

    /// Its state.
    state: Arc<Mutex<State>>,

    /// Every request it received.
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl FakeRegistry {
    /// Starts an empty registry.
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responder_state = Arc::clone(&state);

        let base_url = http_server::start(
            Arc::new(move |request| respond(&responder_state, request)),
            Arc::clone(&requests),
        )
        .await;

        Self {
            base_url,
            state,
            requests,
        }
    }

    /// Publishes a single-architecture image carrying a revision label.
    pub fn publish(&self, repository: &str, tag: &str, revision: &str) {
        let config_digest = format!("sha256:config-{revision}");
        let manifest = format!(
            r#"{{"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"digest":"{config_digest}"}}}}"#
        );
        let blob =
            format!(r#"{{"config":{{"Labels":{{"org.opencontainers.image.revision":"{revision}"}}}}}}"#);

        self.insert(
            repository,
            tag,
            Tagged {
                digest: digest_of(&format!("{repository}{tag}")),
                manifest,
                blobs: BTreeMap::from([(config_digest, blob)]),
            },
        );
    }

    /// Publishes an image with no labels at all.
    pub fn publish_unlabelled(&self, repository: &str, tag: &str) {
        let config_digest = "sha256:config-bare".to_owned();
        let manifest = format!(
            r#"{{"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"digest":"{config_digest}"}}}}"#
        );

        self.insert(
            repository,
            tag,
            Tagged {
                digest: digest_of(&format!("{repository}{tag}")),
                manifest,
                blobs: BTreeMap::from([(config_digest, "{}".to_owned())]),
            },
        );
    }

    /// Publishes a multi-architecture index, one revision per architecture.
    ///
    /// Takes a revision per child rather than one for the index, so a test can
    /// publish children that disagree -- which is the case the adapter has to
    /// notice rather than believe the first one it reads.
    pub fn publish_index(&self, repository: &str, tag: &str, children: &[(&str, &str)]) {
        let index_digest = digest_of(&format!("{repository}{tag}index"));
        let mut entries: Vec<String> = children
            .iter()
            .map(|(architecture, _)| {
                let child = digest_of(&format!("{repository}{tag}{architecture}"));
                format!(
                    r#"{{"digest":"{child}","platform":{{"os":"linux","architecture":"{architecture}"}}}}"#
                )
            })
            .collect();

        // What buildx puts in the same index for a provenance attestation. It
        // carries no revision label, and an adapter that inspected it would
        // report every multi-architecture image as unprovenanced.
        entries.push(format!(
            r#"{{"digest":"{}","platform":{{"os":"unknown","architecture":"unknown"}}}}"#,
            digest_of(&format!("{repository}{tag}attestation"))
        ));

        // Each per-platform manifest is addressable by its own digest.
        for (architecture, revision) in children {
            let child = digest_of(&format!("{repository}{tag}{architecture}"));
            let config_digest = format!("sha256:config-{revision}-{architecture}");
            self.insert(
                repository,
                &child,
                Tagged {
                    digest: child.clone(),
                    manifest: format!(
                        r#"{{"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"digest":"{config_digest}"}}}}"#
                    ),
                    blobs: BTreeMap::from([(
                        config_digest,
                        format!(
                            r#"{{"config":{{"Labels":{{"org.opencontainers.image.revision":"{revision}"}}}}}}"#
                        ),
                    )]),
                },
            );
        }

        self.insert(
            repository,
            tag,
            Tagged {
                digest: index_digest,
                manifest: format!(
                    r#"{{"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{}]}}"#,
                    entries.join(",")
                ),
                blobs: BTreeMap::new(),
            },
        );
    }

    /// The digest the fake actually stored for a tag.
    ///
    /// Read back from its state rather than recomputed. A test comparing the
    /// adapter's answer against a second call to the same hash function would
    /// pass even if both were wrong; this compares it against what was served.
    pub fn digest_for(&self, repository: &str, tag: &str) -> String {
        let path = repository.strip_prefix(&format!("{HOST}/")).unwrap_or(repository);

        self.locked()
            .published
            .get(&(path.to_owned(), tag.to_owned()))
            .unwrap_or_else(|| panic!("{repository}:{tag} was never published"))
            .digest
            .clone()
    }

    /// Answers every tag listing one tag at a time, with a `Link` header.
    pub fn paginate(&self) {
        self.locked().paginate = true;
    }

    /// Answers `tags/list` with a status instead of a listing.
    pub fn tags_answer(&self, status: u16) {
        self.locked().tags_status = Some(status);
    }

    /// Refuses the next token the fake minted, as an aged-out one would be.
    pub fn expire_the_current_token(&self) {
        let mut state = self.locked();
        state.stale_token = Some(format!("token-{}", state.mints));
    }

    /// How many tokens have been minted.
    pub fn mints(&self) -> u64 {
        self.locked().mints
    }

    /// Every request path the fake was asked for.
    pub fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.path.clone())
            .collect()
    }

    fn insert(&self, repository: &str, reference: &str, tagged: Tagged) {
        let path = repository.strip_prefix(&format!("{HOST}/")).unwrap_or(repository);
        self.locked()
            .published
            .insert((path.to_owned(), reference.to_owned()), tagged);
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

/// A stable, obviously-fake digest.
fn digest_of(seed: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("sha256:{hash:016x}{hash:016x}{hash:016x}{hash:016x}")
}

/// Answers one request.
fn respond(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> Reply {
    let mut state = state.lock().unwrap();

    if request.path.starts_with("/token") {
        state.mints += 1;
        return Reply::json(200, format!(r#"{{"token":"token-{}"}}"#, state.mints));
    }

    // A token the fake has decided is no longer good. The adapter must notice
    // the `401`, mint another, and retry -- rather than failing the pass.
    if let Some(stale) = state.stale_token.clone() {
        if request.authorization.as_deref() == Some(&format!("Bearer {stale}")) {
            return Reply::json(401, "{}");
        }
    }

    let Some(rest) = request.path.strip_prefix("/v2/") else {
        return Reply::json(404, "{}");
    };

    if let Some((repository, query)) = rest.split_once("/tags/list") {
        return tags(&state, repository, query);
    }

    if let Some((repository, reference)) = split_on(rest, "/manifests/") {
        return match state.published.get(&(repository, reference)) {
            // The digest a deployment pins comes from this header, not from
            // the body: for an index it is the index's own.
            Some(tagged) => {
                Reply::json(200, tagged.manifest.clone()).with("Docker-Content-Digest", tagged.digest.clone())
            }
            None => Reply::json(404, "{}"),
        };
    }

    if let Some((repository, digest)) = split_on(rest, "/blobs/") {
        for ((published, _), tagged) in &state.published {
            if published == &repository {
                if let Some(blob) = tagged.blobs.get(&digest) {
                    return Reply::json(200, blob.clone());
                }
            }
        }
        return Reply::json(404, "{}");
    }

    Reply::json(404, "{}")
}

/// `GET /v2/<name>/tags/list`.
fn tags(state: &State, repository: &str, query: &str) -> Reply {
    if let Some(status) = state.tags_status {
        return Reply::json(status, "{}");
    }

    let mut all: Vec<&String> = state
        .published
        .keys()
        .filter(|(published, reference)| published == repository && !reference.starts_with("sha256:"))
        .map(|(_, reference)| reference)
        .collect();
    all.sort();

    if !state.paginate {
        return Reply::json(200, listing(&all));
    }

    let from: usize = query
        .strip_prefix("?last=")
        .and_then(|value| value.split('&').next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let page: Vec<&&String> = all.iter().skip(from).take(1).collect();
    let body = listing(&page.into_iter().copied().collect::<Vec<&String>>());

    if from + 1 < all.len() {
        // The header the adapter has to follow, as a path on this registry.
        return Reply::json(200, body).with(
            "Link",
            format!("</v2/{repository}/tags/list?last={}>; rel=\"next\"", from + 1),
        );
    }

    Reply::json(200, body)
}

/// A tag list body.
fn listing(tags: &[&String]) -> String {
    let quoted: Vec<String> = tags.iter().map(|tag| format!("\"{tag}\"")).collect();
    format!(r#"{{"tags":[{}]}}"#, quoted.join(","))
}

/// Splits `<repository>/<what>/<reference>` on a separator.
fn split_on(rest: &str, separator: &str) -> Option<(String, String)> {
    let index = rest.rfind(separator)?;
    Some((
        rest.get(..index)?.to_owned(),
        rest.get(index + separator.len()..)?.to_owned(),
    ))
}
