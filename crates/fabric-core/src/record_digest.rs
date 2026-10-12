//! The SHA-256 that identifies one publication record exactly.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A SHA-256 digest, written as 64 lowercase hexadecimal characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordDigest([u8; 32]);

/// Why a string is not a [`RecordDigest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a record digest is 64 lowercase hexadecimal characters")]
pub struct NotADigest;

impl RecordDigest {
    /// The digest whose bytes are `bytes`.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parses a digest.
    ///
    /// # Errors
    ///
    /// [`NotADigest`] unless `value` is exactly 64 characters of `0-9a-f`.
    pub fn parse(value: &str) -> Result<Self, NotADigest> {
        if value.len() != 64 {
            return Err(NotADigest);
        }
        let mut bytes = [0_u8; 32];
        let (pairs, _) = value.as_bytes().as_chunks::<2>();
        for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
            *byte = (nibble(*high)? << 4) | nibble(*low)?;
        }
        Ok(Self(bytes))
    }
}

fn nibble(character: u8) -> Result<u8, NotADigest> {
    match character {
        b'0'..=b'9' => Ok(character - b'0'),
        b'a'..=b'f' => Ok(character - b'a' + 10),
        _ => Err(NotADigest),
    }
}

impl TryFrom<String> for RecordDigest {
    type Error = NotADigest;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RecordDigest> for String {
    fn from(digest: RecordDigest) -> Self {
        digest.to_string()
    }
}

impl fmt::Display for RecordDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(formatter, "{byte:02x}"))
    }
}
