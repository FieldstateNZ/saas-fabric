//! A stand-in for the Git Data API, with a branch that can be raced.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::support::http_server::{self, Delay, RecordedRequest, Traffic};

/// The account the fake repository belongs to.
pub const OWNER: &str = "fieldstatenz";

/// The fake repository's name.
pub const REPOSITORY: &str = "saas-fabric-platform";

/// The branch it serves.
pub const BRANCH: &str = "main";

/// A tree waiting to be committed: what it is layered on, and what it changes.
#[derive(Clone)]
struct Tree {
    /// The tree this one extends.
    base: String,

    /// Path to blob hash.
    entries: BTreeMap<String, String>,
}

/// A commit that has been created, whether or not the branch moved to it.
#[derive(Clone)]
struct Commit {
    /// The tree it points at.
    tree: String,

    /// The commit it was built on.
    parent: String,
}

/// The fake's whole state.
struct State {
    /// The commit the branch points at.
    head: String,

    /// Files as of each commit, so a pinned read answers honestly.
    snapshots: BTreeMap<String, BTreeMap<String, String>>,

    /// Blob content, by hash.
    blobs: BTreeMap<String, String>,

    /// Trees, by hash.
    trees: BTreeMap<String, Tree>,

    /// Commits, by hash.
    commits: BTreeMap<String, Commit>,

    /// Commits somebody else lands, one applied before each ref update.
    ///
    /// This is the whole point of the fake. It is what puts a commit on the
    /// branch in the window between the adapter reading the head and asking
    /// the branch to move.
    interference: Vec<BTreeMap<String, String>>,

    /// How many ref updates have been attempted.
    ref_updates: u64,

    /// Set if the adapter ever sent `force: true`.
    forced: bool,

    /// A status the ref update answers with instead of judging the update.
    ref_update_status: Option<u16>,
}

/// A Git host holding a branch in memory.
pub struct FakePlatformHost {
    /// Where it is listening.
    pub base_url: String,

    /// Its state.
    state: Arc<Mutex<State>>,

    /// What it received, and what it finished answering.
    traffic: Traffic,
}

impl FakePlatformHost {
    /// Starts a host whose branch holds the given files.
    pub async fn start(files: &[(&str, &str)]) -> Self {
        Self::start_delaying(files, Arc::new(|_| Duration::ZERO)).await
    }

    /// The same, but sitting on chosen requests before answering them.
    ///
    /// The delay is per request rather than global so a test can single out one
    /// call — the ref update, say — and leave the rest prompt. That is what
    /// separates "the operation gave up on a call already sent" from "the
    /// operation declined to start the next one".
    pub async fn start_delaying(files: &[(&str, &str)], delay: Delay) -> Self {
        let snapshot: BTreeMap<String, String> = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();

        let head = "commit-0".to_owned();
        let mut snapshots = BTreeMap::new();
        snapshots.insert(head.clone(), snapshot);

        let state = Arc::new(Mutex::new(State {
            head: head.clone(),
            snapshots,
            blobs: BTreeMap::new(),
            trees: BTreeMap::new(),
            commits: BTreeMap::new(),
            interference: Vec::new(),
            ref_updates: 0,
            forced: false,
            ref_update_status: None,
        }));

        let traffic = Traffic::new();
        let responder_state = Arc::clone(&state);

        let base_url = http_server::start_delaying(
            Arc::new(move |request| respond(&responder_state, request)),
            traffic.clone(),
            delay,
        )
        .await;

        Self {
            base_url,
            state,
            traffic,
        }
    }

    /// Queues a commit by somebody else, applied just before the next ref
    /// update is judged.
    pub fn someone_else_commits(&self, files: &[(&str, &str)]) {
        let changes = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();

        self.state.lock().unwrap().interference.push(changes);
    }

    /// Makes the ref update answer with a status of its own.
    ///
    /// Used to prove that a status which is *not* `409` never enters the
    /// retry path, however plausible it looks.
    pub fn ref_update_answers(&self, status: u16) {
        self.state.lock().unwrap().ref_update_status = Some(status);
    }

    /// The text the branch currently holds at a path.
    pub fn current(&self, path: &str) -> Option<String> {
        let state = self.state.lock().unwrap();
        state.snapshots.get(&state.head)?.get(path).cloned()
    }

    /// How many times the branch was asked to move.
    pub fn ref_updates(&self) -> u64 {
        self.state.lock().unwrap().ref_updates
    }

    /// Whether the adapter ever asked to force.
    pub fn was_forced(&self) -> bool {
        self.state.lock().unwrap().forced
    }

    /// Every path the fake was asked for.
    pub fn paths(&self) -> Vec<String> {
        self.traffic
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| format!("{} {}", request.method, request.path))
            .collect()
    }

    /// Every request whose answer the fake wrote in full.
    ///
    /// A request that appears in [`paths`](Self::paths) and not here was
    /// abandoned by the caller partway through, which is precisely what the
    /// operation budget must never do.
    pub fn completed(&self) -> Vec<String> {
        self.traffic.completed.lock().unwrap().clone()
    }

    /// When each request reached the fake.
    pub fn request_times(&self) -> Vec<Instant> {
        self.traffic
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.at)
            .collect()
    }
}

