//! Held states the recovery tests share: every part at revision 3, under a
//! record naming exactly the bytes held.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};

use super::HeldObservation;
use crate::digest::sha256_hex;
use crate::{
    data_sources_canonical_json, tenants_canonical_json, ConnectionSelectorDocument, DataResidencyDocument,
    DataSourceDocument, DocumentKind, DocumentManifest, DocumentRevision, IssuerRegistration,
    IssuerRegistryDocument, ObservedDocument, PlacementClassDocument, PublicationRecord, RecordedDocument,
    RecordedDocuments, RecordedRegistry, RegistryGeneration, RegistryIssuer, ResourceHistory,
    TargetObservation, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

pub(super) const TORN: &[u8] = b"[{\"tenant\":";

pub(super) fn data_source(id: &str) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        connector: crate::ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: crate::ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: crate::PoolSettingsDocument::default(),
        capabilities: crate::DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

pub(super) fn tenant(id: &str, data_source: &str) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: crate::IsolationModelDocument::Database {},
        },
    );
    TenantBindingDocument {
        tenant: TenantId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

pub(super) fn tenants() -> Vec<u8> {
    tenants_canonical_json(&[tenant("acme", "shared")]).unwrap()
}

pub(super) fn data_sources() -> Vec<u8> {
    data_sources_canonical_json(&[data_source("shared")]).unwrap()
}

pub(super) fn registry(tenant: &str) -> Vec<u8> {
    IssuerRegistryDocument::new(
        RegistryGeneration::FIRST,
        vec![IssuerRegistration {
            issuer: RegistryIssuer::parse("https://id.example.com/realms/one").unwrap(),
            tenant: TenantId::try_new(tenant).unwrap(),
        }],
    )
    .unwrap()
    .canonical_json()
    .unwrap()
}

pub(super) fn manifest(document: DocumentKind, revision: u64) -> Vec<u8> {
    DocumentManifest::new(document, DocumentRevision::new(revision))
        .canonical_json()
        .unwrap()
}

/// Every part held at revision 3, under a record naming exactly these
/// tenants, data-sources and (if any) registry bytes.
pub(super) fn held_naming(tenants: &[u8], data_sources: &[u8], registry: Option<&[u8]>) -> HeldObservation {
    let at = |bytes: &[u8]| RecordedDocument::of(DocumentRevision::new(3), bytes);
    let record = PublicationRecord::new(
        7,
        RecordedDocuments {
            tenants: at(tenants),
            data_sources: at(data_sources),
            catalog: at(b"{}"),
        },
        ResourceHistory::default(),
    )
    .with_registry(registry.map(|bytes| RecordedRegistry {
        generation: RegistryGeneration::FIRST,
        sha256: sha256_hex(bytes),
    }));
    let document = |kind, payload: &[u8]| ObservedDocument {
        manifest: Some(manifest(kind, 3)),
        payload: Some(payload.to_vec()),
    };
    HeldObservation {
        observation: TargetObservation {
            record: Some(record.canonical_json().unwrap()),
            tenants: document(DocumentKind::Tenants, tenants),
            data_sources: document(DocumentKind::DataSources, data_sources),
            catalog: document(DocumentKind::Catalog, b"{}"),
            registry: registry.map(<[u8]>::to_vec),
        },
        stable: true,
    }
}

/// The ordinary case: tenants torn under a record that names `tenants()`.
pub(super) fn torn_tenants() -> HeldObservation {
    let mut held = held_naming(&tenants(), &data_sources(), None);
    held.observation.tenants.payload = Some(TORN.to_vec());
    held
}
