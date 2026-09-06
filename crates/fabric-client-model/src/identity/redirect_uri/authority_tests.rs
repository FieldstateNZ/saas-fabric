//! Hostile tests for the scheme and host rule.
//!
//! This is the security-bearing half of `RedirectUri`: it decides where an
//! authorisation code may be delivered, and which of four kinds of callback a
//! URI is. Most of what follows is not "does the good case work" but "does a
//! host that merely *looks* internal, or *reaches* loopback, get in".

use crate::{RedirectUri, RedirectUriKind};

/// Whether the rule accepts a URI.
fn accepts(value: &str) -> bool {
    RedirectUri::try_new(value).is_ok()
}

/// What kind of callback a URI is, for the tests that care.
fn kind(value: &str) -> RedirectUriKind {
    RedirectUri::try_new(value)
        .unwrap_or_else(|error| panic!("{value} must classify: {error}"))
        .kind()
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
    // Must still hold now that a private-use scheme is representable: a branch
    // admitting any `scheme:` that is not http would take every one of these.
    assert!(!accepts("javascript:alert(1)"));
    assert!(!accepts("data:text/html,x"));
    assert!(!accepts("file:///etc/passwd"));
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

#[test]
fn an_upper_case_scheme_is_the_same_scheme() {
    // RFC 3986 makes the scheme case-insensitive. Refused before this change,
    // because the prefix was compared byte for byte.
    assert_eq!(kind("HTTPS://www.example.com/cb"), RedirectUriKind::Https);
    assert_eq!(kind("HtTp://localhost:5173/cb"), RedirectUriKind::Loopback);
}

#[test]
fn an_upper_case_loopback_host_is_accepted_over_plain_http() {
    // Refused before this change, because the loopback membership test was
    // case-sensitive. A widening, and deliberate.
    assert_eq!(kind("http://LOCALHOST:5173/cb"), RedirectUriKind::Loopback);
}

#[test]
fn an_upper_case_loopback_host_is_still_a_loopback_host() {
    // The host is not examined at all under `https` before this change, so
    // this classified as an ordinary public callback.
    assert_eq!(kind("https://LOCALHOST:5173/cb"), RedirectUriKind::Loopback);
}

#[test]
fn an_upper_case_internal_host_is_still_a_private_network_host() {
    assert_eq!(
        kind("https://ADMIN.CORP.INTERNAL/cb"),
        RedirectUriKind::PrivateNetwork
    );
}

#[test]
fn a_loopback_host_is_a_loopback_callback_even_over_tls() {
    // The host rule's sharpest edge: the scheme is right and the
    // classification is still loopback.
    assert_eq!(kind("https://localhost:5173/callback"), RedirectUriKind::Loopback);
    assert_eq!(kind("https://localhost/cb"), RedirectUriKind::Loopback);
}

#[test]
fn the_ipv6_loopback_address_is_a_development_callback() {
    // Requires bracket-aware host parsing: splitting `[::1]:5173` on the first
    // colon yields `[`, and on the last yields `[::1]`.
    assert_eq!(kind("http://[::1]:5173/callback"), RedirectUriKind::Loopback);
    assert_eq!(kind("https://[::1]/callback"), RedirectUriKind::Loopback);
}

#[test]
fn only_three_loopback_hosts_are_a_development_callback() {
    // All three reach loopback on some machine, and none of them is one of the
    // three spellings this model recognises. `[::ffff:127.0.0.1]` parsed under
    // `https://` before this change, so refusing it is a deliberate narrowing:
    // a claimed-HTTPS entitlement satisfied by an address that never leaves
    // the machine is the entitlement failing to mean anything.
    for host in ["127.0.0.2", "[::ffff:127.0.0.1]", "localhost.localdomain"] {
        assert!(!accepts(&format!("https://{host}/callback")), "{host}");
        assert!(!accepts(&format!("http://{host}/callback")), "{host}");
    }
}

#[test]
fn a_refused_loopback_near_miss_names_the_boundary() {
    // Not a parse failure. The entitlement is a statement about a *declared*
    // callback, and a declaration that can only be recognised by resolving a
    // name is not a declaration — so the message says what loopback is.
    let error = RedirectUri::try_new("https://127.0.0.2/callback").unwrap_err();

    assert!(error.to_string().contains("127.0.0.1"), "{error}");
    assert!(error.to_string().contains("::1"), "{error}");
    assert!(error.to_string().contains("localhost"), "{error}");
}

#[test]
fn a_private_use_scheme_is_its_own_kind() {
    assert_eq!(
        kind("nz.fieldstate.slipway:/callback"),
        RedirectUriKind::PrivateUseScheme
    );
    assert_eq!(
        kind("nz.fieldstate.slipway://callback"),
        RedirectUriKind::PrivateUseScheme
    );
}

#[test]
fn a_private_use_scheme_with_a_loopback_authority_is_still_a_private_use_scheme() {
    // The row scheme-first exists for. A host-first partition would classify
    // this by the `localhost` in its authority and hand a native
    // application's callback the entitlement a development HTTP callback has.
    // The authority of a private-use URI is not a network location at all.
    assert_eq!(
        kind("nz.fieldstate.slipway://localhost/cb"),
        RedirectUriKind::PrivateUseScheme
    );
    assert_eq!(
        kind("NZ.Fieldstate.Slipway://127.0.0.1:5173/cb"),
        RedirectUriKind::PrivateUseScheme
    );
}
