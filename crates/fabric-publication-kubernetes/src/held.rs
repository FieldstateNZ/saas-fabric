//! Turning what the cluster holds into the shape the plan decides against.

use std::collections::BTreeMap;

use fabric_runtime_publication::{
    DocumentKind, DocumentManifest, HeldDocument, HeldDocuments, PublicationError,
};

use crate::client::{Client, Found};
use crate::errors::unreadable;
use crate::object::{name_of, path_of, Keys};

/// One document's object, as read: the held state the plan needs, and the
/// version to send back on a replace.
pub(crate) struct Read {
    pub(crate) held: HeldDocument,
    pub(crate) resource_version: Option<String>,
}

/// All three, read once before a publication is planned.
pub(crate) struct Reads {
    pub(crate) tenants: Read,
    pub(crate) data_sources: Read,
    pub(crate) catalog: Read,
}

impl Reads {
    pub(crate) async fn fetch(client: &Client, namespace: &str) -> Result<Self, PublicationError> {
        Ok(Self {
            tenants: read_one(client, namespace, DocumentKind::Tenants).await?,
            data_sources: read_one(client, namespace, DocumentKind::DataSources).await?,
            catalog: read_one(client, namespace, DocumentKind::Catalog).await?,
        })
    }

    pub(crate) fn held(&self) -> HeldDocuments {
        HeldDocuments {
            tenants: self.tenants.held.clone(),
            data_sources: self.data_sources.held.clone(),
            catalog: self.catalog.held.clone(),
        }
    }
}

async fn read_one(
    client: &Client,
    namespace: &str,
    document: DocumentKind,
) -> Result<Read, PublicationError> {
    match client.get(&path_of(namespace, document), document).await? {
        Found::Absent => Ok(Read {
            held: HeldDocument::absent(),
            resource_version: None,
        }),
        Found::Object(object) => {
            if object.metadata.name != name_of(document) {
                return Err(unreadable(
                    document,
                    "the cluster answered with a different object",
                ));
            }
            Ok(Read {
                held: held_of(&object.data, document)?,
                resource_version: object.metadata.resource_version,
            })
        }
    }
}

fn held_of(
    data: &BTreeMap<String, String>,
    document: DocumentKind,
) -> Result<HeldDocument, PublicationError> {
    let keys = Keys::of(document);
    let manifest = match data.get(keys.manifest) {
        None => None,
        Some(text) => {
            let manifest: DocumentManifest = serde_json::from_str(text)
                .map_err(|_| unreadable(document, "the held manifest cannot be read"))?;
            if manifest.document() != document {
                return Err(unreadable(
                    document,
                    "the held manifest describes a different document",
                ));
            }
            Some(manifest)
        }
    };
    Ok(HeldDocument {
        manifest,
        payload: data.get(keys.payload).map(|text| text.as_bytes().to_vec()),
    })
}
