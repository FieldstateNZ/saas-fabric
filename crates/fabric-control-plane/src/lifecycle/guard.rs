//! Refusing a use the recorded lifecycle does not permit.

use fabric_client_model::lifecycle::Readiness;
use fabric_client_model::{ClientDocument, ClientLifecycle, LifecycleUse};

use crate::ControlPlaneError;

/// Refuses `operation` on a tombstoned client, or one whose deletion is
/// confirmed.
///
/// Readiness is not assessed here, so this is for the uses saved intent
/// permits — editing, placement, publication — and never for routing, which
/// it would refuse for every client.
pub(crate) fn require(document: &ClientDocument, operation: LifecycleUse) -> Result<(), ControlPlaneError> {
    ClientLifecycle::derive(document.lifecycle(), Readiness::unassessed())
        .permits(operation)
        .map_err(ControlPlaneError::from)
}
