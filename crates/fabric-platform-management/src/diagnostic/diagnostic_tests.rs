//! Credentials that reach a message must not reach a console.

use super::{SafeDiagnostic, MAX};

fn sanitised(text: &str) -> String {
    SafeDiagnostic::sanitise(text).as_str().to_owned()
}

#[test]
fn a_bearer_token_is_redacted() {
    // The realistic shape: an auth failure that quotes the header it sent.
    let safe =
        sanitised("moving the branch failed: Authorization: Bearer ghs_16C7e42F292c6912E7710c838347Ae178B4a");

    assert!(!safe.contains("ghs_"), "{safe}");
    assert!(safe.contains("[redacted]"), "{safe}");
    assert!(
        safe.contains("moving the branch failed"),
        "the message itself was lost: {safe}"
    );
}

#[test]
fn every_credential_shape_this_platform_handles_is_redacted() {
    for credential in [
        "ghp_16C7e42F292c6912E7710c838347Ae178B4a",
        "gho_16C7e42F292c6912E7710c838347Ae178B4a",
        "ghu_16C7e42F292c6912E7710c838347Ae178B4a",
        "ghs_16C7e42F292c6912E7710c838347Ae178B4a",
        "ghr_16C7e42F292c6912E7710c838347Ae178B4a",
        "github_pat_11ABCDEFG0abcdefghijkl_ABCDEFGH",
        "hvs.CAESIJlBc3RvcmVkVG9rZW4",
        "hvb.AAAAAQKgYWJjZGVmZ2hpams",
    ] {
        let safe = sanitised(&format!("the store said no: {credential}"));

        assert!(!safe.contains(credential), "{credential} survived: {safe}");
        assert!(safe.contains("[redacted]"), "{safe}");
    }
}

#[test]
fn a_private_key_takes_everything_after_it_with_it() {
    let safe = sanitised(
        "could not sign: -----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA3Zx\n-----END RSA PRIVATE KEY-----",
    );

    assert!(!safe.contains("MIIEow"), "{safe}");
    assert!(!safe.contains("BEGIN"), "{safe}");
    assert!(safe.starts_with("could not sign: "), "{safe}");
}

#[test]
fn a_url_keeps_its_path_and_loses_its_query() {
    // Which registry and which repository is worth reading. A signature or a
    // token in the query is not.
    let safe = sanitised("reading a manifest failed: https://ghcr.io/v2/fieldstatenz/saas-fabric/manifests/0.3.0?token=abc123&sig=deadbeef");

    assert!(
        safe.contains("https://ghcr.io/v2/fieldstatenz/saas-fabric/manifests/0.3.0"),
        "{safe}"
    );
    assert!(!safe.contains("abc123"), "{safe}");
    assert!(!safe.contains("deadbeef"), "{safe}");
}

#[test]
fn a_value_after_the_word_token_is_redacted_whatever_it_looks_like() {
    // The shape that defeats a prefix list: a credential nobody wrote a rule
    // for, introduced by a word that says what it is.
    for message in [
        "refused: token=s3cr3t-not-a-known-shape",
        "refused: password s3cr3t-not-a-known-shape",
        "refused: secret: s3cr3t-not-a-known-shape",
    ] {
        let safe = sanitised(message);

        assert!(!safe.contains("s3cr3t"), "{message} -> {safe}");
    }
}

#[test]
fn a_response_body_cannot_arrive_whole() {
    // What a `Debug` on an upstream response would produce. A truncated leak
    // is still a leak and a much smaller one, and this is the rule that
    // survives a mistake nobody anticipated.
    let body = "x".repeat(4000);
    let safe = sanitised(&format!("the registry said: {body}"));

    assert!(
        safe.chars().count() <= MAX + 1,
        "{} characters",
        safe.chars().count()
    );
    assert!(safe.ends_with('…'));
}

#[test]
fn an_ordinary_diagnostic_survives_intact() {
    // The guard is worthless if it mangles the message it is protecting. A
    // digest and a version are exactly what an operator needs to read.
    let message = "saas-fabric: the registry is unavailable: listing tags timed out";
    assert_eq!(sanitised(message), message);

    let digest = "saas-fabric: 0.3.0-preview.3 resolves to sha256:809876b52ed25985201784a76d3bfc4e1f4da4160bec3284ad40f71314da95e1";
    assert_eq!(sanitised(digest), digest);
}

#[test]
fn truncation_does_not_split_a_character() {
    // A multi-byte character straddling the cap would panic a naive slice.
    let safe = sanitised(&"é".repeat(400));

    assert!(safe.chars().count() <= MAX + 1);
}
