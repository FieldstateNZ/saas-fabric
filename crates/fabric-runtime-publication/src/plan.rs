//! The decision every adapter makes before it writes anything: given what
//! is held and what is offered, which documents change, to which bytes, at
//! which revision — and which offers are refused. Pure; no I/O.
//!
//! In the 121-150 band: the one function every adapter's refusals are
//! decided by, whole so its order of checks reads in one place.
//!
//! An adapter reads what it holds into a [`HeldDocuments`], calls
//! [`plan_publication`], and writes each [`DocumentPlan`] whose outcome is
//! [`DocumentOutcome::Written`], in the order [`PublicationPlan`]'s fields
//! are declared. The guards live here and nowhere else.

mod held;
mod parse;
mod record;

pub use held::{HeldDocument, HeldDocuments};
pub use record::RecordPlan;

use crate::verdict::{verdict, Held, Incoming};
use crate::{
    data_sources_canonical_json, tenants_canonical_json, DocumentKind, DocumentOutcome, DocumentRevision,
    PublicationError, PublicationRecord, RegistryPlan, RuntimeSnapshot,
};

/// One document's decided fate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPlan {
    /// Whether the adapter must write it.
    pub outcome: DocumentOutcome,
    /// The canonical bytes to write when it must.
    pub bytes: Vec<u8>,
    /// The revision its manifest must then carry.
    pub revision: DocumentRevision,
}

/// The whole publication, decided. Fields are declared in the order an
/// adapter must write them (ADR 0018 part 3): data sources, then the
/// catalogue, then tenants, then the record that makes them one generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationPlan {
    /// `data-sources.json`.
    pub data_sources: DocumentPlan,
    /// `catalog.json`.
    pub catalog: DocumentPlan,
    /// `tenants.json`.
    pub tenants: DocumentPlan,
    /// `issuer-registry.json`, after the tenants it names, if offered.
    pub registry: Option<RegistryPlan>,
    /// `publication.json`, written last.
    pub record: RecordPlan,
}

/// Decides a publication without performing it.
///
/// # Errors
///
/// Every refusal the port promises: a stale or divergent document, a
/// dangling or still-bound data source, an unintended emptying, an empty
/// catalogue or one naming a logical data source no tenant binds, a tenant
/// with no data, a resource that does not advance past its last revision, a
/// held tenant removed, a held payload gone missing, held bytes that will
/// not parse, a held record above this publisher's rollback floor, or a
/// [`crate::RegistryRefusal`]. An error means no document may be written.
pub fn plan_publication(
    snapshot: &RuntimeSnapshot,
    held: &HeldDocuments,
) -> Result<PublicationPlan, PublicationError> {
    let held_record = held
        .record
        .as_deref()
        .map(PublicationRecord::parse_held)
        .transpose()?;
    let published = held.resources()?;
    crate::validate::validate_snapshot(snapshot, &published.tenants, &published.data_sources)?;
    let recorded = held_record.as_ref().map(PublicationRecord::documents);

    let data_sources = decide(
        DocumentKind::DataSources,
        held.data_sources.held(recorded.map(|r| r.data_sources.revision)),
        snapshot.data_sources.revision,
        data_sources_canonical_json(&snapshot.data_sources.payload),
    )?;
    let catalog = decide(
        DocumentKind::Catalog,
        held.catalog.held(recorded.map(|r| r.catalog.revision)),
        snapshot.catalog.revision,
        snapshot.catalog.payload.canonical_json(),
    )?;
    let tenants = decide(
        DocumentKind::Tenants,
        held.tenants.held(recorded.map(|r| r.tenants.revision)),
        snapshot.tenants.revision,
        tenants_canonical_json(&snapshot.tenants.payload),
    )?;

    // After the document verdicts, so a whole document offered stale or
    // divergent is reported as such rather than by its first resource.
    crate::resource_revisions::refuse_resource_changes(snapshot, &published)?;
    let history = published
        .history
        .including(&snapshot.tenants.payload, &snapshot.data_sources.payload);
    let recorded = held_record.as_ref().and_then(PublicationRecord::registry);
    let registry = crate::registry::decide(recorded, held.registry.as_deref(), snapshot)?;
    let documents = [&data_sources, &catalog, &tenants];
    let record = record::decide(held_record, documents, history, registry.as_ref())?;

    Ok(PublicationPlan {
        data_sources,
        catalog,
        tenants,
        registry,
        record,
    })
}

fn decide(
    document: DocumentKind,
    held: Option<Held<'_>>,
    revision: DocumentRevision,
    bytes: Result<Vec<u8>, serde_json::Error>,
) -> Result<DocumentPlan, PublicationError> {
    let bytes = bytes.map_err(|cause| PublicationError::Unwritable {
        document,
        cause: Box::new(cause),
    })?;
    let outcome = verdict(
        held,
        &Incoming {
            document,
            revision,
            payload: &bytes,
        },
    )?
    .into();
    Ok(DocumentPlan {
        outcome,
        bytes,
        revision,
    })
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
