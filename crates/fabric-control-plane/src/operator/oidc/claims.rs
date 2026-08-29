//! What the platform reads out of an operator's token, and what it ignores.

use std::collections::BTreeSet;

use serde::Deserialize;

/// The claims this platform reads from an operator's access token.
///
/// # Deliberately a small subset
///
/// An access token from a realm carries a great deal more than this — session
/// state, scopes, client roles, whatever the provider was configured to add.
/// Naming only the four fields that decide anything keeps the coupling to the
/// provider's token shape visible in one place, and means a provider adding a
/// claim cannot change how this platform behaves.
#[derive(Debug, Deserialize)]
pub(super) struct OperatorClaims {
    /// The provider's stable identifier for the human. Always present.
    pub(super) sub: String,

    /// How the human is named to other humans, when the provider says so.
    ///
    /// Preferred over `sub` for attribution because `sub` is a UUID, and an
    /// audit record naming `f81d4fae-…` answers nobody's question about who
    /// changed a client.
    #[serde(default)]
    pub(super) preferred_username: Option<String>,

    /// Used only if the provider states no username.
    #[serde(default)]
    pub(super) email: Option<String>,

    /// The client the token was issued to.
    ///
    /// Checked instead of `aud`, and that is not a shortcut. A realm mints
    /// access tokens whose audience is the resource server the caller asked
    /// for — commonly `account` — while the client that obtained the token is
    /// named here. Requiring `aud` to equal the console's client id therefore
    /// refuses every genuine token until somebody adds an audience mapper, and
    /// the failure looks like a broken key rather than a missing mapper.
    ///
    /// `azp` answers the question actually worth asking: was this token
    /// obtained by the console, or by some other client in the same realm
    /// whose holder is now presenting it here?
    #[serde(default)]
    pub(super) azp: Option<String>,

    /// The realm roles the provider asserts.
    ///
    /// Absent when the human holds none, which is why this is defaulted rather
    /// than required: a token with no roles is a well-formed token belonging
    /// to somebody who is not an operator, and it should be refused by the
    /// role check rather than by a deserialisation error that reads like the
    /// provider is broken.
    #[serde(default)]
    pub(super) realm_access: RealmAccess,
}

/// The realm-role container, as providers nest it.
#[derive(Debug, Default, Deserialize)]
pub(super) struct RealmAccess {
    /// The role names.
    #[serde(default)]
    pub(super) roles: BTreeSet<String>,
}

impl OperatorClaims {
    /// How this operator should be named in an audit record and a commit.
    ///
    /// Falls back through the claims most useful to a human first, and ends at
    /// `sub`, which is always there — so this never yields an empty name.
    pub(super) fn subject(&self) -> &str {
        self.preferred_username
            .as_deref()
            .or(self.email.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.sub)
    }

    /// Whether the provider asserts the role that confers operator authority.
    pub(super) fn holds(&self, role: &str) -> bool {
        self.realm_access.roles.contains(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> OperatorClaims {
        serde_json::from_str(json).expect("claims should parse")
    }

    #[test]
    fn prefers_the_username_for_attribution() {
        let claims = parse(r#"{"sub":"f81d4fae","preferred_username":"brett","email":"b@example.com"}"#);

        assert_eq!(claims.subject(), "brett");
    }

    #[test]
    fn falls_back_to_email_then_to_the_opaque_subject() {
        assert_eq!(
            parse(r#"{"sub":"f81d4fae","email":"b@example.com"}"#).subject(),
            "b@example.com"
        );
        assert_eq!(parse(r#"{"sub":"f81d4fae"}"#).subject(), "f81d4fae");
    }

    #[test]
    fn a_blank_username_does_not_become_the_name() {
        let claims = parse(r#"{"sub":"f81d4fae","preferred_username":"   "}"#);

        assert_eq!(claims.subject(), "f81d4fae");
    }

    #[test]
    fn a_token_with_no_roles_parses_and_holds_nothing() {
        let claims = parse(r#"{"sub":"f81d4fae"}"#);

        assert!(!claims.holds("fabric-operator"));
    }

    #[test]
    fn reads_realm_roles() {
        let claims = parse(r#"{"sub":"x","realm_access":{"roles":["fabric-operator","other"]}}"#);

        assert!(claims.holds("fabric-operator"));
        assert!(!claims.holds("absent"));
    }
}
