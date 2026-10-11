//! What the tenants object holds beyond the tenants document, when it must
//! be written, and the three objects a publication writes, in order.

use fabric_runtime_publication::{
    DocumentKind, DocumentManifest, DocumentOutcome, DocumentPlan, PublicationPlan, ISSUER_REGISTRY_FILE,
    PUBLICATION_RECORD_FILE,
};

use crate::held::{Read, Reads};

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
fn tenants_carried(plan: &PublicationPlan) -> Vec<(&'static str, &[u8])> {
    let mut carried = vec![(PUBLICATION_RECORD_FILE, plan.record.bytes.as_slice())];
    if let Some(registry) = &plan.registry {
        carried.push((ISSUER_REGISTRY_FILE, registry.bytes.as_slice()));
    }
    carried
}

/// The manifest the tenants object carries. A tenants document the plan
/// leaves unchanged keeps the manifest it is held with -- including the
/// contract version an older publisher stamped -- so rewriting the object for
/// a new record or registry changes neither the payload nor its manifest,
/// as the filesystem adapter leaves both files alone.
fn tenants_manifest(plan: &PublicationPlan, held: &Read) -> DocumentManifest {
    match (&held.held.manifest, plan.tenants.outcome) {
        (Some(manifest), DocumentOutcome::Unchanged) => manifest.clone(),
        _ => DocumentManifest::new(DocumentKind::Tenants, plan.tenants.revision),
    }
}

/// One object to write: which document, its plan, its manifest, the keys it
/// carries beside them, and how it was read.
pub(crate) type Object<'a> = (
    DocumentKind,
    &'a DocumentPlan,
    DocumentManifest,
    Vec<(&'static str, &'a [u8])>,
    &'a Read,
);

/// The three objects, in ADR 0018 part 3's write order: data sources, the
/// catalogue, then the tenants object carrying the record and the registry.
pub(crate) fn objects<'a>(
    plan: &'a PublicationPlan,
    tenants: &'a DocumentPlan,
    reads: &'a Reads,
) -> [Object<'a>; 3] {
    [
        (
            DocumentKind::DataSources,
            &plan.data_sources,
            DocumentManifest::new(DocumentKind::DataSources, plan.data_sources.revision),
            Vec::new(),
            &reads.data_sources,
        ),
        (
            DocumentKind::Catalog,
            &plan.catalog,
            DocumentManifest::new(DocumentKind::Catalog, plan.catalog.revision),
            Vec::new(),
            &reads.catalog,
        ),
        (
            DocumentKind::Tenants,
            tenants,
            tenants_manifest(plan, &reads.tenants),
            tenants_carried(plan),
            &reads.tenants,
        ),
    ]
}
