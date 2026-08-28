//! The revision of a stored desired-state document.

use std::fmt;

use fabric_core::IdentifierError;

/// The version of a client document, as the desired-state repository reports
/// it.
///
/// **Deliberately opaque.** A repository decides what a revision means — the
/// Git-backed implementation uses the blob hash of the stored file — and
/// nothing above the repository may parse, order, or construct one. It is
/// compared for equality and nothing else, which is exactly what optimistic
/// concurrency needs and all it needs (see ADR 0008).
///
/// That opacity is why this is not a `BindingRevision`: the runtime plane's
/// revision is a monotonic counter it can compare with `>`, and giving this
/// type the same shape would invite code that asks whether one desired state
/// is *newer* than another. A content hash cannot answer that, and a
/// comparison that silently means "different" would be a bug that only appears
/// under concurrent edits.
///
/// The value travels to the browser as an HTTP entity tag and comes back in
/// `If-Match`, so it is checked here for the characters a quoted entity tag
/// may hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClientRevision(String);

impl ClientRevision {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "revision";

    /// The inclusive maximum length. A Git object id is 40 or 64 characters;
    /// this leaves room for a longer opaque token without inviting one that
    /// would not fit in a header.
    const MAX_LENGTH: usize = 128;

    /// The permitted set, described for the error message.
    const EXPECTED: &'static str = "ASCII letters, digits, hyphens, underscores, colons, and \
                                    full stops";

    /// Parses a revision as reported by a repository, or echoed back by a
    /// client in `If-Match`.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 128
    /// bytes, or contains a character that could not survive a round trip
    /// through an entity tag.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        let value = value.as_ref();

        if value.is_empty() {
            return Err(IdentifierError::Empty { kind: Self::KIND });
        }

        if value.len() > Self::MAX_LENGTH {
            return Err(IdentifierError::TooLong {
                kind: Self::KIND,
                max: Self::MAX_LENGTH,
                actual: value.len(),
            });
        }

        for character in value.chars() {
            let permitted = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':' | '.');

            if !permitted {
                return Err(IdentifierError::DisallowedCharacter {
                    kind: Self::KIND,
                    character,
                    expected: Self::EXPECTED,
                });
            }
        }

        Ok(Self(value.to_owned()))
    }

    /// Borrows the revision as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClientRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for ClientRevision {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<ClientRevision> for String {
    fn from(value: ClientRevision) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_git_blob_hash() {
        assert!(ClientRevision::try_new("9daeafb9864cf43055ae93beb0afd6c7d144bfa4").is_ok());
    }

    #[test]
    fn a_quote_is_refused_because_the_value_becomes_an_entity_tag() {
        assert!(ClientRevision::try_new("abc\"def").is_err());
    }
}