/// Answers one request.
fn respond(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> (u16, String) {
    let prefix = format!("/repos/{OWNER}/{REPOSITORY}");
    let Some(path) = request.path.strip_prefix(prefix.as_str()) else {
        if request.path.contains("/access_tokens") {
            return (
                200,
                r#"{"token":"minted","expires_at":"2999-01-01T00:00:00Z"}"#.to_owned(),
            );
        }
        return (404, "{}".to_owned());
    };

    let mut state = state.lock().unwrap();

    match (request.method.as_str(), path) {
        ("GET", path) if path == format!("/git/ref/heads/{BRANCH}") => {
            (200, format!(r#"{{"object":{{"sha":"{}"}}}}"#, state.head))
        }
        ("GET", path) if path.starts_with("/contents/") => contents(&state, path),
        ("GET", path) if path.starts_with("/git/commits/") => commit_of(&state, path),
        ("POST", "/git/blobs") => create_blob(&mut state, &request.body),
        ("POST", "/git/trees") => create_tree(&mut state, &request.body),
        ("POST", "/git/commits") => create_commit(&mut state, &request.body),
        ("PATCH", path) if path == format!("/git/refs/heads/{BRANCH}") => {
            update_ref(&mut state, &request.body)
        }
        _ => (404, "{}".to_owned()),
    }
}

/// `GET /contents/{path}?ref={sha}`.
fn contents(state: &State, path: &str) -> (u16, String) {
    let rest = path.trim_start_matches("/contents/");
    let (file, query) = rest.split_once('?').unwrap_or((rest, ""));
    let at = query.strip_prefix("ref=").unwrap_or(&state.head);

    let Some(text) = state.snapshots.get(at).and_then(|files| files.get(file)) else {
        return (404, "{}".to_owned());
    };

    (
        200,
        format!(
            r#"{{"sha":"{}","content":"{}"}}"#,
            blob_hash(text),
            BASE64.encode(text)
        ),
    )
}

/// `GET /git/commits/{sha}`.
fn commit_of(state: &State, path: &str) -> (u16, String) {
    let sha = path.trim_start_matches("/git/commits/");

    // Every commit that exists has a tree; the base commit's is synthesised so
    // the first write has something to layer on.
    if state.commits.contains_key(sha) || state.snapshots.contains_key(sha) {
        return (200, format!(r#"{{"tree":{{"sha":"tree-of-{sha}"}}}}"#));
    }

    (404, "{}".to_owned())
}

/// `POST /git/blobs`.
fn create_blob(state: &mut State, body: &str) -> (u16, String) {
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    let text = sent["content"].as_str().unwrap().to_owned();
    assert_eq!(
        sent["encoding"], "utf-8",
        "the adapter should send text, not base64"
    );

    let sha = blob_hash(&text);
    state.blobs.insert(sha.clone(), text);

    (201, format!(r#"{{"sha":"{sha}"}}"#))
}

/// `POST /git/trees`.
fn create_tree(state: &mut State, body: &str) -> (u16, String) {
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    let base = sent["base_tree"].as_str().unwrap().to_owned();

    let entries = sent["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            assert_eq!(entry["mode"], "100644");
            assert_eq!(entry["type"], "blob");
            (
                entry["path"].as_str().unwrap().to_owned(),
                entry["sha"].as_str().unwrap().to_owned(),
            )
        })
        .collect();

    let sha = format!("tree-{}", state.trees.len() + 1);
    state.trees.insert(sha.clone(), Tree { base, entries });

    (201, format!(r#"{{"sha":"{sha}"}}"#))
}

/// `POST /git/commits`.
fn create_commit(state: &mut State, body: &str) -> (u16, String) {
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    let parents = sent["parents"].as_array().unwrap();
    assert_eq!(parents.len(), 1, "a desired-state commit has exactly one parent");

    let sha = format!("commit-{}", state.commits.len() + 1);
    state.commits.insert(
        sha.clone(),
        Commit {
            tree: sent["tree"].as_str().unwrap().to_owned(),
            parent: parents[0].as_str().unwrap().to_owned(),
        },
    );

    (201, format!(r#"{{"sha":"{sha}"}}"#))
}

/// `PATCH /git/refs/heads/{branch}` — the fast-forward rule, and the race.
fn update_ref(state: &mut State, body: &str) -> (u16, String) {
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();

    if sent["force"].as_bool() != Some(false) {
        state.forced = true;
        return (422, "{}".to_owned());
    }

    // Somebody else lands their commit in the window between the adapter
    // reading the head and asking the branch to move.
    if let Some(changes) = state.interference.first().cloned() {
        state.interference.remove(0);
        land(state, &changes);
    }

    state.ref_updates += 1;

    if let Some(status) = state.ref_update_status {
        return (status, "{}".to_owned());
    }

    let sha = sent["sha"].as_str().unwrap().to_owned();

    let Some(commit) = state.commits.get(&sha).cloned() else {
        return (422, "{}".to_owned());
    };

    // The fast-forward rule, which is the whole concurrency mechanism: a
    // commit whose parent is not the head cannot contain the head.
    if commit.parent != state.head {
        return (409, r#"{"message":"Update is not a fast forward"}"#.to_owned());
    }

    let mut snapshot = state.snapshots.get(&state.head).cloned().unwrap_or_default();
    let tree = state.trees.get(&commit.tree).cloned().unwrap();
    for (path, blob) in tree.entries {
        snapshot.insert(path, state.blobs.get(&blob).cloned().unwrap_or_default());
    }

    state.snapshots.insert(sha.clone(), snapshot);
    state.head = sha;

    (200, r#"{"ref":"refs/heads/main"}"#.to_owned())
}

/// Applies somebody else's change as a new head.
fn land(state: &mut State, changes: &BTreeMap<String, String>) {
    let mut snapshot = state.snapshots.get(&state.head).cloned().unwrap_or_default();
    snapshot.extend(changes.clone());

    let sha = format!("external-{}", state.snapshots.len());
    state.snapshots.insert(sha.clone(), snapshot);
    state.head = sha;
}

/// Content-addressed, like the real thing: the same text is always the same
/// hash, which is what lets the adapter build blobs once and retry cheaply.
fn blob_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("blob-{hash:016x}")
}
