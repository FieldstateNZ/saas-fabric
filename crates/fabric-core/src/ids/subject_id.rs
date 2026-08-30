//! Who an authorization decision is about.

use crate::{ids::slug, IdentifierError};

/// The longest subject this platform will carry.
///
/// A Keycloak subject is a 36-character UUID and most providers are shorter.
/// The bound is generous rather than tight because the value is *foreign* —
/// it is whatever the identity provider minted — and a limit that fits today's
/// provider is a limit that rejects tomorrow's. It exists so the value cannot
/// be unbounded, not to describe any provider in particular.
const MAX_SUBJECT: usize = 255;

/// Characters that cannot appear in a subject.
///
/// The first three are the authorization service's: `:` separates a type from
/// an identifier, `#` introduces a userset, and `*` is the wildcard. The
/// fourth is this platform's own separator. A subject carrying any of them
/// would produce an identifier that parses as something other than itself.
const RESERVED: [char; 4] = [':', '#', '*', '/'];

/// Who a decision is about: a subject, qualified by the realm that issued it.
///
/// # Why the realm is part of the name
///
/// A provider's subject is unique **within its realm and nowhere else**. It is
/// an opaque identifier minted in one issuer's namespace, and reading it
/// outside that namespace is a category error even when the string happens to
/// be unique in practice.
///
/// Store-per-client means the realm is usually implied by *which* store a
/// decision is made in, so qualifying looks redundant. It is not, and the
/// reason is what happens when something goes wrong: a tuple written into the
/// wrong client's store with a bare subject is a grant that silently applies
/// to whoever holds that subject there. The same tuple carrying its realm
/// matches nothing. The qualification turns a misrouted grant from a security
/// failure into an inert row.
///
/// # Why this is validated here rather than trusted to the provider
///
/// The authorization service does not check the shape. `acme/`, `/subject`
/// and `acme//doubled` are all accepted by it as ordinary identifiers —
/// measured, not assumed. Each is a distinct subject that will never match the
/// one intended, so a malformed value fails as a silently-denied request
/// rather than as an error anybody sees. This type is where that is caught.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubjectId {
    /// The realm that issued the subject.
    realm: String,

    /// The subject as the provider minted it.
    subject: String,
}

impl SubjectId {
    /// Names a subject within a realm.
    ///
    /// The realm takes the same DNS-label rule a realm name takes everywhere
    /// else in the platform, so one spelling is valid in one place only.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] describing the first rule broken — by the
    /// realm or by the subject.
    pub fn try_new(realm: &str, subject: &str) -> Result<Self, IdentifierError> {
        let realm = slug::parse_dns_label("realm", realm)?;

        if subject.is_empty() {
            return Err(IdentifierError::Empty { kind: "subject" });
        }

        if subject.len() > MAX_SUBJECT {
            return Err(IdentifierError::TooLong {
                kind: "subject",
                max: MAX_SUBJECT,
                actual: subject.len(),
            });
        }

        for character in subject.chars() {
            if character.is_whitespace() || RESERVED.contains(&character) {
                return Err(IdentifierError::DisallowedCharacter {
                    kind: "subject",
                    character,
                    expected: "any non-whitespace character except ':', '#', '*' and '/'",
                });
            }
        }

        Ok(Self {
            realm,
            subject: subject.to_owned(),
        })
    }

    /// The realm that issued the subject.
    #[must_use]
    pub fn realm(&self) -> &str {
        &self.realm
    }

    /// The subject as the provider minted it.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

impl std::fmt::Display for SubjectId {
    /// `<realm>/<subject>` — the canonical form, and what is written into a
    /// tuple.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.realm, self.subject)
    }
}
