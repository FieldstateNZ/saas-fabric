//! The private-use URI scheme a native application registers with its
//! operating system.
//!
//! Its own file because this is a distinct security argument: a private-use
//! scheme is not a network location, and no host rule applies to it.

use std::fmt;

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "app scheme";

/// The permitted set, described for the error message.
const EXPECTED: &str = "a reverse-domain scheme such as nz.fieldstate.slipway";

/// Whether a URI scheme candidate is a private-use scheme in RFC 8252 §7.1
/// form.
///
/// The RFC asks a native application to use "a domain name under their
/// control, in reverse order". The observable part of that is the dot: an
/// ordinary scheme (`http`, `javascript`, `data`, `file`, `ftp`) has none, so
/// requiring at least one is what keeps this test from swallowing the schemes
/// the classifier exists to refuse.
///
/// It cannot tell a reversed domain from a forward one — `www.example.com` is
/// a syntactically valid scheme — and mostly does not try. What it does check
/// is `rest`, the text after the colon: a digit immediately there means the
/// text before the colon was never a scheme at all, it was a host, and the
/// colon introduced a port. `www.example.com:8080/cb` has a dot and would
/// otherwise pass the reverse-domain check; refusing it here is what stops a
/// missing `https://` prefix from being reported as a native application's
/// callback. RFC 8252 §7.1's own examples never put a digit straight after
/// the colon — `nz.fieldstate.slipway:/cb` and
/// `nz.fieldstate.slipway://localhost/cb` do not, and stay private-use.
///
/// Nothing is *admitted* by being a private-use scheme: such a URI is only
/// ever accepted under the strategy declaring the same scheme, and that
/// strategy is itself refused until the phase that will carry it.
pub(super) fn is_private_use(scheme: &str, rest: &str) -> bool {
    if rest.starts_with(|character: char| character.is_ascii_digit()) {
        return false;
    }

    AppScheme::check(scheme).is_ok()
}

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
    fn check(value: &str) -> Result<(), IdentifierError> {
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
    fn a_reverse_domain_scheme_is_a_private_use_scheme() {
        assert!(is_private_use("nz.fieldstate.slipway", "/cb"));
        assert!(is_private_use("com.example.app", "://host/cb"));
    }

    #[test]
    fn an_ordinary_scheme_is_not_a_private_use_scheme() {
        // The regression this rule prevents: every one of these would become a
        // private-use scheme under a "not http, therefore private-use" test,
        // and `javascript:` would then be a callback this model accepts.
        // `www.example.com` is the sharper case: it has a dot and would pass
        // the reverse-domain check on its own — what refuses it is the digit
        // straight after the colon, a host:port with no scheme at all.
        for (scheme, rest) in [
            ("http", "//x"),
            ("https", "//x"),
            ("javascript", "alert(1)"),
            ("data", "text/html,x"),
            ("file", "///etc/passwd"),
            ("ftp", "//x"),
            ("www.example.com", "8080/cb"),
        ] {
            assert!(!is_private_use(scheme, rest), "{scheme}");
        }
    }

    #[test]
    fn a_scheme_shaped_candidate_followed_by_a_path_is_still_private_use() {
        // The digit check only fires on a digit. A scheme followed by a path
        // that happens to start with a letter is unaffected.
        assert!(is_private_use("nz.fieldstate.slipway", "alert/cb"));
    }

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
