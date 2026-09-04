//! The one streamed GET that reads a chart index, and its size bound.

use fabric_platform_management::RegistryError;

use super::transport::shown;
use crate::errors::{status_failure, transport_failure};

/// How much of an index this will read.
///
/// A chart repository serves every chart it holds in one document, and a busy
/// one is large: the index this platform reads today is around 200 kB for 220
/// releases. Eight megabytes is far past any of that and still a bound — an
/// unbounded read of a remote document is an unbounded allocation decided by
/// somebody else.
const MOST: usize = 8 * 1024 * 1024;

/// Reads `url`'s body as text, refusing anything past [`MOST`] bytes.
///
/// Exactly one request per call: a redirect the transport policy allows is
/// followed by the HTTP client itself, inside this same `.send()`, never by
/// this function calling out again.
///
/// `url` reaches here only after `transport::validated_index_url` has
/// already refused any userinfo, query, or fragment on it, so every message
/// below could safely format it directly. It goes through
/// [`shown`](super::transport::shown) anyway: one rule for how a URL is
/// shown, applied everywhere this crate names one, is worth more than an
/// exception that is only correct as long as nothing upstream of this
/// function ever changes.
///
/// # Errors
///
/// [`RegistryError::Unavailable`] if the request could not be sent, timed
/// out, or the repository answered with a server error or an exhausted rate
/// limit. [`RegistryError::Refused`] if a redirect left the transport
/// policy, if the response status was otherwise unsuccessful, if the body
/// passed [`MOST`] bytes, or if it was not valid UTF-8.
pub(super) async fn bounded_get(http: &reqwest::Client, url: reqwest::Url) -> Result<String, RegistryError> {
    let response = http.get(url.clone()).send().await.map_err(|error| {
        if error.is_redirect() {
            // A policy refusal, not an outage: the repository was reachable
            // and this reader's own transport policy said no to where it was
            // sent next. Grouping it with `Unavailable` would tell an
            // operator to retry a request that will refuse again.
            //
            // `reqwest` keeps the `transport::RedirectRefused` this reader's
            // own policy raised as this error's `source()`, so surface that
            // reason rather than a generic one -- it already names which
            // rule refused and which URL it refused.
            let detail = std::error::Error::source(&error).map_or_else(
                || {
                    format!(
                        "reading a chart index at {} was redirected off the allowed transport",
                        shown(&url)
                    )
                },
                ToString::to_string,
            );
            RegistryError::Refused { detail }
        } else {
            transport_failure("reading a chart index", &error)
        }
    })?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(status_failure(
            "reading a chart index",
            status,
            response.headers(),
        ));
    }

    // Streamed rather than `text()`, so the bound is applied as the body
    // arrives instead of after it has all been held.
    let mut body = Vec::new();
    let mut stream = response;

    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|error| transport_failure("reading a chart index", &error))?
    {
        if body.len() + chunk.len() > MOST {
            return Err(RegistryError::Refused {
                detail: format!("the chart index at {} is larger than {MOST} bytes", shown(&url)),
            });
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|_| RegistryError::Refused {
        detail: format!("the chart index at {} is not text", shown(&url)),
    })
}
