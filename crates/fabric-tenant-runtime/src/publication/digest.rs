//! The digest a publication record names each document's bytes by.

use fabric_core::RecordDigest;

/// The lowercase hex SHA-256 of `bytes`, as the publisher computes it.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    sha256_digest(bytes).to_string()
}

/// The SHA-256 of `bytes`, as the digest type a replica reports serving.
pub(super) fn sha256_digest(bytes: &[u8]) -> RecordDigest {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_ref());
    RecordDigest::from_bytes(out)
}
