//! The scheme itself, as a value a strategy can declare and a URI can carry.
//!
//! Split from the predicate above it because they answer different questions.
//! [`super::is_private_use`] asks "is this text, in this position, a
//! private-use scheme?" — a classification, run on every redirect URI. This
//! file holds the *value*: the thing a `customScheme` strategy names, that is
//! serialised into a document and compared against a URI's own scheme. The
//! rule is shared, so `check` is the one copy of it and the predicate calls
//! it; the type is not.

use std::fmt;

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "app scheme";

/// The permitted set, described for the error message.
const EXPECTED: &str = "a reverse-domain scheme such as nz.fieldstate.slipway";

/// A private-use URI scheme a native application answers on.
///
/// Its own type rather than a `String`, so the scheme a strategy declares and
/// the scheme a URI carries are compared as the same validated thing. It is
/// lower-cased on the way in: RFC 3986 makes schemes case-insensitive, and
/// `NZ.Fieldstate.Slipway` naming a different scheme from
/// `nz.fieldstate.slipway` is a difference no operating system would honour.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AppScheme(String);

impl AppScheme {
    /// Parses a private-use URI scheme.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, does not start with
    /// a letter, carries a character RFC 3986 does not permit in a scheme, or
    /// carries no dot — which is what would make it an ordinary scheme rather
    /// than a reverse-domain one.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        let value = value.as_ref().to_ascii_lowercase();
        Self::check(&value)?;

        Ok(Self(value))
    }

    /// Borrows the scheme as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The RFC 8252 §7.1 rule, applied to an already-lower-cased value.
    ///
    /// Reachable from the predicate beside this module, which is the point:
    /// one rule, asked twice, rather than a classifier and a constructor that
    /// can drift into disagreeing about what a scheme is.
    pub(super) fn check(value: &str) -> Result<(), IdentifierError> {
        let mut characters = value.chars();

        let first = characters.next().ok_or(IdentifierError::Empty { kind: KIND })?;
        if !first.is_ascii_lowercase() {
            return Err(IdentifierError::BadBoundary { kind: KIND });
        }

        for character in characters {
            let permitted = character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '+' | '-' | '.');

            if !permitted {
                return Err(IdentifierError::DisallowedCharacter {
                    kind: KIND,
                    character,
                    expected: EXPECTED,
                });
            }
        }

        if !value.split('.').skip(1).any(|label| !label.is_empty()) {
            return Err(IdentifierError::Unadmitted {
                kind: KIND,
                expected: EXPECTED,
            });
        }

        Ok(())
    }
}

impl fmt::Display for AppScheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for AppScheme {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<AppScheme> for String {
    fn from(value: AppScheme) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scheme_is_held_lower_cased() {
        let scheme = AppScheme::try_new("NZ.Fieldstate.Slipway").unwrap();

        assert_eq!(scheme.as_str(), "nz.fieldstate.slipway");
    }

    #[test]
    fn a_scheme_must_start_with_a_letter_and_carry_no_stray_characters() {
        assert!(AppScheme::try_new("1nz.fieldstate").is_err());
        assert!(AppScheme::try_new("nz.field state").is_err());
        assert!(AppScheme::try_new("nz.field/state").is_err());
        assert!(AppScheme::try_new("").is_err());
    }

    #[test]
    fn a_trailing_dot_does_not_make_a_reverse_domain() {
        assert!(AppScheme::try_new("slipway.").is_err());
    }
}
