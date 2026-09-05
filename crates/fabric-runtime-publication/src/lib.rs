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
//! # What this crate does not own
//!
//! Only the wire contract: the document types, [`DocumentManifest`], and
//! canonical serialisation. It has no port and no filesystem adapter — those
//! belong to a later slice. There is, on purpose, no production caller of
//! anything here yet.
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
mod ids;
mod manifest;

pub use document::{
    data_sources_canonical_json, tenants_canonical_json, CatalogDocument, ConfigurationBindingDocument,
    ConnectionSelectorDocument, DataResidencyDocument, DataSourceCapabilitiesDocument, DataSourceDocument,
    IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument, ResourceDefinitionDocument,
    StorageBindingDocument, TenantBindingDocument, TenantDataBindingDocument,
};
pub use document_revision::DocumentRevision;
pub use ids::{ConnectionName, ConnectorId, FieldName};
pub use manifest::{
    DocumentKind, DocumentManifest, CATALOG_FILE, CATALOG_MANIFEST_FILE, CONTRACT_VERSION, DATA_SOURCES_FILE,
    DATA_SOURCES_MANIFEST_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};

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
