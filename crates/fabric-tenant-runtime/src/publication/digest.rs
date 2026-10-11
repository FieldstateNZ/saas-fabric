//! The digest a publication record names each document's bytes by.

use std::fmt::Write as _;

/// The lowercase hex SHA-256 of `bytes`, as the publisher computes it.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
