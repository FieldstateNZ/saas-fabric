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

/// The operation vocabulary, which now lives in the crate both planes share.
///
/// Re-exported rather than redefined: a client's desired state declares which
/// relations permit which operations, so the control plane names these too,
/// and two definitions of "read" would drift without ever failing a build.
pub use fabric_core::OperationKind;

/// The scope conventionally required for an operation on a resource.
///
/// `data:customers:read`, `data:customers:write`. Reads and writes share two
/// scopes rather than five so that tokens stay small and policies stay
/// legible; finer control belongs in the catalogue's `operations` list.
///
/// A free function rather than a method, because [`OperationKind`] belongs to
/// `fabric-core` now and this convention does not: a scope name is this API's
/// way of asking a token a question, and the shared vocabulary should not
/// carry it.
#[must_use]
pub fn required_scope(operation: OperationKind, resource: &str) -> String {
    let action = if operation.is_write() { "write" } else { "read" };

    format!("data:{resource}:{action}")
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

        identity.has_scope(&required_scope(operation, resource))
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::TenantId;
    use fabric_identity::{
        encode_unsigned_token, IdentityConfig, IdentityResolver, TrustedIngressReader, TrustedIssuer,
    };
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;

    /// The issuer registered to `acme`, which every fixture below claims.
    const ACME_ISSUER: &str = "https://identity.test.invalid/realms/acme";

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
        let serde_json::Value::Object(mut object) = claims else {
            panic!("claims must be an object");
        };

        // The resolver binds the tenant through the issuer registry, so a
        // fixture token has to come from an issuer this configuration knows.
        object.insert(
            "iss".to_owned(),
            serde_json::Value::String(ACME_ISSUER.to_owned()),
        );

        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            format!("Bearer {}", encode_unsigned_token(&object))
                .parse()
                .unwrap(),
        );

        let config = IdentityConfig {
            trusted_issuers: vec![TrustedIssuer::new(
                ACME_ISSUER,
                TenantId::try_new("acme").unwrap(),
            )],
            ..IdentityConfig::default()
        };

        IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))))
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
            required_scope(OperationKind::Create, "customers"),
            "data:customers:write"
        );
        assert_eq!(
            required_scope(OperationKind::Update, "customers"),
            "data:customers:write"
        );
    }
}
