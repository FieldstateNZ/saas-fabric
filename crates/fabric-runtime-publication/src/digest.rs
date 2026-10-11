//! The content digest a publication record names each document by.

use std::fmt::Write as _;

/// The lowercase hex SHA-256 of `bytes`.
///
/// `ring::digest`, the one hash implementation the workspace already trusts
/// (see the workspace `Cargo.toml`). A digest rather than the revision alone,
/// because the filesystem adapter renames a payload before its manifest: a
/// reader holding a revision cannot tell which bytes it names, and a digest
/// is what the bytes themselves say.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_of_nothing_is_the_published_sha256_of_the_empty_string() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
