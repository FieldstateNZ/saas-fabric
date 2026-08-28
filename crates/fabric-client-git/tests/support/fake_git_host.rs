//! A stateful stand-in for the contents API.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::support::http_server::{self, RecordedRequest};

/// A file the fake holds.
#[derive(Clone)]
struct Blob {
    /// The file's text.
    text: String,

    /// Its current hash.
    sha: String,
}

/// The fake's whole state.
#[derive(Default)]
struct State {
    /// Files, keyed by repository-relative path.
    files: BTreeMap<String, Blob>,

    /// Directories that exist even with no file in them.
    directories: BTreeSet<String>,

    /// How many writes have been accepted, which is where hashes come from.
    writes: u64,

    /// How many installation tokens have been minted.
    mints: u64,

    /// A bearer the host refuses with `401`, as a revoked token would be.
    rejected_bearer: Option<String>,

    /// Whether every bearer is refused, as a revoked installation would be.
    reject_all: bool,

    /// Whether the host reports itself rate limited.
    rate_limited: bool,
}

/// A Git host holding files in memory.
pub struct FakeGitHost {
    /// Where it is listening.
    pub base_url: String,

    /// The files it holds.
    state: Arc<Mutex<State>>,

    /// Every request it received.
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl FakeGitHost {
    /// Starts a host holding the given files, keyed by path.
    pub async fn start(files: &[(&str, &str)]) -> Self {
        let mut state = State::default();

        for (index, (path, text)) in files.iter().enumerate() {
            state.files.insert(
                (*path).to_owned(),
                Blob {
                    text: (*text).to_owned(),
                    sha: format!("sha-{index}"),
                },
            );
            if let Some((directory, _)) = path.rsplit_once('/') {
                state.directories.insert(directory.to_owned());
            }
        }

        let state = Arc::new(Mutex::new(state));
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

    /// Makes the host reject one specific bearer with `401`.
    pub fn reject_bearer(&self, bearer: String) {
        self.state.lock().unwrap().rejected_bearer = Some(bearer);
    }

    /// Makes the host reject every bearer with `401`.
    pub fn reject_all_bearers(&self) {
        self.state.lock().unwrap().reject_all = true;
    }

    /// Makes the host answer as though the installation is rate limited.
    pub fn rate_limit(&self) {
        self.state.lock().unwrap().rate_limited = true;
    }

    /// Declares a directory that holds no file.
    pub fn add_empty_directory(&self, path: &str) {
        self.state.lock().unwrap().directories.insert(path.to_owned());
    }

    /// The hash a path is currently at.
    pub fn sha(&self, path: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .files
            .get(path)
            .map(|blob| blob.sha.clone())
    }

    /// The text currently stored at a path.
    pub fn text(&self, path: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .files
            .get(path)
            .map(|blob| blob.text.clone())
    }

    /// Replaces a path's content as if someone else had committed.
    pub fn overwrite(&self, path: &str, text: &str) {
        let mut state = self.state.lock().unwrap();
        state.writes += 1;
        let sha = format!("sha-other-{}", state.writes);

        state.files.insert(
            path.to_owned(),
            Blob {
                text: text.to_owned(),
                sha,
            },
        );
    }

    /// Every request received so far.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// The requests that used a given method.
    pub fn requests_with(&self, method: &str) -> Vec<RecordedRequest> {
        self.requests()
            .into_iter()
            .filter(|request| request.method == method)
            .collect()
    }
}

/// The stem of the bearer the fake mints for a GitHub App installation.
///
/// Each mint appends its ordinal — `…-1`, `…-2` — so a test can tell a
/// *replaced* token from a reused one, which is the whole subject of the
/// rejection-retry behaviour.
pub const MINTED_TOKEN: &str = "ghs_mintedinstallationtoken";

/// Answers one request.
fn respond(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> (u16, String) {
    // The installation-token endpoint is not a contents path. It is answered
    // here rather than by a test's own responder so that every test exercising
    // the App posture mints through the same code the real host would.
    if request.path.contains("/access_tokens") {
        let mut held = state.lock().unwrap();
        held.mints += 1;

        return (
            201,
            serde_json::json!({
                "token": format!("{MINTED_TOKEN}-{}", held.mints),
                // An hour after the fixture clock the tests run at, so the
                // adapter caches rather than re-minting per request.
                "expires_at": "2023-11-14T23:13:20Z",
            })
            .to_string(),
        );
    }

    {
        let held = state.lock().unwrap();

        if held.rate_limited {
            // GitHub's shape for an exhausted quota: 403 with no remaining
            // requests. A fresh token does not fix it.
            return (403, r#"{"message":"API rate limit exceeded"}"#.to_owned());
        }

        let bearer = request
            .authorization
            .as_deref()
            .map(|value| value.trim_start_matches("Bearer ").to_owned());

        if held.reject_all || (held.rejected_bearer.is_some() && held.rejected_bearer == bearer) {
            return (401, r#"{"message":"Bad credentials"}"#.to_owned());
        }
    }

    let path = repository_path(&request.path);

    match request.method.as_str() {
        "GET" => get(state, &path),
        "PUT" => put(state, &path, &request.body),
        _ => (405, "{}".to_owned()),
    }
}

/// Reads a file or a directory.
fn get(state: &Arc<Mutex<State>>, path: &str) -> (u16, String) {
    let state = state.lock().unwrap();

    if let Some(blob) = state.files.get(path) {
        return (
            200,
            serde_json::json!({
                "type": "file",
                "name": path.rsplit('/').next().unwrap_or(path),
                "sha": blob.sha,
                "content": BASE64.encode(&blob.text),
            })
            .to_string(),
        );
    }

    let children: BTreeSet<&str> = state
        .files
        .keys()
        .chain(state.directories.iter())
        .filter_map(|stored| stored.strip_prefix(&format!("{path}/")))
        .filter_map(|rest| rest.split('/').next())
        .collect();

    if children.is_empty() {
        return (404, r#"{"message":"Not Found"}"#.to_owned());
    }

    let entries: Vec<serde_json::Value> = children
        .into_iter()
        .map(|name| {
            let is_file = state.files.contains_key(&format!("{path}/{name}"));
            serde_json::json!({
                "type": if is_file { "file" } else { "dir" },
                "name": name,
                "sha": "sha-dir",
            })
        })
        .collect();

    (200, serde_json::Value::Array(entries).to_string())
}

/// Writes a file, refusing a stale hash.
fn put(state: &Arc<Mutex<State>>, path: &str, body: &str) -> (u16, String) {
    let Ok(request) = serde_json::from_str::<serde_json::Value>(body) else {
        return (400, "{}".to_owned());
    };

    let expected = request["sha"].as_str().unwrap_or_default();
    let encoded = request["content"].as_str().unwrap_or_default();
    let Ok(decoded) = BASE64.decode(encoded) else {
        return (400, "{}".to_owned());
    };

    let mut state = state.lock().unwrap();

    match state.files.get(path) {
        None => return (404, r#"{"message":"Not Found"}"#.to_owned()),
        Some(blob) if blob.sha != expected => {
            return (409, r#"{"message":"is at a different sha"}"#.to_owned())
        }
        Some(_) => {}
    }

    state.writes += 1;
    let sha = format!("sha-written-{}", state.writes);

    state.files.insert(
        path.to_owned(),
        Blob {
            text: String::from_utf8_lossy(&decoded).to_string(),
            sha: sha.clone(),
        },
    );

    (200, serde_json::json!({"content": {"sha": sha}}).to_string())
}

/// Strips the repository prefix and any query string from a request path.
fn repository_path(request_path: &str) -> String {
    request_path
        .split('?')
        .next()
        .unwrap_or(request_path)
        .split("/contents/")
        .nth(1)
        .unwrap_or_default()
        .to_owned()
}
