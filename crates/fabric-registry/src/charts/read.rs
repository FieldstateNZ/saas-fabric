//! The one streamed GET that reads a chart index, and its size bound.

use fabric_platform_management::RegistryError;

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
            RegistryError::Refused {
                detail: format!("reading a chart index at {url} was redirected off HTTPS"),
            }
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
                detail: format!("the chart index at {url} is larger than {MOST} bytes"),
            });
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|_| RegistryError::Refused {
        detail: format!("the chart index at {url} is not text"),
    })
}
