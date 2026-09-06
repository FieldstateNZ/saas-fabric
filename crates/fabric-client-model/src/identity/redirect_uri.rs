//! Where an application client may be sent back to after authenticating.
//!
//! Over the 120-line advisory threshold, and the reason is the one
//! `docs/architecture/file-size-policy.md` names: this is one newtype together
//! with its impls. Every rule it applies already lives in a module of its own
//! — `kind` classifies, `host_kind` decides the host, `characters` places the
//! wildcards, `private_use_scheme` carries RFC 8252 §7.1 — and what is left is
//! the type, four accessors, and the three trivial conversions serde needs.
//! Splitting those from the struct would separate things that only make sense
//! together.

mod authority;
#[cfg(test)]
mod authority_tests;
mod characters;
mod host_kind;
mod kind;
mod private_use_scheme;

use std::fmt;

use fabric_core::IdentifierError;

pub use kind::RedirectUriKind;
pub use private_use_scheme::AppScheme;

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
/// Which schemes and hosts are permitted, and the argument for each, is the
/// `kind` module's; where a wildcard may stand is `characters`'.
///
/// # Why the kind is carried with the value
///
/// Parsing one proves what kind of callback it is, so [`Self::kind`] is a
/// field read rather than a rule re-run, and no caller can be handed a URI
/// whose kind nothing has decided. It is derived entirely from the string, so
/// including it in the derived comparisons changes no ordering and no
/// equality: two equal strings always classify the same way.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RedirectUri {
    /// The URI exactly as the document spells it.
    value: String,

    /// What kind of callback it is, decided once at construction.
    kind: RedirectUriKind,
}

impl RedirectUri {
    /// The label used in error messages when parsing fails.
    const KIND: &'static str = "redirect uri";

    /// The inclusive maximum length.
    const MAX_LENGTH: usize = 512;

    /// Parses a redirect URI.
    ///
    /// The parser widens universally and a strategy narrows: a wildcard port
    /// is a spelling accepted anywhere here, and which strategies may hold one
    /// is `redirect_strategy::rules`' question, not this one.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] if the value is empty, longer than 512
    /// bytes, or breaks either of the two rules below.
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

        let kind = kind::classify(value)?;
        characters::check(value)?;

        Ok(Self {
            value: value.to_owned(),
            kind,
        })
    }

    /// Borrows the URI as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// What kind of callback this is, which is what a strategy is stated
    /// against.
    #[must_use]
    pub const fn kind(&self) -> RedirectUriKind {
        self.kind
    }

    /// Whether it ends in a wildcard standing for a path prefix.
    #[must_use]
    pub fn has_path_wildcard(&self) -> bool {
        characters::has_path_wildcard(&self.value)
    }

    /// Whether it names every port rather than one.
    #[must_use]
    pub fn has_wildcard_port(&self) -> bool {
        characters::has_wildcard_port(&self.value)
    }
}

impl fmt::Display for RedirectUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
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
        value.value
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
        // intended to trust. Load-bearing in a way it was not before the
        // wildcard *port* widened the same rule.
        assert!(RedirectUri::try_new("https://*.example.com/callback").is_err());
        assert!(RedirectUri::try_new("https://*.example.com:8443/callback").is_err());
    }

    #[test]
    fn refuses_a_javascript_scheme() {
        // Must still fail after a private-use scheme became representable:
        // admitting any `scheme:` that is not http is the single most
        // plausible way this stops holding.
        assert!(RedirectUri::try_new("javascript:alert(1)").is_err());
        assert!(RedirectUri::try_new("file:///etc/passwd").is_err());
    }

    #[test]
    fn a_wildcard_port_is_how_a_loopback_callback_says_any_port() {
        let uri = RedirectUri::try_new("http://127.0.0.1:*/callback").unwrap();

        assert!(uri.has_wildcard_port());
        assert!(!uri.has_path_wildcard());
        assert_eq!(uri.kind(), RedirectUriKind::Loopback);
    }

    #[test]
    fn a_wildcard_after_a_colon_in_the_path_is_not_a_port() {
        // The looser test this rule could have been: "a `*` preceded by `:`".
        assert!(RedirectUri::try_new("https://www.example.com/a:*/b").is_err());
    }

    #[test]
    fn a_trailing_wildcard_is_a_path_wildcard_and_a_bare_wildcard_port_is_not() {
        assert!(RedirectUri::try_new("https://www.example.com/*")
            .unwrap()
            .has_path_wildcard());
        assert!(!RedirectUri::try_new("http://127.0.0.1:*")
            .unwrap()
            .has_path_wildcard());
    }

    #[test]
    fn refuses_a_uri_longer_than_the_bound_or_carrying_a_space() {
        let long = format!("https://www.example.com/{}", "a".repeat(512));

        assert!(RedirectUri::try_new(long).is_err());
        assert!(RedirectUri::try_new("https://www.example.com/a b").is_err());
        assert!(RedirectUri::try_new("").is_err());
    }
}
