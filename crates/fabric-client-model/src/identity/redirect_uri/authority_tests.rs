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
    // Every one of these reaches loopback on some machine, and none of them
    // is one of the three spellings this model recognises. `[::ffff:127.0.0.1]`
    // parsed under `https://` before this change, so refusing it is a
    // deliberate narrowing: a claimed-HTTPS entitlement satisfied by an
    // address that never leaves the machine is the entitlement failing to
    // mean anything. `127.1`, `2130706433` and `0x7f000001` are the
    // abbreviated, all-numeric and hexadecimal spellings `inet_aton` — and
    // therefore curl, a browser, and most libc resolvers — read as the same
    // address; a strict `IpAddr::from_str` accepted none of them, which is
    // the bypass this classifier exists to close.
    for host in [
        "127.0.0.2",
        "[::ffff:127.0.0.1]",
        "localhost.localdomain",
        "127.1",
        "2130706433",
        "0x7f000001",
        "0177.0.0.1",
    ] {
        assert!(!accepts(&format!("https://{host}/callback")), "{host}");
        assert!(!accepts(&format!("http://{host}/callback")), "{host}");
    }
}

#[test]
fn an_ip_literal_is_never_a_claimed_https_callback() {
    // Universal Links and App Links require a registered domain, so the rule
    // is stated positively — a host is admitted because it *is* a registered
    // domain, not because a parser failed to recognise it as an address.
    // Every spelling below was admitted by the negative rule this replaced:
    //
    // - `0x` and `0x.0x.0x.0x`: a browser reads an empty hexadecimal tail as
    //   0, so both dial `0.0.0.0`, which is the machine it is already on.
    // - the fullwidth digits: UTS-46 maps them back to `127.0.0.1`.
    // - `[::1%25lo0]`: a zone id names an interface only one machine has.
    // - `[foo]` and `[::1`: a bracketed authority is an IPv6 literal or it is
    //   nothing, and an unclosed bracket classified as *loopback*.
    for (host, expected) in [
        ("93.184.216.34", "registered domain"),
        ("134744072", "registered domain"),
        ("0x08080808", "registered domain"),
        ("[2001:db8::1]", "registered domain"),
        ("0x", "registered domain"),
        ("0x.0x.0x.0x", "registered domain"),
        ("１２７．０．０．１", "xn--"),
        ("[::1%25lo0]", "bracketed authority"),
        ("[foo]", "bracketed authority"),
        ("[::1", "bracketed authority"),
    ] {
        let error = RedirectUri::try_new(format!("https://{host}/callback")).unwrap_err();
        assert!(error.to_string().contains(expected), "{host}: {error}");
    }
}

#[test]
fn a_registered_domain_has_at_least_two_labels() {
    // A single-label name is whatever the resolver in front of the browser
    // decides it is — an intranet search domain, a hosts file, a wildcard
    // resolver — so it is not something an entitlement can be stated against.
    assert!(!accepts("https://intranet/cb"));
    assert!(accepts("https://intranet.example.com/cb"));
}

#[test]
fn an_underscore_is_not_a_hostname_character() {
    // Legal in a DNS record, never in a hostname, and a browser will not
    // claim an App Link against one.
    assert!(!accepts("https://my_host.example.com/cb"));
    assert!(!accepts("https://-example.com/cb"));
    assert!(!accepts("https://example-.com/cb"));
}

#[test]
fn plain_http_on_a_public_host_names_the_boundary_rather_than_a_typo() {
    // Not a parse failure: the value is well-formed and outside the rule.
    // "must start and end with an alphanumeric character" — what this used to
    // say — sends its author looking for a typo that is not there.
    let error = RedirectUri::try_new("http://www.example.com/callback").unwrap_err();

    assert!(error.to_string().contains("https for a public host"), "{error}");
    assert!(error.to_string().contains(".internal"), "{error}");
}

#[test]
fn a_scheme_with_no_authority_is_told_that_and_not_that_its_scheme_is_wrong() {
    // `https:` is a scheme this model classifies, so naming the scheme would
    // point its author at the one part of the URI that is right.
    let error = RedirectUri::try_new("https:foo").unwrap_err();

    assert!(
        error.to_string().contains("an authority after the scheme"),
        "{error}"
    );
}

#[test]
fn a_digit_straight_after_the_colon_is_a_port_and_the_refusal_names_both_readings() {
    // `nz.fieldstate.slipway:8080/cb` is a native application's scheme with a
    // port that does not belong to it; `www.example.com:8080/cb` is a host
    // that lost its `https://`. Only the author knows which, so both are
    // named. `www.example.com:/cb` has no digit and stays private-use — see
    // the migrator, which is where an operator meets that reading.
    let error = RedirectUri::try_new("nz.fieldstate.slipway:8080/cb").unwrap_err();

    assert!(error.to_string().contains("reads as a port"), "{error}");
    assert!(error.to_string().contains("nz.fieldstate.slipway:/cb"), "{error}");
    assert!(error.to_string().contains("https://www.example.com"), "{error}");
    assert_eq!(kind("www.example.com:/cb"), RedirectUriKind::PrivateUseScheme);
}

#[test]
fn an_empty_host_is_refused() {
    // `https:///cb` and `https://:8443/cb` both parse to an empty host, which
    // is not a domain this model can name a strategy against.
    assert!(!accepts("https:///cb"));
    assert!(!accepts("https://:8443/cb"));
}

#[test]
fn an_empty_first_label_is_refused() {
    // `.internal` on its own is a leading dot, not the private top-level
    // domain — a substring-style `ends_with` would accept it anyway.
    assert!(!accepts("https://.internal/cb"));
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
