//! Layer 1: the scope a token must carry.

use fabric_identity::TenantIdentity;

use crate::OperationKind;

/// The scope required for an operation on a resource.
///
/// `data:customers:read`, `data:customers:write`. Reads and writes share two
/// scopes rather than five so that tokens stay small and policies stay
/// legible; finer control belongs in the catalogue's `operations` list, and
/// per-object control in layer 2.
///
/// A free function rather than a method, because [`OperationKind`] belongs to
/// `fabric-core` and this convention does not: a scope name is this API's way
/// of asking a token a question, and the shared vocabulary should not carry it.
#[must_use]
pub fn required_scope(operation: OperationKind, resource: &str) -> String {
    let action = if operation.is_write() { "write" } else { "read" };

    format!("data:{resource}:{action}")
}

/// Whether the identity's token carries the scope this operation requires.
///
/// The whole of layer 1, and deliberately nothing else: no role is consulted,
/// so no role can stand in for a scope (D01-12).
#[must_use]
pub(crate) fn scopes_permit(identity: &TenantIdentity, operation: OperationKind, resource: &str) -> bool {
    identity.has_scope(&required_scope(operation, resource))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::authorization::caller::fixtures::identity_with;

    #[test]
    fn a_matching_scope_permits_the_operation() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:customers:read"}));

        assert!(scopes_permit(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn a_read_scope_does_not_permit_a_write() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:customers:read"}));

        assert!(!scopes_permit(&identity, OperationKind::Delete, "customers"));
    }

    #[test]
    fn a_scope_for_one_resource_does_not_permit_another() {
        let identity = identity_with(json!({"tenant_id": "acme", "scope": "data:orders:read"}));

        assert!(!scopes_permit(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn an_identity_with_no_scopes_is_refused() {
        let identity = identity_with(json!({"tenant_id": "acme"}));

        assert!(!scopes_permit(&identity, OperationKind::List, "customers"));
    }

    #[test]
    fn no_role_stands_in_for_a_scope() {
        // The role this crate used to treat as a bypass, and the operator role,
        // both held — neither is a scope, so neither permits anything.
        let identity = identity_with(json!({
            "tenant_id": "acme",
            "roles": ["platform-admin", "fabric-operator"]
        }));

        for operation in OperationKind::ALL {
            assert!(!scopes_permit(&identity, operation, "customers"), "{operation}");
        }
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
