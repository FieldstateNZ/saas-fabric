//! Applying the authorization policy.

use fabric_core::LogicalResourceName;

use crate::authorization::Refusal;
use crate::{logging, Caller, DataApiError, DataApiService, OperationKind};

impl DataApiService {
    /// Applies both authorization layers, logging a refusal.
    ///
    /// Its own file, away from [`Self::prepare`], so the §23 boundary is
    /// visible rather than asserted: this function receives an operation and a
    /// caller and returns a `Result<()>`. It is handed nothing that could
    /// change the tenant, and has no way to return one.
    ///
    /// Note what else it is not handed: the [`ResourceDefinition`] `prepare`
    /// has already looked up. The decision is about the *caller* and the record
    /// they named, so it can be reached without describing the resource —
    /// which is what lets `prepare` put it ahead of every step that does.
    ///
    /// Every denial is the same 403 whichever layer refused, so the status
    /// cannot tell a caller whether a resource declares relations. A Check
    /// that could not be decided is refused too, as a 5xx: it is the platform's
    /// failure, and only a caller who already holds the scope can reach it.
    ///
    /// [`ResourceDefinition`]: crate::ResourceDefinition
    pub(super) async fn authorize(
        &self,
        caller: &Caller,
        operation: OperationKind,
        resource_name: &LogicalResourceName,
        key: Option<&str>,
    ) -> Result<(), DataApiError> {
        match self
            .authorization
            .decide(caller, operation, resource_name, key)
            .await
        {
            Ok(()) => Ok(()),

            Err(Refusal::Denied(reason)) => {
                logging::operation_forbidden(
                    resource_name.as_str(),
                    operation.as_str(),
                    caller.identity().subject(),
                    reason.as_str(),
                );

                Err(DataApiError::Forbidden {
                    resource: resource_name.to_string(),
                    operation: operation.as_str(),
                })
            }

            Err(Refusal::Undecided(failure)) => Err(DataApiError::AuthorizationUndecided(failure)),
        }
    }
}
