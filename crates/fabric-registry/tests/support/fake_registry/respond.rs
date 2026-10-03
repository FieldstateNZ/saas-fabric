//! How the fake answers each request.

use std::sync::{Arc, Mutex};

use serde_json::json;

use super::publish::{OCI_INDEX, OCI_MANIFEST};
use super::State;
use crate::support::http_server::{RecordedRequest, Reply};

/// Answers one request to the registry, chunked if the fake was asked to.
pub(super) fn respond(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> Reply {
    let mut reply = registry(state, request);
    reply.unlengthed = state.lock().unwrap().modes.unlengthed;
    reply
}

/// What the registry answers one request with.
fn registry(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> Reply {
    let mut state = state.lock().unwrap();

    if let Some(fixed) = state
        .fixed
        .iter()
        .find(|fixed| fixed.method == request.method && request.path.starts_with(&fixed.path))
    {
        let mut reply = Reply::json(fixed.status, fixed.body.clone());
        reply.headers.clone_from(&fixed.headers);
        return reply;
    }

    if request.path.starts_with("/token") {
        state.mints += 1;
        let key = state.token_key.clone().unwrap_or_else(|| "token".to_owned());
        return Reply::json(200, json!({ key: format!("token-{}", state.mints) }).to_string());
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
    if let Some((repository, subject)) = split_on(rest, "/referrers/") {
        return referrers(&state, &repository, &subject);
    }
    if let Some((repository, reference)) = split_on(rest, "/manifests/") {
        return manifest(&state, request, &repository, &reference);
    }
    if let Some((repository, digest)) = split_on(rest, "/blobs/") {
        if let Some(cdn) = &state.cdn {
            return Reply::json(307, "").with("Location", format!("{cdn}/blobs/{digest}"));
        }
        let corrupt = state.corrupt.contains(&digest);
        return match state.blobs.get(&(repository, digest)) {
            Some(blob) => {
                Reply::json(200, served(blob, corrupt)).with("Content-Type", "application/octet-stream")
            }
            None => Reply::json(404, "{}"),
        };
    }

    Reply::json(404, "{}")
}

/// Answers one request to the CDN: any blob the registry holds, by digest,
/// at `/blobs/<digest>` — or, when it redirects twice, a redirect from there
/// to `/again/<digest>`, where it serves the blob.
pub(super) fn cdn(state: &Arc<Mutex<State>>, request: &RecordedRequest) -> Reply {
    let state = state.lock().unwrap();
    let (digest, again) = match request.path.strip_prefix("/again/") {
        Some(digest) => (digest, true),
        None => (request.path.strip_prefix("/blobs/").unwrap_or_default(), false),
    };

    if state.modes.cdn_second_hop && !again {
        let cdn = state.cdn.clone().unwrap_or_default();
        return Reply::json(302, "").with("Location", format!("{cdn}/again/{digest}"));
    }

    let mut reply = state
        .blobs
        .iter()
        .find(|((_, stored), _)| stored == digest)
        .map_or_else(
            || Reply::json(404, "{}"),
            |(_, blob)| Reply::json(200, served(blob, state.corrupt.contains(digest))),
        );
    reply.unlengthed = state.modes.unlengthed;
    reply
}

/// `GET` or `HEAD` `/v2/<name>/manifests/<reference>`.
fn manifest(state: &State, request: &RecordedRequest, repository: &str, reference: &str) -> Reply {
    let digest = if reference.starts_with("sha256:") {
        Some(reference.to_owned())
    } else {
        state
            .tags
            .get(&(repository.to_owned(), reference.to_owned()))
            .cloned()
    };
    let Some(body) = digest
        .as_ref()
        .and_then(|digest| state.manifests.get(&(repository.to_owned(), digest.clone())))
    else {
        return Reply::json(
            404,
            json!({ "errors": [{ "code": "MANIFEST_UNKNOWN" }] }).to_string(),
        );
    };
    let digest = digest.unwrap_or_default();
    let media_type = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["mediaType"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| OCI_MANIFEST.to_owned());

    if state.modes.strict_accept
        && !request
            .accept
            .as_deref()
            .is_some_and(|accept| accept.contains(media_type.as_str()))
    {
        return Reply::json(
            404,
            json!({ "errors": [{ "code": "MANIFEST_UNKNOWN" }] }).to_string(),
        );
    }

    if request.method == "HEAD" && state.head == super::Head::NotAllowed {
        return Reply::json(405, "");
    }

    let served = if state.corrupt.contains(&digest) {
        format!("{body} ")
    } else {
        body.clone()
    };
    let reply = Reply::json(200, served).with("Content-Type", media_type);

    if request.method == "HEAD" && state.head == super::Head::WithoutDigest {
        return reply;
    }
    reply.with(
        "Docker-Content-Digest",
        state.digest_header.clone().unwrap_or(digest),
    )
}

/// `GET /v2/<name>/referrers/<digest>`.
fn referrers(state: &State, repository: &str, subject: &str) -> Reply {
    if !state.serves_referrers {
        return Reply::json(
            404,
            json!({ "errors": [{ "code": "MANIFEST_UNKNOWN" }] }).to_string(),
        );
    }

    let entries = state
        .referrers
        .get(&(repository.to_owned(), subject.to_owned()))
        .cloned()
        .unwrap_or_default();
    let index = json!({ "schemaVersion": 2, "mediaType": OCI_INDEX, "manifests": entries });
    Reply::json(200, index.to_string()).with("Content-Type", OCI_INDEX)
}

/// `GET /v2/<name>/tags/list`.
fn tags(state: &State, repository: &str, query: &str) -> Reply {
    if let Some(status) = state.tags_status {
        return Reply::json(status, "{}");
    }

    let all: Vec<&String> = state
        .tags
        .keys()
        .filter(|(published, _)| published == repository)
        .map(|(_, tag)| tag)
        .collect();

    if !state.paginate {
        return Reply::json(200, listing(&all));
    }

    let from: usize = query
        .strip_prefix("?last=")
        .and_then(|value| value.split('&').next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let page: Vec<&String> = all.iter().skip(from).take(1).copied().collect();

    if from + 1 < all.len() {
        // The header the adapter has to follow: a path on this registry, or
        // an absolute URL when a test asked for one.
        let origin = state.link_origin.clone().unwrap_or_default();
        return Reply::json(200, listing(&page)).with(
            "Link",
            format!(
                "<{origin}/v2/{repository}/tags/list?last={}>; rel=\"next\"",
                from + 1
            ),
        );
    }

    Reply::json(200, listing(&page))
}

/// A tag list body.
fn listing(tags: &[&String]) -> String {
    json!({ "tags": tags }).to_string()
}

/// A blob's bytes as served: one byte longer, so they no longer hash to
/// their digest, when a test corrupted it.
fn served(blob: &str, corrupt: bool) -> String {
    if corrupt {
        format!("{blob} ")
    } else {
        blob.to_owned()
    }
}

/// Splits `<repository>/<what>/<reference>` on a separator.
fn split_on(rest: &str, separator: &str) -> Option<(String, String)> {
    let index = rest.rfind(separator)?;
    Some((
        rest.get(..index)?.to_owned(),
        rest.get(index + separator.len()..)?.to_owned(),
    ))
}
