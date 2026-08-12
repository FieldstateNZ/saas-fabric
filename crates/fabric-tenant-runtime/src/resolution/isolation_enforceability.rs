//! Whether a DataSource can actually provide the isolation a binding asks
//! for.
//!
//! Its own module rather than a method on the resolver, because it is a
//! different question. Resolution walks a chain and answers *which*
//! DataSource; this answers whether the one it found may be used at all. The
//! two failing for unrelated reasons is easier to read when they are not
//! interleaved.

use fabric_connector::IsolationModel;
use fabric_core::TenantId;

use crate::data_source::PlacementClass;
use crate::{DataSource, ResolveError};

/// Refuses a binding whose isolation the DataSource cannot actually provide.
///
/// The one place placement is read on the request path, and it is read to
/// *refuse*, never to choose — see
/// [`placement_inertness_tests`](crate::resolution) for the tests that pin
/// the difference. Nothing here can change which DataSource is resolved;
/// it can only decide that the resolved one must not be used.
///
/// # Errors
///
/// [`ResolveError::IsolationNotEnforceable`], whose docs carry the full
/// reasoning: a shared DataSource has one connection, and structural
/// isolation has no predicate, so the two together isolate nothing.
pub(super) fn check_isolation_is_enforceable(
    tenant: &TenantId,
    data_source: &DataSource,
    isolation: &IsolationModel,
) -> Result<(), ResolveError> {
    let structural = matches!(
        isolation,
        IsolationModel::Database | IsolationModel::Schema { .. }
    );

    if structural && data_source.placement == PlacementClass::Shared {
        return Err(ResolveError::IsolationNotEnforceable {
            tenant: tenant.clone(),
            data_source: data_source.id.clone(),
            isolation: isolation.telemetry_label(),
        });
    }

    Ok(())
}
