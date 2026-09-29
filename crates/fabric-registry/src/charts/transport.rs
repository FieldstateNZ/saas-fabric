//! Which URLs a chart index read may speak to, and which redirects it may follow.
//!
//! # HTTPS end to end, not just on the first hop
//!
//! A chart repository is read anonymously and its answer is trusted to name
//! a version that gets pinned into what Argo deploys, so the crate's one
//! transport rule, [`permits`], applies to the address a caller asked to read
//! and to every redirect afterwards. This module words that rule's refusals
//! for a chart index — naming the address through [`shown`], which a chart
//! repository's operator needs and which carries no credential — and
//! [`index_url`] validates the address before the first request goes out.

mod index_url;

pub(super) use index_url::validated_index_url;

pub(super) use crate::transport::Transport;
use crate::transport::{permits, shown, Refusal, MAX_REDIRECTS};

/// Builds the redirect policy a chart reader follows.
pub(super) fn policy(transport: Transport) -> reqwest::redirect::Policy {
    crate::transport::policy(move |previous, next| decide(transport, previous, next))
}

/// One step of following a chart index, worded for a chart repository's
/// operator: [`permits`], with the refused address shown.
fn decide(transport: Transport, previous: &[reqwest::Url], next: &reqwest::Url) -> Result<(), String> {
    permits(transport, previous, next).map_err(|refusal| {
        // A redirect target the repository named -- rendered via `shown`, not its own `Display`.
        let next = shown(next);
        match (refusal, transport) {
            (Refusal::TooManyRedirects, _) => {
                format!("reading a chart index followed more than {MAX_REDIRECTS} redirects, at {next}")
            }
            (Refusal::NotHttps, Transport::Https) => format!("the chart index at {next} must use HTTPS"),
            (Refusal::NotHttps, Transport::LoopbackToo) => {
                format!("the chart index at {next} must use HTTPS, or plain HTTP to a loopback host")
            }
        }
    })
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
