//! The decision every adapter makes before it writes anything: given what
//! is held and what is offered, which documents change, to which bytes, at
//! which revision — and which offers are refused. Pure; no I/O.
//!
//! An adapter reads what it holds into a [`HeldDocuments`], calls
//! [`plan_publication`], and writes each [`DocumentPlan`] whose outcome is
//! [`DocumentOutcome::Written`], in the order [`PublicationPlan`]'s fields
//! are declared. The guards ADR 0018 parts 3–6 name live here and nowhere
//! else, so a second adapter cannot drift from the first by
//! re-implementing them.

mod held;
mod parse;

pub use held::{HeldDocument, HeldDocuments};

use crate::verdict::{verdict, Held, Incoming};
use crate::{
    data_sources_canonical_json, tenants_canonical_json, DataSourceDocument, DocumentKind, DocumentOutcome,
    DocumentRevision, PublicationError, RuntimeSnapshot, TenantBindingDocument,
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
/// catalogue, then tenants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationPlan {
    /// `data-sources.json`.
    pub data_sources: DocumentPlan,
    /// `catalog.json`.
    pub catalog: DocumentPlan,
    /// `tenants.json`.
    pub tenants: DocumentPlan,
}

/// Decides a publication without performing it.
///
/// # Errors
///
/// Every refusal the port promises: a stale or divergent document, a
/// dangling or still-bound data source, an unintended emptying, an empty
/// catalogue, a tenant with no data, a held payload gone missing, or held
/// bytes that will not parse. Nothing is decided partially: an error means
/// no document may be written.
pub fn plan_publication(
    snapshot: &RuntimeSnapshot,
    held: &HeldDocuments,
) -> Result<PublicationPlan, PublicationError> {
    let held_tenants: Vec<TenantBindingDocument> = parse::parse_held_documents(
        held.tenants.manifest.as_ref(),
        held.tenants.payload.as_deref(),
        DocumentKind::Tenants,
    )?;
    let held_data_sources: Vec<DataSourceDocument> = parse::parse_held_documents(
        held.data_sources.manifest.as_ref(),
        held.data_sources.payload.as_deref(),
        DocumentKind::DataSources,
    )?;
    crate::validate::validate_snapshot(snapshot, &held_tenants, &held_data_sources)?;

    let data_sources = decide(
        DocumentKind::DataSources,
        held.data_sources.held(),
        snapshot.data_sources.revision,
        data_sources_canonical_json(&snapshot.data_sources.payload),
    )?;
    let catalog = decide(
        DocumentKind::Catalog,
        held.catalog.held(),
        snapshot.catalog.revision,
        snapshot.catalog.payload.canonical_json(),
    )?;
    let tenants = decide(
        DocumentKind::Tenants,
        held.tenants.held(),
        snapshot.tenants.revision,
        tenants_canonical_json(&snapshot.tenants.payload),
    )?;

    Ok(PublicationPlan {
        data_sources,
        catalog,
        tenants,
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
