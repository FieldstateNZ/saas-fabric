//! Hashing, and which references and digests are accepted.

use fabric_platform_management::RegistryError;

use super::digest::{matching, Content};
use super::reference::{reference, Reference};

const EMPTY: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn hashing_names_the_sha256_of_the_bytes() {
    assert_eq!(Content::hashed(Vec::new()).digest, EMPTY);
    assert_eq!(
        Content::hashed(b"{}".to_vec()).digest,
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
    );
}

#[test]
fn a_tag_and_a_sha256_digest_are_references() {
    assert_eq!(
        reference("0.3.0-preview.2"),
        Ok(Reference::Tag("0.3.0-preview.2"))
    );
    assert_eq!(reference("_x"), Ok(Reference::Tag("_x")));
    assert_eq!(reference(EMPTY), Ok(Reference::Digest(EMPTY)));
}

#[test]
fn anything_else_is_refused() {
    for text in [
        "",
        ".hidden",
        "a/b",
        "a?b",
        "a#b",
        "sha512:abcd",
        "sha256:ABCDEF",
        "sha256:e3b0",
        &"a".repeat(129),
    ] {
        assert!(reference(text).is_err(), "{text:?}");
    }
}

#[test]
fn a_mismatch_is_refused_and_a_match_is_kept() {
    assert!(matching(Content::hashed(Vec::new()), EMPTY, "a blob").is_ok());

    let refused = matching(Content::hashed(b"x".to_vec()), EMPTY, "a blob").expect_err("mismatch");
    assert!(matches!(refused, RegistryError::Refused { .. }), "{refused:?}");
}
