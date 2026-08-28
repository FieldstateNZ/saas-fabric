//! The name of a realm role, as an operator reads it.

use std::fmt;

use fabric_core::IdentifierError;

/// A realm role, such as `Client Realm Administrator`.
///
/// The one name in this crate that is deliberately *human* rather than
/// machine-shaped. Roles appear in a client's contract and in the operator UI,
/// and the platform's own required roles are written as English (see
/// [`required_roles`](crate::required_roles)), so the rule permits mixed case
/// and interior spaces where the other names do not.
///
/// What it still refuses is anything that would make two roles look identical
/// and compare unequal — a leading or trailing space, or a doubled interior
/// space. That matters more here than it looks: the reconciler decides whether
/// a role exists by comparing this value with what Keycloak returned, so
/// `Client  Realm User` would be created on every pass, forever, and no
/// operator reading either screen could see why.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoleName(String);

impl RoleName {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "role name";

    /// The inclusive maximum length. Longer than an identifier because the
    /// value is a phrase, short enough to stay readable in a table.
    const MAX_LENGTH: usize = 128;

    /// The permitted set, described for the error message.
    const EXPECTED: &'static str = "ASCII letters, digits, single interior spaces, hyphens, \
                                    underscores, and full stops";

    /// Parses a role name.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 128
    /// bytes, contains a character outside the permitted set, begins or ends
    /// with anything other than a letter or digit, or contains two adjacent
    /// spaces.
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

        Self::check_characters(value)?;
        Self::check_boundaries(value)?;

        Ok(Self(value.to_owned()))
    }

    /// Borrows the role name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Rejects disallowed characters and doubled spaces in one pass.
    fn check_characters(value: &str) -> Result<(), IdentifierError> {
        let mut previous_was_space = false;

        for character in value.chars() {
            let permitted = character.is_ascii_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.');

            if !permitted || (character == ' ' && previous_was_space) {
                return Err(IdentifierError::DisallowedCharacter {
                    kind: Self::KIND,
                    character,
                    expected: Self::EXPECTED,
                });
            }

            previous_was_space = character == ' ';
        }

        Ok(())
    }

    /// Rejects a value that does not start and end with a letter or digit.
    fn check_boundaries(value: &str) -> Result<(), IdentifierError> {
        let ends_are_alphanumeric = value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
            && value
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());

        if ends_are_alphanumeric {
            Ok(())
        } else {
            Err(IdentifierError::BadBoundary { kind: Self::KIND })
        }
    }
}

impl fmt::Display for RoleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RoleName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RoleName> for String {
    fn from(value: RoleName) -> Self {
        value.0
    }
}
