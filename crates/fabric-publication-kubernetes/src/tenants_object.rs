//! What the tenants object holds beyond the tenants document, and when it
//! must be written.

use fabric_runtime_publication::{
    DocumentOutcome, DocumentPlan, PublicationPlan, ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE,
};

/// The tenants object's plan: written when the tenants document, the record
/// or the registry changed, since all three are projected in the tenants
/// volume. An `Unchanged` plan carries the held bytes at the held revision,
/// so the payload and manifest go back exactly as held.
pub(crate) fn tenants_object_plan(plan: &PublicationPlan) -> DocumentPlan {
    let mut tenants = plan.tenants.clone();
    let registry_written = plan
        .registry
        .as_ref()
        .is_some_and(|registry| registry.outcome == DocumentOutcome::Written);
    if plan.record.outcome == DocumentOutcome::Written || registry_written {
        tenants.outcome = DocumentOutcome::Written;
    }
    tenants
}

/// The keys the tenants object carries beside its own: the record, always,
/// and the registry whenever one is published -- unchanged or not, because
/// a replace writes the whole object and would otherwise drop it.
pub(crate) fn tenants_carried(plan: &PublicationPlan) -> Vec<(&'static str, &[u8])> {
    let mut carried = vec![(PUBLICATION_RECORD_FILE, plan.record.bytes.as_slice())];
    if let Some(registry) = &plan.registry {
        carried.push((ISSUER_REGISTRY_FILE, registry.bytes.as_slice()));
    }
    carried
}
