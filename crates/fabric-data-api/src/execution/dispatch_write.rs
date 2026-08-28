//! Scoping a write, sending it, and deciding whether it may be called a
//! success.
//!
//! Split from `write_operations` because it is the single point every mutation
//! passes through, and the two guarantees that live here are easy to lose in
//! among the three operations that call it: the tenant scoping applied on the
//! way out, and the reconciliation applied on the way back.

use fabric_connector::MutationSpec;
use fabric_core::LogicalResourceName;

use crate::execution::prepared::Prepared;
use crate::execution::write_integrity::{ensure_consistent, RowBudget};
use crate::{logging, DataApiError, WriteResponse};

/// Applies tenant scoping, dispatches a write, and checks what came back.
///
/// `budget` is what the operation asked for, expressed as a row count. The
/// outcome is refused unless the backend's own count agrees with it — see
/// `execution::write_integrity` for why that comparison is the only guarantee
/// available here, and what it still cannot establish.
///
/// A free function rather than a method: it needs nothing from
/// [`DataApiService`](crate::DataApiService), because everything an operation
/// requires is already on its [`Prepared`], and taking `&self` would suggest
/// otherwise.
///
/// # Errors
///
/// Any [`DataApiError`] the connector raises, plus
/// [`DataApiError::PartiallyApplied`] if fewer rows were written than sent.
pub(super) async fn dispatch(
    prepared: &Prepared<'_>,
    spec: &MutationSpec,
    resource_name: &LogicalResourceName,
    budget: &RowBudget,
) -> Result<WriteResponse, DataApiError> {
    let target = &prepared.resolved.target;

    logging::operation_dispatched(
        resource_name,
        &prepared.resource.data_source,
        spec.operation_name(),
        target,
    );

    // `for_target` scopes the predicate and stamps the tenant discriminator
    // onto written rows. Every write goes through it.
    let scoped = spec.for_target(target);

    // `failed` rather than `?`: a mutation's transport failure is answered by
    // *where on the wire* it broke, and that answer only exists once the
    // operation is known. See `errors::connector_mapping`.
    let outcome = prepared
        .connector
        .mutate(target, &scoped)
        .await
        .map_err(|error| prepared.failed(error))?;

    // Before the response is built, so a count that disagrees with the request
    // can never be serialised into a success body.
    ensure_consistent(budget, &outcome, prepared.connector.id(), prepared.operation)?;

    Ok(WriteResponse::from_outcome(&outcome, &prepared.visible_fields()))
}
