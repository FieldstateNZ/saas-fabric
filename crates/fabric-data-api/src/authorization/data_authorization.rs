//! The two layers, composed.

use std::sync::Arc;

use fabric_core::LogicalResourceName;

use crate::authorization::required_scope::scopes_permit;
use crate::authorization::{DenialReason, Refusal};
use crate::{
    AuthorizationCheck, Caller, CheckDecision, CheckObject, CheckQuestion, NoCheckConfigured, OperationKind,
    TenantPolicies,
};

/// Decides whether a caller may perform an operation (ADR 0027).
///
/// Composition rather than a trait over "policies": the layers are not
/// alternatives to choose between, they are applied in a fixed order, and the
/// only thing that varies is the Check behind layer 2 — which is already a
/// port.
pub struct DataAuthorization {
    /// Which tenants need layer 2, and on which resources.
    policies: TenantPolicies,

    /// Where layer 2's decisions come from.
    check: Arc<dyn AuthorizationCheck>,
}

impl DataAuthorization {
    /// Composes the per-tenant policies with a Check.
    #[must_use]
    pub fn new(policies: TenantPolicies, check: Arc<dyn AuthorizationCheck>) -> Self {
        Self { policies, check }
    }

    /// Applies layer 1, then layer 2 where the caller's tenant requires it.
    ///
    /// `key` is the record the request names, when it names one.
    ///
    /// Layer 1 runs first and alone decides a refusal: a caller without the
    /// scope never causes a Check, so an unscoped caller cannot use the
    /// authorization service's latency or availability to learn which
    /// resources declare relations.
    pub(crate) async fn decide(
        &self,
        caller: &Caller,
        operation: OperationKind,
        resource: &LogicalResourceName,
        key: Option<&str>,
    ) -> Result<(), Refusal> {
        let identity = caller.identity();

        if !scopes_permit(identity, operation, resource.as_str()) {
            return Err(Refusal::Denied(DenialReason::MissingScope));
        }

        if !self.policies.requires_check(identity.tenant(), resource) {
            return Ok(());
        }

        let key = key.ok_or(Refusal::Denied(DenialReason::NoObjectToCheck))?;
        let object = CheckObject::try_new(resource.clone(), key)
            .map_err(|_| Refusal::Denied(DenialReason::UnnameableObject))?;

        match self
            .check
            .check(caller.credential(), &CheckQuestion { operation, object })
            .await
        {
            Ok(CheckDecision::Allowed) => Ok(()),
            Ok(CheckDecision::Denied) => Err(Refusal::Denied(DenialReason::CheckDenied)),
            Err(failure) => Err(Refusal::Undecided(failure)),
        }
    }
}

/// No tenant policies and no Check: every tenant is held to its scopes, and a
/// policy added later without a Check fails closed rather than open.
impl Default for DataAuthorization {
    fn default() -> Self {
        Self::new(TenantPolicies::default(), Arc::new(NoCheckConfigured))
    }
}

#[cfg(test)]
mod data_authorization_tests;
