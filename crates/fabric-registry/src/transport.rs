//! Which URLs this crate may speak to, which redirects it may follow, and how
//! much of an answer it will read.
//!
//! # HTTPS end to end, not just on the first hop
//!
//! `reqwest`'s default redirect policy follows a redirect anywhere, including
//! from `https://` down to `http://`. Both readers here are anonymous and
//! trusted to name something that gets pinned into what Argo deploys — a
//! chart version, an image digest — so an attacker who can only intercept
//! the *first* request could otherwise answer it correctly and then redirect
//! every later hop to a host of their choosing. Requiring HTTPS on the
//! initial request and refusing to leave it on any hop closes that.
//!
//! [`permits`] is the one rule; each reader words its own refusal, and
//! [`policy`] wires a reader's decision into `reqwest`'s redirect callback.
//!
//! # Two policies, one production
//!
//! [`Transport::Https`] is the only variant a production constructor builds.
//! [`Transport::LoopbackToo`] exists for the `#[doc(hidden)]`
//! `plain_http_to_loopback` constructors, which a test uses to serve from a
//! real socket without widening what a production reader accepts.

mod bounded;
mod origin;
mod redirect;
mod shown;
#[cfg(test)]
mod transport_tests;

pub(crate) use bounded::bounded_body;
pub(crate) use origin::{is_loopback, same_origin};
pub(crate) use redirect::policy;
pub(crate) use shown::shown;

/// How many redirects any request here will follow before refusing.
///
/// A bound instead of an unlimited loop: a server redirecting a reader in a
/// circle would otherwise hang a discovery pass instead of answering it. Ten
/// is `reqwest`'s own default, and far more than any registry or chart
/// repository this platform reads uses, even during a CDN failover.
pub(crate) const MAX_REDIRECTS: usize = 10;

/// The transport rule a reader enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transport {
    /// Every request and every redirect hop must be HTTPS.
    Https,

    /// The same rule, plus plain HTTP to a loopback host that has not yet
    /// been reached by way of an HTTPS hop — for a test server with no
    /// certificate to offer. A redirect may not leave loopback on plain
    /// HTTP, and once a hop has used HTTPS, a later hop may not fall back to
    /// HTTP even to loopback.
    LoopbackToo,
}

/// Why [`permits`] said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// More than [`MAX_REDIRECTS`] hops.
    TooManyRedirects,

    /// Not HTTPS, and not the plain HTTP to loopback a test may use.
    NotHttps,
}

/// One step of a request: the initial one (`previous` empty), or a redirect
/// hop afterwards.
///
/// A pure function over the policy, the hops already followed, and the URL
/// the next request would go to — so the same rule governs the address a
/// caller asked to read and every redirect afterwards, testable with no
/// connection anywhere.
pub(crate) fn permits(
    transport: Transport,
    previous: &[reqwest::Url],
    next: &reqwest::Url,
) -> Result<(), Refusal> {
    // `previous` already carries the original request's own URL by the time
    // the first redirect is checked -- `reqwest` pushes it before calling a
    // policy -- so a bound of `MAX_REDIRECTS` allows exactly that many hops,
    // the way `reqwest`'s own `limited(10)` follows ten redirects, not nine.
    if previous.len() > MAX_REDIRECTS {
        return Err(Refusal::TooManyRedirects);
    }

    if next.scheme() == "https" {
        return Ok(());
    }

    // Plain HTTP is only ever considered under the loopback-tolerant test
    // policy, to a loopback host, and only if nothing earlier in this chain
    // has already spoken HTTPS: falling back would strip the guarantee that
    // HTTPS hop established, loopback destination or not.
    let no_earlier_https = previous.iter().all(|hop| hop.scheme() != "https");

    if transport == Transport::LoopbackToo && no_earlier_https && is_loopback(next) {
        return Ok(());
    }

    Err(Refusal::NotHttps)
}
