//! One tenant's authorization policy.

use std::collections::BTreeSet;

use fabric_core::LogicalResourceName;

/// Which layers a tenant's requests pass through (D01-10).
///
/// An enum rather than a `bool` so the trial exception is named wherever it
/// applies, and so it cannot be mistaken for "authorization off": a trial
/// tenant is still held to its scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantPosture {
    /// The isolated synthetic trial. Scopes only; any relations its client
    /// declares are stated, and not enforced.
    SyntheticTrial,

    /// Every other tenant. Scopes, then a relation Check on every resource
    /// that declares relations.
    RelationChecked,
}

/// One tenant's posture and the resources its client declares relations on.
///
/// Two independent facts. Which resources declare relations is the client's
/// desired state (ADR 0013); whether this tenant is the trial is a launch
/// decision. Keeping them apart means leaving the trial is one change to the
/// posture, and nothing about the resources has to be re-stated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantAuthorization {
    /// Which layers apply.
    posture: TenantPosture,

    /// The resources whose `spec.authorization` declares relations.
    relation_resources: BTreeSet<LogicalResourceName>,
}

impl TenantAuthorization {
    /// A tenant in the given posture whose client declares relations on
    /// `relation_resources`.
    #[must_use]
    pub fn new(
        posture: TenantPosture,
        relation_resources: impl IntoIterator<Item = LogicalResourceName>,
    ) -> Self {
        Self {
            posture,
            relation_resources: relation_resources.into_iter().collect(),
        }
    }

    /// Which layers apply.
    #[must_use]
    pub const fn posture(&self) -> TenantPosture {
        self.posture
    }

    /// Whether an operation on `resource` must also pass a relation Check.
    #[must_use]
    pub fn requires_check(&self, resource: &LogicalResourceName) -> bool {
        self.posture == TenantPosture::RelationChecked && self.relation_resources.contains(resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> LogicalResourceName {
        LogicalResourceName::try_new(value).unwrap()
    }

    #[test]
    fn a_checked_tenant_checks_a_resource_that_declares_relations() {
        let policy = TenantAuthorization::new(TenantPosture::RelationChecked, [name("customers")]);

        assert!(policy.requires_check(&name("customers")));
    }

    #[test]
    fn a_checked_tenant_does_not_check_a_resource_that_declares_none() {
        let policy = TenantAuthorization::new(TenantPosture::RelationChecked, [name("customers")]);

        assert!(!policy.requires_check(&name("orders")));
    }

    #[test]
    fn the_trial_checks_nothing_even_where_relations_are_declared() {
        let policy = TenantAuthorization::new(TenantPosture::SyntheticTrial, [name("customers")]);

        assert!(!policy.requires_check(&name("customers")));
    }
}
