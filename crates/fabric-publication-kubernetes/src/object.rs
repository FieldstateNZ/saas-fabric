//! The object each document becomes, and the rules on its size and name.

use std::collections::BTreeMap;

use fabric_runtime_publication::{
    DocumentKind, DocumentManifest, DocumentPlan, PublicationError, CATALOG_FILE, CATALOG_MANIFEST_FILE,
    DATA_SOURCES_FILE, DATA_SOURCES_MANIFEST_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};

use crate::errors::unwritable;
use crate::wire::{ConfigMap, Metadata, MANAGED_BY_LABEL};

/// One mebibyte: the API server's cap on a whole `ConfigMap`. A document
/// that would pass it is refused before any write, never split (ADR 0018).
pub(crate) const OBJECT_CAP: usize = 1024 * 1024;

/// The `data` keys one document occupies.
pub(crate) struct Keys {
    pub(crate) payload: &'static str,
    pub(crate) manifest: &'static str,
}

impl Keys {
    pub(crate) const fn of(document: DocumentKind) -> Self {
        match document {
            DocumentKind::Tenants => Self {
                payload: TENANTS_FILE,
                manifest: TENANTS_MANIFEST_FILE,
            },
            DocumentKind::DataSources => Self {
                payload: DATA_SOURCES_FILE,
                manifest: DATA_SOURCES_MANIFEST_FILE,
            },
            DocumentKind::Catalog => Self {
                payload: CATALOG_FILE,
                manifest: CATALOG_MANIFEST_FILE,
            },
        }
    }
}

/// ADR 0018's names. Fixed: the platform repository mounts them by name.
pub(crate) const fn name_of(document: DocumentKind) -> &'static str {
    match document {
        DocumentKind::Tenants => "fabric-runtime-tenants",
        DocumentKind::DataSources => "fabric-runtime-data-sources",
        DocumentKind::Catalog => "fabric-runtime-catalog",
    }
}

/// The collection path (for a create) or the object path (for a read or a
/// replace). The namespace is this deployment's, validated at construction;
/// the name is a constant. Nothing from a document reaches a path.
pub(crate) fn collection_of(namespace: &str) -> String {
    format!("/api/v1/namespaces/{namespace}/configmaps")
}

pub(crate) fn path_of(namespace: &str, document: DocumentKind) -> String {
    format!("{}/{}", collection_of(namespace), name_of(document))
}

/// Builds the object a document's plan writes: both keys, the managing
/// label, and — for a replace — the version it was read at.
///
/// # Errors
///
/// [`PublicationError::Unwritable`] when the object would exceed the API
/// server's size cap, decided here so nothing is written first.
pub(crate) fn object_for(
    namespace: &str,
    document: DocumentKind,
    plan: &DocumentPlan,
    resource_version: Option<String>,
) -> Result<ConfigMap, PublicationError> {
    let keys = Keys::of(document);
    let payload = String::from_utf8(plan.bytes.clone())
        .map_err(|_| unwritable(document, "the document's bytes are not UTF-8"))?;
    let manifest = DocumentManifest::new(document, plan.revision)
        .canonical_json()
        .map_err(|_| unwritable(document, "the manifest could not be rendered"))?;
    let manifest =
        String::from_utf8(manifest).map_err(|_| unwritable(document, "the manifest is not UTF-8"))?;

    if payload.len() + manifest.len() + keys.payload.len() + keys.manifest.len() > OBJECT_CAP {
        return Err(unwritable(
            document,
            "the document would exceed the cluster's object size limit",
        ));
    }

    let mut data = BTreeMap::new();
    data.insert(keys.payload.to_owned(), payload);
    data.insert(keys.manifest.to_owned(), manifest);
    let mut labels = BTreeMap::new();
    labels.insert(MANAGED_BY_LABEL.0.to_owned(), MANAGED_BY_LABEL.1.to_owned());

    Ok(ConfigMap {
        metadata: Metadata {
            name: name_of(document).to_owned(),
            namespace: namespace.to_owned(),
            labels,
            resource_version,
        },
        data,
    })
}
