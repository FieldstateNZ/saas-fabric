//! Deciding whether an established identity may perform an operation.
//!
//! Specification §23 separates two questions that are easy to conflate:
//!
//! - **Tenant resolution** — which tenant's resources does this target?
//! - **Authorization** — may this identity perform this operation?
//!
//! This module answers only the second. Nothing here can influence the first,
//! and that is enforced structurally: these functions receive the operation and
//! the identity's roles and scopes, and return a `bool`. They are never given
//! anything that could change the tenant, and they have no way to return one.

use fabric_identity::TenantIdentity;

/// The kind of operation being attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// Fetch one row by key.
    Read,
    /// Fetch many rows.
    List,
    /// Insert rows.
    Create,
    /// Modify rows.
    Update,
    /// Remove rows.
    Delete,
}

impl OperationKind {
    /// A stable name for telemetry (§29).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::List => "list",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }

    /// Whether the operation modifies data.
    #[must_use]
    pub const fn is_write(self) -> bool {
        matches!(self, Self::Create | Self::Update | Self::Delete)
    }

    /// The scope conventionally required for this operation on a resource.
    ///
    /// `data:customers:read`, `data:customers:write`. Reads and writes share
    /// two scopes rather than five so that tokens stay small and policies stay
    /// legible; finer control belongs in the catalogue's `operations` list.
    #[must_use]
    pub fn required_scope(self, resource: &str) -> String {
        let action = if self.is_write() { "write" } else { "read" };

        format!("data:{resource}:{action}")
    }
}

/// How a deployment decides who may do what.
///
/// Deliberately simple, and deliberately *not* a policy engine. A real
/// deployment will have opinions this cannot express, and the right place for
/// those is a policy service the platform calls — not a DSL grown here. What
/// this provides is the default that makes the API safe out of the box.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ResourcePermissions {
    /// Whether scope checks are enforced at all.
    ///
    /// Enforced by default. A deployment whose gateway already authorises data
    /// operations can switch this off, but the safe default is on.
    pub require_scopes: bool,

    /// A role that may perform any operation on any resource.
    pub administrator_role: String,
}

impl Default for ResourcePermissions {
    fn default() -> Self {
        Self {
            require_scopes: true,
            administrator_role: "platform-admin".to_owned(),
        }
    }
}

impl ResourcePermissions {
    /// Whether this identity may perform the operation on the resource.
    ///
    /// Note the signature: it takes an operation and an identity and returns a
    /// `bool`. There is no path by which a decision here reaches tenant
    /// selection (§23).
    #[must_use]
    pub fn permits(&self, identity: &TenantIdentity, operation: OperationKind, resource: &str) -> bool {
        if !self.require_scopes {
            return true;
        }

        if identity.has_role(&self.administrator_role) {
            return true;
        }

        identity.has_scope(&operation.required_scope(resource))
    }
}

#[cfg(test)]
mod tests {
    use fabric_identity::{encode_unsigned_token, IdentityConfig, IdentityResolver, TrustedIngressReader};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;

    struct FixedClock;

    impl fabric_core::Clock for FixedClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            1_000
        }
    }

    /// Builds an identity by going through the real resolver, so these tests
    /// exercise the same construction path production uses.
    fn identity_with(claims: serde_json::Value) -> TenantIdentity {
        let serde_json::Value::Object(object) = claims else {
            panic!("claims must be an object");
        };

        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            format!("Bearer {}", encode_unsigned_token(&object))
                .parse()
                .unwrap(),
        );

        IdentityResolver::new(
            IdentityConfig::default(),
            Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
        )
        .resolve(&headers)
        .unwrap()
    }

    #[test]
    fn a_matching_scope_permits_the_operation() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:customers:read"}));

        assert!(ResourcePermissions::default().permits(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn a_read_scope_does_not_permit_a_write() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:customers:read"}));

        assert!(!ResourcePermissions::default().permits(&identity, OperationKind::Delete, "customers"));
    }

    #[test]
    fn a_scope_for_one_resource_does_not_permit_another() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:orders:read"}));

        assert!(!ResourcePermissions::default().permits(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn the_administrator_role_permits_everything() {
        let identity = identity_with(json!({"tenant_id": "acme", "roles": ["platform-admin"]}));

        assert!(ResourcePermissions::default().permits(&identity, OperationKind::Delete, "customers"));
    }

    #[test]
    fn an_identity_with_no_scopes_is_refused_by_default() {
        let identity = identity_with(json!({"tenant_id": "acme"}));

        assert!(!ResourcePermissions::default().permits(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn scope_checks_can_be_delegated_to_the_gateway() {
        let permissions = ResourcePermissions {
            require_scopes: false,
            ..ResourcePermissions::default()
        };
        let identity = identity_with(json!({"tenant_id": "acme"}));

        assert!(permissions.permits(&identity, OperationKind::Delete, "customers"));
    }

    #[test]
    fn administrator_privilege_does_not_extend_across_tenants() {
        // The critical §23 property: an administrator is an administrator *of
        // their own tenant*. Authorization says yes; the tenant is still
        // whatever the token said, and nothing here can change it.
        let identity = identity_with(json!({"tenant_id": "acme", "roles": ["platform-admin"]}));

        assert!(ResourcePermissions::default().permits(&identity, OperationKind::Delete, "customers"));
        assert_eq!(identity.tenant().as_str(), "acme");
    }

    #[test]
    fn write_operations_share_a_scope() {
        assert_eq!(
            OperationKind::Create.required_scope("customers"),
            "data:customers:write"
        );
        assert_eq!(
            OperationKind::Update.required_scope("customers"),
            "data:customers:write"
        );
    }
}
