//! Content addressing: what a digest is here, and bytes proven to have one.
//!
//! # Only `sha256`
//!
//! A digest in an algorithm this client does not compute is a digest it
//! cannot check, and a digest it cannot check is a claim, not a fact. So a
//! reference, a header or a listed digest in any other algorithm is refused
//! rather than trusted (ADR 0026 section 3).

use std::fmt::Write as _;
use std::sync::Arc;

use fabric_platform_management::RegistryError;

/// What every accepted digest starts with.
const SHA256: &str = "sha256:";

/// Bytes, and the digest this client computed from them.
///
/// The only way to get one is [`hashed`](Self::hashed), so holding one is
/// holding proof: a digest stored with bytes is never one a registry
/// supplied.
#[derive(Debug, Clone)]
pub(super) struct Content {
    /// `sha256:` and the hex of the bytes' SHA-256.
    pub(super) digest: String,

    /// The bytes, shared rather than copied between the cache and a reader.
    pub(super) bytes: Arc<[u8]>,
}

impl Content {
    /// Hashes `bytes`.
    pub(super) fn hashed(bytes: Vec<u8>) -> Self {
        let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
        let mut digest = String::with_capacity(SHA256.len() + 64);
        digest.push_str(SHA256);
        for byte in hash.as_ref() {
            let _ = write!(digest, "{byte:02x}");
        }

        Self {
            digest,
            bytes: bytes.into(),
        }
    }
}

/// `text`, if it is `sha256:` and 64 lower-case hex digits.
///
/// # Errors
///
/// [`RegistryError::Refused`] naming `what` — never `text`, which came from
/// somewhere this client does not trust.
pub(super) fn sha256<'a>(text: &'a str, what: &str) -> Result<&'a str, RegistryError> {
    let well_formed = text.strip_prefix(SHA256).is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    });

    if well_formed {
        Ok(text)
    } else {
        Err(RegistryError::Refused {
            detail: format!("{what} is not a sha256 digest, the only algorithm accepted"),
        })
    }
}

/// `content`, if its computed digest is `expected`.
///
/// # Errors
///
/// [`RegistryError::Refused`] naming `what` and the digest asked for, which
/// has already been checked as `sha256` and is safe to show.
pub(super) fn matching(content: Content, expected: &str, what: &str) -> Result<Content, RegistryError> {
    if content.digest == expected {
        Ok(content)
    } else {
        Err(RegistryError::Refused {
            detail: format!("{what} {expected} was served as bytes with another digest"),
        })
    }
}
