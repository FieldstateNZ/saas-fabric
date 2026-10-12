//! The three held objects as the plan reads them, and the one object a
//! restore writes back.

use fabric_runtime_publication::{
    DocumentKind, ObservedDocument, PublicationError, RestorePlan, TargetObservation, ISSUER_REGISTRY_FILE,
    PUBLICATION_RECORD_FILE,
};

use crate::errors::unwritable;
use crate::object::{Keys, OBJECT_CAP};
use crate::wire::{ConfigMap, MANAGED_BY_LABEL};

/// The tenants, data-sources and catalogue objects, each absent or as read.
pub(super) type Held = [Option<ConfigMap>; 3];

pub(super) fn observation([tenants, data_sources, catalog]: &Held) -> TargetObservation {
    let key = |object: &Option<ConfigMap>, key: &str| {
        object
            .as_ref()
            .and_then(|object| object.data.get(key))
            .map(|text| text.as_bytes().to_vec())
    };
    let document = |object: &Option<ConfigMap>, kind: DocumentKind| ObservedDocument {
        manifest: key(object, Keys::of(kind).manifest),
        payload: key(object, Keys::of(kind).payload),
    };
    TargetObservation {
        record: key(tenants, PUBLICATION_RECORD_FILE),
        tenants: document(tenants, DocumentKind::Tenants),
        data_sources: document(data_sources, DocumentKind::DataSources),
        catalog: document(catalog, DocumentKind::Catalog),
        registry: key(tenants, ISSUER_REGISTRY_FILE),
    }
}

pub(super) fn version(object: Option<&ConfigMap>) -> Option<&str> {
    object.and_then(|object| object.metadata.resource_version.as_deref())
}

/// The held object with the plan's payload and manifest put back, every
/// other key as held, replaced at the version it was read at.
pub(super) fn restored(held: &ConfigMap, plan: &RestorePlan) -> Result<ConfigMap, PublicationError> {
    let document = plan.document;
    let keys = Keys::of(document);
    let text = |bytes: Vec<u8>| {
        String::from_utf8(bytes).map_err(|_| unwritable(document, "the document's bytes are not UTF-8"))
    };
    if version(Some(held)).is_none() {
        return Err(unwritable(
            document,
            "the held object has no version to replace it at",
        ));
    }
    let mut object = held.clone();
    if let Some(payload) = &plan.payload {
        object
            .data
            .insert(keys.payload.to_owned(), text(payload.clone())?);
    }
    if let Some(manifest) = &plan.manifest {
        let manifest = manifest
            .canonical_json()
            .map_err(|_| unwritable(document, "the manifest could not be rendered"))?;
        object.data.insert(keys.manifest.to_owned(), text(manifest)?);
    }
    object
        .metadata
        .labels
        .insert(MANAGED_BY_LABEL.0.to_owned(), MANAGED_BY_LABEL.1.to_owned());
    let size: usize = object
        .data
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum();
    if size > OBJECT_CAP {
        return Err(unwritable(
            document,
            "the document would exceed the cluster's object size limit",
        ));
    }
    Ok(object)
}
