//! A callback, as the ledger keeps it: a digest, never the URI.

use std::fmt;

use fabric_client_model::RedirectUri;

/// The SHA-256 of one redirect URI, in lowercase hexadecimal.
///
/// The ledger is a durable record read by people investigating; it names a
/// callback only by digest, as the stored-projection report names one only
/// by field (D01-19b). Equality of digests is what classification needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallbackDigest(String);

impl CallbackDigest {
    /// The length of a SHA-256 digest in hexadecimal.
    const LENGTH: usize = 64;

    /// The digest of `uri`, exactly as the document spells it.
    #[must_use]
    pub fn of(uri: &RedirectUri) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, uri.as_str().as_bytes());
        Self(digest.as_ref().iter().fold(String::new(), |mut text, byte| {
            use std::fmt::Write as _;
            // Writing to a `String` cannot fail.
            let _ = write!(text, "{byte:02x}");
            text
        }))
    }

    /// Reads a digest back from a record.
    ///
    /// # Errors
    ///
    /// The text, when it is not 64 lowercase hexadecimal characters.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() == Self::LENGTH && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(text.to_owned()))
        } else {
            Err(format!("`{text}` is not a callback digest"))
        }
    }

    /// The digest as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CallbackDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use fabric_client_model::RedirectUri;

    use super::CallbackDigest;

    #[test]
    fn a_digest_is_sha256_of_the_uri_and_parses_back() {
        let digest =
            CallbackDigest::of(&RedirectUri::try_new("https://acme.lucentroot.internal/callback").unwrap());

        assert_eq!(digest.as_str().len(), 64);
        assert_eq!(CallbackDigest::parse(digest.as_str()).unwrap(), digest);
        assert_ne!(
            digest,
            CallbackDigest::of(&RedirectUri::try_new("http://acme.lucentroot.internal/callback").unwrap()),
            "the scheme is part of what was written"
        );
    }

    #[test]
    fn anything_but_a_digest_is_refused() {
        for text in [
            "",
            "https://www.example.com/callback",
            &"A".repeat(64),
            &"0".repeat(63),
        ] {
            assert!(CallbackDigest::parse(text).is_err(), "{text}");
        }
    }
}
