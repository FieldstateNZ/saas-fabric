//! Applying the authorization policy.

use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;

use crate::{logging, DataApiError, DataApiService, OperationKind};

impl DataApiService {
    /// Applies the authorization policy, logging a refusal.
    ///
    /// Its own file, away from [`Self::prepare`], so the §23 boundary is
    /// visible rather than asserted: this function receives an operation and an
    /// identity and returns a `Result<()>`. It is handed nothing that could
    /// change the tenant, and has no way to return one.
    ///
    /// Note what else it is not handed: the [`ResourceDefinition`] `prepare`
    /// has already looked up. The decision is a fact about the *caller*, so it
    /// can be reached without describing the resource — which is precisely what
    /// lets `prepare` put it ahead of every step that does describe one.
    ///
    /// [`ResourceDefinition`]: crate::ResourceDefinition
    pub(super) fn authorize(
        &self,
        identity: &TenantIdentity,
        operation: OperationKind,
        resource_name: &LogicalResourceName,
    ) -> Result<(), DataApiError> {
        if self
            .permissions
            .permits(identity, operation, resource_name.as_str())
        {
            return Ok(());
        }

        logging::operation_forbidden(resource_name.as_str(), operation.as_str(), identity.subject());

        Err(DataApiError::Forbidden {
            resource: resource_name.to_string(),
            operation: operation.as_str(),
        })
    }
}
