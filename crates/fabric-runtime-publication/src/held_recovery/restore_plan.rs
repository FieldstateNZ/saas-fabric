//! Deciding a restore without performing it. Pure; no I/O.

use super::held_parse::{blocking, parse_manifest, parse_payload};
use super::RestoreRefusal;
use crate::digest::sha256_hex;
use crate::{DocumentKind, DocumentManifest, DocumentRevision, PublicationRecord, TargetObservation};

/// One restore, decided: which bytes go back, at which revision. An adapter
/// writes the payload, then the manifest, as every publication writes a
/// document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RestorePlan {
    /// The document restored.
    pub document: DocumentKind,
    /// The revision the held record names for it, which the held state
    /// already reports: a restore never moves it.
    pub revision: DocumentRevision,
    /// The generation of the record that vouches for the bytes.
    pub generation: u64,
    /// The SHA-256 the record names, and the restored bytes have.
    pub sha256: String,
    /// Why the document was blocking publication.
    pub blocked_because: String,
    /// The SHA-256 of the payload held before, if any.
    pub held_payload_sha256: Option<String>,
    /// The payload to write, when the held one is not already these bytes.
    #[serde(rename = "writes_payload", serialize_with = "present")]
    pub payload: Option<Vec<u8>>,
    /// The manifest to write, when none is held or the held one cannot be
    /// read. A readable manifest already states [`Self::revision`].
    #[serde(rename = "writes_manifest")]
    pub manifest: Option<DocumentManifest>,
}

/// Decides whether `offered` may be put back as the held `document`.
///
/// # Errors
///
/// A [`RestoreRefusal`] when the document is not one a publication parses,
/// is not blocking publication, no usable record is held, the held manifest
/// states another revision than the record, or `offered` is not the bytes
/// the record names.
pub fn plan_restore(
    observation: &TargetObservation,
    document: DocumentKind,
    offered: &[u8],
) -> Result<RestorePlan, RestoreRefusal> {
    let held = match document {
        DocumentKind::Tenants => &observation.tenants,
        DocumentKind::DataSources => &observation.data_sources,
        DocumentKind::Catalog => return Err(RestoreRefusal::NotRestorable { document }),
    };
    let blocked_because = blocking(document, held).ok_or(RestoreRefusal::NotBlocked { document })?;
    let record = observation.record.as_deref().ok_or(RestoreRefusal::NoRecord)?;
    let record = PublicationRecord::parse_held(record).map_err(|error| RestoreRefusal::RecordUnusable {
        detail: error.to_string(),
    })?;
    let recorded = match document {
        DocumentKind::Tenants => &record.documents().tenants,
        _ => &record.documents().data_sources,
    };
    if let Some(Ok(manifest)) = held
        .manifest
        .as_deref()
        .map(|bytes| parse_manifest(bytes, document))
    {
        if manifest.revision() != recorded.revision {
            return Err(RestoreRefusal::ManifestDisagrees {
                document,
                manifest: manifest.revision(),
                recorded: recorded.revision,
            });
        }
    }
    let offered_sha256 = sha256_hex(offered);
    if offered_sha256 != recorded.sha256 {
        return Err(RestoreRefusal::DigestMismatch {
            document,
            recorded: recorded.sha256.clone(),
            offered: offered_sha256,
        });
    }
    parse_payload(offered, document)
        .map_err(|detail| RestoreRefusal::OfferedUnparseable { document, detail })?;

    let manifest_readable = held
        .manifest
        .as_deref()
        .is_some_and(|bytes| parse_manifest(bytes, document).is_ok());
    let manifest = (!manifest_readable).then(|| DocumentManifest::new(document, recorded.revision));
    Ok(RestorePlan {
        document,
        revision: recorded.revision,
        generation: record.generation(),
        sha256: recorded.sha256.clone(),
        blocked_because,
        held_payload_sha256: held.payload.as_deref().map(sha256_hex),
        payload: (held.payload.as_deref() != Some(offered)).then(|| offered.to_vec()),
        manifest,
    })
}

#[allow(clippy::ref_option)]
fn present<S: serde::Serializer>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bool(bytes.is_some())
}

#[cfg(test)]
#[path = "restore_plan_tests.rs"]
mod tests;
