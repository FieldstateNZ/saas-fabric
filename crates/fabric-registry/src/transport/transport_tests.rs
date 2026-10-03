//! The transport rule, and what an origin is.

use super::{is_loopback, permits, same_origin, Refusal, Transport, MAX_REDIRECTS};

fn url(text: &str) -> reqwest::Url {
    reqwest::Url::parse(text).expect("test URL parses")
}

#[test]
fn https_is_permitted_and_plain_http_is_not() {
    assert_eq!(
        permits(Transport::Https, &[], &url("https://ghcr.io/v2/")),
        Ok(())
    );
    assert_eq!(
        permits(Transport::Https, &[], &url("http://127.0.0.1:1/v2/")),
        Err(Refusal::NotHttps)
    );
}

#[test]
fn plain_http_to_loopback_is_only_for_the_test_policy_and_never_after_https() {
    assert_eq!(
        permits(Transport::LoopbackToo, &[], &url("http://127.0.0.1:1/v2/")),
        Ok(())
    );
    assert_eq!(
        permits(
            Transport::LoopbackToo,
            &[url("https://ghcr.io/")],
            &url("http://127.0.0.1:1/")
        ),
        Err(Refusal::NotHttps)
    );
    assert_eq!(
        permits(Transport::LoopbackToo, &[], &url("http://ghcr.io/")),
        Err(Refusal::NotHttps)
    );
}

#[test]
fn past_the_redirect_bound_is_refused() {
    let previous = vec![url("https://ghcr.io/"); MAX_REDIRECTS + 1];
    assert_eq!(
        permits(Transport::Https, &previous, &url("https://ghcr.io/")),
        Err(Refusal::TooManyRedirects)
    );
}

#[test]
fn an_origin_is_scheme_host_and_port() {
    assert!(same_origin(
        &url("https://ghcr.io/a"),
        &url("https://ghcr.io:443/b?c")
    ));
    assert!(!same_origin(&url("https://ghcr.io/"), &url("http://ghcr.io/")));
    assert!(!same_origin(
        &url("https://ghcr.io/"),
        &url("https://pkg.ghcr.io/")
    ));
    assert!(!same_origin(
        &url("http://127.0.0.1:1/"),
        &url("http://127.0.0.1:2/")
    ));
    assert!(!same_origin(
        &url("http://127.0.0.1:1/"),
        &url("http://localhost:1/")
    ));
}

#[test]
fn loopback_is_named_or_addressed() {
    assert!(is_loopback(&url("http://localhost:1/")));
    assert!(is_loopback(&url("http://127.0.0.2:1/")));
    assert!(is_loopback(&url("http://[::1]:1/")));
    assert!(!is_loopback(&url("http://10.0.0.1:1/")));
}
