//! Hostile tests for the scheme and host rule.
//!
//! This is the security-bearing half of `RedirectUri`: it decides where an
//! authorisation code may be delivered. Most of what follows is not "does the
//! good case work" but "does a host that merely *looks* internal get in".

use crate::RedirectUri;

/// Whether the rule accepts a URI.
fn accepts(value: &str) -> bool {
    RedirectUri::try_new(value).is_ok()
}

#[test]
fn https_is_permitted_anywhere() {
    assert!(accepts("https://www.example.com/callback"));
    assert!(accepts("https://acme.lucentroot.internal/callback"));
    assert!(accepts("https://www.example.com/*"));
}

#[test]
fn plain_http_is_permitted_on_loopback() {
    assert!(accepts("http://localhost:5173/callback"));
    assert!(accepts("http://127.0.0.1:8080/callback"));
}

#[test]
fn plain_http_is_permitted_on_the_private_top_level_domain() {
    // The LucentRoot case. Its gateway has one HTTP listener and no TLS,
    // because `.internal` cannot receive a publicly-trusted certificate.
    assert!(accepts("http://acme.lucentroot.internal/*"));
    assert!(accepts("http://acme.lucentroot.internal:8080/callback"));
    assert!(accepts("http://internal/callback"));
}

#[test]
fn plain_http_is_refused_on_a_public_host() {
    // The case the whole rule exists for: an authorisation code delivered in
    // clear text to somewhere on the internet.
    assert!(!accepts("http://www.example.com/callback"));
    assert!(!accepts("http://192.0.2.10/callback"));
}

#[test]
fn a_public_host_cannot_smuggle_the_private_domain_through_the_path() {
    // A substring test for `.internal` would accept every one of these.
    assert!(!accepts("http://evil.example.com/.internal"));
    assert!(!accepts("http://evil.example.com/callback?next=.internal"));
    assert!(!accepts("http://evil.example.com/callback#.internal"));
}

#[test]
fn a_public_host_cannot_smuggle_the_private_domain_through_userinfo() {
    // `x.internal@evil.example.com` is a *public* host with an internal
    // -looking prefix. Refused as userinfo rather than parsed around.
    assert!(!accepts("http://x.internal@evil.example.com/callback"));
    assert!(!accepts("https://x.internal@evil.example.com/callback"));
}

#[test]
fn the_private_domain_must_be_the_suffix_and_not_a_label() {
    assert!(!accepts("http://x.internal.evil.example.com/callback"));
    assert!(!accepts("http://internal.evil.example.com/callback"));
}

#[test]
fn a_host_merely_ending_in_the_letters_is_refused() {
    // `notinternal` ends with `internal` as a string and is not under the TLD.
    assert!(!accepts("http://notinternal/callback"));
    assert!(!accepts("http://myinternal/callback"));
}

#[test]
fn a_scheme_that_is_not_http_is_refused() {
    assert!(!accepts("javascript:alert(1)"));
    assert!(!accepts("data:text/html,x"));
    assert!(!accepts("ftp://acme.lucentroot.internal/"));
    assert!(!accepts("acme.lucentroot.internal"));
}

#[test]
fn a_wildcard_in_the_host_is_still_refused() {
    // Unchanged by the `.internal` allowance, and worth re-asserting here: a
    // subdomain wildcard would accept a host the operator never intended.
    assert!(!accepts("https://*.example.com/callback"));
    assert!(!accepts("http://*.lucentroot.internal/callback"));
}
