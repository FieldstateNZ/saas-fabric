//! The runtime state wire contract: three independently versioned documents,
//! owned by neither plane.
//!
//! `fabric-tenant-runtime` and `fabric-data-api` already know how to
//! *consume* `tenants.json`, `data-sources.json`, and `catalog.json` — the
//! reconciled-resource lifecycle, the divergent-payload guard, the last-good
//! snapshot on a failed load, all of it already exists and is tested. What
//! is missing is anything that *writes* those files, and this crate is the
//! contract that writer will publish against.
//!
//! # Why this crate is in neither plane
//!
//! The producer cannot share a Rust type with the consumer without breaking
//! plane separation: `TenantRuntimeBinding` and `DataSource` live in
//! `fabric-tenant-runtime`, `IsolationModel` and `ConnectionSelector` live in
//! `fabric-connector`, and both are runtime plane. A future control-plane
//! caller must never depend on either, so this crate declares its own copy
//! of every wire shape instead, and depends on nothing but `fabric-core` —
//! exactly as `fabric-core` itself depends on nothing.
//!
//! Fidelity between the two copies is not a shared type. It is
//! `#[serde(deny_unknown_fields)]` on the consumer's own types, which turns a
//! field this crate adds (and the consumer does not know) into a
//! deserialisation error, and a field this crate stops emitting (that the
//! consumer requires) into a missing-field error. Neither drift can pass
//! silently. The round-trip tests beside each document type in this crate
//! are what pin that guarantee down in code rather than leaving it as an
//! assertion in a doc comment.
//!
//! # What this crate owns
//!
//! The wire contract (the document types, [`DocumentManifest`], and
//! canonical serialisation), the [`RuntimePublication`] port, the pure
//! verdict a publication is decided by, and [`FilesystemRuntimePublication`],
//! the adapter that writes it to a local disk. There is, on purpose, still
//! no production *caller* of any of it — that is the control-plane crate
//! named in ADR 0018, "The production owner", and it does not exist yet.
//! A composed acceptance test driving this crate's output through the real
//! runtime and Data API lives under `tests/`, alongside the filesystem
//! adapter's own integration tests.
//!
//! # No field anywhere can hold a secret value
//!
//! A connection is a selector: a name the connector already holds
//! configuration for ([`ConnectionSelectorDocument::Named`]), or a reference
//! to a secret ([`ConnectionSelectorDocument::Secret`]). A tenant's
//! `secrets` field, and a storage area's `credentials` field, are base
//! paths, never values. Nothing in this crate is handed a secret resolver,
//! and it depends on no crate that has one.

mod canonical;
mod document;
mod document_revision;
mod errors;
mod filesystem;
mod ids;
mod manifest;
mod port;
mod published_revisions;
mod report;
mod snapshot;
mod validate;
#[cfg(test)]
mod validate_tests;
mod verdict;
#[cfg(test)]
mod verdict_tests;

pub use document::{
    data_sources_canonical_json, tenants_canonical_json, CatalogDocument, ConfigurationBindingDocument,
    ConnectionSelectorDocument, DataResidencyDocument, DataSourceCapabilitiesDocument, DataSourceDocument,
    EmptyTenantDataBindingsError, IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument,
    ResourceDefinitionDocument, StorageBindingDocument, TenantBindingDocument, TenantDataBindingDocument,
    TenantDataBindings,
};
pub use document_revision::DocumentRevision;
pub use errors::PublicationError;
pub use filesystem::FilesystemRuntimePublication;
pub use ids::{CollectionName, ConnectionName, ConnectorId, FieldName, SchemaName};
pub use manifest::{
    DocumentKind, DocumentManifest, CATALOG_FILE, CATALOG_MANIFEST_FILE, CONTRACT_VERSION, DATA_SOURCES_FILE,
    DATA_SOURCES_MANIFEST_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};
pub use port::RuntimePublication;
pub use published_revisions::PublishedRevisions;
pub use report::{DocumentOutcome, PublicationReport};
pub use snapshot::{DocumentInput, Emptying, RuntimeSnapshot};

#[cfg(test)]
mod tests {
    use super::*;

    const TENANTS_JSON: &str = include_str!("../../../examples/tenants.json");
    const DATA_SOURCES_JSON: &str = include_str!("../../../examples/data-sources.json");
    const CATALOG_JSON: &str = include_str!("../../../examples/catalog.json");

    #[test]
    fn the_shipped_example_documents_parse_as_published_documents() {
        // Not regenerated -- the weaker, cheaper property this milestone
        // asks for is that the publisher's own types can read what the
        // repository already ships, not that the publisher produced it.
        let tenants: Vec<TenantBindingDocument> = serde_json::from_str(TENANTS_JSON).unwrap();
        let data_sources: Vec<DataSourceDocument> = serde_json::from_str(DATA_SOURCES_JSON).unwrap();
        let catalog: CatalogDocument = serde_json::from_str(CATALOG_JSON).unwrap();

        assert!(!tenants.is_empty());
        assert!(!data_sources.is_empty());
        assert!(!catalog.is_empty());
    }

    #[test]
    fn the_shipped_example_documents_survive_the_whole_fidelity_path() {
        // The test above stops at the producer's own read. This one carries
        // the shipped corpus the rest of the way: re-render every document
        // through the canonical serialiser this crate would actually publish
        // with, then re-parse the result as the consumer's own types -- the
        // same round trip a real publication takes.
        let tenants: Vec<TenantBindingDocument> = serde_json::from_str(TENANTS_JSON).unwrap();
        let data_sources: Vec<DataSourceDocument> = serde_json::from_str(DATA_SOURCES_JSON).unwrap();
        let catalog: CatalogDocument = serde_json::from_str(CATALOG_JSON).unwrap();

        let tenants_bytes = tenants_canonical_json(&tenants).unwrap();
        let data_sources_bytes = data_sources_canonical_json(&data_sources).unwrap();
        let catalog_bytes = catalog.canonical_json().unwrap();

        let runtime_tenants: Vec<fabric_tenant_runtime::TenantRuntimeBinding> =
            serde_json::from_slice(&tenants_bytes).unwrap();
        let runtime_data_sources: Vec<fabric_tenant_runtime::DataSource> =
            serde_json::from_slice(&data_sources_bytes).unwrap();
        let runtime_catalog: fabric_data_api::ResourceCatalog =
            serde_json::from_slice(&catalog_bytes).unwrap();

        assert_eq!(runtime_tenants.len(), tenants.len());
        assert_eq!(runtime_data_sources.len(), data_sources.len());
        assert!(!runtime_catalog.is_empty());
    }

    #[test]
    fn a_published_document_carries_a_secret_reference_and_never_a_value() {
        // Proof by construction: each of these three fields is a bare
        // `String` reference path, with no sibling field a resolved value
        // could occupy. What goes in is exactly what comes out on the wire.
        let reference = "vault/tenants/acme".to_owned();

        let selector = ConnectionSelectorDocument::Secret {
            reference: reference.clone(),
        };
        assert_eq!(serde_json::to_value(&selector).unwrap()["reference"], reference);

        let storage = StorageBindingDocument {
            provider: "azure-blob".to_owned(),
            container: "tenant-acme".to_owned(),
            prefix: None,
            credentials: Some(reference.clone()),
        };
        assert_eq!(serde_json::to_value(&storage).unwrap()["credentials"], reference);
    }
}
