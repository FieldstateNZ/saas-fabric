//! Reading the three objects raw for an inspection, bracketed by the tenants
//! object that carries the record.

use fabric_runtime_publication::{
    DocumentKind, ObservedDocument, PublicationCompleteness, TargetObservation, UnobservedReason,
    ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE,
};

use crate::client::{Client, Found};
use crate::object::{name_of, path_of, Keys};
use crate::wire::ConfigMap;

/// Reads the tenants object, the data-sources and catalogue objects, then
/// the tenants object again, and judges them. The tenants object is one
/// read, so its payload, manifest, record and registry are always one
/// observation; a second read at another `resourceVersion` means the
/// others may belong to another publication.
pub(crate) async fn inspect(client: &Client, namespace: &str) -> PublicationCompleteness {
    let read = async {
        let tenants = read(client, namespace, DocumentKind::Tenants).await?;
        let data_sources = read(client, namespace, DocumentKind::DataSources).await?;
        let catalog = read(client, namespace, DocumentKind::Catalog).await?;
        let again = read(client, namespace, DocumentKind::Tenants).await?;
        Some((tenants, data_sources, catalog, again))
    };
    let Some((tenants, data_sources, catalog, again)) = read.await else {
        return unobserved(UnobservedReason::Unavailable);
    };
    if version(tenants.as_ref()) != version(again.as_ref()) {
        return unobserved(UnobservedReason::Unstable);
    }
    let observation = TargetObservation {
        record: key(tenants.as_ref(), PUBLICATION_RECORD_FILE),
        tenants: document(tenants.as_ref(), DocumentKind::Tenants),
        data_sources: document(data_sources.as_ref(), DocumentKind::DataSources),
        catalog: document(catalog.as_ref(), DocumentKind::Catalog),
        registry: key(tenants.as_ref(), ISSUER_REGISTRY_FILE),
    };
    observation.assess(key(again.as_ref(), PUBLICATION_RECORD_FILE).as_deref())
}

/// One object, absent as `Some(None)`; any failure, or an answer naming
/// another object, as `None`.
async fn read(client: &Client, namespace: &str, document: DocumentKind) -> Option<Option<ConfigMap>> {
    match client.get(&path_of(namespace, document), document).await.ok()? {
        Found::Absent => Some(None),
        Found::Object(object) if object.metadata.name == name_of(document) => Some(Some(object)),
        Found::Object(_) => None,
    }
}

fn version(object: Option<&ConfigMap>) -> Option<&str> {
    object.and_then(|object| object.metadata.resource_version.as_deref())
}

fn key(object: Option<&ConfigMap>, key: &str) -> Option<Vec<u8>> {
    object
        .and_then(|object| object.data.get(key))
        .map(|text| text.as_bytes().to_vec())
}

fn document(object: Option<&ConfigMap>, kind: DocumentKind) -> ObservedDocument {
    let keys = Keys::of(kind);
    ObservedDocument {
        manifest: key(object, keys.manifest),
        payload: key(object, keys.payload),
    }
}

const fn unobserved(reason: UnobservedReason) -> PublicationCompleteness {
    PublicationCompleteness::Unobserved { reason }
}

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
