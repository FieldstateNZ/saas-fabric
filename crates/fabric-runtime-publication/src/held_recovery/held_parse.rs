//! Reading held bytes by the rules a publication reads them by, so a
//! diagnosis or a restore never disagrees with the plan about what blocks.

use crate::{DataSourceDocument, DocumentKind, DocumentManifest, ObservedDocument, TenantBindingDocument};

/// Parses a held manifest, refusing one that does not parse or that names
/// another document, as `filesystem/held.rs` and the Kubernetes adapter's
/// `held.rs` both do before a publication is planned.
pub(super) fn parse_manifest(bytes: &[u8], document: DocumentKind) -> Result<DocumentManifest, String> {
    let manifest: DocumentManifest = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if manifest.document() == document {
        Ok(manifest)
    } else {
        Err(format!(
            "the manifest describes {:?}, not {document:?}",
            manifest.document()
        ))
    }
}

/// Parses a held payload as the document it is held as, the way
/// `plan/parse.rs` does. The catalogue is never parsed before a
/// publication, so it always passes here.
pub(super) fn parse_payload(bytes: &[u8], document: DocumentKind) -> Result<(), String> {
    let parsed = match document {
        DocumentKind::Tenants => serde_json::from_slice::<Vec<TenantBindingDocument>>(bytes).map(drop),
        DocumentKind::DataSources => serde_json::from_slice::<Vec<DataSourceDocument>>(bytes).map(drop),
        DocumentKind::Catalog => Ok(()),
    };
    parsed.map_err(|error| error.to_string())
}

/// Why this held document refuses every publication, if it does: a manifest
/// that cannot be read, a manifest whose payload is gone, or a payload that
/// does not parse. `None` when a publication could be planned against it.
pub(super) fn blocking(document: DocumentKind, held: &ObservedDocument) -> Option<String> {
    let manifest = match held
        .manifest
        .as_deref()
        .map(|bytes| parse_manifest(bytes, document))
    {
        Some(Err(detail)) => return Some(format!("unreadable manifest: {detail}")),
        Some(Ok(manifest)) => Some(manifest),
        None => None,
    };
    match (manifest, held.payload.as_deref()) {
        (Some(_), None) if document != DocumentKind::Catalog => {
            Some("held payload lost: the manifest is held but the payload is gone".to_owned())
        }
        (_, Some(bytes)) => parse_payload(bytes, document)
            .err()
            .map(|detail| format!("unreadable payload: {detail}")),
        (_, None) => None,
    }
}
