//! The established identity context that the rest of the runtime consumes.

use fabric_core::TenantId;

/// The tenant identity context for one request.
///
/// This is the *output* of identity resolution and the input to everything
/// downstream. Holding one means the request carried a token the platform
/// accepted and that the token named a syntactically valid tenant.
///
/// Note what it does **not** mean: the tenant is not yet known to exist. That
/// is the runtime registry's job, and an unknown tenant is rejected there
/// (§28).
///
/// # Why the fields are private
///
/// `tenant` has exactly one accessor and no setter. Tenant selection must have
/// a single authoritative source (§11), so there is no way to construct this
/// type with a tenant that did not come from the token, and no way to change it
/// afterwards. Authorization code may read `roles` and `scopes` freely — but
/// per §23, nothing it decides can alter `tenant`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantIdentity {
    tenant: TenantId,
    subject: String,
    roles: Vec<String>,
    scopes: Vec<String>,
}

impl TenantIdentity {
    /// Builds an identity context. Called only by
    /// [`IdentityResolver`](crate::IdentityResolver), from verified claims.
    #[must_use]
    pub(crate) fn new(tenant: TenantId, subject: String, roles: Vec<String>, scopes: Vec<String>) -> Self {
        Self {
            tenant,
            subject,
            roles,
            scopes,
        }
    }

    /// The tenant this request represents. The only tenant it may ever
    /// represent.
    #[must_use]
    pub const fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// The authenticated principal, from the `sub` claim.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The role names carried by the token, for authorization decisions.
    #[must_use]
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// The scopes carried by the token, for authorization decisions.
    #[must_use]
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// Whether the token carries the named role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|held| held == role)
    }

    /// Whether the token carries the named scope.
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|held| held == scope)
    }
}
