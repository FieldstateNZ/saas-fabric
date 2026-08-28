//! Where an application client may be sent back to after authenticating.

use std::fmt;

use fabric_core::IdentifierError;

/// A redirect URI an application client is permitted to return to.
///
/// # Why this is validated here rather than left to the identity provider
///
/// A redirect URI is the security boundary of an OAuth flow: an over-broad
/// entry is how an authorisation code ends up somewhere it should not. Keycloak
/// would accept almost anything, so the refusal has to happen before the value
/// is written to Git, not after it has been reconciled — otherwise the
/// dangerous value is already the desired state and the platform is arguing
/// with its own source of truth.
///
/// The rule permits `https://` anywhere, and `http://` only for `localhost` and
/// `127.0.0.1`, which is what a development flow genuinely needs and is the
/// narrowest exception that covers it. A single trailing `*` is allowed
/// because Keycloak's own path wildcard is the ordinary way to register a
/// callback path prefix; a `*` anywhere else is refused, since a wildcard in
/// the host is the mistake this check exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RedirectUri(String);

impl RedirectUri {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "redirect uri";

    /// The inclusive maximum length.
    const MAX_LENGTH: usize = 512;

    /// The permitted set, described for the error message.
    const EXPECTED: &'static str =
        "an https:// URI, or an http:// URI on localhost, with no spaces and at most one \
         trailing wildcard";

    /// The prefixes an `http://` URI may start with.
    const LOOPBACK_PREFIXES: [&'static str; 2] = ["http://localhost", "http://127.0.0.1"];

    /// Parses a redirect URI.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 512
    /// bytes, does not use a permitted scheme, contains whitespace or a
    /// control character, or contains a `*` anywhere but the final position.
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

        Self::check_scheme(value)?;
        Self::check_characters(value)?;

        Ok(Self(value.to_owned()))
    }

    /// Borrows the URI as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Refuses any scheme but `https://`, and `http://` off the loopback host.
    fn check_scheme(value: &str) -> Result<(), IdentifierError> {
        let permitted = value.starts_with("https://")
            || Self::LOOPBACK_PREFIXES
                .into_iter()
                .any(|prefix| value.starts_with(prefix));

        if permitted {
            Ok(())
        } else {
            Err(IdentifierError::BadBoundary { kind: Self::KIND })
        }
    }

    /// Refuses whitespace, control characters, and a misplaced wildcard.
    fn check_characters(value: &str) -> Result<(), IdentifierError> {
        let last = value.len().saturating_sub(1);

        for (index, character) in value.char_indices() {
            let misplaced_wildcard = character == '*' && index != last;

            if character.is_whitespace() || character.is_control() || misplaced_wildcard {
                return Err(IdentifierError::DisallowedCharacter {
                    kind: Self::KIND,
                    character,
                    expected: Self::EXPECTED,
                });
            }
        }

        Ok(())
    }
}

impl fmt::Display for RedirectUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RedirectUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RedirectUri> for String {
    fn from(value: RedirectUri) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_https_callback_and_a_trailing_wildcard() {
        assert!(RedirectUri::try_new("https://www.example.com/callback").is_ok());
        assert!(RedirectUri::try_new("https://www.example.com/*").is_ok());
    }

    #[test]
    fn accepts_loopback_over_plain_http_for_development() {
        assert!(RedirectUri::try_new("http://localhost:5173/callback").is_ok());
    }

    #[test]
    fn refuses_plain_http_anywhere_else() {
        assert!(RedirectUri::try_new("http://www.example.com/callback").is_err());
    }

    #[test]
    fn refuses_a_wildcard_in_the_host() {
        // The mistake this check exists for: `https://*.example.com` reads as
        // a subdomain wildcard and would accept a host the operator never
        // intended to trust.
        assert!(RedirectUri::try_new("https://*.example.com/callback").is_err());
    }

    #[test]
    fn refuses_a_javascript_scheme() {
        assert!(RedirectUri::try_new("javascript:alert(1)").is_err());
    }
}
