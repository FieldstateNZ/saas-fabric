//! Where each admin resource lives.
//!
//! One module rather than format strings at call sites, so every URL this
//! crate constructs is visible in one place — which is what makes it checkable
//! that no caller-supplied value is interpolated without having been through a
//! validated newtype first.

use fabric_client_model::{OidcClientId, RealmName};

/// Builds admin API paths for one Keycloak deployment.
pub(crate) struct Paths {
    /// The base URL, without a trailing slash.
    base: String,
}

impl Paths {
    /// Builds a path helper.
    ///
    /// The trailing slash is trimmed once here rather than being guarded
    /// against at every join, which is how a `//` ends up in a URL that then
    /// 404s for reasons nobody can see.
    pub(crate) fn new(base_url: &str) -> Self {
        Self {
            base: base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// The base URL, safe to log — it names an endpoint and no credential.
    pub(crate) fn describe(&self) -> String {
        format!("keycloak at {}", self.base)
    }

    /// The token endpoint for the realm the platform authenticates against.
    pub(crate) fn token(&self, admin_realm: &str) -> String {
        format!("{}/realms/{admin_realm}/protocol/openid-connect/token", self.base)
    }

    /// The collection every realm is created in.
    pub(crate) fn realms(&self) -> String {
        format!("{}/admin/realms", self.base)
    }

    /// One realm.
    ///
    /// `realm` is a [`RealmName`], which is validated as a DNS label — so it
    /// cannot contain a slash or a `..` and cannot address a different
    /// resource. That check happens at parse time precisely so that this
    /// interpolation is safe without escaping here.
    pub(crate) fn realm(&self, realm: &RealmName) -> String {
        format!("{}/admin/realms/{realm}", self.base)
    }

    /// A realm's roles.
    pub(crate) fn roles(&self, realm: &RealmName) -> String {
        format!("{}/admin/realms/{realm}/roles", self.base)
    }

    /// A realm's roles, bounded so a truncated page can be detected.
    pub(crate) fn roles_page(&self, realm: &RealmName, max: usize) -> String {
        format!(
            "{}/admin/realms/{realm}/roles?briefRepresentation=true&first=0&max={max}",
            self.base
        )
    }

    /// A realm's application clients.
    pub(crate) fn clients(&self, realm: &RealmName) -> String {
        format!("{}/admin/realms/{realm}/clients", self.base)
    }

    /// One application client, looked up by the id an application presents.
    pub(crate) fn client_by_client_id(&self, realm: &RealmName, client: &OidcClientId) -> String {
        format!("{}/admin/realms/{realm}/clients?clientId={client}", self.base)
    }

    /// One application client, addressed by Keycloak's internal identifier.
    ///
    /// `internal_id` comes from a Keycloak response rather than from the
    /// platform's model, so it is the one interpolation here whose value this
    /// crate did not validate. It is a UUID Keycloak generated; treating it as
    /// opaque is correct, and it never originates with a caller.
    pub(crate) fn client(&self, realm: &RealmName, internal_id: &str) -> String {
        format!("{}/admin/realms/{realm}/clients/{internal_id}", self.base)
    }
}
