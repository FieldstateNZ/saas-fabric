//! Which URLs a chart index read may speak to, and which redirects it may follow.
//!
//! # HTTPS end to end, not just on the first hop
//!
//! `reqwest`'s default redirect policy follows a redirect anywhere, including
//! from `https://` down to `http://`. A chart repository is read anonymously
//! and its answer is trusted to name a version that gets pinned into what
//! Argo deploys — so an attacker who can only intercept the *first* request
//! could otherwise answer it correctly and then redirect every later hop to a
//! host of their choosing. Requiring HTTPS on the initial request and
//! refusing to leave it on any hop closes that: every byte of an index this
//! reads travelled over a connection nobody in the middle could rewrite.
//!
//! [`decide`] below is the one rule this whole module exists to state; the
//! two submodules are its two consumers rather than rules of their own —
//! [`index_url`] validates the address a caller asked to read before the
//! first request goes out, and [`redirect`] wires the same decision into
//! `reqwest`'s own redirect policy for every hop afterwards.

mod index_url;
mod redirect;

pub(super) use index_url::validated_index_url;
pub(super) use redirect::policy;

/// How many redirects a chart index read will follow before refusing.
///
/// A bound instead of an unlimited loop: a chart repository redirecting a
/// reader in a circle would otherwise hang a discovery pass instead of
/// answering it. Ten is far more than any repository this platform reads
/// today uses even during a CDN failover.
const MAX_REDIRECTS: usize = 10;

/// The transport rule a chart index reader enforces.
///
/// # Two policies, one production
///
/// [`Transport::Https`] is the only variant the composition root can reach —
/// `HelmCharts::new` is the sole way to build a reader outside a test, and it
/// always chooses `Https`. [`Transport::LoopbackToo`] exists for
/// `HelmCharts::plain_http_to_loopback`, which a test uses to serve an index
/// from a real socket without widening what a production reader accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Transport {
    /// Every request and every redirect hop must be HTTPS.
    Https,

    /// The same rule, plus plain HTTP to a loopback host that has not yet
    /// been reached by way of an HTTPS hop — for a test server with no
    /// certificate to offer. A redirect may not leave loopback on plain
    /// HTTP, and once a hop has used HTTPS, a later hop may not fall back to
    /// HTTP even to loopback.
    LoopbackToo,
}

/// One step of following a chart index: the initial request, or a redirect
/// hop afterwards.
///
/// A pure function over the reader's policy, the hops already followed, and
/// the URL the next request would go to — so the same rule governs the
/// address a caller asked to read and every redirect afterwards, and the
/// whole decision is testable with no HTTP connection anywhere.
fn decide(transport: Transport, previous: &[reqwest::Url], next: &reqwest::Url) -> Result<(), String> {
    // `previous` already carries the original request's own URL by the time
    // the first redirect is checked -- `reqwest` pushes it before calling
    // this policy -- so a bound of `MAX_REDIRECTS` means `MAX_REDIRECTS`
    // hops are allowed, the same way `reqwest`'s own default `limited(10)`
    // policy follows exactly ten redirects rather than nine.
    if previous.len() > MAX_REDIRECTS {
        return Err(format!(
            "reading a chart index followed more than {MAX_REDIRECTS} redirects, at {next}"
        ));
    }

    if next.scheme() == "https" {
        return Ok(());
    }

    // Plain HTTP is only ever considered under the loopback-tolerant test
    // policy, to a loopback host, and only if nothing earlier in this chain
    // -- including the very first request -- has already spoken HTTPS. Once
    // HTTPS has been used, falling back to HTTP would strip the guarantee
    // that first HTTPS hop established, loopback destination or not.
    let no_earlier_https = previous.iter().all(|hop| hop.scheme() != "https");

    if transport == Transport::LoopbackToo && no_earlier_https && is_loopback(next) {
        return Ok(());
    }

    Err(match transport {
        Transport::Https => format!("the chart index at {next} must use HTTPS"),
        Transport::LoopbackToo => {
            format!("the chart index at {next} must use HTTPS, or plain HTTP to a loopback host")
        }
    })
}

/// Whether a URL's host is loopback: `127.0.0.0/8`, `::1`, or `localhost`.
fn is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let bare = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);

    bare.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> reqwest::Url {
        reqwest::Url::parse(text).expect("test URL parses")
    }

    #[test]
    fn initial_plain_http_is_refused_under_the_https_policy() {
        assert!(decide(Transport::Https, &[], &url("http://example.test/index.yaml")).is_err());
    }

    #[test]
    fn initial_https_is_accepted() {
        assert!(decide(Transport::Https, &[], &url("https://example.test/index.yaml")).is_ok());
    }

    #[test]
    fn https_to_https_is_followed() {
        let previous = [url("https://example.test/index.yaml")];
        assert!(decide(
            Transport::Https,
            &previous,
            &url("https://example.test/moved/index.yaml")
        )
        .is_ok());
    }

    #[test]
    fn https_to_http_is_refused_even_to_loopback() {
        let previous = [url("https://example.test/index.yaml")];
        assert!(decide(Transport::Https, &previous, &url("http://127.0.0.1:1/index.yaml")).is_err());
    }

    #[test]
    fn exactly_the_bound_of_hops_is_still_followed() {
        // `previous` already holds the original request's URL by the time
        // the first redirect is checked, so exactly `MAX_REDIRECTS` entries
        // is the boundary that must still be allowed -- matching `reqwest`'s
        // own `limited(10)`, which follows ten redirects, not nine.
        let previous = vec![url("https://example.test/index.yaml"); MAX_REDIRECTS];
        assert!(decide(
            Transport::Https,
            &previous,
            &url("https://example.test/index.yaml")
        )
        .is_ok());
    }

    #[test]
    fn one_hop_past_the_bound_is_refused() {
        let previous = vec![url("https://example.test/index.yaml"); MAX_REDIRECTS + 1];
        assert!(decide(
            Transport::Https,
            &previous,
            &url("https://example.test/index.yaml")
        )
        .is_err());
    }

    #[test]
    fn loopback_policy_accepts_plain_http_to_a_loopback_host() {
        assert!(decide(
            Transport::LoopbackToo,
            &[],
            &url("http://127.0.0.1:8080/index.yaml")
        )
        .is_ok());
        assert!(decide(
            Transport::LoopbackToo,
            &[],
            &url("http://localhost:8080/index.yaml")
        )
        .is_ok());
        assert!(decide(Transport::LoopbackToo, &[], &url("http://[::1]:8080/index.yaml")).is_ok());
    }

    #[test]
    fn loopback_policy_refuses_plain_http_off_loopback() {
        assert!(decide(
            Transport::LoopbackToo,
            &[],
            &url("http://charts.example.test/index.yaml")
        )
        .is_err());
    }

    #[test]
    fn loopback_policy_permits_an_upgrade_to_https() {
        let previous = [url("http://127.0.0.1:8080/index.yaml")];
        assert!(decide(
            Transport::LoopbackToo,
            &previous,
            &url("https://127.0.0.1:1/index.yaml")
        )
        .is_ok());
    }

    #[test]
    fn loopback_policy_refuses_falling_back_to_http_after_an_https_hop() {
        let previous = [url("https://example.test/index.yaml")];
        assert!(decide(
            Transport::LoopbackToo,
            &previous,
            &url("http://127.0.0.1:1/index.yaml")
        )
        .is_err());
    }
}
