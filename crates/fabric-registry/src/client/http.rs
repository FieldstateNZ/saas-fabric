//! The two HTTP clients a registry is read with, and where each may be
//! redirected.

use std::time::Duration;

use crate::transport::{permits, policy, same_origin, Refusal, Transport, MAX_REDIRECTS};

/// The user agent every request here names.
const USER_AGENT: &str = "saas-fabric-control-plane";

/// The API client and the blob client, in that order.
///
/// The blob client follows no redirect itself: a blob's redirects are
/// followed hop by hop in `blob/follow.rs`, which decides per hop whether the
/// pull token goes too — something a `reqwest` policy cannot do for a chain
/// longer than one hop.
pub(super) fn clients(
    timeout: Duration,
    transport: Transport,
) -> Result<(reqwest::Client, reqwest::Client), String> {
    Ok((
        client(timeout, api_policy(transport))?,
        client(timeout, reqwest::redirect::Policy::none())?,
    ))
}

/// One client: bounded in time, no `Referer`, and no ambient proxy.
///
/// A `Referer` would tell a CDN which registry path sent it there, and an
/// ambient proxy is a hop nobody configured for this integration.
fn client(timeout: Duration, redirects: reqwest::redirect::Policy) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .referer(false)
        .no_proxy()
        .redirect(redirects)
        .build()
        .map_err(|_| "registry: could not build an HTTP client".to_owned())
}

/// Manifests, tags, referrers and tokens: the transport rule, and never off
/// the origin the request was sent to.
///
/// No redirect target is ever worded into a refusal: its query may be a
/// signed credential, and none of it is an operator's business.
fn api_policy(transport: Transport) -> reqwest::redirect::Policy {
    policy(move |previous, next| {
        permits(transport, previous, next).map_err(refusal)?;

        if previous.first().is_some_and(|first| same_origin(first, next)) {
            Ok(())
        } else {
            Err(
                "a registry request was redirected to another origin, which only a blob read follows"
                    .to_owned(),
            )
        }
    })
}

/// A refusal, worded without the URL it refused.
pub(super) fn refusal(refusal: Refusal) -> String {
    match refusal {
        Refusal::TooManyRedirects => {
            format!("a registry request followed more than {MAX_REDIRECTS} redirects")
        }
        Refusal::NotHttps => "a registry request was redirected off HTTPS".to_owned(),
    }
}
