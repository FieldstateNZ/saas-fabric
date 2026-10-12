//! Deciding a restore without performing it. Pure; no I/O.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! plan type and the one function whose order of refusals it records.

use super::held_parse::{parse_payload, record_refusal};
use super::problem::{Part, Problem};
use super::restore_checks::{manifest_plan, offered_problems};
use super::{diagnose, HeldObservation, RestoreRefusal};
use crate::digest::sha256_hex;
use crate::{DocumentKind, DocumentManifest, DocumentRevision, PublicationRecord};

/// One restore, decided: which bytes go back, at which revision, against
/// which record. An adapter writes the payload, then the manifest.
///
/// It says nothing about the generation as a whole: `peers` lists every
/// other part a fresh inspection found unclear, and only a later inspection
/// can find the generation clear.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RestorePlan {
    /// The document restored.
    pub document: DocumentKind,
    /// The revision the held record names for it. A restore never moves it.
    pub revision: DocumentRevision,
    /// The generation of the record the plan is bound to.
    pub generation: u64,
    /// The SHA-256 of that record's exact bytes.
    pub record_sha256: String,
    /// The SHA-256 the record names, and the restored bytes have.
    pub sha256: String,
    /// The SHA-256 of the payload held before, if any.
    pub held_payload_sha256: Option<String>,
    /// The payload to write, when the held one is not already these bytes.
    #[serde(rename = "writes_payload", serialize_with = "present")]
    pub payload: Option<Vec<u8>>,
    /// The manifest to write, only when none is held or it is malformed.
    #[serde(rename = "writes_manifest")]
    pub manifest: Option<DocumentManifest>,
    /// Every other part that is unclear, or that a rule could not be
    /// checked against because it is unusable.
    pub peers: Vec<Problem>,
}

impl RestorePlan {
    /// Nothing to write: the recorded bytes and a valid manifest are held.
    #[must_use]
    pub const fn already_restored(&self) -> bool {
        self.payload.is_none() && self.manifest.is_none()
    }
}

/// Decides whether `offered` may be put back as the held `document`.
///
/// Digest agreement shows the bytes are the ones the record held now names.
/// It is not signed history: whoever can replace the record and payload
/// together is not stopped by it.
///
/// # Errors
///
/// A [`RestoreRefusal`] saying which rule refused it.
pub fn plan_restore(
    held: &HeldObservation,
    document: DocumentKind,
    offered: &[u8],
) -> Result<RestorePlan, RestoreRefusal> {
    let observation = &held.observation;
    let held_document = match document {
        DocumentKind::Tenants => &observation.tenants,
        DocumentKind::DataSources => &observation.data_sources,
        DocumentKind::Catalog => return Err(RestoreRefusal::NotRestorable { document }),
    };
    if !held.stable {
        return Err(RestoreRefusal::Unstable);
    }
    let record_bytes = observation.record.as_deref().ok_or(RestoreRefusal::NoRecord)?;
    let record =
        PublicationRecord::parse_held(record_bytes).map_err(|error| RestoreRefusal::RecordUnusable {
            detail: record_refusal(&error),
        })?;
    let recorded = match document {
        DocumentKind::Tenants => &record.documents().tenants,
        _ => &record.documents().data_sources,
    };
    let manifest = manifest_plan(held_document.manifest.as_deref(), document, recorded.revision)?;
    let offered_sha256 = sha256_hex(offered);
    if offered_sha256 != recorded.sha256 {
        return Err(RestoreRefusal::DigestMismatch {
            document,
            recorded: recorded.sha256.clone(),
            offered: offered_sha256,
        });
    }
    let parsed = parse_payload(offered, document)
        .map_err(|detail| RestoreRefusal::OfferedUnparseable { document, detail })?;
    let peers: Vec<Problem> = diagnose(held)
        .problems
        .into_iter()
        .filter(|problem| problem.part != Part::from(document))
        .collect();
    let problems = offered_problems(&parsed, observation, &record);
    if !problems.is_empty() {
        return Err(RestoreRefusal::OfferedInvalid { document, problems });
    }
    let payload = match held_document.payload.as_deref() {
        Some(bytes) if bytes == offered => None,
        Some(bytes) if parse_payload(bytes, document).is_ok() => {
            return Err(RestoreRefusal::HeldReadable { document })
        }
        _ => Some(offered.to_vec()),
    };
    Ok(RestorePlan {
        document,
        revision: recorded.revision,
        generation: record.generation(),
        record_sha256: sha256_hex(record_bytes),
        sha256: recorded.sha256.clone(),
        held_payload_sha256: held_document.payload.as_deref().map(sha256_hex),
        payload,
        manifest,
        peers,
    })
}

#[allow(clippy::ref_option)]
fn present<S: serde::Serializer>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bool(bytes.is_some())
}

#[cfg(test)]
#[path = "restore_plan_tests.rs"]
mod tests;
