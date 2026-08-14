//! Rendering the readiness verdict, at two levels of disclosure.
//!
//! Two functions rather than one function with a flag, so that "what an
//! unauthenticated caller may learn" is a whole body written out in one place
//! and cannot acquire a field by accident. See
//! [`detail_access`](crate::health::detail_access) for why the split exists.

use serde_json::{json, Value};

use crate::health::connector_health::ConnectorOutcome;
use crate::health::readiness_facts::RegistryFacts;

/// The body any caller may see: the verdict, and nothing else.
///
/// One bit, and an orchestrator already has it from the status code. Nothing
/// here names a connector, a region, or the size of the estate.
pub(super) fn minimal(ready: bool) -> Value {
    json!({ "ready": ready })
}

/// The body an authorised caller sees: everything needed to diagnose.
///
/// This is the §34 promise — an operator can see exactly what is degraded
/// without reading logs — kept intact and moved behind a credential, rather
/// than dropped.
pub(super) fn detailed(
    ready: bool,
    degraded: bool,
    tenants: &RegistryFacts,
    data_sources: &RegistryFacts,
    outcomes: &[ConnectorOutcome],
) -> Value {
    json!({
        "ready": ready,
        "degraded": degraded,
        "tenants_primed": tenants.primed,
        "data_sources_primed": data_sources.primed,
        "tenants": tenants.count,
        "data_sources": data_sources.count,
        "connectors": outcomes.iter().map(connector).collect::<Vec<Value>>(),
    })
}

/// One connector's entry in the detailed body.
///
/// `reason` is present only for a connector that answered unhealthy: there is
/// nothing to say about a healthy one, and inventing a reason for an unknown
/// one would dress a timeout up as a diagnosis.
fn connector(outcome: &ConnectorOutcome) -> Value {
    let mut entry = json!({
        "id": outcome.id,
        "status": outcome.health.status(),
    });

    if let (Some(reason), Some(object)) = (outcome.health.reason(), entry.as_object_mut()) {
        object.insert("reason".to_owned(), Value::from(reason));
    }

    entry
}
