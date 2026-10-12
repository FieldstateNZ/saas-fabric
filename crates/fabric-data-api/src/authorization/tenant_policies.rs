//! Per-tenant policy selection.

use std::collections::BTreeMap;

use fabric_core::{LogicalResourceName, TenantId};

use crate::TenantAuthorization;

/// Every tenant's authorization policy, keyed by the tenant the token named.
///
/// # Why per tenant, and what an absent tenant means
///
/// The disposition for D01-10 is per tenant: the trial runs on scopes, every
/// tenant beyond it is checked. A global flag could not say that, and would
/// make adding a second tenant a moment where the first one's protection
/// silently changed.
///
/// So the trial is the thing that has to be *named*. A tenant with no entry
/// is beyond the trial; it has simply declared no relations this process
/// knows of, so it is held to its scopes alone. Give it relations and they are
/// enforced, with no posture to remember to change.
///
/// Where the declarations come from is owed (ADR 0027): today only the
/// composition root supplies them, and it supplies none.
#[derive(Debug, Clone, Default)]
pub struct TenantPolicies {
    /// Each tenant's policy.
    tenants: BTreeMap<TenantId, TenantAuthorization>,
}

impl TenantPolicies {
    /// Adds or replaces one tenant's policy.
    #[must_use]
    pub fn with(mut self, tenant: TenantId, policy: TenantAuthorization) -> Self {
        self.tenants.insert(tenant, policy);
        self
    }

    /// Whether an operation by `tenant` on `resource` must also pass a Check.
    #[must_use]
    pub fn requires_check(&self, tenant: &TenantId, resource: &LogicalResourceName) -> bool {
        self.tenants
            .get(tenant)
            .is_some_and(|policy| policy.requires_check(resource))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TenantPosture;

    fn tenant(value: &str) -> TenantId {
        TenantId::try_new(value).unwrap()
    }

    fn customers() -> LogicalResourceName {
        LogicalResourceName::try_new("customers").unwrap()
    }

    #[test]
    fn each_tenant_gets_its_own_policy() {
        let policies = TenantPolicies::default()
            .with(
                tenant("trial"),
                TenantAuthorization::new(TenantPosture::SyntheticTrial, [customers()]),
            )
            .with(
                tenant("acme"),
                TenantAuthorization::new(TenantPosture::RelationChecked, [customers()]),
            );

        assert!(!policies.requires_check(&tenant("trial"), &customers()));
        assert!(policies.requires_check(&tenant("acme"), &customers()));
    }

    #[test]
    fn a_tenant_with_no_policy_has_no_declared_relations_to_check() {
        assert!(!TenantPolicies::default().requires_check(&tenant("acme"), &customers()));
    }
}
