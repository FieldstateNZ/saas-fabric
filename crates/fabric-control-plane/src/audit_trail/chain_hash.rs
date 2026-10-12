//! A SHA-256 link in an audit chain.

use std::fmt::Write as _;

/// The SHA-256 of one sealed audit entry, which the next entry names as its
/// predecessor.
///
/// Serialised as 64 lowercase hex characters, and parsed back only from
/// exactly that, so a stored hash cannot be written two ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChainHash([u8; 32]);

impl ChainHash {
    /// The predecessor the first entry of every stream names.
    pub const GENESIS: Self = Self([0; 32]);

    /// The SHA-256 of `bytes`, through `ring::digest` (see the workspace
    /// `Cargo.toml` for why that implementation).
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
        let mut hash = [0; 32];
        for (slot, byte) in hash.iter_mut().zip(digest.as_ref()) {
            *slot = *byte;
        }
        Self(hash)
    }
}

impl std::fmt::Display for ChainHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex = self.0.iter().fold(String::with_capacity(64), |mut hex, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        formatter.write_str(&hex)
    }
}

impl TryFrom<String> for ChainHash {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let refused = || format!("`{value}` is not a lowercase hex SHA-256");
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(refused());
        }
        let mut hash = [0; 32];
        let (pairs, _) = value.as_bytes().as_chunks::<2>();
        for (slot, pair) in hash.iter_mut().zip(pairs) {
            let text = std::str::from_utf8(pair).map_err(|_| refused())?;
            *slot = u8::from_str_radix(text, 16).map_err(|_| refused())?;
        }
        Ok(Self(hash))
    }
}

impl From<ChainHash> for String {
    fn from(hash: ChainHash) -> Self {
        hash.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_of_nothing_is_the_published_sha256_of_the_empty_string() {
        assert_eq!(
            ChainHash::of(b"").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_hash_round_trips_and_only_its_canonical_form_parses() {
        let hash = ChainHash::of(b"audit");
        assert_eq!(ChainHash::try_from(hash.to_string()).unwrap(), hash);
        assert!(ChainHash::try_from(hash.to_string().to_uppercase()).is_err());
        assert!(ChainHash::try_from("00".to_owned()).is_err());
    }
}
